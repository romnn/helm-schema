use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ValueKind;
use crate::bound_value_analysis::{GetBinding, extract_bound_values};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperFragmentOutputUse, HelperOutputMeta, extend_type_hints,
};
use crate::value_path_context::ValuePathContext;

pub(crate) struct OutputValueAnalysis {
    pub(crate) default_fallback_values: BTreeSet<String>,
    pub(crate) values: BTreeSet<String>,
    pub(crate) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) bound_values: Vec<String>,
    pub(crate) helper_output_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) helper_fragment_output_values: Vec<String>,
    pub(crate) helper_fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) helper_dependency_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) helper_guard_values: BTreeSet<String>,
    pub(crate) helper_type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) suppress_direct_values: BTreeSet<String>,
    pub(crate) chart_value_defaults: BTreeSet<String>,
}

impl OutputValueAnalysis {
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
            && self.bound_values.is_empty()
            && self.helper_output_values.is_empty()
            && self.helper_fragment_output_values.is_empty()
            && self.helper_fragment_output_uses.is_empty()
            && self.helper_dependency_values.is_empty()
            && self.helper_guard_values.is_empty()
            && self.helper_type_hints.is_empty()
    }
}

pub(crate) fn collect_output_value_analysis(
    text: &str,
    kind: ValueKind,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    helper_analysis: Option<BoundHelperAnalysis>,
) -> OutputValueAnalysis {
    let default_fallback_values = value_path_context.resolved_default_fallback_paths(text);
    let mut values: BTreeSet<String> = value_path_context
        .resolved_values_paths(text)
        .into_iter()
        .collect();
    let local_output_meta = value_path_context.local_alias_output_meta_for_text(text);
    values.extend(default_fallback_values.iter().cloned());

    let bound_values = extract_bound_values(text, range_domains, get_bindings);

    let mut helper_output_values = BTreeMap::new();
    let mut helper_fragment_output_values = Vec::new();
    let mut helper_fragment_output_uses = Vec::new();
    let mut helper_dependency_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    let mut helper_guard_values = BTreeSet::new();
    let mut helper_type_hints = BTreeMap::new();
    let mut suppress_direct_values = BTreeSet::new();
    let mut chart_value_defaults = BTreeSet::new();

    if let Some(bound) = helper_analysis {
        helper_output_values.extend(bound.output);
        helper_fragment_output_uses.extend(bound.fragment_output_uses);
        for path in bound.dependency_paths {
            helper_dependency_values.entry(path).or_default();
        }
        for (path, meta) in bound.dependency_meta {
            let entry = helper_dependency_values.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        if kind == ValueKind::Fragment {
            helper_fragment_output_values.extend(bound.fragment_output);
        }
        helper_guard_values.extend(bound.guard_paths);
        extend_type_hints(&mut helper_type_hints, bound.type_hints);
        suppress_direct_values.extend(bound.suppress_roots);
        chart_value_defaults.extend(bound.chart_defaults);
        helper_fragment_output_values.sort();
        helper_fragment_output_values.dedup();
    }

    OutputValueAnalysis {
        default_fallback_values,
        values,
        local_output_meta,
        bound_values,
        helper_output_values,
        helper_fragment_output_values,
        helper_fragment_output_uses,
        helper_dependency_values,
        helper_guard_values,
        helper_type_hints,
        suppress_direct_values,
        chart_value_defaults,
    }
}
