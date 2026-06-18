use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::ValueKind;
use crate::bound_value_analysis::{GetBinding, extract_bound_values};
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
use crate::value_path_context::ValuePathContext;

pub(crate) struct DocumentValueAnalysis {
    pub(super) default_fallback_values: BTreeSet<String>,
    pub(super) values: BTreeSet<String>,
    pub(super) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(super) bound_values: Vec<String>,
    pub(super) helper: DocumentHelperSummary,
}

#[derive(Default)]
pub(super) struct DocumentHelperSummary {
    pub(crate) output_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) fragment_output_values: Vec<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) dependency_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_values: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) suppress_direct_values: BTreeSet<String>,
    pub(crate) chart_value_defaults: BTreeSet<String>,
}

impl DocumentHelperSummary {
    fn from_helper_summary(summary: HelperSummary, output_kind: ValueKind) -> Self {
        let mut dependency_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for path in summary.dependency_paths {
            dependency_values.entry(path).or_default();
        }
        for (path, meta) in summary.dependency_meta {
            dependency_values.entry(path).or_default().merge(meta);
        }

        let mut fragment_output_values = Vec::new();
        if output_kind == ValueKind::Fragment {
            fragment_output_values.extend(summary.fragment_output);
            fragment_output_values.sort();
            fragment_output_values.dedup();
        }

        Self {
            output_values: summary.output,
            fragment_output_values,
            fragment_output_uses: summary.fragment_output_uses,
            dependency_values,
            guard_values: summary.guard_paths,
            type_hints: summary.type_hints,
            suppress_direct_values: summary.suppress_roots,
            chart_value_defaults: summary.chart_defaults,
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

pub(crate) fn collect_document_value_analysis_from_exprs(
    text: &str,
    exprs: &[TemplateExpr],
    kind: ValueKind,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    helper_summary: Option<HelperSummary>,
) -> DocumentValueAnalysis {
    let default_fallback_values =
        value_path_context.resolved_default_fallback_paths_in_exprs(exprs);
    let mut values: BTreeSet<String> = value_path_context
        .resolved_values_paths_in_exprs(exprs)
        .into_iter()
        .collect();
    let local_output_meta = value_path_context.local_alias_output_meta_for_exprs(exprs);
    values.extend(default_fallback_values.iter().cloned());

    let bound_values = extract_bound_values(text, range_domains, get_bindings);

    let helper = helper_summary
        .map(|summary| DocumentHelperSummary::from_helper_summary(summary, kind))
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

    use super::DocumentHelperSummary;
    use crate::ValueKind;
    use crate::helper_summary::{HelperOutputMeta, HelperSummary};
    use crate::predicate::Predicate;

    #[test]
    fn document_helper_summary_preserves_helper_summary_fields() {
        let mut summary = HelperSummary::default();
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::truthy_path("enabled".to_string())]),
            defaulted: true,
            provenance: Vec::new(),
        };
        summary.add_output_meta("image.tag".to_string(), meta.clone());
        summary.fragment_output.insert("extraEnv".to_string());
        summary.dependency_paths.insert("global".to_string());
        summary
            .dependency_meta
            .insert("global.image.tag".to_string(), meta.clone());
        summary.guard_paths.insert("service.enabled".to_string());
        summary
            .type_hints
            .entry("image.tag".to_string())
            .or_default()
            .insert("string".to_string());
        summary.suppress_roots.insert("image".to_string());
        summary.chart_defaults.insert("nameOverride".to_string());

        let helper = DocumentHelperSummary::from_helper_summary(summary, ValueKind::Fragment);

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
