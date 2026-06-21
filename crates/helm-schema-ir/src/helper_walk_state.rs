use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_assignment::merge_fragment_locals;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HelperRuntimeLocals {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
}

impl HelperRuntimeLocals {
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.bindings = merge_fragment_locals(self.bindings, other.bindings);
        self.default_paths = merge_default_paths(self.default_paths, other.default_paths);
        self
    }
}

pub(crate) struct HelperValuesWalkState<'context, 'state> {
    pub(crate) locals: &'state mut HelperRuntimeLocals,
    pub(crate) local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut HelperSummary,
}

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) locals: &'state mut HelperRuntimeLocals,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

fn merge_default_paths(
    mut base: HashMap<String, BTreeSet<String>>,
    other: HashMap<String, BTreeSet<String>>,
) -> HashMap<String, BTreeSet<String>> {
    base.retain(|key, base_paths| {
        let Some(other_paths) = other.get(key) else {
            return false;
        };
        base_paths.extend(other_paths.iter().cloned());
        true
    });
    base
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use test_util::prelude::sim_assert_eq;

    use super::HelperRuntimeLocals;
    use crate::abstract_value::AbstractValue;

    #[test]
    fn merge_intersects_default_paths_by_branch_presence() {
        let base = HelperRuntimeLocals {
            bindings: HashMap::new(),
            default_paths: HashMap::from([
                (
                    "serviceAccount".to_string(),
                    BTreeSet::from(["left.default".to_string()]),
                ),
                (
                    "leftOnly".to_string(),
                    BTreeSet::from(["left.only".to_string()]),
                ),
            ]),
        };
        let other = HelperRuntimeLocals {
            bindings: HashMap::new(),
            default_paths: HashMap::from([
                (
                    "serviceAccount".to_string(),
                    BTreeSet::from(["right.default".to_string()]),
                ),
                (
                    "rightOnly".to_string(),
                    BTreeSet::from(["right.only".to_string()]),
                ),
            ]),
        };

        let merged = base.merge(other);

        sim_assert_eq!(
            have: merged.default_paths,
            want: HashMap::from([(
                "serviceAccount".to_string(),
                BTreeSet::from(["left.default".to_string(), "right.default".to_string()])
            )])
        );
    }

    #[test]
    fn merge_unions_fragment_local_bindings() {
        let base = HelperRuntimeLocals {
            bindings: HashMap::from([(
                "config".to_string(),
                AbstractValue::ValuesPath("left".to_string()),
            )]),
            default_paths: HashMap::new(),
        };
        let other = HelperRuntimeLocals {
            bindings: HashMap::from([(
                "config".to_string(),
                AbstractValue::ValuesPath("right".to_string()),
            )]),
            default_paths: HashMap::new(),
        };

        let merged = base.merge(other);

        sim_assert_eq!(
            have: merged.bindings.get("config").cloned(),
            want: Some(AbstractValue::Choice(BTreeSet::from([
                AbstractValue::ValuesPath("left".to_string()),
                AbstractValue::ValuesPath("right".to_string())
            ])))
        );
    }
}
