use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::define_body_cache::DefineBodyCache;
use crate::fragment_assignment::{
    AssignmentKind, apply_local_set_mutations, parse_helper_assignment,
};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_range_scope::range_body_renders_mapping_entries_from_ast;
use crate::helper_summary::HelperSummaryCache;
use test_util::prelude::sim_assert_eq;

fn empty_context<'a>(
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
    helper_summaries: &'a HelperSummaryCache,
) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(defines, define_bodies, helper_summaries)
}

#[test]
fn parse_helper_assignment_detects_declaration_from_ast() {
    let Some(assignment) = parse_helper_assignment(r#"{{- $image := .Values.image.repository -}}"#)
    else {
        panic!("parse helper assignment");
    };

    sim_assert_eq!(assignment.variable, "image");
    sim_assert_eq!(assignment.kind, AssignmentKind::Declaration);
    sim_assert_eq!(assignment.rhs, ".Values.image.repository");
    sim_assert_eq!(
        assignment.rhs_expr,
        TemplateExpr::Field(vec![
            "Values".to_string(),
            "image".to_string(),
            "repository".to_string()
        ])
    );
}

#[test]
fn parse_helper_assignment_detects_assignment_from_ast() {
    let Some(assignment) = parse_helper_assignment(r#"{{- $image = .Values.global.image -}}"#)
    else {
        panic!("parse helper assignment");
    };

    sim_assert_eq!(assignment.variable, "image");
    sim_assert_eq!(assignment.kind, AssignmentKind::Assignment);
    sim_assert_eq!(assignment.rhs, ".Values.global.image");
    sim_assert_eq!(
        assignment.rhs_expr,
        TemplateExpr::Field(vec![
            "Values".to_string(),
            "global".to_string(),
            "image".to_string()
        ])
    );
}

#[test]
fn local_set_mutation_uses_shared_expression_eval_for_computed_key() {
    let mut locals = HashMap::from([(
        "config".to_string(),
        AbstractValue::Dict(BTreeMap::from([
            (
                "name".to_string(),
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            ),
            (
                "annotations".to_string(),
                AbstractValue::ValuesPath("serviceAccount.annotations".to_string()),
            ),
        ])),
    )]);
    let defines = DefineIndex::new();
    let define_bodies = DefineBodyCache::new(&defines);
    let helper_summaries = HelperSummaryCache::new();
    let context = empty_context(&defines, &define_bodies, &helper_summaries);
    let mut seen = HashSet::new();

    assert!(apply_local_set_mutations(
        r#"{{- $_ := set $config (printf "%s" "name") "generated" -}}"#,
        &mut locals,
        None,
        context,
        &mut seen,
    ));

    sim_assert_eq!(
        locals.get("config"),
        Some(&AbstractValue::Overlay {
            entries: BTreeMap::from([(
                "name".to_string(),
                AbstractValue::StringSet(BTreeSet::from(["generated".to_string()])),
            )]),
            fallback: Box::new(AbstractValue::Dict(BTreeMap::from([
                (
                    "name".to_string(),
                    AbstractValue::ValuesPath("serviceAccount.name".to_string()),
                ),
                (
                    "annotations".to_string(),
                    AbstractValue::ValuesPath("serviceAccount.annotations".to_string()),
                ),
            ]))),
        })
    );
}

#[test]
fn range_body_mapping_entry_detection_sees_dynamic_template_key() {
    let source = indoc::indoc! {r#"
        {{- range $key, $value := .Values.annotations }}
        {{ $key }}: {{ $value | quote }}
        {{- end }}
    "#};
    let tree = parse_raw_template_tree(source);
    let range = find_first_node(tree.root_node(), "range_action").expect("range action");

    assert!(range_body_renders_mapping_entries_from_ast(range, source));
}

#[test]
fn range_body_mapping_entry_detection_ignores_mutation_only_body() {
    let source = indoc::indoc! {r#"
        {{- range $key, $value := .Values.contexts }}
          {{- $_ := set $value "dir" (printf "/etc/%s" $key) }}
        {{- end }}
    "#};
    let tree = parse_raw_template_tree(source);
    let range = find_first_node(tree.root_node(), "range_action").expect("range action");

    assert!(!range_body_renders_mapping_entries_from_ast(range, source));
}

#[test]
fn range_body_mapping_entry_detection_ignores_sequence_item_mapping() {
    let source = indoc::indoc! {r#"
        {{- range $key, $value := .Values.containers }}
        - name: {{ $key }}
          image: {{ $value.image }}
        {{- end }}
    "#};
    let tree = parse_raw_template_tree(source);
    let range = find_first_node(tree.root_node(), "range_action").expect("range action");

    assert!(!range_body_renders_mapping_entries_from_ast(range, source));
}

#[test]
fn range_body_mapping_entry_detection_ignores_static_wrapper_mapping() {
    let source = indoc::indoc! {r#"
        {{- range $key, $value := .Values.annotations }}
        labels:
          {{ $key }}: {{ $value | quote }}
        {{- end }}
    "#};
    let tree = parse_raw_template_tree(source);
    let range = find_first_node(tree.root_node(), "range_action").expect("range action");

    assert!(!range_body_renders_mapping_entries_from_ast(range, source));
}

fn parse_raw_template_tree(source: &str) -> tree_sitter::Tree {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("set language");
    parser.parse(source, None).expect("parse template")
}

fn find_first_node<'tree>(
    node: tree_sitter::Node<'tree>,
    kind: &str,
) -> Option<tree_sitter::Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_node(child, kind) {
            return Some(found);
        }
    }
    None
}
