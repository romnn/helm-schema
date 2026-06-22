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
    let (_path, facts) = summary
        .path_facts()
        .find(|(path, _facts)| *path == "signoz.serviceAccount.name")
        .expect("service account name output metadata");
    let meta = facts
        .output_meta()
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
        have: facts.type_hints(),
        want: &["string".to_string()].into_iter().collect(),
        "defaulted scalar output should retain string type hint"
    );
    assert!(
        summary
            .path_facts()
            .all(|(_path, facts)| !facts.has_fragment_output_uses())
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
    let (_repository_path, repository_facts) = summary
        .path_facts()
        .find(|(path, _facts)| *path == "image.repository")
        .expect("repository type hint");
    let (_tag_path, tag_facts) = summary
        .path_facts()
        .find(|(path, _facts)| *path == "image.tag")
        .expect("tag type hint");

    sim_assert_eq!(
        have: repository_facts.type_hints(),
        want: &BTreeSet::from(["string".to_string()])
    );
    sim_assert_eq!(
        have: tag_facts.type_hints(),
        want: &BTreeSet::from(["string".to_string()])
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
    let outputs = summary
        .path_facts()
        .flat_map(|(_path, facts)| facts.fragment_output_uses().cloned())
        .collect::<Vec<_>>();

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
