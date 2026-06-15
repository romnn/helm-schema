use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ValueKind;
use crate::bound_value_analysis::{GetBinding, extract_bound_values};
use crate::helper_analysis::{BoundHelperAnalysis, HelperFragmentOutputUse, HelperOutputMeta};
use crate::value_path_context::ValuePathContext;

pub(crate) struct DocumentValueAnalysis {
    pub(super) default_fallback_values: BTreeSet<String>,
    pub(super) values: BTreeSet<String>,
    pub(super) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(super) bound_values: Vec<String>,
    pub(super) helper: DocumentHelperValueAnalysis,
}

#[derive(Default)]
pub(super) struct DocumentHelperValueAnalysis {
    pub(crate) output_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) fragment_output_values: Vec<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) dependency_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_values: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) suppress_direct_values: BTreeSet<String>,
    pub(crate) chart_value_defaults: BTreeSet<String>,
}

impl DocumentHelperValueAnalysis {
    fn from_bound_helper(bound: BoundHelperAnalysis, output_kind: ValueKind) -> Self {
        let mut dependency_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for path in bound.dependency_paths {
            dependency_values.entry(path).or_default();
        }
        for (path, meta) in bound.dependency_meta {
            dependency_values.entry(path).or_default().merge(meta);
        }

        let mut fragment_output_values = Vec::new();
        if output_kind == ValueKind::Fragment {
            fragment_output_values.extend(bound.fragment_output);
            fragment_output_values.sort();
            fragment_output_values.dedup();
        }

        Self {
            output_values: bound.output,
            fragment_output_values,
            fragment_output_uses: bound.fragment_output_uses,
            dependency_values,
            guard_values: bound.guard_paths,
            type_hints: bound.type_hints,
            suppress_direct_values: bound.suppress_roots,
            chart_value_defaults: bound.chart_defaults,
        }
    }

    fn is_empty(&self) -> bool {
        self.output_values.is_empty()
            && self.fragment_output_values.is_empty()
            && self.fragment_output_uses.is_empty()
            && self.dependency_values.is_empty()
            && self.guard_values.is_empty()
            && self.type_hints.is_empty()
    }
}

impl DocumentValueAnalysis {
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty() && self.bound_values.is_empty() && self.helper.is_empty()
    }

    pub(crate) fn take_chart_value_defaults(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.helper.chart_value_defaults)
    }
}

pub(crate) fn collect_document_value_analysis(
    text: &str,
    kind: ValueKind,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    helper_analysis: Option<BoundHelperAnalysis>,
) -> DocumentValueAnalysis {
    let default_fallback_values = value_path_context.resolved_default_fallback_paths(text);
    let mut values: BTreeSet<String> = value_path_context
        .resolved_values_paths(text)
        .into_iter()
        .collect();
    let local_output_meta = value_path_context.local_alias_output_meta_for_text(text);
    values.extend(default_fallback_values.iter().cloned());

    let bound_values = extract_bound_values(text, range_domains, get_bindings);

    let helper = helper_analysis
        .map(|bound| DocumentHelperValueAnalysis::from_bound_helper(bound, kind))
        .unwrap_or_default();

    DocumentValueAnalysis {
        default_fallback_values,
        values,
        local_output_meta,
        bound_values,
        helper,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::DocumentHelperValueAnalysis;
    use crate::ValueKind;
    use crate::helper_analysis::{BoundHelperAnalysis, HelperOutputMeta};
    use crate::predicate::Predicate;

    #[test]
    fn document_helper_analysis_preserves_bound_helper_fields() {
        let mut analysis = BoundHelperAnalysis::default();
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::truthy_path("enabled".to_string())]),
            defaulted: true,
        };
        analysis.add_output_meta("image.tag".to_string(), meta.clone());
        analysis.fragment_output.insert("extraEnv".to_string());
        analysis.dependency_paths.insert("global".to_string());
        analysis
            .dependency_meta
            .insert("global.image.tag".to_string(), meta.clone());
        analysis.guard_paths.insert("service.enabled".to_string());
        analysis
            .type_hints
            .entry("image.tag".to_string())
            .or_default()
            .insert("string".to_string());
        analysis.suppress_roots.insert("image".to_string());
        analysis.chart_defaults.insert("nameOverride".to_string());

        let helper = DocumentHelperValueAnalysis::from_bound_helper(analysis, ValueKind::Fragment);

        assert_eq!(
            helper.output_values,
            BTreeMap::from([("image.tag".to_string(), meta.clone())])
        );
        assert_eq!(helper.fragment_output_values, vec!["extraEnv".to_string()]);
        assert_eq!(
            helper.dependency_values,
            BTreeMap::from([
                ("global".to_string(), HelperOutputMeta::default()),
                ("global.image.tag".to_string(), meta),
            ])
        );
        assert_eq!(
            helper.guard_values,
            BTreeSet::from(["service.enabled".to_string()])
        );
        assert_eq!(
            helper.type_hints,
            BTreeMap::from([(
                "image.tag".to_string(),
                BTreeSet::from(["string".to_string()])
            )])
        );
        assert_eq!(
            helper.suppress_direct_values,
            BTreeSet::from(["image".to_string()])
        );
        assert_eq!(
            helper.chart_value_defaults,
            BTreeSet::from(["nameOverride".to_string()])
        );
    }
}
