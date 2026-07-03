use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::helper_summary::HelperOutputMeta;

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) locals: HashMap<String, AbstractValue>,
    pub(crate) local_default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) bound_values: BoundValueContext,
    pub(crate) allow_field_root_lookup: bool,
    pub(crate) skip_helper_call_args: bool,
}

impl EvalEnv {
    pub(crate) fn from_helper_context(
        bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            root_fields: bindings.cloned().unwrap_or_default(),
            allow_field_root_lookup: true,
            ..Self::default()
        }
    }

    pub(crate) fn from_fragment_context(
        locals: &HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            root_fields: locals.clone(),
            locals: locals.clone(),
            allow_field_root_lookup: false,
            ..Self::default()
        }
    }

    pub(crate) fn without_helper_call_args(mut self) -> Self {
        self.skip_helper_call_args = true;
        self
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
}
