//! Bounded projections of immutable heap history.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ahash::AHashMap;
use helm_schema_core::ValuesPath;

use super::heap_cache::ObjectLookup;

use super::{
    Field, HeapNode, HeapVersion, Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder,
};

const MAX_QUERY_WORK: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ObjectQueryKey {
    pub(super) version: u64,
    pub(super) object: u64,
}

pub(super) struct QueryBudget {
    pub(super) remaining: usize,
    depth: usize,
    exhausted: bool,
}

impl QueryBudget {
    pub(super) fn new(remaining: usize) -> Self {
        Self {
            remaining,
            depth: 0,
            exhausted: false,
        }
    }
    pub(super) fn charge(&mut self, work: usize) -> bool {
        if let Some(remaining) = self.remaining.checked_sub(work) {
            self.remaining = remaining;
            true
        } else {
            self.exhaust();
            false
        }
    }

    pub(super) fn exhaust(&mut self) {
        self.remaining = 0;
        self.exhausted = true;
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Pins derived views only when a caller needs to borrow canonical fields.
/// Completed views are bounded by the allocator's object-class limit.
#[derive(Default)]
pub(super) struct BorrowedObjects {
    retained: std::cell::Cell<usize>,
    exhausted: std::cell::OnceCell<Rc<ObjectVersion>>,
    root: std::cell::OnceCell<Box<BorrowedObjectNode>>,
}

#[derive(Default)]
struct BorrowedObjectNode {
    value: std::cell::OnceCell<Option<Rc<ObjectVersion>>>,
    even: std::cell::OnceCell<Box<Self>>,
    odd: std::cell::OnceCell<Box<Self>>,
}

impl BorrowedObjects {
    fn get(&self, mut ordinal: u64) -> Option<&Option<Rc<ObjectVersion>>> {
        let mut node = self.root.get()?.as_ref();
        while ordinal != 0 {
            node = if ordinal & 1 == 0 {
                node.even.get()?
            } else {
                node.odd.get()?
            };
            ordinal >>= 1;
        }
        node.value.get()
    }
}

impl HeapVersion {
    pub(super) fn borrowed_object(&self, object: &ObjectId) -> Option<&ObjectVersion> {
        if let Some(answer) = self.borrowed_objects.get(object.ordinal) {
            return answer.as_deref();
        }
        if !self.has_allocated_object(object) {
            return None;
        }
        let mut node = self
            .borrowed_objects
            .root
            .get_or_init(Box::default)
            .as_ref();
        let mut ordinal = object.ordinal;
        while ordinal != 0 {
            let next = if ordinal & 1 == 0 {
                &node.even
            } else {
                &node.odd
            };
            if next.get().is_none() && self.borrowed_objects.retained.get() >= 4096 {
                return Some(self.borrowed_unknown());
            }
            node = next.get_or_init(Box::default);
            ordinal >>= 1;
        }
        if node.value.get().is_none() && self.borrowed_objects.retained.get() >= 4096 {
            return Some(self.borrowed_unknown());
        }
        if node.value.get().is_none() {
            let mut budget = QueryBudget::new(MAX_QUERY_WORK);
            let answer = self.query_object(object, &mut budget);
            if budget.is_exhausted() {
                return Some(self.borrowed_unknown());
            }
            if node.value.set(answer).is_err() {
                return Some(self.borrowed_unknown());
            }
            self.borrowed_objects
                .retained
                .set(self.borrowed_objects.retained.get() + 1);
        }
        node.value.get().and_then(Option::as_deref)
    }

    fn borrowed_unknown(&self) -> &ObjectVersion {
        self.borrowed_objects
            .exhausted
            .get_or_init(|| self.exhausted_object())
    }
}

#[derive(Debug, Default)]
pub(super) struct ObjectAnswers {
    inline: [Option<ObjectAnswer>; 4],
    overflow: AHashMap<u64, Option<Rc<ObjectVersion>>>,
}

#[derive(Debug)]
struct ObjectAnswer {
    version: u64,
    storage: Option<Rc<ObjectVersion>>,
}

impl ObjectAnswers {
    fn get(&self, version: u64) -> Option<&Option<Rc<ObjectVersion>>> {
        if !self.overflow.is_empty() {
            return self.overflow.get(&version);
        }
        self.inline
            .iter()
            .flatten()
            .find(|entry| entry.version == version)
            .map(|entry| &entry.storage)
    }

    fn contains_key(&self, version: u64) -> bool {
        self.get(version).is_some()
    }

    fn insert(&mut self, version: u64, storage: Option<Rc<ObjectVersion>>) {
        if !self.overflow.is_empty() {
            self.overflow.insert(version, storage);
            return;
        }
        if let Some(entry) = self
            .inline
            .iter_mut()
            .flatten()
            .find(|entry| entry.version == version)
        {
            entry.storage = storage;
            return;
        }
        if let Some(slot) = self.inline.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(ObjectAnswer { version, storage });
            return;
        }
        for slot in &mut self.inline {
            if let Some(ObjectAnswer { version, storage }) = slot.take() {
                self.overflow.insert(version, storage);
            }
        }
        self.overflow.insert(version, storage);
    }

    fn remove(&mut self, version: u64) -> Option<Rc<ObjectVersion>> {
        if !self.overflow.is_empty() {
            return self.overflow.remove(&version).flatten();
        }
        self.inline
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|entry| entry.version == version))?
            .take()
            .and_then(|entry| entry.storage)
    }

    fn len(&self) -> usize {
        self.overflow.len() + self.inline.iter().flatten().count()
    }

    fn capacity(&self) -> usize {
        self.overflow.capacity()
    }

    fn clear(&mut self) {
        for slot in &mut self.inline {
            *slot = None;
        }
        self.overflow.clear();
    }
}

enum Task<'version> {
    Visit(&'version HeapVersion),
    Finish(&'version HeapVersion),
    Forward {
        version: &'version HeapVersion,
        ancestor: &'version HeapVersion,
    },
}

impl HeapVersion {
    /// Tests the physical allocation recorded by this immutable version.
    pub(super) fn has_allocated_object(&self, object: &ObjectId) -> bool {
        self.allocations.get(object).is_some()
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn object_count(&self) -> usize {
        self.allocations.len()
    }

    pub(super) fn query_object_ids(&self, budget: &mut QueryBudget) -> Option<Vec<ObjectId>> {
        self.allocations.objects(budget)
    }

    /// Materializes one bounded object view; the cache owns no semantic facts.
    #[tracing::instrument(skip_all)]
    pub(super) fn object(&self, object: &ObjectId) -> Option<Rc<ObjectVersion>> {
        self.object_with_budget(object, MAX_QUERY_WORK)
    }

    pub(super) fn object_with_budget(
        &self,
        object: &ObjectId,
        work: usize,
    ) -> Option<Rc<ObjectVersion>> {
        self.query_object(object, &mut QueryBudget::new(work))
    }

    #[tracing::instrument(skip_all, level = "debug", fields(version = self.id.ordinal, object = object.ordinal, work = budget.remaining, cache_hit = tracing::field::Empty, visited = tracing::field::Empty))]
    pub(super) fn query_object(
        &self,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Rc<ObjectVersion>> {
        if budget.depth >= 128 || !budget.charge(1) {
            budget.exhaust();
            return Some(self.exhausted_object());
        }
        let multiplicity = self.allocations.get(object)?;
        let arena = self.arena.upgrade();
        let ancestor = self.object_ancestor(object, budget);
        if budget.is_exhausted() {
            return Some(self.exhausted_object());
        }
        let version = ancestor.as_deref().unwrap_or(self);
        if let HeapNode::Install {
            parent: _,
            object: target,
            storage,
        } = &version.node
            && target == object
        {
            if !budget.charge(1) {
                return Some(self.exhausted_object());
            }
            return Some(Rc::clone(storage));
        }
        if let Some(arena) = &arena {
            if budget.remaining == 0 {
                budget.exhaust();
                return Some(self.exhausted_object());
            }
            let cached = arena.heap_queries.borrow_mut().get(&ObjectQueryKey {
                version: version.id.ordinal,
                object: object.ordinal,
            });
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::Span::current()
                    .record("cache_hit", matches!(&cached, ObjectLookup::Hit(_)));
            }
            if let ObjectLookup::Hit(answer) = cached {
                budget.charge(1);
                return answer;
            }
        }
        if !budget.charge(1) {
            return Some(self.exhausted_object());
        }
        match &version.node {
            HeapNode::Install {
                parent: _,
                object: target,
                storage,
            } if target == object => return Some(Rc::clone(storage)),
            HeapNode::UnknownWrite {
                parent: _,
                objects,
                inputs,
            } if aliases_contain(objects, object) => {
                let answer = version.widened_object(multiplicity, inputs);
                if let Some(arena) = &arena {
                    cache_answer(arena, version, object, Some(&answer));
                }
                return Some(answer);
            }
            HeapNode::Empty
            | HeapNode::Input { parent: _ }
            | HeapNode::Install {
                parent: _,
                object: _,
                storage: _,
            }
            | HeapNode::UnknownWrite {
                parent: _,
                objects: _,
                inputs: _,
            }
            | HeapNode::Write {
                parent: _,
                object: _,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent: _,
                object: _,
                index: _,
                value: _,
            }
            | HeapNode::Join {
                decision: _,
                yes: _,
                no: _,
            } => {}
        }
        budget.depth += 1;
        let answer = version.materialize_object(object, budget);
        budget.depth -= 1;
        answer
    }

    fn object_ancestor(
        &self,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Rc<HeapVersion>> {
        let mut version = self;
        let mut root = None;
        while let Some(ancestor) = version.index.skip_root(object) {
            if !Rc::ptr_eq(&version.input_paths, &ancestor.input_paths)
                && !version
                    .input_paths
                    .same_mapping(&ancestor.input_paths, object)
            {
                break;
            }
            if !budget.charge(1) {
                return None;
            }
            version = ancestor;
            root = Some(ancestor);
        }
        root.cloned()
    }

    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::heap_query::walk", fields(version = self.id.ordinal, object = object.ordinal))]
    fn materialize_object(
        &self,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Rc<ObjectVersion>> {
        let arena = self.arena.upgrade();
        let mut answers = arena.as_ref().map_or_else(ObjectAnswers::default, |arena| {
            std::mem::take(&mut *arena.object_answers.borrow_mut())
        });
        let result = self.materialize_into(object, budget, &mut answers, arena.as_deref());
        answers.clear();
        if answers.capacity() <= MAX_QUERY_WORK
            && let Some(arena) = arena
        {
            let mut scratch = arena.object_answers.borrow_mut();
            if scratch.capacity() < answers.capacity() {
                *scratch = answers;
            }
        }
        result
    }

    fn materialize_into(
        &self,
        object: &ObjectId,
        budget: &mut QueryBudget,
        answers: &mut ObjectAnswers,
        arena: Option<&super::Arena>,
    ) -> Option<Rc<ObjectVersion>> {
        let mut pending = vec![Task::Visit(self)];
        while let Some(task) = pending.pop() {
            let version = match task {
                Task::Visit(version) => {
                    if answers.contains_key(version.id.ordinal) {
                        continue;
                    }
                    if !budget.charge(1) {
                        return Some(self.exhausted_object());
                    }
                    let Some(multiplicity) = version.allocations.get(object) else {
                        answers.insert(version.id.ordinal, None);
                        continue;
                    };
                    if let HeapNode::Install {
                        parent: _,
                        object: target,
                        storage,
                    } = &version.node
                        && target == object
                    {
                        answers.insert(version.id.ordinal, Some(Rc::clone(storage)));
                        continue;
                    }
                    let key = ObjectQueryKey {
                        version: version.id.ordinal,
                        object: object.ordinal,
                    };
                    let cached = if version.id == self.id {
                        ObjectLookup::Miss
                    } else {
                        arena.map_or(ObjectLookup::Miss, |arena| {
                            arena.heap_queries.borrow_mut().get(&key)
                        })
                    };
                    if let ObjectLookup::Hit(answer) = cached {
                        answers.insert(version.id.ordinal, answer);
                    } else if let HeapNode::UnknownWrite {
                        parent: _,
                        objects,
                        inputs,
                    } = &version.node
                        && aliases_contain(objects, object)
                    {
                        // The event discards storage; only identity and multiplicity survive.
                        if !budget.charge(1) {
                            return Some(self.exhausted_object());
                        }
                        if budget.remaining == 0 {
                            budget.exhaust();
                            return Some(self.exhausted_object());
                        }
                        let answer = Some(version.widened_object(multiplicity, inputs));
                        if let Some(arena) = arena {
                            cache_answer(arena, version, object, answer.as_ref());
                        }
                        answers.insert(version.id.ordinal, answer);
                    } else {
                        schedule_object(version, object, &mut pending, answers);
                    }
                    continue;
                }
                Task::Forward { version, ancestor } => {
                    let answer = answer(answers, ancestor);
                    answers.insert(version.id.ordinal, answer);
                    continue;
                }
                Task::Finish(version) => version,
            };
            let answer = self.reduce_version(version, object, answers, budget);
            if budget.remaining == 0 {
                budget.exhaust();
                return Some(self.exhausted_object());
            }
            if let Some(arena) = arena
                && version.id != self.id
                && changes_object(&version.node, object)
            {
                cache_answer(arena, version, object, answer.as_ref());
            }
            answers.insert(version.id.ordinal, answer);
        }
        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::Span::current().record("visited", answers.len());
        }
        let answer = answers.remove(self.id.ordinal);
        if let Some(arena) = arena {
            cache_answer(arena, self, object, answer.as_ref());
        }
        answer
    }

    fn reduce_version(
        &self,
        version: &HeapVersion,
        object: &ObjectId,
        answers: &ObjectAnswers,
        budget: &mut QueryBudget,
    ) -> Option<Rc<ObjectVersion>> {
        match &version.node {
            HeapNode::Empty => None,
            HeapNode::Input { parent }
            | HeapNode::Install {
                parent,
                object: _,
                storage: _,
            } => answer(answers, parent),
            HeapNode::UnknownWrite {
                parent,
                objects,
                inputs,
            } => {
                let previous = answer(answers, parent);
                if aliases_contain(objects, object) {
                    previous.map(|previous| {
                        Rc::new(ObjectVersion {
                            revision: version.id,
                            multiplicity: previous.multiplicity,
                            data: ObjectData::Widened {
                                inputs: Rc::clone(inputs),
                            },
                        })
                    })
                } else {
                    previous
                }
            }
            HeapNode::Write {
                parent,
                object: target,
                key,
                field,
            } => {
                let previous = answer(answers, parent);
                if target == object {
                    self.apply_write(version, parent, key, field, previous, budget)
                } else {
                    previous
                }
            }
            HeapNode::WriteSlot {
                parent,
                object: target,
                index,
                value,
            } => {
                let previous = answer(answers, parent);
                if target == object {
                    previous.map(|previous| {
                        if !budget.charge(copy_work(&previous.data)) {
                            return self.exhausted_object();
                        }
                        let mut storage = previous.as_ref().clone();
                        storage.revision = version.id;
                        if let ObjectData::Strings(slots) = &mut storage.data
                            && let Some(slot) = slots.get_mut(*index)
                        {
                            *slot = value.clone();
                        }
                        Rc::new(storage)
                    })
                } else {
                    previous
                }
            }
            HeapNode::Join {
                decision: _,
                yes: _,
                no: _,
            } => self.join_objects(version, object, answers, budget),
        }
    }

    fn apply_write(
        &self,
        version: &HeapVersion,
        parent: &Rc<HeapVersion>,
        key: &str,
        field: &Field,
        previous: Option<Rc<ObjectVersion>>,
        budget: &mut QueryBudget,
    ) -> Option<Rc<ObjectVersion>> {
        let previous = previous?;
        if !budget.charge(copy_work(&previous.data)) {
            return Some(self.exhausted_object());
        }
        let mut storage = previous.as_ref().clone();
        storage.revision = version.id;
        match &mut storage.data {
            ObjectData::Map {
                fields,
                remainder: _,
            } => {
                Rc::make_mut(fields).insert(key.to_owned(), field.clone());
                if fields.len() > 128 {
                    storage.data = ObjectData::Widened {
                        inputs: Rc::new(data_inputs_with_budget(parent, &storage.data, budget)),
                    };
                }
            }
            ObjectData::Widened { inputs } => {
                storage.data = ObjectData::Map {
                    fields: Rc::new(BTreeMap::from([(key.to_owned(), field.clone())])),
                    remainder: Remainder::Unknown {
                        inputs: Rc::clone(inputs),
                    },
                };
            }
            ObjectData::Strings(_) | ObjectData::Opaque { reachable: _ } => {}
        }
        Some(Rc::new(storage))
    }

    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::heap_query::join")]
    fn join_objects(
        &self,
        version: &HeapVersion,
        object: &ObjectId,
        answers: &ObjectAnswers,
        budget: &mut QueryBudget,
    ) -> Option<Rc<ObjectVersion>> {
        let HeapNode::Join { decision, yes, no } = &version.node else {
            return None;
        };
        let yes_object = answer(answers, yes);
        let no_object = answer(answers, no);
        if yes_object
            .as_ref()
            .zip(no_object.as_ref())
            .is_some_and(|(yes, no)| Rc::ptr_eq(yes, no) || yes == no)
        {
            return yes_object;
        }
        if yes_object.is_none() && no_object.is_none() {
            return None;
        }
        if !budget.charge(version.input_paths.lookup_work()) {
            return Some(self.exhausted_object());
        }
        let input_path = version.input_paths.get(object);
        let original_input = input_path.is_some();
        let baseline = if yes_object.is_none() || no_object.is_none() {
            input_path.as_ref().map(|_| {
                Rc::new(ObjectVersion {
                    revision: version.id,
                    multiplicity: Multiplicity::Singleton,
                    data: ObjectData::Map {
                        fields: Rc::new(BTreeMap::new()),
                        remainder: Remainder::Input,
                    },
                })
            })
        } else {
            None
        };
        let yes_object = yes_object.or_else(|| baseline.clone());
        let no_object = no_object.or(baseline);
        match (yes_object, no_object) {
            (Some(yes_object), Some(no_object)) => {
                if !budget.charge(copy_work(&yes_object.data) + copy_work(&no_object.data)) {
                    return Some(self.exhausted_object());
                }
                let arena = self.arena.upgrade();
                let mut data = super::heap::join_data(
                    decision,
                    &yes_object.data,
                    &no_object.data,
                    arena.as_ref().map(|arena| &arena.input_unions),
                    budget,
                )
                .unwrap_or_else(|_| ObjectData::Widened {
                    inputs: Rc::new(BTreeSet::from([ValuesPath::default()])),
                });
                if let ObjectData::Widened { inputs } = &mut data {
                    for (branch, storage) in [(yes, &yes_object), (no, &no_object)] {
                        if !matches!(storage.data, ObjectData::Widened { inputs: _ }) {
                            let added =
                                super::FrozenValue::object_inputs(branch, object, storage, budget);
                            if !added.is_subset(inputs) {
                                Rc::make_mut(inputs).extend(added);
                            }
                        }
                    }
                    if let Some(path) = input_path
                        && !inputs.contains(path)
                    {
                        Rc::make_mut(inputs).insert(path.clone());
                    }
                }
                let multiplicity = yes_object.multiplicity.join(no_object.multiplicity);
                if !original_input && let ObjectData::Widened { inputs } = &data {
                    return Some(version.widened_object(multiplicity, inputs));
                }
                Some(Rc::new(ObjectVersion {
                    revision: version.id,
                    multiplicity,
                    data,
                }))
            }
            (Some(storage), None) | (None, Some(storage)) => Some(storage),
            (None, None) => None,
        }
    }

    fn exhausted_object(&self) -> Rc<ObjectVersion> {
        Rc::new(ObjectVersion {
            revision: self.id,
            multiplicity: Multiplicity::Summary,
            data: ObjectData::Widened {
                inputs: Rc::new(BTreeSet::from([ValuesPath::default()])),
            },
        })
    }

    fn widened_object(
        &self,
        multiplicity: Multiplicity,
        inputs: &Rc<BTreeSet<ValuesPath>>,
    ) -> Rc<ObjectVersion> {
        if let Some(arena) = self.arena.upgrade() {
            return arena
                .widened_queries
                .borrow_mut()
                .object(self.id, multiplicity, inputs);
        }
        Rc::new(ObjectVersion {
            revision: self.id,
            multiplicity,
            data: ObjectData::Widened {
                inputs: Rc::clone(inputs),
            },
        })
    }
}

fn schedule_object<'version>(
    version: &'version HeapVersion,
    object: &ObjectId,
    pending: &mut Vec<Task<'version>>,
    answers: &mut ObjectAnswers,
) {
    if let Some(ancestor) = version.index.skip(object) {
        pending.push(Task::Forward { version, ancestor });
        pending.push(Task::Visit(ancestor));
        return;
    }
    match &version.node {
        HeapNode::Empty => {
            answers.insert(version.id.ordinal, None);
        }
        HeapNode::Install {
            parent: _,
            object: target,
            storage,
        } if target == object => {
            answers.insert(version.id.ordinal, Some(Rc::clone(storage)));
        }
        HeapNode::Input { parent }
        | HeapNode::Install {
            parent,
            object: _,
            storage: _,
        }
        | HeapNode::Write {
            parent,
            object: _,
            key: _,
            field: _,
        }
        | HeapNode::WriteSlot {
            parent,
            object: _,
            index: _,
            value: _,
        }
        | HeapNode::UnknownWrite {
            parent,
            objects: _,
            inputs: _,
        } => {
            pending.push(Task::Finish(version));
            pending.push(Task::Visit(parent));
        }
        HeapNode::Join {
            decision: _,
            yes,
            no,
        } => {
            pending.push(Task::Finish(version));
            pending.push(Task::Visit(no));
            pending.push(Task::Visit(yes));
        }
    }
}

fn changes_object(node: &HeapNode, object: &ObjectId) -> bool {
    match node {
        HeapNode::Empty | HeapNode::Input { parent: _ } => false,
        HeapNode::Install {
            parent: _,
            object: target,
            storage: _,
        }
        | HeapNode::Write {
            parent: _,
            object: target,
            key: _,
            field: _,
        }
        | HeapNode::WriteSlot {
            parent: _,
            object: target,
            index: _,
            value: _,
        } => target == object,
        HeapNode::UnknownWrite {
            parent: _,
            objects,
            inputs: _,
        } => aliases_contain(objects, object),
        HeapNode::Join {
            decision: _,
            yes: _,
            no: _,
        } => true,
    }
}

fn cache_answer(
    arena: &super::Arena,
    version: &HeapVersion,
    object: &ObjectId,
    answer: Option<&Rc<ObjectVersion>>,
) {
    let key = ObjectQueryKey {
        version: version.id.ordinal,
        object: object.ordinal,
    };
    arena.heap_queries.borrow_mut().insert(key, answer);
}

fn answer(answers: &ObjectAnswers, version: &HeapVersion) -> Option<Rc<ObjectVersion>> {
    answers.get(version.id.ordinal).cloned().flatten()
}

fn aliases_contain(aliases: &super::AliasSet, object: &ObjectId) -> bool {
    match aliases {
        super::AliasSet::Known(objects) => objects.contains(object),
        super::AliasSet::RenderGraph => true,
    }
}

fn copy_work(data: &ObjectData) -> usize {
    match data {
        ObjectData::Map {
            fields,
            remainder: _,
        } => fields.len() + 1,
        ObjectData::Strings(slots) => slots.len() + 1,
        ObjectData::Opaque {
            reachable: super::AliasSet::Known(objects),
        } => objects.len() + 1,
        ObjectData::Opaque {
            reachable: super::AliasSet::RenderGraph,
        }
        | ObjectData::Widened { inputs: _ } => 1,
    }
}

pub(super) fn data_inputs(heap: &Rc<HeapVersion>, data: &ObjectData) -> BTreeSet<ValuesPath> {
    data_inputs_with_budget(heap, data, &mut QueryBudget::new(MAX_QUERY_WORK))
}

fn data_inputs_with_budget(
    heap: &Rc<HeapVersion>,
    data: &ObjectData,
    budget: &mut QueryBudget,
) -> BTreeSet<ValuesPath> {
    let mut fields = Vec::new();
    let mut values = Vec::new();
    let mut inputs = BTreeSet::new();
    match data {
        ObjectData::Map {
            fields: known,
            remainder,
        } => {
            fields.extend(known.values());
            if let Remainder::Unknown { inputs: known } = remainder {
                inputs.extend(known.iter().cloned());
            }
        }
        ObjectData::Strings(slots) => values.extend(slots.iter().cloned()),
        ObjectData::Opaque { reachable } => values.push(super::Value::leaf(super::Datum::Unknown(
            super::UnknownValue {
                types: super::TypeCoverage::Unresolved,
                aliases: reachable.clone(),
            },
        ))),
        ObjectData::Widened { inputs: known } => inputs.extend(known.iter().cloned()),
    }
    while let Some(field) = fields.pop() {
        if !budget.charge(1) {
            return BTreeSet::from([ValuesPath::default()]);
        }
        match field {
            Field::Present(value) => values.push(value.clone()),
            Field::Select {
                decision: _,
                yes,
                no,
            } => fields.extend([yes.as_ref(), no.as_ref()]),
            Field::Unknown | Field::Absent => {}
        }
    }
    let include_unknown_aliases = matches!(data, ObjectData::Opaque { reachable: _ });
    for value in values {
        let frozen = super::FrozenValue::at(
            Rc::clone(heap),
            heap.consume_value(&value, budget, &mut AHashMap::new()),
        );
        inputs.extend(frozen.collect_inputs_with_budget(include_unknown_aliases, budget));
    }
    inputs
}

impl Drop for HeapVersion {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.index.detach(&mut pending);
        detach(
            std::mem::replace(&mut self.node, HeapNode::Empty),
            &mut pending,
        );
        while let Some(version) = pending.pop() {
            if let Ok(mut version) = Rc::try_unwrap(version) {
                version.index.detach(&mut pending);
                detach(
                    std::mem::replace(&mut version.node, HeapNode::Empty),
                    &mut pending,
                );
            }
        }
    }
}

fn detach(node: HeapNode, pending: &mut Vec<Rc<HeapVersion>>) {
    match node {
        HeapNode::Empty => {}
        HeapNode::Input { parent }
        | HeapNode::Install {
            parent,
            object: _,
            storage: _,
        }
        | HeapNode::Write {
            parent,
            object: _,
            key: _,
            field: _,
        }
        | HeapNode::WriteSlot {
            parent,
            object: _,
            index: _,
            value: _,
        }
        | HeapNode::UnknownWrite {
            parent,
            objects: _,
            inputs: _,
        } => pending.push(parent),
        HeapNode::Join {
            decision: _,
            yes,
            no,
        } => pending.extend([yes, no]),
    }
}

impl std::fmt::Debug for HeapVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeapVersion")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl HeapVersion {
    /// Resolves live slots against this version without revisiting shared value nodes.
    #[tracing::instrument(skip_all)]
    pub(super) fn consume(&self, value: &super::Value) -> super::Value {
        if matches!(
            value.node.as_ref(),
            super::ValueNode::Leaf(super::Leaf {
                datum: super::Datum::Text(_)
                    | super::Datum::Scalar {
                        go_type: _,
                        content: _
                    }
                    | super::Datum::Map { view: _, target: _ }
                    | super::Datum::Sequence(_)
                    | super::Datum::Strings(_)
                    | super::Datum::Unknown(_),
                origin: _,
            })
        ) {
            return value.clone();
        }
        self.consume_value(
            value,
            &mut QueryBudget::new(MAX_QUERY_WORK),
            &mut AHashMap::new(),
        )
    }

    fn consume_value(
        &self,
        value: &super::Value,
        budget: &mut QueryBudget,
        memo: &mut AHashMap<*const super::ValueNode, super::Value>,
    ) -> super::Value {
        let key = Rc::as_ptr(&value.node);
        if let Some(value) = memo.get(&key) {
            return value.clone();
        }
        if !budget.charge(1) {
            return super::Value::influenced(BTreeSet::from([ValuesPath::default()]));
        }
        let consumed = match value.node.as_ref() {
            super::ValueNode::Select { decision, yes, no } => {
                let consumed_yes = self.consume_value(yes, budget, memo);
                let consumed_no = self.consume_value(no, budget, memo);
                if Rc::ptr_eq(&yes.node, &consumed_yes.node)
                    && Rc::ptr_eq(&no.node, &consumed_no.node)
                {
                    value.clone()
                } else {
                    super::Value::select(Rc::clone(decision), consumed_yes, consumed_no)
                }
            }
            super::ValueNode::Leaf(super::Leaf {
                datum: super::Datum::Slot { backing, index },
                origin: _,
            }) => match self.query_object(backing, budget).as_deref() {
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Strings(slots),
                }) => slots
                    .get(*index)
                    .cloned()
                    .unwrap_or_else(super::Value::unknown),
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Widened { inputs },
                }) => super::Value {
                    node: Rc::new(super::ValueNode::Leaf(super::Leaf {
                        datum: super::Datum::Scalar {
                            go_type: super::ScalarType::String,
                            content: None,
                        },
                        origin: super::Origin {
                            derivation: None,
                            identity: None,
                            influences: inputs.as_ref().clone(),
                        },
                    })),
                },
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data:
                        ObjectData::Map {
                            fields: _,
                            remainder: _,
                        }
                        | ObjectData::Opaque { reachable: _ },
                })
                | None => super::Value::unknown(),
            },
            super::ValueNode::Leaf(super::Leaf {
                datum:
                    super::Datum::Text(_)
                    | super::Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | super::Datum::Map { view: _, target: _ }
                    | super::Datum::Sequence(_)
                    | super::Datum::Strings(_)
                    | super::Datum::Unknown(_),
                origin: _,
            }) => value.clone(),
        };
        memo.insert(key, consumed.clone());
        consumed
    }
}

#[cfg(test)]
#[path = "../tests/runtime_heap_query.rs"]
mod tests;
