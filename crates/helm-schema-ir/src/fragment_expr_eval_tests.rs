use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr, TreeSitterParser, parse_action_expressions};

use crate::define_body_cache::DefineBodyCache;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_expr, fragment_binding_from_outer_expr,
    helper_binding_from_expr_with_fragment_locals,
};
use crate::helper_binding::HelperBinding;
use crate::helper_summary::HelperSummaryCache;

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn empty_context<'a>(
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
    helper_summaries: &'a HelperSummaryCache,
) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(defines, define_bodies, helper_summaries)
}

fn helper_binding_from_fragment_locals(
    action: &str,
    fragment_locals: &HashMap<String, FragmentBinding>,
) -> Option<HelperBinding> {
    let expr = single_expr(action);
    let defines = DefineIndex::new();
    let define_bodies = DefineBodyCache::new(&defines);
    let helper_summaries = HelperSummaryCache::new();
    let context = empty_context(&defines, &define_bodies, &helper_summaries);
    let mut seen = HashSet::new();
    helper_binding_from_expr_with_fragment_locals(
        &expr,
        fragment_locals,
        None,
        None,
        context,
        &mut seen,
    )
}

fn context_local() -> HashMap<String, FragmentBinding> {
    HashMap::from([(
        "ctx".to_string(),
        FragmentBinding::Dict(BTreeMap::from([(
            "config".to_string(),
            FragmentBinding::ValuesPath("serviceAccount".to_string()),
        )])),
    )])
}

fn helper_context<'a>(
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
    helper_summaries: &'a HelperSummaryCache,
) -> FragmentEvalContext<'a> {
    empty_context(defines, define_bodies, helper_summaries)
}

#[test]
fn outer_expr_bare_dot_uses_root_bindings_as_current_context() {
    let expr = single_expr(".");
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        HelperBinding::ValuesPath(String::new()),
    )]);

    assert_eq!(
        fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
        Some(FragmentBinding::Dict(BTreeMap::from([(
            "Values".to_string(),
            FragmentBinding::ValuesRoot,
        )])))
    );
}

#[test]
fn outer_expr_root_variable_uses_root_bindings_as_current_context() {
    let expr = single_expr("$");
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        HelperBinding::ValuesPath(String::new()),
    )]);

    assert_eq!(
        fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
        Some(FragmentBinding::Dict(BTreeMap::from([(
            "Values".to_string(),
            FragmentBinding::ValuesRoot,
        )])))
    );
}

#[test]
fn outer_expr_fragment_local_selector_uses_shared_expression_eval() {
    let expr = single_expr(r#"dict "name" $ctx.config.name"#);
    let fragment_locals = context_local();

    assert_eq!(
        fragment_binding_from_outer_expr(&expr, Some(&fragment_locals), None, None),
        Some(FragmentBinding::Dict(BTreeMap::from([(
            "name".to_string(),
            FragmentBinding::ValuesPath("serviceAccount.name".to_string()),
        )])))
    );
}

#[test]
fn helper_binding_fragment_local_selector_uses_shared_expression_eval() {
    let binding = helper_binding_from_fragment_locals(
        r#"$ctx.config.name | toYaml | fromYaml"#,
        &context_local(),
    );

    assert_eq!(
        binding,
        Some(HelperBinding::ValuesPath("serviceAccount.name".to_string()))
    );
}

#[test]
fn helper_binding_fragment_local_dict_uses_shared_expression_eval() {
    let binding =
        helper_binding_from_fragment_locals(r#"dict "name" $ctx.config.name"#, &context_local());

    assert_eq!(
        binding,
        Some(HelperBinding::Dict(BTreeMap::from([(
            "name".to_string(),
            HelperBinding::ValuesPath("serviceAccount.name".to_string()),
        )])))
    );
}

#[test]
fn helper_binding_fragment_local_index_uses_shared_expression_eval() {
    let binding =
        helper_binding_from_fragment_locals(r#"index $ctx.config "name""#, &context_local());

    assert_eq!(
        binding,
        Some(HelperBinding::ValuesPath("serviceAccount.name".to_string()))
    );
}

#[test]
fn bound_helper_call_uses_single_value_resolver_for_helper_projection() {
    let mut defines = DefineIndex::new();
    defines
        .add_source(
            &TreeSitterParser,
            r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
        )
        .expect("parse helper source");
    let define_bodies = DefineBodyCache::new(&defines);
    let helper_summaries = HelperSummaryCache::new();
    let context = helper_context(&defines, &define_bodies, &helper_summaries);
    let expr = single_expr(r#"include "common.name" ."#);
    let mut seen = HashSet::new();

    let Some(HelperBinding::OutputSet(output_set)) = helper_binding_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    ) else {
        panic!("expected helper projection to resolve to an output-set binding");
    };

    let meta = output_set
        .get("nameOverride")
        .expect("nameOverride output meta should be present");
    assert!(meta.predicates.is_empty());
    assert!(!meta.defaulted);
    assert!(
        meta.provenance.iter().any(|provenance| {
            provenance.template_path == "<inline:0>"
                && provenance.helper_chain == vec!["common.name".to_string()]
                && provenance.span.start < provenance.span.end
        }),
        "expected helper projection to retain helper-body provenance, got {meta:?}",
    );
}

#[test]
fn bound_helper_call_uses_single_value_resolver_for_fragment_projection() {
    let mut defines = DefineIndex::new();
    defines
        .add_source(
            &TreeSitterParser,
            r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
        )
        .expect("parse helper source");
    let define_bodies = DefineBodyCache::new(&defines);
    let helper_summaries = HelperSummaryCache::new();
    let context = helper_context(&defines, &define_bodies, &helper_summaries);
    let expr = single_expr(r#"include "common.name" ."#);
    let mut seen = HashSet::new();

    assert_eq!(
        fragment_binding_from_expr(&expr, &HashMap::new(), None, context, &mut seen),
        Some(FragmentBinding::OutputSet(
            ["nameOverride".to_string()].into_iter().collect()
        )),
    );
}
