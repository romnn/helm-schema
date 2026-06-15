use std::collections::BTreeSet;

use crate::abstract_value::AbstractValue;
use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_binding::HelperBinding;
use crate::output_path;

pub(crate) fn helper_to_fragment_binding(binding: &HelperBinding) -> FragmentBinding {
    AbstractValue::from_helper_binding(binding)
        .to_fragment_binding()
        .unwrap_or(FragmentBinding::Unknown)
}

pub(crate) fn helper_strings(binding: &HelperBinding) -> BTreeSet<String> {
    AbstractValue::from_helper_binding(binding).strings()
}

pub(crate) fn helper_item_binding(binding: &HelperBinding) -> Option<HelperBinding> {
    AbstractValue::from_helper_binding(binding)
        .helper_range_item()
        .and_then(|value| value.to_helper_binding())
}

pub(crate) fn helper_definitely_nonempty_iterable(binding: &HelperBinding) -> bool {
    AbstractValue::from_helper_binding(binding).definitely_nonempty_iterable()
}

pub(crate) fn project_fragment_binding(analysis: BoundHelperAnalysis) -> Option<FragmentBinding> {
    project_binding_value(analysis, ProjectionTarget::Fragment)
        .and_then(|value| value.to_fragment_binding())
        .and_then(|binding| FragmentBinding::merge_all(vec![binding]))
}

pub(crate) fn project_helper_binding(analysis: BoundHelperAnalysis) -> Option<HelperBinding> {
    project_binding_value(analysis, ProjectionTarget::Helper)
        .and_then(|value| value.to_helper_binding())
}

#[derive(Clone, Copy)]
enum ProjectionTarget {
    Helper,
    Fragment,
}

fn project_binding_value(
    analysis: BoundHelperAnalysis,
    target: ProjectionTarget,
) -> Option<AbstractValue> {
    let structured_sources = structured_fragment_sources(&analysis);
    let rendered_sources = rendered_sources(&analysis, &structured_sources);

    let mut values = Vec::new();
    if !analysis.string_output.is_empty() {
        values.push(AbstractValue::StringSet(analysis.string_output));
    }
    for output in analysis.fragment_output_uses {
        values.push(AbstractValue::for_output_path(
            output.source_expr,
            &output.relative_path,
            output.meta,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            values.push(fragment_output_value(source, target));
        }
    }
    for (source, meta) in analysis.output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            values.push(AbstractValue::OutputSet(
                [(source, meta)].into_iter().collect(),
            ));
        }
    }
    AbstractValue::merge_all(values)
}

fn fragment_output_value(source: String, target: ProjectionTarget) -> AbstractValue {
    match target {
        ProjectionTarget::Helper => AbstractValue::PathSet([source].into_iter().collect()),
        ProjectionTarget::Fragment => {
            AbstractValue::OutputSet([(source, Default::default())].into_iter().collect())
        }
    }
}

fn structured_fragment_sources(analysis: &BoundHelperAnalysis) -> BTreeSet<String> {
    analysis
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect()
}

fn rendered_sources(
    analysis: &BoundHelperAnalysis,
    structured_sources: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut rendered_sources = structured_sources.clone();
    rendered_sources.extend(analysis.fragment_output.iter().cloned());
    rendered_sources.extend(analysis.output.keys().cloned());
    rendered_sources
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{project_fragment_binding, project_helper_binding};
    use crate::fragment_binding::FragmentBinding;
    use crate::helper_analysis::{BoundHelperAnalysis, HelperOutputMeta};
    use crate::helper_binding::HelperBinding;
    use crate::predicate::Predicate;
    use crate::{ValueKind, YamlPath};

    #[test]
    fn helper_binding_projection_preserves_structured_output_metadata() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::truthy_path("enabled".to_string())]),
            defaulted: true,
        };
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            meta.clone(),
        );

        assert_eq!(
            project_helper_binding(analysis),
            Some(HelperBinding::Dict(BTreeMap::from([(
                "app".to_string(),
                HelperBinding::OutputSet(BTreeMap::from([("podLabels".to_string(), meta)])),
            )])))
        );
    }

    #[test]
    fn fragment_binding_projection_preserves_structured_output_path() {
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        );

        assert_eq!(
            project_fragment_binding(analysis),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "app".to_string(),
                FragmentBinding::OutputSet(BTreeSet::from(["podLabels".to_string()])),
            )])))
        );
    }

    #[test]
    fn fragment_binding_projection_merges_scalar_outputs_into_one_output_set() {
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_output_meta("image.repository".to_string(), HelperOutputMeta::default());
        analysis.add_output_meta("image.tag".to_string(), HelperOutputMeta::default());
        analysis.fragment_output.insert("extraEnv".to_string());

        assert_eq!(
            project_fragment_binding(analysis),
            Some(FragmentBinding::OutputSet(BTreeSet::from([
                "extraEnv".to_string(),
                "image.repository".to_string(),
                "image.tag".to_string(),
            ])))
        );
    }
}
