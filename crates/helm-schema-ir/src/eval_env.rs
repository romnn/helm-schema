use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::helper_summary::HelperOutputMeta;

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) locals: HashMap<String, AbstractValue>,
    pub(crate) local_default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
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
            root_fields: bindings
                .map(|bindings| {
                    bindings
                        .iter()
                        .map(|(name, binding)| (name.clone(), binding.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            locals: HashMap::new(),
            local_default_paths: HashMap::new(),
            local_output_meta: HashMap::new(),
            allow_field_root_lookup: true,
            skip_helper_call_args: false,
        }
    }

    pub(crate) fn from_helper_context_with_fragment_locals(
        bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        fragment_locals: &HashMap<String, AbstractValue>,
    ) -> Self {
        let mut env = Self::from_helper_context(bindings, current_dot);
        env.locals = fragment_locals
            .iter()
            .map(|(name, binding)| (name.clone(), binding.clone()))
            .collect();
        env
    }

    pub(crate) fn from_outer_fragment_expr_context(
        fragment_locals: Option<&HashMap<String, AbstractValue>>,
        root_bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        let root_fields = root_bindings
            .map(|bindings| {
                bindings
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let locals = fragment_locals
            .map(|locals| {
                locals
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let dot = root_bindings
            .map(|bindings| {
                AbstractValue::Dict(
                    bindings
                        .iter()
                        .map(|(name, binding)| (name.clone(), binding.clone()))
                        .collect(),
                )
            })
            .or_else(|| current_dot.cloned());
        Self {
            dot,
            root_fields,
            locals,
            local_default_paths: HashMap::new(),
            local_output_meta: HashMap::new(),
            allow_field_root_lookup: true,
            skip_helper_call_args: false,
        }
    }

    pub(crate) fn from_fragment_context(
        locals: &HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        let locals: HashMap<String, AbstractValue> = locals
            .iter()
            .map(|(name, binding)| (name.clone(), binding.clone()))
            .collect();
        Self {
            dot: current_dot.cloned(),
            root_fields: locals.clone(),
            locals,
            local_default_paths: HashMap::new(),
            local_output_meta: HashMap::new(),
            allow_field_root_lookup: false,
            skip_helper_call_args: false,
        }
    }

    pub(crate) fn from_local_facts(
        locals: &HashMap<String, AbstractValue>,
        local_default_paths: &HashMap<String, BTreeSet<String>>,
        local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    ) -> Self {
        Self {
            locals: locals.clone(),
            skip_helper_call_args: true,
            ..Self::default()
        }
        .with_local_facts(local_default_paths, local_output_meta)
    }

    pub(crate) fn with_local_facts(
        mut self,
        local_default_paths: &HashMap<String, BTreeSet<String>>,
        local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    ) -> Self {
        self.local_default_paths = local_default_paths.clone();
        self.local_output_meta = local_output_meta.clone();
        self
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
