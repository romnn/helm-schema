use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;

use ahash::AHashSet;
use std::rc::Rc;

use super::RuntimeRead;

type Link = Option<Rc<Node>>;

#[derive(Debug)]
struct Node {
    entry: Rc<RuntimeRead>,
    left: Link,
    right: Link,
    height: usize,
    length: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ReadSet {
    root: Link,
    coverage: Option<Rc<CoverageRequest>>,
    resolved: OnceCell<Link>,
}

#[derive(Debug)]
enum CoverageRequest {
    Empty,
    Observe {
        parent: Option<Rc<Self>>,
        heap: Rc<super::HeapVersion>,
        condition: helm_schema_core::Predicate,
    },
    Union {
        yes: Rc<Self>,
        no: Rc<Self>,
    },
}

impl Drop for CoverageRequest {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        detach_coverage(self, &mut pending);
        while let Some(request) = pending.pop() {
            if let Ok(mut request) = Rc::try_unwrap(request) {
                detach_coverage(&mut request, &mut pending);
            }
        }
    }
}

fn detach_coverage(request: &mut CoverageRequest, pending: &mut Vec<Rc<CoverageRequest>>) {
    match request {
        CoverageRequest::Empty => {}
        CoverageRequest::Observe {
            parent,
            heap: _,
            condition: _,
        } => pending.extend(parent.take()),
        CoverageRequest::Union { yes, no } => {
            let empty = Rc::new(CoverageRequest::Empty);
            pending.push(std::mem::replace(yes, Rc::clone(&empty)));
            pending.push(std::mem::replace(no, empty));
        }
    }
}

impl ReadSet {
    pub(super) fn len(&self) -> usize {
        length(&self.root)
    }

    pub(super) fn clear(&mut self) {
        self.root = None;
        self.coverage = None;
        self.resolved.take();
    }

    pub(super) fn insert(&mut self, entry: RuntimeRead) -> bool {
        self.resolved.take();
        let before = self.len();
        self.root = Some(insert(&self.root, Rc::new(entry)));
        self.len() != before
    }

    pub(super) fn union(&self, other: &Self) -> Self {
        Self {
            root: union(&self.root, &other.root),
            coverage: match (&self.coverage, &other.coverage) {
                (Some(yes), Some(no)) if !Rc::ptr_eq(yes, no) => {
                    Some(Rc::new(CoverageRequest::Union {
                        yes: Rc::clone(yes),
                        no: Rc::clone(no),
                    }))
                }
                (yes, no) => yes.clone().or_else(|| no.clone()),
            },
            resolved: OnceCell::new(),
        }
    }

    pub(super) fn without_coverage(&self, coverage: super::ReadCoverage) -> Self {
        if !contains_coverage(&self.root, coverage) {
            return self.clone();
        }
        Self {
            root: union(
                &before_coverage(&self.root, coverage),
                &after_coverage(&self.root, coverage),
            ),
            coverage: self.coverage.clone(),
            resolved: OnceCell::new(),
        }
    }

    pub(super) fn observe_heap(
        &mut self,
        heap: Rc<super::HeapVersion>,
        condition: helm_schema_core::Predicate,
    ) {
        self.resolved.take();
        if let Some(request) = &self.coverage
            && let CoverageRequest::Observe {
                parent: _,
                heap: previous,
                condition: previous_condition,
            } = request.as_ref()
            && Rc::ptr_eq(previous, &heap)
            && previous_condition == &condition
        {
            return;
        }
        self.coverage = Some(Rc::new(CoverageRequest::Observe {
            parent: self.coverage.take(),
            heap,
            condition,
        }));
    }

    fn projection(&self) -> &Link {
        self.resolved.get_or_init(|| {
            let mut reads = Self {
                root: self.root.clone(),
                ..Self::default()
            };
            let mut groups = BTreeMap::<_, Vec<_>>::new();
            let mut pending = Vec::new();
            if let Some(request) = &self.coverage {
                pending.push(request.as_ref());
            }
            let mut seen = AHashSet::new();
            while let Some(request) = pending.pop() {
                if !seen.insert(std::ptr::from_ref(request)) {
                    continue;
                }
                match request {
                    CoverageRequest::Empty => {}
                    CoverageRequest::Observe {
                        parent,
                        heap,
                        condition,
                    } => {
                        groups
                            .entry((condition, heap.input_paths.id(), heap.arena.as_ptr()))
                            .or_default()
                            .push(heap.as_ref());
                        if let Some(parent) = parent {
                            pending.push(parent.as_ref());
                        }
                    }
                    CoverageRequest::Union { yes, no } => {
                        pending.extend([no.as_ref(), yes.as_ref()]);
                    }
                }
            }
            for ((condition, _generation, _arena), mut roots) in groups {
                roots.sort_unstable_by_key(|root| root.id.ordinal);
                roots.dedup_by_key(|root| root.id.ordinal);
                if let Some(first) = roots.first() {
                    if let Some(arena) = first.arena.upgrade() {
                        arena.coverage_queries.borrow_mut().project(
                            condition,
                            &first.input_paths,
                            &roots,
                            &mut reads,
                        );
                    } else {
                        super::heap_coverage::CoverageProjection::new().project(
                            condition,
                            &first.input_paths,
                            &roots,
                            &mut reads,
                        );
                    }
                }
            }
            reads.root
        })
    }

    pub(super) fn iter(&self) -> ReadIter<'_> {
        let root = self.projection();
        let mut result = ReadIter {
            pending: Vec::new(),
            remaining: length(root),
        };
        result.descend(root.as_deref());
        result
    }
}

impl FromIterator<RuntimeRead> for ReadSet {
    fn from_iter<T: IntoIterator<Item = RuntimeRead>>(iter: T) -> Self {
        let mut result = Self::default();
        for entry in iter {
            result.insert(entry);
        }
        result
    }
}

impl PartialEq for ReadSet {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for ReadSet {}

impl<'a> IntoIterator for &'a ReadSet {
    type Item = &'a RuntimeRead;
    type IntoIter = ReadIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(super) struct ReadIter<'a> {
    pending: Vec<&'a Node>,
    remaining: usize,
}

impl<'a> ReadIter<'a> {
    fn descend(&mut self, mut current: Option<&'a Node>) {
        while let Some(node) = current {
            self.pending.push(node);
            current = node.left.as_deref();
        }
    }
}

impl<'a> Iterator for ReadIter<'a> {
    type Item = &'a RuntimeRead;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.pending.pop()?;
        self.remaining -= 1;
        self.descend(node.right.as_deref());
        Some(&node.entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for ReadIter<'_> {}

fn height(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.height)
}

fn length(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.length)
}

// Coverage is the leading sort key, so filtering cuts two search boundaries.
fn contains_coverage(root: &Link, coverage: super::ReadCoverage) -> bool {
    let mut current = root.as_ref();
    while let Some(node) = current {
        match node.entry.coverage.cmp(&coverage) {
            Ordering::Less => current = node.right.as_ref(),
            Ordering::Greater => current = node.left.as_ref(),
            Ordering::Equal => return true,
        }
    }
    false
}

fn before_coverage(root: &Link, coverage: super::ReadCoverage) -> Link {
    let root = root.as_ref()?;
    if root.entry.coverage >= coverage {
        return before_coverage(&root.left, coverage);
    }
    let right = before_coverage(&root.right, coverage);
    if same_link(&root.right, &right) {
        return Some(Rc::clone(root));
    }
    Some(join(Rc::clone(&root.entry), root.left.clone(), right))
}

fn after_coverage(root: &Link, coverage: super::ReadCoverage) -> Link {
    let root = root.as_ref()?;
    if root.entry.coverage <= coverage {
        return after_coverage(&root.right, coverage);
    }
    let left = after_coverage(&root.left, coverage);
    if same_link(&root.left, &left) {
        return Some(Rc::clone(root));
    }
    Some(join(Rc::clone(&root.entry), left, root.right.clone()))
}

fn same_link(left: &Link, right: &Link) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        (None, Some(_)) | (Some(_), None) => false,
    }
}

fn node(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    Rc::new(Node {
        height: 1 + height(&left).max(height(&right)),
        length: 1 + length(&left) + length(&right),
        entry,
        left,
        right,
    })
}

fn balance(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    if height(&left) > height(&right) + 1
        && let Some(left) = &left
    {
        if height(&left.right) > height(&left.left)
            && let Some(pivot) = &left.right
        {
            return node(
                Rc::clone(&pivot.entry),
                Some(node(
                    Rc::clone(&left.entry),
                    left.left.clone(),
                    pivot.left.clone(),
                )),
                Some(node(entry, pivot.right.clone(), right)),
            );
        }
        return node(
            Rc::clone(&left.entry),
            left.left.clone(),
            Some(node(entry, left.right.clone(), right)),
        );
    }
    if height(&right) > height(&left) + 1
        && let Some(right) = &right
    {
        if height(&right.left) > height(&right.right)
            && let Some(pivot) = &right.left
        {
            return node(
                Rc::clone(&pivot.entry),
                Some(node(entry, left, pivot.left.clone())),
                Some(node(
                    Rc::clone(&right.entry),
                    pivot.right.clone(),
                    right.right.clone(),
                )),
            );
        }
        return node(
            Rc::clone(&right.entry),
            Some(node(entry, left, right.left.clone())),
            right.right.clone(),
        );
    }
    node(entry, left, right)
}

fn insert(root: &Link, entry: Rc<RuntimeRead>) -> Rc<Node> {
    let Some(current) = root else {
        return node(entry, None, None);
    };
    let (left, right) = match entry.cmp(&current.entry) {
        Ordering::Equal => return Rc::clone(current),
        Ordering::Less => {
            let left = insert(&current.left, entry);
            if current
                .left
                .as_ref()
                .is_some_and(|old| Rc::ptr_eq(old, &left))
            {
                return Rc::clone(current);
            }
            (Some(left), current.right.clone())
        }
        Ordering::Greater => {
            let right = insert(&current.right, entry);
            if current
                .right
                .as_ref()
                .is_some_and(|old| Rc::ptr_eq(old, &right))
            {
                return Rc::clone(current);
            }
            (current.left.clone(), Some(right))
        }
    };
    balance(Rc::clone(&current.entry), left, right)
}

// All left entries precede entry, and all right entries follow it; subtree heights may differ.
fn join(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    if height(&left) > height(&right) + 1
        && let Some(root) = &left
    {
        let tail = join(entry, root.right.clone(), right);
        return balance(Rc::clone(&root.entry), root.left.clone(), Some(tail));
    }
    if height(&right) > height(&left) + 1
        && let Some(root) = &right
    {
        let head = join(entry, left, root.left.clone());
        return balance(Rc::clone(&root.entry), Some(head), root.right.clone());
    }
    node(entry, left, right)
}

// Partition around entry, omitting an equal entry; join restores AVL balance after each cut.
fn split(root: &Link, entry: &RuntimeRead) -> (Link, Link) {
    let Some(root) = root else {
        return (None, None);
    };
    match entry.cmp(&root.entry) {
        Ordering::Equal => (root.left.clone(), root.right.clone()),
        Ordering::Less => {
            let (lower, upper) = split(&root.left, entry);
            (
                lower,
                Some(join(Rc::clone(&root.entry), upper, root.right.clone())),
            )
        }
        Ordering::Greater => {
            let (lower, upper) = split(&root.right, entry);
            (
                Some(join(Rc::clone(&root.entry), root.left.clone(), lower)),
                upper,
            )
        }
    }
}

fn union(left: &Link, right: &Link) -> Link {
    let (Some(left), Some(right)) = (left, right) else {
        return left.clone().or_else(|| right.clone());
    };
    if Rc::ptr_eq(left, right) {
        return Some(Rc::clone(left));
    }
    let (lower, upper) = split(&Some(Rc::clone(right)), &left.entry);
    Some(join(
        Rc::clone(&left.entry),
        union(&left.left, &lower),
        union(&left.right, &upper),
    ))
}

#[cfg(test)]
#[path = "../tests/persistent_read_set.rs"]
mod tests;
