use std::collections::{BTreeSet, HashMap, HashSet};
use test_util::prelude::sim_assert_eq;

use helm_schema_ast::{DefineIndex, TreeSitterParser};

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
    defines
        .add_source(&TreeSitterParser, source)
        .expect("define source");
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&defines, &analysis_db);
    let resolution = BoundHelperCallResolution {
        bindings: HashMap::new(),
        helper_body_dot: None,
        helper_fragment_dot: None,
    };
    let mut seen = HashSet::new();

    let summary =
        interpret_bound_helper_body("serviceAccountName", &resolution, context, &mut seen);
    let output_meta = &summary.scalar_output_meta;
    let meta = output_meta
        .get("signoz.serviceAccount.name")
        .expect("service account name output metadata");
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
    assert!(summary.fragment_output_uses.is_empty());
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
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&defines, &analysis_db);
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
    defines
        .add_source(&TreeSitterParser, source)
        .expect("define source");
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = FragmentEvalContext::new(&defines, &analysis_db);
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
    let outputs = &summary.fragment_output_uses;

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
