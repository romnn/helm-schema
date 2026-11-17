//! Maps built over open key sets by `deepCopy` and `merge`, read key by key (ADR 0072 §1).
//!
//! Sprig's `merge` and `mergeOverwrite` run dario.cat/mergo v1.0.1 (`merge.go:58-166`): they
//! mutate the destination in place, recurse into a nested map both sides hold, and assign a
//! source member by reference where the destination lacks a non-empty one. `deepCopy` separates
//! every nested map. When a key set is open (an input map), the object keeps its sources as
//! layers and resolves each key when it is read. A nested map receives its share of the layers
//! when it is first used as an object, so an alias taken before the merge sees the merge.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Guard, Predicate, ValuesPath};

use super::{
    Coverage, Datum, Field, FrozenTest, FrozenValue, Leaf, MapType, Multiplicity, ObjectData,
    ObjectId, ObjectVersion, Remainder, State, TypeCoverage, UnknownValue, Value, ValueNode,
};

/// Nested merges deeper than this stop recursing and leave the member unknown.
const MAX_MERGE_DEPTH: usize = 32;

/// The branch outcomes under which a merge runs; empty when it runs on every path.
pub(super) type Outcomes = Rc<[(Rc<FrozenTest>, bool)]>;

/// `guard` narrowed by one more branch outcome.
fn extended(guard: &Outcomes, decision: &Rc<FrozenTest>, taken: bool) -> Outcomes {
    guard
        .iter()
        .cloned()
        .chain(std::iter::once((Rc::clone(decision), taken)))
        .collect()
}

/// Steps one member merge may take before it reads unknown (shared choices multiply).
const MAX_MERGE_WORK: usize = 1024;

/// Layers one object keeps; past it the object's unwritten keys widen to unknown (ADR 0072 §10).
const MAX_LAYERS: usize = 64;

/// Whether a layer list stays within its bound, counting the widening when it does not.
fn within_layer_bound(merges: &[MergeLayer]) -> bool {
    if merges.len() <= MAX_LAYERS {
        return true;
    }
    let _bound =
        tracing::debug_span!(target: "helm_schema_ir::counters", "merge_layer_bound").entered();
    false
}

/// How an object resolves a key it has not written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Layers {
    base: Base,
    /// Merges applied over the base, oldest first.
    merges: Vec<MergeLayer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Base {
    /// The key is absent.
    Closed,
    /// The key reads the object's own input path.
    Input,
    /// The key reads a copy of this map's member, frozen at the copy.
    Copy(FrozenValue),
    /// The key set and members are unknown (a decoded map of unknown text, a widened map):
    /// they depend on these inputs and may alias these objects.
    Unknown(Rc<BTreeSet<ValuesPath>>, super::Reach),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MergeLayer {
    /// The source map, frozen at the merge: its scalars are read as they were then.
    source: FrozenValue,
    overwrite: bool,
    /// Keys the merge resolved eagerly; the layer does not apply to them.
    explicit: Rc<BTreeSet<String>>,
    /// The branch outcomes under which the merge ran; empty when it ran on every path.
    guard: Outcomes,
}

impl State {
    /// The field `key` of `object` when its remainder is layered and the key is unwritten.
    pub(super) fn layered_field(&mut self, object: &ObjectId, key: &str) -> Option<Field> {
        let storage = self.heap.version.object(object)?;
        let ObjectData::Map {
            fields,
            remainder: Remainder::Layered(layers),
        } = &storage.data
        else {
            return None;
        };
        if fields.contains_key(key) {
            return None;
        }
        let path = self.heap.version.input_paths.get(object).cloned();
        Some(self.resolve_layers(object, path.as_ref(), layers, key))
    }

    fn resolve_layers(
        &mut self,
        object: &ObjectId,
        path: Option<&ValuesPath>,
        layers: &Layers,
        key: &str,
    ) -> Field {
        let Layers { base, merges } = layers;
        let mut field = match base {
            Base::Closed => Field::Absent,
            Base::Unknown(_, _) => Field::Unknown,
            Base::Input => path.map_or(Field::Unknown, |path| {
                let mut child = path.clone();
                child.push(key);
                Field::Present(Value::input(child))
            }),
            Base::Copy(source) => {
                let member = self.member_at(source, key);
                self.copy_member(object, key, source, member)
            }
        };
        for MergeLayer {
            source,
            overwrite,
            explicit,
            guard,
        } in merges
        {
            if explicit.contains(key) {
                continue;
            }
            let member = self.member_at(source, key);
            let merged = self.merge_field(field.clone(), member, *overwrite);
            field = bounded_member(guarded(guard, merged, &field));
        }
        field
    }

    /// A member of a frozen map, read at the heap version it was frozen at.
    fn member_at(&mut self, frozen: &FrozenValue, key: &str) -> Field {
        self.member_of(&frozen.heap, &frozen.value, key, 0)
    }

    fn member_of(
        &mut self,
        heap: &Rc<super::HeapVersion>,
        value: &Value,
        key: &str,
        depth: usize,
    ) -> Field {
        if depth > MAX_MERGE_DEPTH {
            return Field::Unknown;
        }
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Field::select(
                Rc::clone(decision),
                self.member_of(heap, yes, key, depth + 1),
                self.member_of(heap, no, key, depth + 1),
            ),
            ValueNode::Leaf(Leaf { datum, origin }) => match datum {
                Datum::Map {
                    view: _,
                    target: Some(id),
                } => self.object_member_at(heap, id, key, depth),
                Datum::Unknown(UnknownValue { types, aliases: _ }) => {
                    let Some(path) = &origin.identity else {
                        return Field::Unknown;
                    };
                    let backing = self
                        .heap
                        .arena
                        .input_objects
                        .borrow()
                        .get(path)
                        .map(|backing| backing.object.clone());
                    if let Some(id) = backing
                        && heap.has_allocated_object(&id)
                    {
                        return self.object_member_at(heap, &id, key, depth);
                    }
                    match types {
                        TypeCoverage::InputNonMap
                        | TypeCoverage::Scalar(_)
                        | TypeCoverage::Sequence => Field::Absent,
                        TypeCoverage::Unresolved | TypeCoverage::Map(_) => {
                            let mut child = path.clone();
                            child.push(key);
                            Field::Present(Value::input(child))
                        }
                    }
                }
                Datum::Map {
                    view: _,
                    target: None,
                }
                | Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Sequence(_)
                | Datum::Strings(_)
                | Datum::Slot {
                    backing: _,
                    index: _,
                } => Field::Absent,
            },
        }
    }

    fn object_member_at(
        &mut self,
        heap: &Rc<super::HeapVersion>,
        id: &ObjectId,
        key: &str,
        depth: usize,
    ) -> Field {
        // A copy child allocated after `heap` was frozen was pristine at that version: its
        // initial storage is what the frozen read sees.
        let storage = heap.object(id).or_else(|| {
            self.heap
                .arena
                .derived
                .borrow()
                .get(id)
                .filter(|_| !heap.has_allocated_object(id))
                .cloned()
        });
        let Some(storage) = storage else {
            return Field::Unknown;
        };
        let ObjectData::Map { fields, remainder } = &storage.data else {
            return Field::Unknown;
        };
        if let Some(field) = fields.get(key) {
            return field.clone();
        }
        match remainder {
            Remainder::Closed => Field::Absent,
            Remainder::Input => heap.input_paths.get(id).map_or(Field::Unknown, |path| {
                let mut child = path.clone();
                child.push(key);
                Field::Present(Value::input(child))
            }),
            Remainder::Open | Remainder::Unknown { inputs: _ } => Field::Unknown,
            Remainder::Layered(layers) => {
                let Layers { base, merges } = layers.as_ref();
                let mut field = match base {
                    Base::Closed => Field::Absent,
                    Base::Unknown(_, _) => Field::Unknown,
                    Base::Input => heap.input_paths.get(id).map_or(Field::Unknown, |path| {
                        let mut child = path.clone();
                        child.push(key);
                        Field::Present(Value::input(child))
                    }),
                    // A copy's map member is the copy occurrence's own child object, never the
                    // source's: a later write to the source does not reach it.
                    Base::Copy(source) => {
                        let source = source.clone();
                        let member = self.member_of(&source.heap, &source.value, key, depth + 1);
                        self.copy_member(id, key, &source, member)
                    }
                };
                for MergeLayer {
                    source,
                    overwrite,
                    explicit,
                    guard,
                } in merges
                {
                    if explicit.contains(key) {
                        continue;
                    }
                    let member = self.member_of(&source.heap, &source.value, key, depth + 1);
                    let merged = self.merge_field(field.clone(), member, *overwrite);
                    field = bounded_member(guarded(guard, merged, &field));
                }
                field
            }
        }
    }

    /// The destination member after mergo merged the source member into it.
    ///
    /// Without overwrite, an absent or nil source member leaves the destination; two maps merge
    /// in place; an absent or empty destination member takes the source's; otherwise the
    /// destination keeps its own. With overwrite, an absent source member leaves the destination,
    /// a nil one is written, two maps merge in place and anything else is replaced.
    fn merge_field(&self, destination: Field, source: Field, overwrite: bool) -> Field {
        let mut budget = MAX_MERGE_WORK;
        let field = self.merge_within(destination, source, overwrite, &mut budget);
        if budget == 0 {
            let _bound =
                tracing::debug_span!(target: "helm_schema_ir::counters", "merge_field_bound")
                    .entered();
        }
        field
    }

    fn merge_within(
        &self,
        destination: Field,
        source: Field,
        overwrite: bool,
        budget: &mut usize,
    ) -> Field {
        let Some(remaining) = budget.checked_sub(1) else {
            return Field::Unknown;
        };
        *budget = remaining;
        match (destination, source) {
            (destination, Field::Absent) => destination,
            (destination, Field::Select { decision, yes, no }) => Field::select(
                decision,
                self.merge_within(destination.clone(), yes.as_ref().clone(), overwrite, budget),
                self.merge_within(destination, no.as_ref().clone(), overwrite, budget),
            ),
            (Field::Select { decision, yes, no }, source) => Field::select(
                decision,
                self.merge_within(yes.as_ref().clone(), source.clone(), overwrite, budget),
                self.merge_within(no.as_ref().clone(), source, overwrite, budget),
            ),
            (Field::Unknown, Field::Unknown | Field::Present(_))
            | (Field::Absent | Field::Present(_), Field::Unknown) => Field::Unknown,
            (destination, Field::Present(source)) => {
                if let ValueNode::Select { decision, yes, no } = source.node.as_ref() {
                    return Field::select(
                        Rc::clone(decision),
                        self.merge_within(
                            destination.clone(),
                            Field::Present(yes.clone()),
                            overwrite,
                            budget,
                        ),
                        self.merge_within(
                            destination,
                            Field::Present(no.clone()),
                            overwrite,
                            budget,
                        ),
                    );
                }
                if let Field::Present(value) = &destination
                    && let ValueNode::Select { decision, yes, no } = value.node.as_ref()
                {
                    return Field::select(
                        Rc::clone(decision),
                        self.merge_within(
                            Field::Present(yes.clone()),
                            Field::Present(source.clone()),
                            overwrite,
                            budget,
                        ),
                        self.merge_within(
                            Field::Present(no.clone()),
                            Field::Present(source),
                            overwrite,
                            budget,
                        ),
                    );
                }
                self.merge_leaf(destination, &source, overwrite)
            }
        }
    }

    /// Merges one source leaf into one destination leaf (`merge.go:58-166`): with overwrite the
    /// destination stays where the source lacks the key or both are maps (merged in place), and
    /// takes the source member, nil included, elsewhere; without overwrite the source member is
    /// assigned where it is not nil, the destination is empty and not both are maps.
    fn merge_leaf(&self, destination: Field, source: &Value, overwrite: bool) -> Field {
        let destination_value = match &destination {
            Field::Present(value) => Some(value.clone()),
            Field::Absent => None,
            Field::Unknown
            | Field::Select {
                decision: _,
                yes: _,
                no: _,
            } => return Field::Unknown,
        };
        let kept = self.split_field(destination);
        let assigned = Field::Present(self.split_leaf(source).unwrap_or_else(|| source.clone()));
        // Two maps merge in place: the destination stays.
        let merged = match &destination_value {
            Some(value) => Field::select(
                self.map_test(value),
                Field::select(self.map_test(source), kept.clone(), assigned.clone()),
                assigned,
            ),
            None => assigned,
        };
        if overwrite {
            return Field::select(self.absent_test(source), kept, merged);
        }
        let merged = match &destination_value {
            Some(value) => Field::select(self.test(value), kept.clone(), merged),
            None => merged,
        };
        Field::select(self.nil_test(source), kept, merged)
    }

    /// A destination field with its unresolved input leaf split by whether it is a map.
    fn split_field(&self, field: Field) -> Field {
        match field {
            Field::Present(value) => Field::Present(self.split_leaf(&value).unwrap_or(value)),
            Field::Absent
            | Field::Unknown
            | Field::Select {
                decision: _,
                yes: _,
                no: _,
            } => field,
        }
    }

    /// Whether a source member is nil or absent: mergo skips it unless it overwrites.
    fn nil_test(&self, value: &Value) -> Rc<FrozenTest> {
        let ValueNode::Leaf(Leaf { datum, origin }) = value.node.as_ref() else {
            return self.unknown_test();
        };
        match datum {
            Datum::Scalar {
                go_type: super::ScalarType::Nil | super::ScalarType::Invalid,
                content: _,
            } => self.constant_test(true),
            Datum::Unknown(UnknownValue { types, aliases: _ }) => match (&origin.identity, types) {
                (
                    Some(path),
                    TypeCoverage::Unresolved | TypeCoverage::Map(_) | TypeCoverage::InputNonMap,
                ) => {
                    let nil = super::kinds::input_nil_predicate(path);
                    self.heap
                        .arena
                        .decision(nil.clone(), nil.negated(), Coverage::Exact)
                }
                (
                    _,
                    TypeCoverage::Unresolved
                    | TypeCoverage::Map(_)
                    | TypeCoverage::InputNonMap
                    | TypeCoverage::Scalar(_)
                    | TypeCoverage::Sequence,
                ) => self.unknown_test(),
            },
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Map { view: _, target: _ }
            | Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => self.constant_test(false),
        }
    }

    /// Whether a source member is absent: its key is not in the source map.
    fn absent_test(&self, value: &Value) -> Rc<FrozenTest> {
        let ValueNode::Leaf(Leaf { datum, origin }) = value.node.as_ref() else {
            return self.unknown_test();
        };
        match datum {
            Datum::Scalar {
                go_type: super::ScalarType::Invalid,
                content: _,
            } => self.constant_test(true),
            Datum::Unknown(UnknownValue {
                types: _,
                aliases: _,
            }) => origin.identity.as_ref().map_or_else(
                || self.unknown_test(),
                |path| {
                    let absent = Predicate::from(Guard::Absent { path: path.clone() });
                    self.heap
                        .arena
                        .decision(absent.clone(), absent.negated(), Coverage::Exact)
                },
            ),
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Map { view: _, target: _ }
            | Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => self.constant_test(false),
        }
    }

    /// Whether a member holds a map, the case where mergo recurses instead of assigning.
    fn map_test(&self, value: &Value) -> Rc<FrozenTest> {
        let ValueNode::Leaf(Leaf { datum, origin }) = value.node.as_ref() else {
            return self.unknown_test();
        };
        match datum {
            Datum::Map { view: _, target: _ } => self.constant_test(true),
            Datum::Unknown(UnknownValue { types, aliases: _ }) => match types {
                TypeCoverage::Map(_) => self.constant_test(true),
                TypeCoverage::InputNonMap | TypeCoverage::Scalar(_) | TypeCoverage::Sequence => {
                    self.constant_test(false)
                }
                TypeCoverage::Unresolved => origin.identity.as_ref().map_or_else(
                    || self.unknown_test(),
                    |path| {
                        let object = Predicate::from(Guard::TypeIs {
                            path: path.clone(),
                            schema_type: "object".to_owned(),
                        });
                        self.heap
                            .arena
                            .decision(object.clone(), object.negated(), Coverage::Exact)
                    },
                ),
            },
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => self.constant_test(false),
        }
    }

    /// Whether a member is a map, and the member as seen where it is and where it is not:
    /// an unresolved input reads as a map in one arm and as a non-map in the other.
    fn map_split(&self, value: &Value) -> (Rc<FrozenTest>, Value, Value) {
        let test = self.map_test(value);
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Unresolved,
                    aliases,
                }),
            origin,
        }) = value.node.as_ref()
        else {
            return (test, value.clone(), value.clone());
        };
        let typed = |types| Value {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(UnknownValue {
                    types,
                    aliases: aliases.clone(),
                }),
                origin: origin.clone(),
            })),
        };
        (
            test,
            typed(TypeCoverage::Map(MapType::Interface)),
            typed(TypeCoverage::InputNonMap),
        )
    }

    /// An unresolved input leaf split by whether it is a map; `None` for any other value.
    fn split_leaf(&self, value: &Value) -> Option<Value> {
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Unresolved,
                    aliases: _,
                }),
            origin,
        }) = value.node.as_ref()
        else {
            return None;
        };
        origin.identity.as_ref()?;
        let (test, map, scalar) = self.map_split(value);
        Some(Value::select(test, map, scalar))
    }

    fn constant_test(&self, value: bool) -> Rc<FrozenTest> {
        let (when_true, when_false) = if value {
            (Predicate::True, Predicate::False)
        } else {
            (Predicate::False, Predicate::True)
        };
        self.heap
            .arena
            .decision(when_true, when_false, Coverage::Exact)
    }

    fn unknown_test(&self) -> Rc<FrozenTest> {
        self.heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial)
    }

    /// Merges `source` into the map object `target` as mergo does, in place. Returns false when
    /// the merge cannot be represented; the caller then widens its operands.
    pub(super) fn merge_into(
        &mut self,
        target: &ObjectId,
        source: &Value,
        overwrite: bool,
        depth: usize,
        guard: &Outcomes,
    ) -> bool {
        if depth > MAX_MERGE_DEPTH {
            return false;
        }
        let Some(source) = map_operand(source) else {
            return false;
        };
        let Some(storage) = self.heap.version.object(target) else {
            return false;
        };
        let ObjectVersion {
            revision: _,
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map { fields, remainder },
        } = storage.as_ref()
        else {
            return false;
        };
        let frozen = self.heap.freeze(&source);
        let mut explicit: BTreeSet<String> = fields.keys().cloned().collect();
        let source_keys = Self::known_member_keys(&frozen);
        let Some((source_keys, source_open)) = source_keys else {
            return false;
        };
        explicit.extend(source_keys);
        let path = self.heap.version.input_paths.get(target).cloned();
        let mut writes = Vec::new();
        // Mergo walks source keys in Go's random map order; when two of them recurse into one
        // shared nested map, the result depends on that order, so it is not represented.
        let mut nested = BTreeSet::new();
        for key in &explicit {
            let destination = match fields.get(key) {
                Some(field) => field.clone(),
                None => match remainder {
                    Remainder::Closed => Field::Absent,
                    Remainder::Input => path.as_ref().map_or(Field::Unknown, |path| {
                        let mut child = path.clone();
                        child.push(key);
                        Field::Present(Value::input(child))
                    }),
                    Remainder::Layered(layers) => {
                        let layers = Rc::clone(layers);
                        self.resolve_layers(target, path.as_ref(), &layers, key)
                    }
                    Remainder::Open | Remainder::Unknown { inputs: _ } => Field::Unknown,
                },
            };
            let member = self.member_at(&frozen, key);
            if matches!(member, Field::Absent) {
                continue;
            }
            if !self.merge_nested(&destination, &member, overwrite, depth, guard, &mut nested) {
                return false;
            }
            let merged = self.merge_field(destination.clone(), member, overwrite);
            writes.push((key.clone(), guarded(guard, merged, &destination)));
        }
        for (key, field) in writes {
            if !self.heap.write(target, &key, field) {
                return false;
            }
        }
        if source_open {
            let layer = MergeLayer {
                source: frozen,
                overwrite,
                explicit: Rc::new(explicit),
                guard: Rc::clone(guard),
            };
            if !self.push_layer(target, &layer, depth) {
                return false;
            }
        }
        true
    }

    /// Recurses into a nested map both members may hold, mutating the destination's own object
    /// so that its earlier aliases see the merge.
    /// Merges a source member into the destination member's own map where both may be maps.
    /// Each alternative of the destination is a separate map; its merge applies only under the
    /// branch outcomes that select it.
    fn merge_nested(
        &mut self,
        destination: &Field,
        source: &Field,
        overwrite: bool,
        depth: usize,
        guard: &Outcomes,
        nested: &mut BTreeSet<ObjectId>,
    ) -> bool {
        let Field::Present(source) = source else {
            return true;
        };
        if self.map_test(source).constant_outcome() == Some(false) {
            return true;
        }
        match destination {
            Field::Absent => true,
            Field::Unknown => false,
            Field::Select { decision, yes, no } => {
                [(yes, true), (no, false)].into_iter().all(|(arm, taken)| {
                    let guard = extended(guard, decision, taken);
                    self.merge_nested(
                        arm,
                        &Field::Present(source.clone()),
                        overwrite,
                        depth,
                        &guard,
                        nested,
                    )
                })
            }
            Field::Present(destination) => {
                self.merge_nested_value(destination, source, overwrite, depth, guard, nested)
            }
        }
    }

    fn merge_nested_value(
        &mut self,
        destination: &Value,
        source: &Value,
        overwrite: bool,
        depth: usize,
        guard: &Outcomes,
        nested: &mut BTreeSet<ObjectId>,
    ) -> bool {
        if let ValueNode::Select { decision, yes, no } = destination.node.as_ref() {
            return [(yes, true), (no, false)].into_iter().all(|(arm, taken)| {
                let guard = extended(guard, decision, taken);
                self.merge_nested_value(arm, source, overwrite, depth, &guard, nested)
            });
        }
        if self.map_test(destination).constant_outcome() == Some(false) {
            return true;
        }
        let Some(target) = self.nested_target(destination) else {
            return false;
        };
        nested.insert(target.clone())
            && self.merge_into(&target, source, overwrite, depth + 1, guard)
    }

    fn nested_target(&mut self, value: &Value) -> Option<ObjectId> {
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Map { view: _, target },
                origin: _,
            }) => target.clone(),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(UnknownValue {
                        types: TypeCoverage::Unresolved | TypeCoverage::Map(_),
                        aliases: _,
                    }),
                origin,
            }) => {
                let path = origin.identity.clone()?;
                Some(self.input_object(&path))
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }

    /// The keys a frozen map lists, and whether its key set is open beyond them.
    fn known_member_keys(frozen: &FrozenValue) -> Option<(BTreeSet<String>, bool)> {
        match frozen.value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: None,
                    },
                origin: _,
            }) => Some((BTreeSet::new(), false)),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: Some(id),
                    },
                origin: _,
            }) => {
                let storage = frozen.heap.object(id)?;
                let ObjectData::Map { fields, remainder } = &storage.data else {
                    return None;
                };
                Some((
                    fields.keys().cloned().collect(),
                    !matches!(remainder, Remainder::Closed),
                ))
            }
            // An input map, or a map of unknown content: any key may follow.
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(UnknownValue {
                        types: TypeCoverage::Unresolved | TypeCoverage::Map(_),
                        aliases: _,
                    }),
                origin: _,
            }) => Some((BTreeSet::new(), true)),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }

    /// Adds a merge layer to `target`, and to each nested map already allocated beneath it, so
    /// that existing aliases of those maps see their share of the merge.
    fn push_layer(&mut self, target: &ObjectId, layer: &MergeLayer, depth: usize) -> bool {
        let Some(storage) = self.heap.version.object(target) else {
            return false;
        };
        let ObjectData::Map { fields, remainder } = &storage.data else {
            return false;
        };
        let layers = match remainder {
            Remainder::Closed => Layers {
                base: Base::Closed,
                merges: vec![layer.clone()],
            },
            Remainder::Input => Layers {
                base: Base::Input,
                merges: vec![layer.clone()],
            },
            Remainder::Layered(layers) => {
                let mut layers = layers.as_ref().clone();
                layers.merges.push(layer.clone());
                if within_layer_bound(&layers.merges) {
                    layers
                } else {
                    // Past the bound only this object's unwritten keys widen, keeping what
                    // they depend on; the rest of the heap is untouched.
                    let path = self.heap.version.input_paths.get(target).cloned();
                    let aliases = self
                        .layer_aliases(&layers)
                        .map_or(super::Reach::Top, |ids| super::Reach::Bounded(Rc::new(ids)));
                    layers.widened(path.as_ref(), aliases)
                }
            }
            Remainder::Open => Layers {
                base: Base::Unknown(Rc::default(), super::Reach::Top),
                merges: vec![layer.clone()],
            },
            Remainder::Unknown { inputs } => Layers {
                base: Base::Unknown(Rc::clone(inputs), super::Reach::Top),
                merges: vec![layer.clone()],
            },
        };
        let mut storage = storage.as_ref().clone();
        storage.data = ObjectData::Map {
            fields: Rc::clone(fields),
            remainder: Remainder::Layered(Rc::new(layers)),
        };
        storage.revision = self.heap.arena.snapshot();
        self.heap.arena.layered.set(true);
        self.heap.install(target.clone(), storage);
        let children = self.allocated_children(target);
        for (key, child) in children {
            if layer.explicit.contains(&key) {
                continue;
            }
            let Some(member) = field_as_value(self.member_at(&layer.source, &key)) else {
                continue;
            };
            if self.map_test(&member).constant_outcome() == Some(false) {
                continue;
            }
            if !self.merge_into(&child, &member, layer.overwrite, depth + 1, &layer.guard) {
                return false;
            }
        }
        true
    }

    /// Nested maps allocated beneath `target` for unwritten keys: input children and copies.
    pub(super) fn allocated_children(&self, target: &ObjectId) -> Vec<(String, ObjectId)> {
        let mut children = Vec::new();
        if let Some(path) = self.heap.version.input_paths.get(target) {
            for (child, backing) in self.heap.arena.input_objects.borrow().iter() {
                if child.parent().as_ref() == Some(path)
                    && let Some(key) = child
                        .segments()
                        .last()
                        .and_then(|segment| segment.literal())
                    && self.heap.version.has_allocated_object(&backing.object)
                {
                    children.push((key.to_owned(), backing.object.clone()));
                }
            }
        }
        for ((parent, key, _), child) in self.heap.arena.copy_children.borrow().iter() {
            if parent == target && self.heap.version.has_allocated_object(child) {
                children.push((key.clone(), child.clone()));
            }
        }
        children
    }

    /// The remainder of the input map at `path` when its parent's merge layers reach it.
    pub(super) fn derived_input_remainder(&mut self, path: &ValuesPath) -> Option<Remainder> {
        if !self.heap.arena.layered.get() {
            return None;
        }
        let parent = path.parent()?;
        let key = path.segments().last()?.literal()?.to_owned();
        let existing = self
            .heap
            .arena
            .input_objects
            .borrow()
            .get(&parent)
            .map(|backing| backing.object.clone())
            .filter(|id| self.heap.version.has_allocated_object(id));
        let parent_object = if let Some(id) = existing {
            id
        } else {
            self.derived_input_remainder(&parent)?;
            self.input_object(&parent)
        };
        let storage = self.heap.version.object(&parent_object)?;
        let ObjectData::Map {
            fields,
            remainder: Remainder::Layered(layers),
        } = &storage.data
        else {
            return None;
        };
        if fields.contains_key(&key) {
            return None;
        }
        let child = self.child_layers(layers, &key)?;
        Some(Remainder::Layered(Rc::new(child)))
    }

    /// The share of a parent's layers that reaches its nested map at `key`.
    fn child_layers(&mut self, layers: &Layers, key: &str) -> Option<Layers> {
        let Layers { base, merges } = layers;
        let base = match base {
            Base::Closed => return None,
            Base::Unknown(inputs, aliases) => Base::Unknown(Rc::clone(inputs), aliases.clone()),
            Base::Input => Base::Input,
            Base::Copy(source) => {
                let Field::Present(member) = self.member_at(source, key) else {
                    return None;
                };
                Base::Copy(FrozenValue::at(Rc::clone(&source.heap), member))
            }
        };
        let merges: Vec<_> = merges
            .iter()
            .filter(|layer| !layer.explicit.contains(key))
            .filter_map(|layer| {
                let member = field_as_value(self.member_at(&layer.source, key))?;
                (self.map_test(&member).constant_outcome() != Some(false)).then(|| MergeLayer {
                    source: FrozenValue::at(Rc::clone(&layer.source.heap), member),
                    overwrite: layer.overwrite,
                    explicit: Rc::default(),
                    guard: Rc::clone(&layer.guard),
                })
            })
            .collect();
        if matches!(base, Base::Input) && merges.is_empty() {
            return None;
        }
        Some(Layers { base, merges })
    }

    /// Remembers the storage a derived object starts from, for joins that never allocated it.
    pub(super) fn remember_derived(&self, id: &ObjectId, storage: &ObjectVersion) {
        if let ObjectData::Map {
            fields: _,
            remainder: Remainder::Layered(_),
        } = &storage.data
        {
            self.heap
                .arena
                .derived
                .borrow_mut()
                .insert(id.clone(), Rc::new(storage.clone()));
        }
    }

    /// `deepCopy` of a map whose key set is open: a new object that reads a frozen copy of the
    /// source key by key, allocating a separate object for each nested map it reads.
    pub(super) fn copy_open_map(&mut self, source: &Value) -> Option<Value> {
        let frozen = self.heap.freeze(source);
        let (_, open) = Self::known_member_keys(&frozen)?;
        if !open {
            return None;
        }
        let id = self.allocate_copy(None, frozen);
        let map = Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(id),
        });
        Some(match source.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(UnknownValue {
                        types: TypeCoverage::Unresolved,
                        aliases: _,
                    }),
                origin,
            }) => self.map_or_input(origin.identity.as_ref(), map),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => map,
        })
    }

    fn allocate_copy(&mut self, slot: Option<&ObjectId>, source: FrozenValue) -> ObjectId {
        let storage = ObjectVersion {
            revision: self.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::new(BTreeMap::new()),
                remainder: Remainder::Layered(Rc::new(Layers {
                    base: Base::Copy(source),
                    merges: Vec::new(),
                })),
            },
        };
        self.heap.arena.layered.set(true);
        let id = self
            .heap
            .allocate_at(slot, storage.data.clone(), Multiplicity::Singleton);
        self.remember_derived(&id, &storage);
        id
    }

    /// An input member that may be a map reads as the copy's own object when it is one.
    fn map_or_input(&self, path: Option<&ValuesPath>, map: Value) -> Value {
        let Some(path) = path else {
            return map;
        };
        let object = Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: "object".to_owned(),
        });
        let decision = self
            .heap
            .arena
            .decision(object.clone(), object.negated(), Coverage::Exact);
        let nonmap = Value {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(UnknownValue {
                    types: TypeCoverage::InputNonMap,
                    aliases: super::Reach::Bounded(Rc::default()),
                }),
                origin: super::Origin {
                    derivation: None,
                    identity: Some(path.clone()),
                    influences: BTreeSet::new(),
                },
            })),
        };
        Value::select(decision, map, nonmap)
    }

    /// A member a copy reads from its source: nested maps become objects of the copy.
    fn copy_member(
        &mut self,
        copy: &ObjectId,
        key: &str,
        source: &FrozenValue,
        member: Field,
    ) -> Field {
        let mut arm = 0;
        self.copy_field(copy, key, source, member, &mut arm)
    }

    /// A copy of each alternative member, keeping its decision; every map alternative is a
    /// separate copy object, numbered by `arm` in a fixed order.
    fn copy_field(
        &mut self,
        copy: &ObjectId,
        key: &str,
        source: &FrozenValue,
        member: Field,
        arm: &mut u32,
    ) -> Field {
        match member {
            Field::Present(value) => {
                Field::Present(self.copy_value(copy, key, source, &value, arm))
            }
            Field::Select { decision, yes, no } => {
                let yes = self.copy_field(copy, key, source, yes.as_ref().clone(), arm);
                let no = self.copy_field(copy, key, source, no.as_ref().clone(), arm);
                Field::select(decision, yes, no)
            }
            Field::Absent | Field::Unknown => member,
        }
    }

    fn copy_value(
        &mut self,
        copy: &ObjectId,
        key: &str,
        source: &FrozenValue,
        value: &Value,
        arm: &mut u32,
    ) -> Value {
        let (datum, origin) = match value.node.as_ref() {
            ValueNode::Leaf(Leaf { datum, origin }) => (datum, origin),
            ValueNode::Select { decision, yes, no } => {
                let yes = self.copy_value(copy, key, source, yes, arm);
                let no = self.copy_value(copy, key, source, no, arm);
                return Value::select(Rc::clone(decision), yes, no);
            }
        };
        let map_like = match datum {
            Datum::Map {
                view: _,
                target: Some(_),
            } => true,
            Datum::Unknown(UnknownValue { types, aliases: _ }) => {
                origin.identity.is_some()
                    && matches!(types, TypeCoverage::Unresolved | TypeCoverage::Map(_))
            }
            Datum::Map {
                view: _,
                target: None,
            }
            | Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => false,
            // Sequences are copied whole by `deepCopy`'s eager path; keep the member's inputs.
            Datum::Sequence(_) => return Value::influenced(value.inputs()),
        };
        if !map_like {
            return value.clone();
        }
        let slot_key = (copy.clone(), key.to_owned(), *arm);
        *arm += 1;
        let slot = self
            .heap
            .arena
            .copy_children
            .borrow()
            .get(&slot_key)
            .cloned();
        let child = match slot {
            Some(child) if self.heap.version.has_allocated_object(&child) => child,
            slot => {
                let frozen = FrozenValue::at(Rc::clone(&source.heap), value.clone());
                let child = self.allocate_copy(slot.as_ref(), frozen);
                self.heap
                    .arena
                    .copy_children
                    .borrow_mut()
                    .insert(slot_key, child.clone());
                child
            }
        };
        let map = Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(child),
        });
        match datum {
            Datum::Unknown(UnknownValue {
                types: TypeCoverage::Unresolved,
                aliases: _,
            }) => self.map_or_input(origin.identity.as_ref(), map),
            Datum::Unknown(_)
            | Datum::Map { view: _, target: _ }
            | Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => map,
        }
    }
}

/// The map arm of a merge operand: Sprig's `merge` aborts on a non-map, so the arm that is not
/// a map never reaches the merge.
pub(super) fn map_operand(value: &Value) -> Option<Value> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => Some(value.clone()),
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            let map = |value: &Value| {
                matches!(
                    value.node.as_ref(),
                    ValueNode::Leaf(Leaf {
                        datum: Datum::Map { view: _, target: _ },
                        origin: _,
                    })
                )
            };
            let nonmap = |value: &Value| {
                matches!(
                    value.node.as_ref(),
                    ValueNode::Leaf(Leaf {
                        datum: Datum::Unknown(UnknownValue {
                            types: TypeCoverage::InputNonMap
                                | TypeCoverage::Scalar(_)
                                | TypeCoverage::Sequence,
                            aliases: _,
                        }) | Datum::Scalar {
                            go_type: _,
                            content: _,
                        } | Datum::Text(_),
                        origin: _,
                    })
                )
            };
            if map(yes) && nonmap(no) {
                Some(yes.clone())
            } else if map(no) && nonmap(yes) {
                Some(no.clone())
            } else {
                None
            }
        }
    }
}

impl State {
    /// The remainder of a `deepCopy` of a map whose own remainder reads more than its fields.
    pub(super) fn copy_remainder(&self, source: &FrozenValue, remainder: &Remainder) -> Remainder {
        match remainder {
            Remainder::Closed | Remainder::Open | Remainder::Unknown { inputs: _ } => {
                remainder.clone()
            }
            Remainder::Input | Remainder::Layered(_) => {
                self.heap.arena.layered.set(true);
                Remainder::Layered(Rc::new(Layers {
                    base: Base::Copy(source.clone()),
                    merges: Vec::new(),
                }))
            }
        }
    }
}

impl Layers {
    /// These layers collapsed to an unknown base that keeps every input they read: the base's
    /// own region (`path` for an input base) and each merge source's.
    fn widened(&self, path: Option<&ValuesPath>, aliases: super::Reach) -> Self {
        let mut inputs = BTreeSet::new();
        match &self.base {
            Base::Closed => {}
            Base::Unknown(known, _) => inputs.extend(known.iter().cloned()),
            Base::Input => inputs.extend(path.cloned()),
            Base::Copy(source) => inputs.extend(source.inputs()),
        }
        for layer in &self.merges {
            inputs.extend(layer.source.inputs());
        }
        Self {
            base: Base::Unknown(Rc::new(inputs), aliases),
            merges: Vec::new(),
        }
    }

    /// The inputs an unknown base still depends on.
    pub(super) fn unknown_inputs(&self) -> Option<&Rc<BTreeSet<ValuesPath>>> {
        match &self.base {
            Base::Unknown(inputs, _) => Some(inputs),
            Base::Closed | Base::Input | Base::Copy(_) => None,
        }
    }

    /// The values this object reads its unwritten keys from.
    pub(super) fn source_values(&self) -> impl Iterator<Item = &Value> {
        let Self { base, merges } = self;
        let base = match base {
            Base::Closed | Base::Input | Base::Unknown(_, _) => None,
            Base::Copy(source) => Some(&source.value),
        };
        base.into_iter()
            .chain(merges.iter().map(|layer| &layer.source.value))
    }
}

impl State {
    /// The object of the input map at `path` when it exists or merge layers reach it.
    pub(super) fn layered_input(&mut self, path: &ValuesPath) -> Option<ObjectId> {
        let existing = self
            .heap
            .arena
            .input_objects
            .borrow()
            .get(path)
            .map(|backing| backing.object.clone());
        if let Some(id) = existing
            && self.heap.version.has_allocated_object(&id)
        {
            return Some(id);
        }
        self.derived_input_remainder(path)?;
        Some(self.input_object(path))
    }
}

/// `merged` where every guard outcome holds, `unmerged` elsewhere.
fn guarded(guard: &[(Rc<FrozenTest>, bool)], merged: Field, unmerged: &Field) -> Field {
    guard.iter().rev().fold(merged, |inner, (decision, taken)| {
        if *taken {
            Field::select(Rc::clone(decision), inner, unmerged.clone())
        } else {
            Field::select(Rc::clone(decision), unmerged.clone(), inner)
        }
    })
}

/// The remainder after a join, when both arms resolve unwritten keys through layers over one
/// base: the shared merges stay, and each arm's own merges apply under its branch outcome.
pub(super) fn join_remainders(
    decision: &Rc<FrozenTest>,
    yes: &Remainder,
    no: &Remainder,
) -> Option<Remainder> {
    let as_layers = |remainder: &Remainder| match remainder {
        Remainder::Closed => Some(Layers {
            base: Base::Closed,
            merges: Vec::new(),
        }),
        Remainder::Input => Some(Layers {
            base: Base::Input,
            merges: Vec::new(),
        }),
        Remainder::Layered(layers) => Some(layers.as_ref().clone()),
        Remainder::Open => Some(Layers {
            base: Base::Unknown(Rc::default(), super::Reach::Top),
            merges: Vec::new(),
        }),
        Remainder::Unknown { inputs } => Some(Layers {
            base: Base::Unknown(Rc::clone(inputs), super::Reach::Top),
            merges: Vec::new(),
        }),
    };
    let yes = as_layers(yes)?;
    let no = as_layers(no)?;
    if yes.base != no.base {
        return None;
    }
    let shared = yes
        .merges
        .iter()
        .zip(&no.merges)
        .take_while(|(yes, no)| yes == no)
        .count();
    let mut merges = yes.merges.get(..shared)?.to_vec();
    for (arm, taken) in [(&yes, true), (&no, false)] {
        for layer in arm.merges.get(shared..)? {
            let mut layer = layer.clone();
            layer.guard = std::iter::once((Rc::clone(decision), taken))
                .chain(layer.guard.iter().cloned())
                .collect();
            merges.push(layer);
        }
    }
    within_layer_bound(&merges).then(|| {
        Remainder::Layered(Rc::new(Layers {
            base: yes.base,
            merges,
        }))
    })
}

/// Whether a value is certainly not a map: a write into it aborts the render.
pub(super) fn known_non_map(value: &Value) -> bool {
    let ValueNode::Leaf(Leaf { datum, origin: _ }) = value.node.as_ref() else {
        return false;
    };
    match datum {
        Datum::Unknown(UnknownValue { types, aliases: _ }) => matches!(
            types,
            TypeCoverage::InputNonMap | TypeCoverage::Scalar(_) | TypeCoverage::Sequence
        ),
        Datum::Text(_)
        | Datum::Scalar {
            go_type: _,
            content: _,
        }
        | Datum::Sequence(_)
        | Datum::Strings(_)
        | Datum::Slot {
            backing: _,
            index: _,
        } => true,
        Datum::Map { view: _, target: _ } => false,
    }
}

impl State {
    /// The member `key` of a consumed map value whose unwritten keys read through layers.
    pub(super) fn layered_member(&mut self, value: &Value, key: &str) -> Option<Field> {
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: Some(id),
                    },
                origin: _,
            }) => self.layered_field(&id.clone(), key),
            ValueNode::Leaf(Leaf { datum: _, origin }) => {
                let path = origin.identity.clone()?;
                let id = self.layered_input(&path)?;
                self.layered_field(&id, key)
            }
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }

    /// Whether a layered member is present: an input member is present where its key is.
    pub(super) fn layered_presence(&self, field: &Field) -> Value {
        match field {
            Field::Absent => Value::literal(helm_schema_core::GuardValue::Bool(false)),
            Field::Unknown => Value::unknown(),
            Field::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.layered_presence(yes),
                self.layered_presence(no),
            ),
            Field::Present(value) => self.value_presence(value),
        }
    }

    fn value_presence(&self, value: &Value) -> Value {
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.value_presence(yes),
                self.value_presence(no),
            ),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(UnknownValue {
                        types: _,
                        aliases: _,
                    }),
                origin,
            }) if origin.identity.is_some() => {
                let Some(path) = &origin.identity else {
                    return Value::unknown();
                };
                let (Some(parent), Some(key)) = (
                    path.parent(),
                    path.segments().last().and_then(|segment| segment.literal()),
                ) else {
                    return Value::unknown();
                };
                let present = Predicate::from(Guard::HasKey {
                    path: parent,
                    key: key.to_owned(),
                });
                let decision =
                    self.heap
                        .arena
                        .decision(present.clone(), present.negated(), Coverage::Exact);
                Value::select(
                    decision,
                    Value::literal(helm_schema_core::GuardValue::Bool(true)),
                    Value::literal(helm_schema_core::GuardValue::Bool(false)),
                )
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => Value::literal(helm_schema_core::GuardValue::Bool(true)),
        }
    }
}

impl State {
    /// The truthiness of a map whose unwritten keys read through layers: it is non-empty when
    /// its base or an applied layer is. A merge never removes a key, so a non-empty destination
    /// stays non-empty; without overwrite a source of only nil members adds nothing, so those
    /// layers prove only emptiness.
    pub(super) fn layered_truth(&self, object: &ObjectId) -> Option<Rc<FrozenTest>> {
        let storage = self.heap.version.object(object)?;
        let ObjectData::Map {
            fields,
            remainder: Remainder::Layered(layers),
        } = &storage.data
        else {
            return None;
        };
        if fields
            .values()
            .any(|field| matches!(field, Field::Present(_)))
        {
            return Some(self.constant_test(true));
        }
        // Every other field is a tombstone (`unset`) or undecided. A tombstone masks a key the
        // layers may hold, possibly the last one, so the layers no longer decide emptiness.
        if !fields.is_empty() {
            return None;
        }
        let Layers { base, merges } = layers.as_ref();
        let base = match base {
            Base::Closed => Predicate::False,
            Base::Unknown(_, _) => return None,
            Base::Input => Predicate::from(Guard::Truthy {
                path: self.heap.version.input_paths.get(object)?.clone(),
            }),
            Base::Copy(source) => map_truth(&source.value)?,
        };
        let mut proven = vec![base.clone()];
        let mut possible = vec![base];
        let mut certain = true;
        for MergeLayer {
            source,
            overwrite,
            explicit: _,
            guard,
        } in merges
        {
            let mut applies = Vec::new();
            for (decision, taken) in guard.iter() {
                if decision.coverage != Coverage::Exact {
                    return None;
                }
                applies.push(if *taken {
                    decision.when_true.clone()
                } else {
                    decision.when_false.clone()
                });
            }
            applies.push(map_truth(&source.value)?);
            let contribution = Predicate::all(applies);
            if *overwrite {
                proven.push(contribution.clone());
            } else {
                certain = false;
            }
            possible.push(contribution);
        }
        let memo = &self.heap.arena.predicate_memo;
        let possible = memo.normalize(Predicate::Or(possible));
        let exact = |predicate: Predicate| {
            let negated = predicate.negated();
            self.heap
                .arena
                .decision(predicate, negated, Coverage::Exact)
        };
        if certain {
            return Some(exact(possible));
        }
        // A non-overwrite source whose members are all nil adds no key (mergo skips nil
        // members), and no guard states "has a non-nil member". The result is decided only when
        // a contribution that always adds a key holds on every path, or when none is possible.
        let proven = memo.normalize(Predicate::Or(proven));
        if proven == Predicate::True {
            return Some(self.constant_test(true));
        }
        if possible == Predicate::False {
            return Some(self.constant_test(false));
        }
        None
    }
}

/// The truthiness of a merge source or copied map, when it is a plain input or a nil map.
fn map_truth(value: &Value) -> Option<Predicate> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: _,
                target: None,
            },
            origin: _,
        }) => Some(Predicate::False),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Unresolved | TypeCoverage::Map(_),
                    aliases: _,
                }),
            origin,
        }) => Some(Predicate::from(Guard::Truthy {
            path: origin.identity.clone()?,
        })),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

/// A member as a value: absent is no member, an unknown member reads unknown at every key, and
/// alternative members keep their decisions (a missing arm reads as Go's invalid value).
fn field_as_value(field: Field) -> Option<Value> {
    match field {
        Field::Absent => None,
        Field::Present(value) => Some(value),
        Field::Unknown => Some(Value::unknown()),
        Field::Select { decision, yes, no } => Some(Value::select(
            decision,
            field_as_value(yes.as_ref().clone()).unwrap_or_else(Value::invalid),
            field_as_value(no.as_ref().clone()).unwrap_or_else(Value::invalid),
        )),
    }
}

/// Alternatives one merged member keeps; past it the member reads unknown (ADR 0072 §10).
const MAX_MEMBER_ALTERNATIVES: usize = 64;

/// A merged member within its alternative bound, or unknown with the widening counted.
fn bounded_member(field: Field) -> Field {
    let mut remaining = MAX_MEMBER_ALTERNATIVES;
    if field_fits(&field, &mut remaining) {
        return field;
    }
    let _bound =
        tracing::debug_span!(target: "helm_schema_ir::counters", "merge_member_bound").entered();
    Field::Unknown
}

fn field_fits(field: &Field, remaining: &mut usize) -> bool {
    match field {
        Field::Absent | Field::Unknown => take_one(remaining),
        Field::Present(value) => value_fits(value, remaining),
        Field::Select {
            decision: _,
            yes,
            no,
        } => field_fits(yes, remaining) && field_fits(no, remaining),
    }
}

fn value_fits(value: &Value, remaining: &mut usize) -> bool {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => take_one(remaining),
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => value_fits(yes, remaining) && value_fits(no, remaining),
    }
}

fn take_one(remaining: &mut usize) -> bool {
    if let Some(left) = remaining.checked_sub(1) {
        *remaining = left;
        true
    } else {
        false
    }
}

impl State {
    /// The objects a layered remainder's unknown keys may alias: the objects its merge sources
    /// and collapsed bases reach, and the allocated input objects under the inputs they read.
    /// `None` when that reach is unbounded.
    pub(super) fn layer_aliases(&self, layers: &Layers) -> Option<BTreeSet<ObjectId>> {
        let mut objects = BTreeSet::new();
        let mut prefixes = Vec::new();
        let Layers { base, merges } = layers;
        match base {
            Base::Closed | Base::Input | Base::Copy(_) => {}
            Base::Unknown(inputs, aliases) => {
                match aliases {
                    super::Reach::Top => return None,
                    super::Reach::Bounded(ids) => objects.extend(ids.iter().cloned()),
                }
                prefixes.extend(inputs.iter().cloned());
            }
        }
        let mut pending: Vec<&Value> = merges.iter().map(|layer| &layer.source.value).collect();
        while let Some(value) = pending.pop() {
            match value.node.as_ref() {
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => pending.extend([yes, no]),
                ValueNode::Leaf(Leaf { datum, origin }) => match datum {
                    Datum::Map {
                        view: _,
                        target: Some(id),
                    } => {
                        objects.insert(id.clone());
                    }
                    Datum::Unknown(UnknownValue { types: _, aliases }) => {
                        if let Some(path) = &origin.identity {
                            prefixes.push(path.clone());
                        } else {
                            match aliases {
                                super::Reach::Top => return None,
                                super::Reach::Bounded(ids) => objects.extend(ids.iter().cloned()),
                            }
                        }
                    }
                    Datum::Map {
                        view: _,
                        target: None,
                    }
                    | Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    } => {}
                },
            }
        }
        for (path, backing) in self.heap.arena.input_objects.borrow().iter() {
            if self.heap.version.has_allocated_object(&backing.object)
                && prefixes
                    .iter()
                    .any(|prefix| path == prefix || path.is_descendant_of(prefix))
            {
                objects.insert(backing.object.clone());
            }
        }
        Some(objects)
    }
}
