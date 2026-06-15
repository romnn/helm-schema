use std::collections::BTreeSet;

use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_binding::HelperBinding;
use crate::output_path;

pub(crate) fn project_fragment_binding(
    mut analysis: BoundHelperAnalysis,
) -> Option<FragmentBinding> {
    let structured_sources = structured_fragment_sources(&analysis);
    let rendered_sources = rendered_sources(&analysis, &structured_sources);

    let mut bindings = Vec::new();
    if !analysis.string_output.is_empty() {
        bindings.push(FragmentBinding::StringSet(analysis.string_output.clone()));
    }
    for output in analysis.fragment_output_uses.drain(..) {
        bindings.push(FragmentBinding::for_output_path(
            output.source_expr,
            &output.relative_path,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
        }
    }
    for source in analysis.output.into_keys() {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
        }
    }
    FragmentBinding::merge_all(bindings)
}

pub(crate) fn project_helper_binding(mut analysis: BoundHelperAnalysis) -> Option<HelperBinding> {
    let structured_sources = structured_fragment_sources(&analysis);
    let rendered_sources = rendered_sources(&analysis, &structured_sources);

    let mut bindings = Vec::new();
    if !analysis.string_output.is_empty() {
        bindings.push(HelperBinding::StringSet(analysis.string_output.clone()));
    }
    for output in analysis.fragment_output_uses.drain(..) {
        bindings.push(HelperBinding::for_output_path(
            output.source_expr,
            &output.relative_path,
            output.meta,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(HelperBinding::PathSet([source].into_iter().collect()));
        }
    }
    for (source, meta) in analysis.output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(HelperBinding::OutputSet(
                [(source, meta)].into_iter().collect(),
            ));
        }
    }
    HelperBinding::merge_all(bindings)
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
}
