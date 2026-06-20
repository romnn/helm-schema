use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::ValueKind;
use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_value_from_expr_with_fragment_locals, values_for_helper_arg_with_fragment_locals,
};
use crate::helper_fragment_output_uses::collect_bound_fragment_output_uses_from_tree;
use crate::helper_summary::HelperSummary;
use crate::helper_value_analysis::collect_bound_helper_values_from_tree;
use crate::helper_walk_state::{FragmentOutputWalkState, HelperValuesWalkState};
use crate::{ContractProvenance, SourceSpan};

pub(crate) struct BoundHelperCallResolution {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) helper_body_dot: Option<AbstractValue>,
    pub(crate) helper_fragment_dot: Option<AbstractValue>,
}

pub(crate) struct ResolveBoundHelperCallParams<'a, 'context> {
    pub(crate) helper_name: &'a str,
    pub(crate) arg: Option<&'a TemplateExpr>,
    pub(crate) outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    pub(crate) current_dot: Option<&'a AbstractValue>,
    pub(crate) fragment_locals: &'a HashMap<String, AbstractValue>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'a HashSet<String>,
}

pub(crate) fn resolve_bound_helper_call(
    params: ResolveBoundHelperCallParams<'_, '_>,
) -> BoundHelperCallResolution {
    let mut binding_seen = params.seen.clone();
    let mut bindings = values_for_helper_arg_with_fragment_locals(
        params.arg,
        params.outer_bindings,
        params.current_dot,
        params.fragment_locals,
        params.context,
        &mut binding_seen,
    );

    let mut dot_seen = params.seen.clone();
    let mut helper_body_dot = params
        .arg
        .and_then(|expr| {
            helper_value_from_expr_with_fragment_locals(
                expr,
                params.fragment_locals,
                params.outer_bindings,
                params.current_dot,
                params.context,
                &mut dot_seen,
            )
        })
        .or_else(|| params.current_dot.cloned());

    let mut helper_fragment_dot = params.arg.and_then(|expr| {
        context_value_from_outer_expr(
            expr,
            Some(params.fragment_locals),
            params.outer_bindings,
            params.current_dot,
        )
    });

    if helper_uses_large_config_arg(params.helper_name) {
        if let Some(binding) = bindings.remove("config") {
            bindings.insert("config".to_string(), abstract_config_binding(binding));
        }
        helper_body_dot = helper_body_dot.map(abstract_config_entry_in_binding);
        helper_fragment_dot = helper_fragment_dot.map(abstract_config_entry_in_binding);
    }

    BoundHelperCallResolution {
        bindings,
        helper_body_dot,
        helper_fragment_dot,
    }
}

fn helper_uses_large_config_arg(name: &str) -> bool {
    name.starts_with("opentelemetry-collector.apply")
}

fn abstract_config_binding(binding: AbstractValue) -> AbstractValue {
    let paths = binding.paths();
    if paths.is_empty() {
        AbstractValue::Top
    } else {
        AbstractValue::PathSet(paths)
    }
}

fn abstract_config_entry_in_binding(binding: AbstractValue) -> AbstractValue {
    match binding {
        AbstractValue::Dict(mut entries) => {
            if let Some(config) = entries.remove("config") {
                entries.insert("config".to_string(), abstract_config_binding(config));
            }
            AbstractValue::Dict(entries)
        }
        other => other,
    }
}

#[tracing::instrument(skip_all, fields(helper = name))]
pub(crate) fn interpret_bound_helper_body(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    let mut analysis = HelperSummary::default();
    collect_value_facts(name, resolution, context, seen, &mut analysis);

    let mut helper_fragment_locals = HashMap::new();
    collect_fragment_output_uses(
        name,
        resolution,
        context,
        seen,
        &mut helper_fragment_locals,
        &mut analysis,
    );
    attach_helper_body_provenance(name, context, &mut analysis);

    analysis
}

fn attach_helper_body_provenance(
    name: &str,
    context: FragmentEvalContext<'_>,
    analysis: &mut HelperSummary,
) {
    let Some(source_path) = context.define_bodies.source_path(name) else {
        return;
    };
    let Some(body_offset) = context.define_bodies.body_offset(name) else {
        return;
    };
    let Some(source) = context.define_bodies.source(name) else {
        return;
    };
    let provenance = ContractProvenance::new(
        source_path,
        SourceSpan::new(body_offset, body_offset + source.len()),
        vec![name.to_string()],
    );

    analysis.add_provenance_to_outputs(provenance.clone());
    for mut output in analysis.take_fragment_output_uses() {
        output.meta.add_provenance_site(provenance.clone());
        analysis.add_fragment_output_use(output);
    }
    analysis.add_provenance_to_dependencies(provenance);
}

fn collect_value_facts(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    analysis: &mut HelperSummary,
) {
    let (Some(src), Some(tree)) = (
        context.define_bodies.source(name),
        context.define_bodies.tree(name),
    ) else {
        return;
    };

    let mut local_bindings = HashMap::new();
    let mut local_default_paths = HashMap::new();
    let mut local_output_meta = HashMap::new();
    let mut helper_values_state = HelperValuesWalkState {
        local_bindings: &mut local_bindings,
        local_default_paths: &mut local_default_paths,
        local_output_meta: &mut local_output_meta,
        context,
        seen,
        analysis,
    };
    collect_bound_helper_values_from_tree(
        tree.root_node(),
        src,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        &mut helper_values_state,
    );
}

fn collect_fragment_output_uses(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    helper_fragment_locals: &mut HashMap<String, AbstractValue>,
    analysis: &mut HelperSummary,
) {
    let (Some(src), Some(tree)) = (
        context.define_bodies.source(name),
        context.define_bodies.tree(name),
    ) else {
        return;
    };

    let mut fragment_output_uses = Vec::new();
    let mut local_default_paths = HashMap::new();
    let mut fragment_output_state = FragmentOutputWalkState {
        local_bindings: helper_fragment_locals,
        local_default_paths: &mut local_default_paths,
        context,
        seen,
        outputs: &mut fragment_output_uses,
    };
    collect_bound_fragment_output_uses_from_tree(
        &tree,
        src,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        resolution.helper_fragment_dot.as_ref(),
        &mut fragment_output_state,
    );
    fragment_output_uses
        .retain(|output| output.kind == ValueKind::Fragment || !output.relative_path.0.is_empty());
    let structured_sources: BTreeSet<String> = fragment_output_uses
        .iter()
        .filter(|output| output.kind == ValueKind::Fragment || !output.relative_path.0.is_empty())
        .map(|output| output.source_expr.clone())
        .collect();
    for source in &structured_sources {
        analysis.remove_output_path(source);
    }
    for output in fragment_output_uses {
        analysis.add_fragment_output_use(output);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap, HashSet};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::{DefineIndex, TreeSitterParser};

    use crate::Guard;
    use crate::define_body_cache::DefineBodyCache;
    use crate::fragment_expr_eval::FragmentEvalContext;
    use crate::helper_summary::HelperSummaryCache;

    use super::{BoundHelperCallResolution, interpret_bound_helper_body};

    #[test]
    fn helper_body_summary_preserves_if_else_output_predicates() {
        let source = r#"
            {{- define "serviceAccountName" -}}
            {{- if .Values.signoz.serviceAccount.create -}}
              {{ default (include "fullname" .) .Values.signoz.serviceAccount.name }}
            {{- else -}}
              {{ default "default" .Values.signoz.serviceAccount.name }}
            {{- end -}}
            {{- end -}}
        "#;
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::new(),
            helper_body_dot: None,
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary =
            interpret_bound_helper_body("serviceAccountName", &resolution, context, &mut seen);
        let output_meta = summary.output_path_meta();
        let meta = output_meta
            .get("signoz.serviceAccount.name")
            .expect("service account name output metadata");
        let guard_sets = meta.contract_guard_sets("signoz.serviceAccount.name");

        assert!(
            guard_sets.contains(&vec![
                Guard::Truthy {
                    path: "signoz.serviceAccount.create".to_string(),
                },
                Guard::Default {
                    path: "signoz.serviceAccount.name".to_string(),
                },
            ]),
            "expected create=true output branch; guard_sets={guard_sets:#?}"
        );
        assert!(
            guard_sets.contains(&vec![
                Guard::Not {
                    path: "signoz.serviceAccount.create".to_string(),
                },
                Guard::Default {
                    path: "signoz.serviceAccount.name".to_string(),
                },
            ]),
            "expected create=false output branch; guard_sets={guard_sets:#?}"
        );
        sim_assert_eq!(
            have: summary.type_hints().get("signoz.serviceAccount.name"),
            want: Some(&["string".to_string()].into_iter().collect()),
            "defaulted scalar output should retain string type hint"
        );
        assert!(summary.fragment_output_uses().is_empty());
    }

    #[test]
    fn helper_body_summary_resolves_string_hints_through_local_aliases() {
        let source = r#"
            {{- define "image" -}}
            {{- $repositoryName := .imageRoot.repository -}}
            {{- $tag := .imageRoot.tag | toString -}}
            {{- printf "%s:%s" $repositoryName $tag -}}
            {{- end -}}
        "#;
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::from([(
                "imageRoot".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("image".to_string()),
            )]),
            helper_body_dot: None,
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary = interpret_bound_helper_body("image", &resolution, context, &mut seen);

        sim_assert_eq!(
            have: summary.type_hints().get("image.repository"),
            want: Some(&BTreeSet::from(["string".to_string()]))
        );
        sim_assert_eq!(
            have: summary.type_hints().get("image.tag"),
            want: Some(&BTreeSet::from(["string".to_string()]))
        );
    }

    #[test]
    fn storage_class_helper_projects_storage_class_name_relative_path() {
        let source = include_str!(
            "../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates/_storage.tpl"
        );
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::from([
                (
                    "persistence".to_string(),
                    crate::abstract_value::AbstractValue::ValuesPath("persistence".to_string()),
                ),
                (
                    "global".to_string(),
                    crate::abstract_value::AbstractValue::ValuesPath("global".to_string()),
                ),
            ]),
            helper_body_dot: Some(crate::abstract_value::AbstractValue::Dict(
                [
                    (
                        "persistence".to_string(),
                        crate::abstract_value::AbstractValue::ValuesPath("persistence".to_string()),
                    ),
                    (
                        "global".to_string(),
                        crate::abstract_value::AbstractValue::ValuesPath("global".to_string()),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary =
            interpret_bound_helper_body("common.storage.class", &resolution, context, &mut seen);
        let outputs = summary.fragment_output_uses();

        assert!(
            outputs.iter().any(|output| {
                output.source_expr == "global.storageClass"
                    && output.relative_path.0 == ["storageClassName".to_string()]
            }),
            "expected global.storageClass to project to storageClassName, got {outputs:#?}"
        );
        assert!(
            outputs.iter().any(|output| {
                output.source_expr == "persistence.storageClass"
                    && output.relative_path.0 == ["storageClassName".to_string()]
            }),
            "expected persistence.storageClass to project to storageClassName, got {outputs:#?}"
        );
    }
}
