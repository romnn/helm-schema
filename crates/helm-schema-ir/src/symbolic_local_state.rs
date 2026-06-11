use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binding::FragmentBinding;
use crate::bound_value_analysis::GetBinding;
use crate::helper_analysis::HelperOutputMeta;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicLocalState {
    pub(crate) range_domains: HashMap<String, Vec<String>>,
    pub(crate) get_bindings: HashMap<String, GetBinding>,
    pub(crate) fragment_bindings: HashMap<String, FragmentBinding>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Values paths defaulted by structural `set X "K" (X.K | default V)`
    /// helper mutations that have already run in source order.
    pub(crate) chart_value_defaults: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SymbolicLocalStateSnapshot {
    state: SymbolicLocalState,
}

impl SymbolicLocalState {
    pub(crate) fn snapshot(&self) -> SymbolicLocalStateSnapshot {
        SymbolicLocalStateSnapshot {
            state: self.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: SymbolicLocalStateSnapshot) {
        *self = snapshot.state;
    }

    pub(crate) fn insert_get_binding(&mut self, variable: String, binding: GetBinding) {
        self.get_bindings.insert(variable, binding);
    }

    pub(crate) fn declare_fragment_binding(&mut self, variable: String, binding: FragmentBinding) {
        self.fragment_bindings.insert(variable, binding);
    }

    pub(crate) fn assign_fragment_binding(&mut self, variable: String, binding: FragmentBinding) {
        self.fragment_bindings.insert(variable, binding);
    }

    pub(crate) fn set_default_paths(&mut self, variable: &str, paths: BTreeSet<String>) {
        if paths.is_empty() {
            self.default_paths.remove(variable);
        } else {
            self.default_paths.insert(variable.to_string(), paths);
        }
    }

    pub(crate) fn set_output_meta(
        &mut self,
        variable: String,
        meta: BTreeMap<String, HelperOutputMeta>,
    ) {
        if meta.is_empty() {
            self.output_meta.remove(&variable);
        } else {
            self.output_meta.insert(variable, meta);
        }
    }

    pub(crate) fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.range_domains.insert(variable, literals);
    }

    pub(crate) fn set_chart_value_defaults(&mut self, defaults: BTreeSet<String>) {
        self.chart_value_defaults = defaults;
    }

    pub(crate) fn append_chart_value_defaults(&mut self, defaults: &mut BTreeSet<String>) {
        self.chart_value_defaults.append(defaults);
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolicLocalState;
    use crate::binding::FragmentBinding;

    #[test]
    fn snapshot_restore_replaces_all_local_state_maps() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "image".to_string(),
            FragmentBinding::ValuesPath("image".to_string()),
        );
        state
            .chart_value_defaults
            .insert("serviceAccount.name".to_string());
        let snapshot = state.snapshot();

        state.declare_fragment_binding(
            "image".to_string(),
            FragmentBinding::ValuesPath("otherImage".to_string()),
        );
        state.insert_range_domain("key".to_string(), vec!["a".to_string()]);
        state
            .chart_value_defaults
            .insert("serviceAccount.labels".to_string());

        state.restore(snapshot);

        assert_eq!(
            state.fragment_bindings.get("image"),
            Some(&FragmentBinding::ValuesPath("image".to_string()))
        );
        assert!(state.range_domains.is_empty());
        assert_eq!(
            state.chart_value_defaults,
            ["serviceAccount.name".to_string()].into_iter().collect()
        );
    }
}
