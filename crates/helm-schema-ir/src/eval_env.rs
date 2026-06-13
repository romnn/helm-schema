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

    pub(crate) fn from_outer_fragment_expr_context(
        fragment_locals: Option<&HashMap<String, FragmentBinding>>,
        root_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Self {
        let root_fields = root_bindings
            .map(|bindings| {
                bindings
                    .iter()
                    .map(|(name, binding)| {
                        (name.clone(), AbstractValue::from_helper_binding(binding))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let locals = fragment_locals
            .map(|locals| {
                locals
                    .iter()
                    .map(|(name, binding)| {
                        (name.clone(), AbstractValue::from_fragment_binding(binding))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let dot = root_bindings
            .map(|bindings| {
                AbstractValue::Dict(
                    bindings
                        .iter()
                        .map(|(name, binding)| {
                            (name.clone(), AbstractValue::from_helper_binding(binding))
                        })
                        .collect(),
                )
            })
            .or_else(|| current_dot.map(AbstractValue::from_helper_binding));
        Self {
            dot,
            root_fields,
            locals,
            allow_field_root_lookup: true,
        }
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
}
