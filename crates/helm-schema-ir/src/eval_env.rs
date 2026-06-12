use std::collections::{BTreeMap, HashMap};

use crate::abstract_value::AbstractValue;
use crate::binding::{FragmentBinding, HelperBinding};

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) locals: HashMap<String, AbstractValue>,
    pub(crate) allow_field_root_lookup: bool,
    local_scopes: Vec<LocalScopeFrame>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LocalScopeFrame {
    previous_values: HashMap<String, Option<AbstractValue>>,
}

impl EvalEnv {
    #[cfg(test)]
    pub(crate) fn from_root_fields(root_fields: HashMap<String, AbstractValue>) -> Self {
        Self {
            root_fields,
            allow_field_root_lookup: true,
            ..Self::default()
        }
    }

    pub(crate) fn from_helper_context(
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Self {
        Self {
            dot: current_dot.map(AbstractValue::from_helper_binding),
            root_fields: bindings
                .map(|bindings| {
                    bindings
                        .iter()
                        .map(|(name, binding)| {
                            (name.clone(), AbstractValue::from_helper_binding(binding))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            locals: HashMap::new(),
            allow_field_root_lookup: true,
            local_scopes: Vec::new(),
        }
    }

    pub(crate) fn from_helper_context_with_fragment_locals(
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
    ) -> Self {
        let mut env = Self::from_helper_context(bindings, current_dot);
        env.locals = fragment_locals
            .iter()
            .map(|(name, binding)| (name.clone(), AbstractValue::from_fragment_binding(binding)))
            .collect();
        env
    }

    pub(crate) fn from_fragment_context(
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
    ) -> Self {
        let locals: HashMap<String, AbstractValue> = locals
            .iter()
            .map(|(name, binding)| (name.clone(), AbstractValue::from_fragment_binding(binding)))
            .collect();
        Self {
            dot: current_dot.map(AbstractValue::from_fragment_binding),
            root_fields: locals.clone(),
            locals,
            allow_field_root_lookup: false,
            local_scopes: Vec::new(),
        }
    }

    pub(crate) fn enter_local_scope(&mut self) {
        self.local_scopes.push(LocalScopeFrame::default());
    }

    pub(crate) fn exit_local_scope(&mut self) {
        let Some(scope) = self.local_scopes.pop() else {
            return;
        };
        for (name, previous_value) in scope.previous_values {
            self.set_local(name, previous_value);
        }
    }

    pub(crate) fn declare_local(&mut self, name: &str, value: Option<AbstractValue>) {
        self.record_scope_shadow(name);
        self.set_local(name.to_string(), value);
    }

    pub(crate) fn assign_local(&mut self, name: &str, value: Option<AbstractValue>) {
        if self.locals.contains_key(name) || self.local_scopes.is_empty() {
            self.set_local(name.to_string(), value);
        } else {
            self.declare_local(name, value);
        }
    }

    pub(crate) fn apply_local_set_mutations(
        &mut self,
        mutations: &BTreeMap<String, BTreeMap<String, AbstractValue>>,
    ) -> bool {
        let mut applied = false;
        for (name, entries) in mutations {
            let Some(value) = self.locals.remove(name) else {
                continue;
            };
            self.locals
                .insert(name.clone(), value.with_overlay_entries(entries.clone()));
            applied = true;
        }
        applied
    }

    pub(crate) fn join_branch_outcomes(entry: &Self, branch_outcomes: Vec<Self>) -> Self {
        if branch_outcomes.is_empty() {
            return entry.clone();
        }

        let mut joined = entry.clone();
        joined.locals.clear();
        let mut names: Vec<_> = entry.locals.keys().cloned().collect();
        for outcome in &branch_outcomes {
            for name in outcome.locals.keys() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names.sort();

        for name in names {
            let mut values = Vec::new();
            let mut present_in_all_branches = true;
            for outcome in &branch_outcomes {
                if let Some(value) = outcome.locals.get(&name) {
                    values.push(value.clone());
                } else {
                    present_in_all_branches = false;
                }
            }

            if present_in_all_branches {
                if let Some(value) = AbstractValue::choice(values) {
                    joined.locals.insert(name, value);
                }
            }
        }

        joined
    }

    fn record_scope_shadow(&mut self, name: &str) {
        let Some(scope) = self.local_scopes.last_mut() else {
            return;
        };
        scope
            .previous_values
            .entry(name.to_string())
            .or_insert_with(|| self.locals.get(name).cloned());
    }

    fn set_local(&mut self, name: String, value: Option<AbstractValue>) {
        if let Some(value) = value {
            self.locals.insert(name, value);
        } else {
            self.locals.remove(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EvalEnv;
    use crate::abstract_value::AbstractValue;

    fn path(path: &str) -> AbstractValue {
        AbstractValue::ValuesPath(path.to_string())
    }

    #[test]
    fn local_scope_restores_shadowed_declaration() {
        let mut env = EvalEnv::default();
        env.declare_local("name", Some(path("outer")));

        env.enter_local_scope();
        env.declare_local("name", Some(path("inner")));
        assert_eq!(env.locals.get("name"), Some(&path("inner")));

        env.exit_local_scope();
        assert_eq!(env.locals.get("name"), Some(&path("outer")));
    }

    #[test]
    fn local_scope_keeps_assignment_to_outer_binding() {
        let mut env = EvalEnv::default();
        env.declare_local("name", Some(path("outer")));

        env.enter_local_scope();
        env.assign_local("name", Some(path("assigned")));
        env.exit_local_scope();

        assert_eq!(env.locals.get("name"), Some(&path("assigned")));
    }

    #[test]
    fn local_scope_does_not_leak_undefined_assignment() {
        let mut env = EvalEnv::default();

        env.enter_local_scope();
        env.assign_local("name", Some(path("inner")));
        assert_eq!(env.locals.get("name"), Some(&path("inner")));
        env.exit_local_scope();

        assert!(!env.locals.contains_key("name"));
    }

    #[test]
    fn branch_join_unions_values_present_in_all_outcomes() {
        let entry = EvalEnv::default();
        let mut first = entry.clone();
        first.declare_local("name", Some(path("first")));
        let mut second = entry.clone();
        second.declare_local("name", Some(path("second")));

        let joined = EvalEnv::join_branch_outcomes(&entry, vec![first, second]);

        let expected = AbstractValue::choice(vec![path("first"), path("second")]);
        assert_eq!(joined.locals.get("name"), expected.as_ref());
    }

    #[test]
    fn branch_join_drops_binding_absent_from_one_outcome() {
        let entry = EvalEnv::default();
        let mut first = entry.clone();
        first.declare_local("name", Some(path("first")));
        let second = entry.clone();

        let joined = EvalEnv::join_branch_outcomes(&entry, vec![first, second]);

        assert!(!joined.locals.contains_key("name"));
    }

    #[test]
    fn branch_join_drops_entry_binding_absent_from_one_outcome() {
        let mut entry = EvalEnv::default();
        entry.declare_local("name", Some(path("entry")));
        let mut first = entry.clone();
        first.assign_local("name", Some(path("first")));
        let mut second = entry.clone();
        second.assign_local("name", None);

        let joined = EvalEnv::join_branch_outcomes(&entry, vec![first, second]);

        assert!(!joined.locals.contains_key("name"));
    }
}
