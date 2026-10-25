use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;
use helm_schema_core::{GuardValue, Predicate, ValuesPath};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{
    DomainConstraints, bound_selector_read, predicate_domain_constraints,
};
use crate::value_path_context::{guard_value_is_truthy, predicate_any};

use super::{LocalSlot, LocalSlotView, LocalSlots, RootSlot, ScalarContent, ScalarValueDispatch};

/// Slot owner shared by expression and fragment evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct State {
    pub(super) slots: LocalSlots,
    pub(crate) chart_value_defaults: BTreeSet<ValuesPath>,
    pub(super) scopes: Vec<HashMap<String, LocalSlot>>,
    pub(super) roots: HashMap<String, RootSlot>,
    pub(super) observed: BTreeMap<String, RootSlot>,
    pub(super) constraints: DomainConstraints,
}

/// Root-only snapshot: local scope restores must not rewind root writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RootSnapshot {
    root: HashMap<String, RootSlot>,
    pub(super) observed: BTreeMap<String, RootSlot>,
}

impl State {
    pub(crate) fn from_roots(roots: HashMap<String, RootSlot>) -> Self {
        Self {
            roots,
            ..Self::default()
        }
    }

    pub(crate) fn with_roots(mut self, roots: HashMap<String, RootSlot>) -> Self {
        self.roots = roots;
        self
    }

    /// R3a identifies snapshots by their complete immutable contents.
    pub(crate) fn snapshot_id(&self) -> &Self {
        self
    }

    pub(crate) fn fork(&self) -> Self {
        self.snapshot_id().clone()
    }

    pub(crate) fn fork_locals(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            chart_value_defaults: self.chart_value_defaults.clone(),
            scopes: self.scopes.clone(),
            ..Self::default()
        }
    }

    pub(crate) fn restore_locals(&mut self, state: Self) {
        let Self {
            slots,
            chart_value_defaults,
            scopes,
            roots: _,
            observed: _,
            constraints: _,
        } = state;
        self.slots = slots;
        self.chart_value_defaults = chart_value_defaults;
        self.scopes = scopes;
    }

    pub(crate) fn expression_state(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            roots: self.roots.clone(),
            ..Self::default()
        }
    }

    pub(crate) fn fork_roots(&self) -> RootSnapshot {
        RootSnapshot {
            root: self.roots.clone(),
            observed: self.observed.clone(),
        }
    }

    pub(crate) fn restore_roots(&mut self, state: &RootSnapshot) {
        self.roots = state.root.clone();
        self.observed = state.observed.clone();
    }

    pub(crate) fn read_local(&self, name: &str) -> LocalSlotView<'_> {
        self.slot(name).binding()
    }

    pub(crate) fn iter_locals(&self) -> impl Iterator<Item = (&String, &LocalSlot)> {
        self.slots.iter()
    }

    pub(crate) fn read_root(&self, name: &str) -> Option<&RootSlot> {
        self.roots.get(name)
    }

    pub(crate) fn roots(&self) -> &HashMap<String, RootSlot> {
        &self.roots
    }

    pub(crate) fn replace_roots(&mut self, roots: HashMap<String, RootSlot>) {
        self.roots = roots;
    }

    pub(crate) fn root_values(&self) -> HashMap<String, AbstractValue> {
        self.roots
            .iter()
            .filter_map(|(name, slot)| {
                let super::RootSlotView {
                    value,
                    truth: _,
                    dispatch: _,
                } = slot.binding();
                value.clone().map(|value| (name.clone(), value))
            })
            .collect()
    }

    pub(crate) fn root_writes(&self) -> &BTreeMap<String, RootSlot> {
        &self.observed
    }

    pub(crate) fn into_root_writes(self) -> BTreeMap<String, RootSlot> {
        self.observed
    }

    pub(crate) fn write_root(&mut self, name: String, slot: RootSlot) {
        *self.write_field(name.clone()) = slot.clone();
        self.observed.insert(name, slot);
    }

    pub(crate) fn write_field(&mut self, name: String) -> &mut RootSlot {
        self.roots.entry(name).or_default()
    }
    pub(crate) fn variable_slots(&self, name: &str) -> [crate::value::LocalSlotView<'_>; 2] {
        [
            self.read_local(name),
            self.read_local(name.trim_start_matches('$')),
        ]
    }

    pub(crate) fn selector_paths(&self, expr: &TemplateExpr) -> BTreeSet<ValuesPath> {
        let Some((variable, rest)) = bound_selector_read(expr) else {
            return BTreeSet::new();
        };
        let Some(binding) = self.read_local(variable).get_binding.as_ref() else {
            return BTreeSet::new();
        };
        let Some(domain) = self.read_local(&binding.key_var).range_domain.as_ref() else {
            return BTreeSet::new();
        };

        domain
            .iter()
            .filter(|value| self.constraints.allows(&binding.key_var, value))
            .map(|value| {
                let mut path = binding.base.clone();
                path.push(value.clone());
                let rest = ValuesPath::parse(&rest);
                ValuesPath::from_segments(path.segments().chain(rest.segments()))
            })
            .collect()
    }

    pub(crate) fn apply_predicate_constraints(&mut self, expr: &TemplateExpr, truthy: bool) {
        let Some(next_constraints) = predicate_domain_constraints(expr, truthy) else {
            return;
        };
        self.constraints = self.constraints.and(&next_constraints);
    }
    /// Join per-arm root `set` outcomes after an if/else region.
    ///
    /// The replay applies each arm's changed keys in source order, matching
    /// the last-write-wins accumulation the pipeline had before the per-arm
    /// entry restore. When the chain is COMPLETE — an unconditional else and
    /// every arm condition decoded without approximation — and every arm
    /// leaves a key holding one scalar string literal, the key additionally
    /// joins into an exact [`ScalarValueDispatch`]: root-field equalities
    /// decode as the disjunction of the arms assigning the compared literal,
    /// and the key's truthiness becomes the disjunction of the arms assigning
    /// a truthy literal.
    pub(crate) fn join_root_set_arms(
        &mut self,
        entry: &RootSnapshot,
        arms: &[(Predicate, bool, RootSnapshot)],
        has_unconditional_else: bool,
    ) {
        let mut keys: BTreeSet<String> = BTreeSet::new();
        for (_, _, state) in arms {
            for (key, value) in state
                .observed
                .iter()
                .filter_map(|(name, slot)| slot.binding().value.as_ref().map(|value| (name, value)))
            {
                if entry
                    .observed
                    .get(key)
                    .and_then(|slot| slot.binding().value.as_ref())
                    != Some(value)
                {
                    keys.insert(key.clone());
                }
            }
        }
        if keys.is_empty() {
            return;
        }
        for (_, _, state) in arms {
            for key in &keys {
                let Some(value) = state
                    .observed
                    .get(key)
                    .and_then(|slot| slot.binding().value.as_ref())
                else {
                    continue;
                };
                if entry
                    .observed
                    .get(key)
                    .and_then(|slot| slot.binding().value.as_ref())
                    == Some(value)
                {
                    continue;
                }
                if let Some(slot) = state.observed.get(key) {
                    self.write_root(key.clone(), slot.clone());
                }
            }
        }
        let complete = has_unconditional_else
            && arms
                .iter()
                .all(|(condition, decoded, _)| *decoded && !condition.contains_approximation());
        if !complete {
            return;
        }
        'keys: for key in &keys {
            let mut dispatch_arms = Vec::new();
            let mut joined_values: BTreeSet<AbstractValue> = BTreeSet::new();
            let mut truthy_conditions = Vec::new();
            for (condition, _, state) in arms {
                let value = state
                    .observed
                    .get(key)
                    .and_then(|slot| slot.binding().value.as_ref())
                    .or_else(|| {
                        entry
                            .observed
                            .get(key)
                            .and_then(|slot| slot.binding().value.as_ref())
                    });
                let Some(value) = value else {
                    continue 'keys;
                };
                let Some(literal) = root_dispatch_literal(value) else {
                    continue 'keys;
                };
                if guard_value_is_truthy(&literal) {
                    truthy_conditions.push(condition.clone());
                }
                dispatch_arms.push((condition.clone(), ScalarContent::Literal(literal)));
                joined_values.insert(value.clone());
            }
            let truthy = predicate_any(truthy_conditions);
            let joined_value = if joined_values.len() == 1 {
                joined_values
                    .into_iter()
                    .next()
                    .unwrap_or(AbstractValue::Unknown)
            } else {
                AbstractValue::Choice(joined_values)
            };
            let dispatch = ScalarValueDispatch::bind_content(dispatch_arms, true);
            let slot = RootSlot::new(Some(joined_value), Some(truthy), Some(dispatch));
            self.write_root(key.clone(), slot);
        }
    }
}

fn root_dispatch_literal(value: &AbstractValue) -> Option<GuardValue> {
    match value {
        AbstractValue::StringSet(values) if values.len() == 1 => {
            values.first().map(GuardValue::string)
        }
        _ => None,
    }
}
