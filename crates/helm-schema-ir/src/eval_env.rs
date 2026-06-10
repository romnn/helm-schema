use std::collections::HashMap;

use crate::abstract_value::AbstractValue;
use crate::binding::HelperBinding;

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) locals: HashMap<String, AbstractValue>,
}

impl EvalEnv {
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
        }
    }
}
