//! The canonical form of runtime guards: one hash-consed, reduced, ordered decision diagram per
//! decision context over its input decisions (seams P2).
//!
//! A runtime guard may test another runtime guard (a join test, a deferred construction, an abort
//! continuation). Its canonical form composes every such test into the diagram of its holder, so
//! the form depends only on which input decisions the guard admits. Two guards are then
//! compatible exactly when the conjunction of their forms reaches true, which is a walk over the
//! two forms instead of a search through the nested tests.
//!
//! Every form and query runs under its own fixed allowance.
//! Completed recursive results retain the exact work of their cold DAG walk.
//! Shared hits account for that work, preserving overlaps already charged by the current proof.
//! Earlier queries and eviction cannot move an exhaustion boundary; an exhausted composition
//! publishes no result.

use std::{
    cell::{Cell, RefCell, RefMut},
    rc::Rc,
};

use ahash::AHashMap;

use super::RuntimeGuard;

/// Steps one form construction or one compatibility walk may take.
const MAX_CANONICAL_WORK: usize = 16_384;
/// Storage bound for a memo generation and its expanded certificates.
const MAX_CANONICAL_GENERATION: usize = 1 << 22;
/// Diagram nodes a context keeps before it starts over (answers do not depend on the tables).
/// Unit tests start over far more often, so every query is checked against fresh tables.
const MAX_CANONICAL_NODES: usize = if cfg!(test) {
    64
} else {
    MAX_CANONICAL_GENERATION
};

const FALSE: u32 = 0;
const TRUE: u32 = 1;

/// One internal node: the input decision it tests (larger decisions nearer the root, the order
/// the decision store builds in) and its two cofactors.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FormNode {
    decision: u64,
    no: u32,
    yes: u32,
}

pub(super) struct CanonicalGuards {
    nodes: Vec<FormNode>,
    unique: AHashMap<FormNode, u32>,
    /// The form of each runtime guard node by its id; `None` when its allowance ran out.
    forms: AHashMap<u64, Option<u32>>,
    /// Completed constructions and their cold charges, keyed by operand forms.
    /// Form ids name one diagram within a generation, and `start_query` clears both tables.
    composed: ProofMemo<[u32; 3], u32>,
    /// Completed ordered compatibility searches and their full cold charges.
    compatible: ProofMemo<[Polar; 2], bool>,
    /// Eviction capacity, independent of the proof allowance.
    memo_capacity: usize,
}

/// Identities survive result eviction, so overlapping certificates still name the same work.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct WorkId(u32);

impl WorkId {
    fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }

    fn ordinal(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy)]
struct WorkSpan {
    first: WorkId,
    last: WorkId,
}

impl WorkSpan {
    fn count(self) -> usize {
        usize::try_from(self.last.ordinal() - self.first.ordinal() + 1).unwrap_or(usize::MAX)
    }

    fn touches(self, other: Self) -> bool {
        self.first.ordinal() <= other.last.ordinal().saturating_add(1)
            && other.first.ordinal() <= self.last.ordinal().saturating_add(1)
    }

    fn union(self, other: Self) -> Self {
        Self {
            first: self.first.min(other.first),
            last: self.last.max(other.last),
        }
    }
}

/// Persistent radix sets share unchanged subtrees across completed cold work certificates.
#[derive(Clone, Default)]
enum WorkSet {
    #[default]
    Empty,
    Span(WorkSpan),
    Bits {
        base: u32,
        bits: u32,
    },
    Branch(Rc<WorkBranch>),
}

/// Counts physical radix allocations across shared certificates and query scratch.
#[derive(Default)]
struct WorkStorage {
    live: Cell<usize>,
    next_query: Cell<u64>,
    // Each proof borrows the scratch words for one epoch.
    marks: RefCell<Vec<MarkedWord>>,
}

#[derive(Clone, Copy, Default)]
struct MarkedWord {
    epoch: u64,
    bits: u32,
}

struct WorkBranch {
    storage: Rc<WorkStorage>,
    // A completed include has marked every identity below this branch.
    included_in: Cell<u64>,
    left: WorkSet,
    right: WorkSet,
    bounds: WorkSpan,
    count: usize,
}

impl Drop for WorkBranch {
    fn drop(&mut self) {
        self.storage.live.set(self.storage.live.get() - 1);
    }
}

struct WorkSplit {
    low: WorkSet,
    high: WorkSet,
    reuse: Option<Rc<WorkBranch>>,
}

impl WorkSet {
    fn count(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Span(span) => span.count(),
            Self::Bits { base: _, bits } => {
                usize::try_from(bits.count_ones()).unwrap_or(usize::MAX)
            }
            Self::Branch(branch) => branch.count,
        }
    }

    fn bounds(&self) -> Option<WorkSpan> {
        match self {
            Self::Empty => None,
            Self::Span(span) => Some(*span),
            Self::Bits { base, bits } => Some(WorkSpan {
                first: WorkId(*base + bits.trailing_zeros()),
                last: WorkId(*base + 31 - bits.leading_zeros()),
            }),
            Self::Branch(branch) => Some(branch.bounds),
        }
    }

    #[cfg(test)]
    fn contains(&self, id: WorkId) -> bool {
        match self {
            Self::Empty => false,
            Self::Span(span) => span.first <= id && id <= span.last,
            Self::Bits { base, bits } => {
                id.ordinal() & !31 == *base && bits & (1 << (id.ordinal() & 31)) != 0
            }
            Self::Branch(branch) => {
                if id < branch.bounds.first || id > branch.bounds.last {
                    return false;
                }
                if branch.left.bounds().is_some_and(|bounds| id <= bounds.last) {
                    branch.left.contains(id)
                } else {
                    branch.right.contains(id)
                }
            }
        }
    }

    fn word(base: u32, bits: u32) -> Self {
        if bits == 0 {
            return Self::Empty;
        }
        let bounds = WorkSpan {
            first: WorkId(base + bits.trailing_zeros()),
            last: WorkId(base + 31 - bits.leading_zeros()),
        };
        if usize::try_from(bits.count_ones()).unwrap_or_default() == bounds.count() {
            Self::Span(bounds)
        } else {
            Self::Bits { base, bits }
        }
    }

    fn bits(&self) -> u32 {
        match self {
            Self::Empty => 0,
            Self::Span(span) => {
                (u32::MAX << (span.first.ordinal() & 31))
                    & (u32::MAX >> (31 - (span.last.ordinal() & 31)))
            }
            Self::Bits { base: _, bits } => *bits,
            Self::Branch(branch) => branch.left.bits() | branch.right.bits(),
        }
    }

    fn split(&self, pivot: WorkId) -> (Self, Self) {
        let Some(bounds) = self.bounds() else {
            return (Self::Empty, Self::Empty);
        };
        if bounds.last < pivot {
            return (self.clone(), Self::Empty);
        }
        if bounds.first >= pivot {
            return (Self::Empty, self.clone());
        }
        match self {
            Self::Empty => (Self::Empty, Self::Empty),
            Self::Span(span) => (
                Self::Span(WorkSpan {
                    first: span.first,
                    last: WorkId(pivot.ordinal() - 1),
                }),
                Self::Span(WorkSpan {
                    first: pivot,
                    last: span.last,
                }),
            ),
            Self::Bits { base, bits } => {
                let lower = u32::MAX >> (32 - (pivot.ordinal() - *base));
                (
                    Self::word(*base, bits & lower),
                    Self::word(*base, bits & !lower),
                )
            }
            // A crossing branch has the same highest differing bit as this union's root.
            Self::Branch(branch) => (branch.left.clone(), branch.right.clone()),
        }
    }

    fn split_owned(self, pivot: WorkId) -> WorkSplit {
        let Some(bounds) = self.bounds() else {
            return WorkSplit {
                low: Self::Empty,
                high: Self::Empty,
                reuse: None,
            };
        };
        if bounds.last < pivot {
            return WorkSplit {
                low: self,
                high: Self::Empty,
                reuse: None,
            };
        }
        if bounds.first >= pivot {
            return WorkSplit {
                low: Self::Empty,
                high: self,
                reuse: None,
            };
        }
        match self {
            Self::Branch(mut branch) => {
                if let Some(parts) = Rc::get_mut(&mut branch) {
                    let low = std::mem::take(&mut parts.left);
                    let high = std::mem::take(&mut parts.right);
                    WorkSplit {
                        low,
                        high,
                        reuse: Some(branch),
                    }
                } else {
                    WorkSplit {
                        low: branch.left.clone(),
                        high: branch.right.clone(),
                        reuse: None,
                    }
                }
            }
            work @ (Self::Empty | Self::Span(_) | Self::Bits { base: _, bits: _ }) => {
                let (low, high) = work.split(pivot);
                WorkSplit {
                    low,
                    high,
                    reuse: None,
                }
            }
        }
    }

    fn union(self, other: &Self, storage: &Rc<WorkStorage>, allowance: &mut usize) -> Option<Self> {
        let Some(left) = self.bounds() else {
            return Some(other.clone());
        };
        let Some(right) = other.bounds() else {
            return Some(self);
        };
        match (&self, other) {
            (Self::Branch(left), Self::Branch(right)) if Rc::ptr_eq(left, right) => {
                return Some(self);
            }
            (
                Self::Span(_),
                Self::Empty | Self::Span(_) | Self::Bits { base: _, bits: _ } | Self::Branch(_),
            ) if left.first <= right.first && left.last >= right.last => {
                return Some(self);
            }
            (
                Self::Empty | Self::Span(_) | Self::Bits { base: _, bits: _ } | Self::Branch(_),
                Self::Span(_),
            ) if right.first <= left.first && right.last >= left.last => {
                return Some(other.clone());
            }
            (Self::Span(_), Self::Span(_)) if left.touches(right) => {
                return Some(Self::Span(left.union(right)));
            }
            (
                Self::Empty | Self::Span(_) | Self::Bits { base: _, bits: _ } | Self::Branch(_),
                Self::Empty | Self::Span(_) | Self::Bits { base: _, bits: _ } | Self::Branch(_),
            ) => {}
        }
        let bounds = left.union(right);
        if bounds.first.ordinal() >> 5 == bounds.last.ordinal() >> 5 {
            return Some(Self::word(
                bounds.first.ordinal() & !31,
                self.bits() | other.bits(),
            ));
        }
        let bit = (bounds.first.ordinal() ^ bounds.last.ordinal()).ilog2();
        let pivot = WorkId((bounds.last.ordinal() >> bit) << bit);
        let WorkSplit { low, high, reuse } = self.split_owned(pivot);
        let (right_low, right_high) = other.split(pivot);
        let low = low.union(&right_low, storage, allowance)?;
        let high = high.union(&right_high, storage, allowance)?;
        if let (Self::Span(low), Self::Span(high)) = (&low, &high)
            && low.touches(*high)
        {
            return Some(Self::Span(low.union(*high)));
        }
        let count = low.count() + high.count();
        if let Some(mut reuse) = reuse
            && let Some(slot) = Rc::get_mut(&mut reuse)
        {
            // Preserve the allocation's registration while replacing its uniquely owned children.
            slot.left = low;
            slot.right = high;
            slot.bounds = bounds;
            slot.count = count;
            slot.included_in.set(0);
            return Some(Self::Branch(reuse));
        }
        *allowance = allowance.checked_sub(1)?;
        storage.live.set(storage.live.get() + 1);
        Some(Self::Branch(Rc::new(WorkBranch {
            storage: Rc::clone(storage),
            included_in: Cell::new(0),
            count,
            left: low,
            right: high,
            bounds,
        })))
    }

    fn compose<'a>(
        id: WorkId,
        children: impl IntoIterator<Item = &'a Self>,
        storage: &Rc<WorkStorage>,
        allowance: &mut usize,
    ) -> Option<Self> {
        let mut work = Self::Span(WorkSpan {
            first: id,
            last: id,
        });
        for child in children {
            work = work.union(child, storage, allowance)?;
        }
        Some(work)
    }
}

/// The exact work DAG remains available when an expanded certificate is not retained.
#[derive(Clone)]
struct WorkNode {
    children: [Option<WorkId>; 2],
    fast: Option<WorkSet>,
    cold: Option<std::num::NonZeroUsize>,
}

#[derive(Clone, Copy)]
struct Completed<T> {
    value: T,
    id: Option<WorkId>,
}

impl<T> Completed<T> {
    fn terminal(value: T) -> Self {
        Self { value, id: None }
    }
}

#[derive(Clone)]
enum WorkEntry<T> {
    Pending,
    Complete { value: Option<T>, work: WorkNode },
}

impl<T: Copy> WorkEntry<T> {
    fn work(&self) -> Option<&WorkNode> {
        match self {
            Self::Pending => None,
            Self::Complete { value: _, work } => Some(work),
        }
    }

    fn value(&self) -> Option<T> {
        match self {
            Self::Pending => None,
            Self::Complete { value, work: _ } => *value,
        }
    }
}

struct ProofQuery<'marks, T> {
    epoch: u64,
    remaining: usize,
    marks: RefMut<'marks, Vec<MarkedWord>>,
    visited_count: usize,
    root: Option<WorkId>,
    certificates: usize,
    /// Query-local values survive persistent eviction without retaining duplicate certificates.
    retained: AHashMap<WorkId, T>,
}

impl<'marks, T: Copy> ProofQuery<'marks, T> {
    fn new(remaining: usize, certificates: usize, storage: &'marks WorkStorage) -> Self {
        let epoch = storage.next_query.get() + 1;
        storage.next_query.set(epoch);
        let marks = storage.marks.borrow_mut();
        Self {
            epoch,
            remaining,
            marks,
            visited_count: 0,
            root: None,
            certificates,
            retained: AHashMap::new(),
        }
    }

    fn contains(&self, id: WorkId) -> bool {
        let word = WorkId(id.ordinal() >> 5).index();
        self.marks.get(word).is_some_and(|word| {
            word.epoch == self.epoch && word.bits & (1 << (id.ordinal() & 31)) != 0
        })
    }

    fn include_word(&mut self, word: u32, bits: u32) -> Option<()> {
        let index = WorkId(word).index();
        if index >= self.marks.len() {
            self.marks
                .resize(index.checked_add(1)?, MarkedWord::default());
        }
        let slot = self.marks.get_mut(index)?;
        let marked = slot.epoch == self.epoch;
        let previous = if marked { slot.bits } else { 0 };
        let missing = bits & !previous;
        if missing == 0 {
            return Some(());
        }
        let added = usize::try_from(missing.count_ones()).ok()?;
        let Some(remaining) = self.remaining.checked_sub(added) else {
            self.remaining = 0;
            return None;
        };
        if marked {
            slot.bits |= bits;
        } else {
            *slot = MarkedWord {
                epoch: self.epoch,
                bits,
            };
        }
        self.remaining = remaining;
        self.visited_count += added;
        Some(())
    }

    fn include_set(&mut self, work: &WorkSet) -> Option<()> {
        match work {
            WorkSet::Empty => Some(()),
            WorkSet::Bits { base, bits } => self.include_word(base >> 5, *bits),
            WorkSet::Span(WorkSpan { first, last }) => {
                let first = first.ordinal();
                let last = last.ordinal();
                let first_word = first >> 5;
                let last_word = last >> 5;
                let lower = u32::MAX << (first & 31);
                let upper = u32::MAX >> (31 - (last & 31));
                if first_word == last_word {
                    return self.include_word(first_word, lower & upper);
                }
                self.include_word(first_word, lower)?;
                for word in first_word + 1..last_word {
                    self.include_word(word, u32::MAX)?;
                }
                self.include_word(last_word, upper)
            }
            WorkSet::Branch(branch) => {
                if branch.included_in.get() != self.epoch {
                    self.include_set(&branch.left)?;
                    self.include_set(&branch.right)?;
                    branch.included_in.set(self.epoch);
                }
                Some(())
            }
        }
    }

    fn visit(&mut self, id: WorkId) -> Option<()> {
        self.include_word(id.ordinal() >> 5, 1 << (id.ordinal() & 31))
    }

    fn include(&mut self, root: WorkId, entries: &[WorkEntry<T>]) -> Option<()> {
        let mut pending = Vec::new();
        let mut id = root;
        loop {
            if !self.contains(id) {
                let work = entries.get(id.index())?.work()?;
                if let Some(fast) = &work.fast {
                    self.include_set(fast)?;
                } else {
                    self.visit(id)?;
                    // Follow one child directly and retain only the other arm of a fork.
                    match work.children {
                        [Some(left), Some(right)] => {
                            pending.push(left);
                            id = right;
                            continue;
                        }
                        [Some(child), None] | [None, Some(child)] => {
                            id = child;
                            continue;
                        }
                        [None, None] => {}
                    }
                }
            }
            let Some(next) = pending.pop() else {
                return Some(());
            };
            id = next;
        }
    }
}

struct Prepared<T> {
    id: WorkId,
    value: Option<T>,
}

/// Direct work dependencies survive value eviction; expanded certificates are bounded accelerators.
struct ProofMemo<K, T> {
    keys: AHashMap<K, WorkId>,
    entries: Vec<WorkEntry<T>>,
    completed: Vec<WorkId>,
    storage: Rc<WorkStorage>,
    certificate_allowance: usize,
}

impl<K, T> Default for ProofMemo<K, T> {
    fn default() -> Self {
        Self {
            keys: AHashMap::new(),
            entries: Vec::new(),
            completed: Vec::new(),
            storage: Rc::default(),
            certificate_allowance: MAX_CANONICAL_WORK * 64,
        }
    }
}

impl<K: Copy + Eq + std::hash::Hash, T: Copy> ProofMemo<K, T> {
    fn get(&self, key: &K) -> Option<&WorkEntry<T>> {
        let entry = self.entries.get(self.keys.get(key)?.index())?;
        entry.value()?;
        Some(entry)
    }

    fn prepare(&mut self, key: K) -> Option<Prepared<T>> {
        let next = WorkId(u32::try_from(self.entries.len()).ok()?);
        let entries = &mut self.entries;
        let id = *self.keys.entry(key).or_insert_with(|| {
            entries.push(WorkEntry::Pending);
            next
        });
        Some(Prepared {
            id,
            value: self.entries.get(id.index())?.value(),
        })
    }

    fn certificate(
        &self,
        id: WorkId,
        children: [Option<WorkId>; 2],
        query: &mut ProofQuery<'_, T>,
    ) -> Option<WorkSet> {
        let mut sets = [None, None];
        for (slot, child) in sets.iter_mut().zip(children) {
            if let Some(child) = child {
                *slot = Some(self.entries.get(child.index())?.work()?.fast.as_ref()?);
            }
        }
        WorkSet::compose(
            id,
            sets.into_iter().flatten(),
            &self.storage,
            &mut query.certificates,
        )
    }

    fn store(
        &mut self,
        id: WorkId,
        value: T,
        mut children: [Option<WorkId>; 2],
        capacity: usize,
        query: &mut ProofQuery<'_, T>,
    ) -> Option<()> {
        if children[0] == children[1] {
            children[1] = None;
        }
        let previous = self.entries.get(id.index())?.work();
        let cold = previous.and_then(|work| work.cold);
        let fast = self.certificate(id, children, query);
        let cold = if query.root == Some(id) {
            std::num::NonZeroUsize::new(query.visited_count)
        } else {
            cold.or_else(|| {
                fast.as_ref()
                    .and_then(|work| std::num::NonZeroUsize::new(work.count()))
            })
            .or_else(|| {
                // A root cannot occur below itself in the ordered work DAG.
                let count = match children {
                    [None, None] => 1,
                    [Some(child), None] | [None, Some(child)] => {
                        self.entries.get(child.index())?.work()?.cold?.get() + 1
                    }
                    [Some(_), Some(_)] => return None,
                };
                std::num::NonZeroUsize::new(count)
            })
        };
        let work = WorkNode {
            children,
            fast,
            cold,
        };
        if capacity == 0 {
            *self.entries.get_mut(id.index())? = WorkEntry::Complete { value: None, work };
            query.retained.insert(id, value);
            return Some(());
        }
        if self.completed.len() >= capacity {
            for previous in self.completed.drain(..) {
                if let Some(WorkEntry::Complete { value, work }) =
                    self.entries.get_mut(previous.index())
                {
                    if let Some(value) = value.take()
                        && query.contains(previous)
                    {
                        query.retained.insert(previous, value);
                    }
                    work.fast = None;
                }
            }
        }
        *self.entries.get_mut(id.index())? = WorkEntry::Complete {
            value: Some(value),
            work,
        };
        self.completed.push(id);
        Some(())
    }

    fn record_cold(&mut self, id: Option<WorkId>, charge: usize) {
        if let Some(id) = id
            && let Some(WorkEntry::Complete { value: _, work }) = self.entries.get_mut(id.index())
        {
            work.cold = std::num::NonZeroUsize::new(charge);
        }
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.entries.clear();
        self.completed.clear();
    }

    fn clear_certificates(&mut self) {
        // Certificate pressure must not discard completed values or their direct work DAG.
        for entry in &mut self.entries {
            if let WorkEntry::Complete { value: _, work } = entry {
                work.fast = None;
            }
        }
    }

    fn start_query(&mut self, identities: usize) {
        if self.entries.len() >= identities {
            self.clear();
        } else if self.storage.live.get() >= MAX_CANONICAL_GENERATION {
            self.clear_certificates();
        }
    }
}

impl Default for CanonicalGuards {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            unique: AHashMap::new(),
            forms: AHashMap::new(),
            composed: ProofMemo::default(),
            compatible: ProofMemo::default(),
            memo_capacity: usize::MAX,
        }
    }
}

/// A form read at one outcome: the documents where the guard evaluates to `value`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Polar {
    form: u32,
    value: bool,
}

impl CanonicalGuards {
    fn node(&self, form: u32) -> Option<FormNode> {
        let index = usize::try_from(form.checked_sub(2)?).ok()?;
        self.nodes.get(index).copied()
    }

    fn top(&self, form: u32) -> Option<u64> {
        self.node(form).map(|node| node.decision)
    }

    fn cofactors(&self, form: u32, decision: u64) -> (u32, u32) {
        match self.node(form) {
            Some(node) if node.decision == decision => (node.no, node.yes),
            Some(_) | None => (form, form),
        }
    }

    fn make(&mut self, decision: u64, no: u32, yes: u32) -> u32 {
        if no == yes {
            return no;
        }
        let node = FormNode { decision, no, yes };
        if let Some(form) = self.unique.get(&node) {
            return *form;
        }
        let form = u32::try_from(self.nodes.len() + 2).unwrap_or(u32::MAX);
        self.nodes.push(node);
        self.unique.insert(node, form);
        form
    }

    fn terminal_ite([test, yes, no]: [u32; 3]) -> Option<u32> {
        match (test, yes, no) {
            (TRUE, _, _) => Some(yes),
            (FALSE, _, _) => Some(no),
            _ if yes == no => Some(yes),
            (_, TRUE, FALSE) => Some(test),
            _ => None,
        }
    }

    fn terminal_joint(left: Polar, right: Polar) -> Option<bool> {
        // A reduced internal node takes both values.
        for (constant, other) in [(left, right), (right, left)] {
            if constant.form <= TRUE {
                let possible = (constant.form == TRUE) == constant.value
                    && (other.form > TRUE || (other.form == TRUE) == other.value);
                return Some(possible);
            }
        }
        if left.form == right.form {
            return Some(left.value == right.value);
        }
        None
    }

    fn ite(&mut self, operands: [u32; 3], work: &mut usize) -> Option<u32> {
        if let Some(value) = Self::terminal_ite(operands) {
            return Some(value);
        }
        self.composed.start_query(MAX_CANONICAL_GENERATION);
        if let Some(entry) = self.composed.get(&operands)
            && let Some(count) = entry.work()?.cold
        {
            let value = entry.value()?;
            let Some(remaining) = work.checked_sub(count.get()) else {
                *work = 0;
                return None;
            };
            *work = remaining;
            return Some(value);
        }
        let allowance = *work;
        let storage = Rc::clone(&self.composed.storage);
        let mut query = ProofQuery::new(*work, self.composed.certificate_allowance, &storage);
        let result = self.ite_inner(operands, &mut query);
        *work = query.remaining;
        if let Some(completed) = result {
            self.composed
                .record_cold(completed.id, allowance - query.remaining);
        }
        result.map(|completed| completed.value)
    }

    fn ite_inner(
        &mut self,
        mut operands: [u32; 3],
        query: &mut ProofQuery<'_, u32>,
    ) -> Option<Completed<u32>> {
        struct IteFrame {
            id: WorkId,
            decision: u64,
            low: [u32; 3],
            high: Option<Completed<u32>>,
        }
        // The logical allowance also bounds this stack, independently of the thread stack.
        let mut pending: Vec<IteFrame> = Vec::new();
        loop {
            let mut completed = if let Some(value) = Self::terminal_ite(operands) {
                Completed::terminal(value)
            } else {
                let Prepared { id, value } = self.composed.prepare(operands)?;
                if let Some(value) = value {
                    query.include(id, &self.composed.entries)?;
                    Completed {
                        value,
                        id: Some(id),
                    }
                } else if let Some(&value) = query.retained.get(&id) {
                    Completed {
                        value,
                        id: Some(id),
                    }
                } else {
                    query.root.get_or_insert(id);
                    query.visit(id)?;
                    let [test, yes, no] = operands;
                    let decision = operands
                        .into_iter()
                        .filter_map(|form| self.top(form))
                        .max()?;
                    let (test_no, test_yes) = self.cofactors(test, decision);
                    let (yes_no, yes_yes) = self.cofactors(yes, decision);
                    let (no_no, no_yes) = self.cofactors(no, decision);
                    pending.push(IteFrame {
                        id,
                        decision,
                        low: [test_no, yes_no, no_no],
                        high: None,
                    });
                    operands = [test_yes, yes_yes, no_yes];
                    continue;
                }
            };
            loop {
                let Some(mut frame) = pending.pop() else {
                    return Some(completed);
                };
                let Some(high) = frame.high else {
                    frame.high = Some(completed);
                    operands = frame.low;
                    pending.push(frame);
                    break;
                };
                let value = self.make(frame.decision, completed.value, high.value);
                self.composed.store(
                    frame.id,
                    value,
                    [high.id, completed.id],
                    self.memo_capacity,
                    query,
                )?;
                completed = Completed {
                    value,
                    id: Some(frame.id),
                };
            }
        }
    }

    /// Whether some assignment of the input decisions gives `left` and `right` their values.
    fn joint(&mut self, left: Polar, right: Polar, work: &mut usize) -> Option<bool> {
        if let Some(value) = Self::terminal_joint(left, right) {
            return Some(value);
        }
        self.compatible.start_query(super::MAX_GUARD_MEMO_ENTRIES);
        if let Some(entry) = self.compatible.get(&[left, right])
            && let Some(count) = entry.work()?.cold
        {
            let value = entry.value()?;
            let Some(remaining) = work.checked_sub(count.get()) else {
                *work = 0;
                return None;
            };
            *work = remaining;
            return Some(value);
        }
        let allowance = *work;
        let storage = Rc::clone(&self.compatible.storage);
        let mut query = ProofQuery::new(*work, self.compatible.certificate_allowance, &storage);
        let result = self.joint_inner(left, right, &mut query);
        *work = query.remaining;
        if let Some(completed) = result {
            self.compatible
                .record_cold(completed.id, allowance - query.remaining);
        }
        result.map(|completed| completed.value)
    }

    fn joint_inner(
        &mut self,
        mut left: Polar,
        mut right: Polar,
        query: &mut ProofQuery<'_, bool>,
    ) -> Option<Completed<bool>> {
        struct JointFrame {
            id: WorkId,
            low: [Polar; 2],
            high: Option<Completed<bool>>,
        }
        let mut pending: Vec<JointFrame> = Vec::new();
        loop {
            let mut completed = if let Some(value) = Self::terminal_joint(left, right) {
                Completed::terminal(value)
            } else {
                let Prepared { id, value } = self.compatible.prepare([left, right])?;
                if let Some(value) = value {
                    query.include(id, &self.compatible.entries)?;
                    Completed {
                        value,
                        id: Some(id),
                    }
                } else if let Some(&value) = query.retained.get(&id) {
                    Completed {
                        value,
                        id: Some(id),
                    }
                } else {
                    query.root.get_or_insert(id);
                    query.visit(id)?;
                    let decision = self.top(left.form)?.max(self.top(right.form)?);
                    let (left_no, left_yes) = self.cofactors(left.form, decision);
                    let (right_no, right_yes) = self.cofactors(right.form, decision);
                    pending.push(JointFrame {
                        id,
                        low: [
                            Polar {
                                form: left_no,
                                value: left.value,
                            },
                            Polar {
                                form: right_no,
                                value: right.value,
                            },
                        ],
                        high: None,
                    });
                    left.form = left_yes;
                    right.form = right_yes;
                    continue;
                }
            };
            loop {
                let Some(mut frame) = pending.pop() else {
                    return Some(completed);
                };
                let children = if let Some(high) = frame.high {
                    [high.id, completed.id]
                } else if completed.value {
                    [completed.id, None]
                } else {
                    frame.high = Some(completed);
                    [left, right] = frame.low;
                    pending.push(frame);
                    break;
                };
                self.compatible.store(
                    frame.id,
                    completed.value,
                    children,
                    self.memo_capacity.min(super::MAX_GUARD_MEMO_ENTRIES),
                    query,
                )?;
                completed.id = Some(frame.id);
            }
        }
    }

    /// The canonical form of `guard`, composing every runtime test it holds; `None` when a
    /// node belongs to another context or a construction exhausts its allowance.
    fn form(&mut self, guard: &RuntimeGuard, context: *const super::GuardQueries) -> Option<u32> {
        let RuntimeGuard::Branch(root) = guard else {
            return self.known(guard);
        };
        if !std::ptr::eq(std::rc::Rc::as_ptr(&root.queries), context) {
            return None;
        }
        if let Some(form) = self.forms.get(&root.id) {
            return *form;
        }
        let mut pending = vec![(guard, true)];
        pending.extend(
            [root.test.runtime.as_ref(), Some(&root.yes), Some(&root.no)]
                .into_iter()
                .flatten()
                .map(|part| (part, false)),
        );
        while let Some((guard, expanded)) = pending.pop() {
            let RuntimeGuard::Branch(node) = guard else {
                continue;
            };
            // FormNode ids repeat across contexts, so the context is checked before the memo.
            if !std::ptr::eq(Rc::as_ptr(&node.queries), context) {
                return None;
            }
            if self.forms.contains_key(&node.id) {
                continue;
            }
            let parts = [node.test.runtime.as_ref(), Some(&node.yes), Some(&node.no)];
            if !expanded {
                pending.push((guard, true));
                pending.extend(parts.into_iter().flatten().map(|part| (part, false)));
                continue;
            }
            let form = self.compose(node.test.id, parts);
            if form.is_none() {
                let _bound =
                    tracing::debug_span!(target: "helm_schema_ir::counters", "canonical_bound")
                        .entered();
            }
            self.forms.insert(node.id, form);
        }
        self.known(guard)
    }

    /// The form of a node from its parts' forms: its test (an input decision is its own
    /// variable), then its two arms.
    fn compose(&mut self, decision: u64, parts: [Option<&RuntimeGuard>; 3]) -> Option<u32> {
        let mut forms = [FALSE; 3];
        for (slot, part) in forms.iter_mut().zip(parts) {
            *slot = if let Some(part) = part {
                self.known(part)?
            } else {
                self.make(decision, FALSE, TRUE)
            };
        }
        self.ite(forms, &mut { MAX_CANONICAL_WORK })
    }

    /// Starts the tables over when they are full. Called once per query, before any of its
    /// forms is built, so no form a query compares can name a node of cleared tables.
    fn start_query(&mut self) {
        if self.nodes.len() >= MAX_CANONICAL_NODES {
            self.nodes.clear();
            self.unique.clear();
            self.forms.clear();
            self.composed.clear();
            self.compatible.clear();
        }
    }

    fn known(&self, guard: &RuntimeGuard) -> Option<u32> {
        match guard {
            RuntimeGuard::False => Some(FALSE),
            RuntimeGuard::True => Some(TRUE),
            RuntimeGuard::Branch(node) => self.forms.get(&node.id).copied().flatten(),
        }
    }

    /// Whether `left` can evaluate to `left_value` while `right` evaluates to `right_value`;
    /// `None` when either form or the walk exhausts its allowance.
    pub(super) fn compatible(
        &mut self,
        [(left, left_value), (right, right_value)]: [(&RuntimeGuard, bool); 2],
        context: *const super::GuardQueries,
    ) -> Option<bool> {
        self.start_query();
        let left = self.form(left, context)?;
        let right = self.form(right, context)?;
        self.joint(
            Polar {
                form: left,
                value: left_value,
            },
            Polar {
                form: right,
                value: right_value,
            },
            &mut { MAX_CANONICAL_WORK },
        )
    }

    /// Whether `guard` can evaluate to `value` while the input decision `atom` takes its arm.
    pub(super) fn possible_atom(
        &mut self,
        guard: &RuntimeGuard,
        atom: &super::DecisionEdgeKey,
        value: bool,
        context: *const super::GuardQueries,
    ) -> Option<bool> {
        self.start_query();
        let form = self.form(guard, context)?;
        let decision = self.make(atom.decision, FALSE, TRUE);
        self.joint(
            Polar { form, value },
            Polar {
                form: decision,
                value: atom.taken,
            },
            &mut { MAX_CANONICAL_WORK },
        )
    }
}

#[cfg(test)]
#[path = "../../tests/runtime_canonical_histories.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/runtime_canonical_guards.rs"]
mod guard_tests;
