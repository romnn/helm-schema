use std::collections::{BTreeSet, HashMap, HashSet};
use test_util::prelude::sim_assert_eq;

use helm_schema_ast::DefineIndex;

use crate::Guard;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_expr_eval::FragmentEvalContext;

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
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::new(),
        helper_body_dot: None,
        helper_fragment_dot: None,
    };
    let mut seen = HashSet::new();

    let summary =
        interpret_bound_helper_body("serviceAccountName", &resolution, context, &mut seen);
    let output = summary
        .output_uses
        .iter()
        .find(|output| output.source_expr == "signoz.serviceAccount.name")
        .expect("service account name output metadata");
    let meta = &output.meta;
    let type_hints = &summary.type_hints;
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
    let string_type_hint = BTreeSet::from(["string".to_string()]);
    sim_assert_eq!(
        have: type_hints.get("signoz.serviceAccount.name"),
        want: Some(&string_type_hint),
        "defaulted scalar output should retain string type hint"
    );
    assert!(
        summary
            .output_uses
            .iter()
            .all(|output| output.is_scalar_summary_output())
    );
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
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
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
    let type_hints = &summary.type_hints;

    sim_assert_eq!(
        have: type_hints.get("image.repository"),
        want: Some(&BTreeSet::from(["string".to_string()]))
    );
    sim_assert_eq!(
        have: type_hints.get("image.tag"),
        want: Some(&BTreeSet::from(["string".to_string()]))
    );
}

#[test]
fn storage_class_helper_projects_storage_class_name_relative_path() {
    let source = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates/_storage.tpl"
    );
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
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
    let outputs = &summary.output_uses;

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

#[test]
fn image_helper_combines_assignment_and_render_branch_guards() {
    let source = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates/_images.tpl"
    );
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::from([
            (
                "imageRoot".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("image".to_string()),
            ),
            (
                "global".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("global".to_string()),
            ),
        ]),
        helper_body_dot: Some(crate::abstract_value::AbstractValue::Dict(
            [
                (
                    "imageRoot".to_string(),
                    crate::abstract_value::AbstractValue::ValuesPath("image".to_string()),
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
        interpret_bound_helper_body("common.images.image", &resolution, context, &mut seen);
    let global_registry = summary
        .output_uses
        .iter()
        .find(|output| output.source_expr == "global.imageRegistry" && output.is_rendered())
        .expect("global image registry output");
    let guard_sets = global_registry
        .meta
        .contract_guard_sets("global.imageRegistry");

    assert!(
        guard_sets.contains(&vec![
            Guard::Truthy {
                path: "global".to_string(),
            },
            Guard::Truthy {
                path: "global.imageRegistry".to_string(),
            },
            Guard::Truthy {
                path: "image.registry".to_string(),
            },
        ]),
        "expected assignment and render guards to combine; guard_sets={guard_sets:#?}"
    );
}

#[test]
fn labels_standard_keeps_name_override_independent_from_custom_labels_guard() {
    let source = [
        include_str!("../../../../testdata/charts/common/templates/_names.tpl"),
        include_str!("../../../../testdata/charts/common/templates/_labels.tpl"),
        include_str!("../../../../testdata/charts/common/templates/_tplvalues.tpl"),
    ]
    .join("\n");
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", &source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let dot = crate::abstract_value::AbstractValue::Dict(
        [
            (
                "customLabels".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("commonLabels".to_string()),
            ),
            (
                "context".to_string(),
                crate::abstract_value::AbstractValue::RootContext,
            ),
        ]
        .into_iter()
        .collect(),
    );
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::from([
            (
                "customLabels".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("commonLabels".to_string()),
            ),
            (
                "context".to_string(),
                crate::abstract_value::AbstractValue::RootContext,
            ),
        ]),
        helper_body_dot: Some(dot.clone()),
        helper_fragment_dot: Some(dot),
    };
    let mut seen = HashSet::new();

    let summary =
        interpret_bound_helper_body("common.labels.standard", &resolution, context, &mut seen);
    let mut saw_default_branch = false;
    let mut saw_name_override = false;
    for output in summary
        .output_uses
        .iter()
        .filter(|output| output.source_expr == "nameOverride" && output.relative_path.0.is_empty())
    {
        saw_name_override = true;
        let guard_sets = output.meta.contract_guard_sets("nameOverride");
        assert!(
            guard_sets.iter().all(|guards| !guards
                .iter()
                .any(|guard| matches!(guard, Guard::Truthy { path } if path == "commonLabels"))),
            "customLabels should not guard the independent common.names.name output; output={output:#?}; guard_sets={guard_sets:#?}"
        );
        saw_default_branch |= guard_sets.contains(&vec![
            Guard::Truthy {
                path: "nameOverride".to_string(),
            },
            Guard::Default {
                path: "nameOverride".to_string(),
            },
        ]);
    }

    assert!(saw_name_override, "expected pathless nameOverride output");
    assert!(
        saw_default_branch,
        "expected nameOverride's own default/truthy branch; outputs={:#?}",
        summary.output_uses
    );
}

#[test]
fn nested_helper_assignment_preserves_branch_guards_for_dependencies() {
    let source = include_str!("../../../../testdata/charts/nats/templates/_helpers.tpl");
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::new(),
        helper_body_dot: None,
        helper_fragment_dot: None,
    };
    let mut seen = HashSet::new();

    let summary =
        interpret_bound_helper_body("nats.defaultValues", &resolution, context, &mut seen);
    let guard_sets = summary
        .output_uses
        .iter()
        .filter(|output| output.source_expr == "fullnameOverride" && output.is_dependency())
        .flat_map(|output| output.meta.contract_guard_sets("fullnameOverride"))
        .collect::<Vec<_>>();

    assert!(
        !guard_sets.iter().any(Vec::is_empty),
        "fullnameOverride dependency must stay branch guarded; outputs={:#?}",
        summary.output_uses
    );
    assert!(
        guard_sets.contains(&vec![Guard::Truthy {
            path: "fullnameOverride".to_string(),
        }]),
        "expected fullnameOverride truthy dependency guard; guard_sets={guard_sets:#?}"
    );
}

#[test]
fn key_selector_helper_outputs_key_strings_not_selected_values() {
    let source = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates/_utils.tpl"
    );
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", source);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::from([
            (
                "keys".to_string(),
                crate::abstract_value::AbstractValue::List(vec![
                    crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                        "global.postgresql.auth.password".to_string(),
                    ])),
                    crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                        "auth.password".to_string(),
                    ])),
                ]),
            ),
            (
                "context".to_string(),
                crate::abstract_value::AbstractValue::RootContext,
            ),
        ]),
        helper_body_dot: Some(crate::abstract_value::AbstractValue::Dict(
            [
                (
                    "keys".to_string(),
                    crate::abstract_value::AbstractValue::List(vec![
                        crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                            "global.postgresql.auth.password".to_string(),
                        ])),
                        crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                            "auth.password".to_string(),
                        ])),
                    ]),
                ),
                (
                    "context".to_string(),
                    crate::abstract_value::AbstractValue::RootContext,
                ),
            ]
            .into_iter()
            .collect(),
        )),
        helper_fragment_dot: None,
    };
    let mut seen = HashSet::new();

    let summary = interpret_bound_helper_body(
        "common.utils.getKeyFromList",
        &resolution,
        context,
        &mut seen,
    );
    let rendered_sources = summary
        .output_uses
        .iter()
        .filter(|output| output.is_rendered())
        .map(|output| output.source_expr.as_str())
        .collect::<BTreeSet<_>>();

    sim_assert_eq!(
        have: summary.string_output,
        want: BTreeSet::from([
            "auth.password".to_string(),
            "global.postgresql.auth.password".to_string(),
        ])
    );
    sim_assert_eq!(have: rendered_sources, want: BTreeSet::new());
}

#[test]
fn passwords_manage_preserves_selected_value_branch_predicates() {
    let utils = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates/_utils.tpl"
    );
    let secrets = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates/_secrets.tpl"
    );
    let names = include_str!(
        "../../../../testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates/_names.tpl"
    );
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:utils>", utils);
    defines.add_file_source("<inline:secrets>", secrets);
    defines.add_file_source("<inline:names>", names);
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::from([
            (
                "providedValues".to_string(),
                crate::abstract_value::AbstractValue::List(vec![
                    crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                        "global.postgresql.auth.password".to_string(),
                    ])),
                    crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                        "auth.password".to_string(),
                    ])),
                ]),
            ),
            (
                "context".to_string(),
                crate::abstract_value::AbstractValue::RootContext,
            ),
            (
                "secret".to_string(),
                crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                    "postgresql".to_string()
                ])),
            ),
            (
                "key".to_string(),
                crate::abstract_value::AbstractValue::StringSet(BTreeSet::from([
                    "password".to_string()
                ])),
            ),
        ]),
        helper_body_dot: None,
        helper_fragment_dot: None,
    };
    let mut seen = HashSet::new();

    let summary = interpret_bound_helper_body(
        "common.secrets.passwords.manage",
        &resolution,
        context,
        &mut seen,
    );
    let auth_password = summary
        .output_uses
        .iter()
        .find(|output| output.source_expr == "auth.password" && output.is_rendered())
        .expect("rendered auth.password output");
    let guard_sets = auth_password.meta.contract_guard_sets("auth.password");

    assert!(
        guard_sets.contains(&vec![
            Guard::Truthy {
                path: "auth.password".to_string(),
            },
            Guard::Truthy {
                path: "global.postgresql.auth.password".to_string(),
            },
        ]),
        "selected value output should retain the branch that proved the selected password exists; guard_sets={guard_sets:#?}"
    );
}
