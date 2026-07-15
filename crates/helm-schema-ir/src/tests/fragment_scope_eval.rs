use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_assignment::{
    AssignmentKind, ParsedHelperAssignment, apply_local_set_mutations_from_exprs,
    parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::FragmentEvalContext;
use helm_schema_ast::parse_expr_text;
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Debug, PartialEq)]
struct ParsedHelperAssignmentWithRhs {
    variable: String,
    kind: AssignmentKind,
    rhs: String,
    rhs_expr: TemplateExpr,
}

fn strip_template_action_wrapping(line: &str) -> Option<String> {
    let after_open = line.trim_start().strip_prefix("{{")?;
    let close_at = after_open.find("}}")?;
    let body = &after_open[..close_at];
    let body = body.strip_prefix('-').unwrap_or(body);
    let body = body.strip_suffix('-').unwrap_or(body);
    Some(body.trim().to_string())
}

fn assignment_rhs_text(text: &str, kind: AssignmentKind) -> Option<String> {
    let owned;
    let trimmed = if text.trim_start().starts_with("{{") {
        owned = strip_template_action_wrapping(text)?;
        owned.trim()
    } else {
        text.trim()
    };
    let (operator, operator_len) = match kind {
        AssignmentKind::Declaration => (":=", 2usize),
        AssignmentKind::Assignment => ("=", 1usize),
    };
    let index = trimmed.find(operator)?;
    Some(trimmed[index + operator_len..].trim().to_string())
}

fn parse_helper_assignment(text: &str) -> Option<ParsedHelperAssignmentWithRhs> {
    let ParsedHelperAssignment {
        variable,
        kind,
        rhs_expr,
    } = parse_helper_assignment_from_exprs(&parse_expr_text(text))?;
    Some(ParsedHelperAssignmentWithRhs {
        variable,
        kind,
        rhs: assignment_rhs_text(text, kind)?,
        rhs_expr,
    })
}

fn apply_local_set_mutations(
    text: &str,
    local_bindings: &mut HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> bool {
    apply_local_set_mutations_from_exprs(
        &parse_expr_text(text),
        local_bindings,
        current_dot,
        context,
        seen,
    )
}

fn empty_context<'a>(analysis_db: &'a IrAnalysisDb) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(analysis_db)
}

#[test]
fn parse_helper_assignment_detects_declaration_from_ast() {
    let Some(assignment) = parse_helper_assignment(r#"{{- $image := .Values.image.repository -}}"#)
    else {
        panic!("parse helper assignment");
    };

    sim_assert_eq!(have: assignment.variable, want: "image");
    sim_assert_eq!(have: assignment.kind, want: AssignmentKind::Declaration);
    sim_assert_eq!(have: assignment.rhs, want: ".Values.image.repository");
    sim_assert_eq!(
        have: assignment.rhs_expr,
        want: TemplateExpr::Field(vec![
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

    sim_assert_eq!(have: assignment.variable, want: "image");
    sim_assert_eq!(have: assignment.kind, want: AssignmentKind::Assignment);
    sim_assert_eq!(have: assignment.rhs, want: ".Values.global.image");
    sim_assert_eq!(
        have: assignment.rhs_expr,
        want: TemplateExpr::Field(vec![
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
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = empty_context(&analysis_db);
    let mut seen = HashSet::new();

    assert!(apply_local_set_mutations(
        r#"{{- $_ := set $config (printf "%s" "name") "generated" -}}"#,
        &mut locals,
        None,
        context,
        &mut seen,
    ));

    sim_assert_eq!(
        have: locals.get("config"),
        want: Some(&AbstractValue::Overlay {
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
fn local_set_mutation_resolves_computed_key_from_literal_local() {
    let mut locals = HashMap::from([
        (
            "patch".to_string(),
            AbstractValue::JsonDecodedPath("patch.*".to_string()),
        ),
        (
            "opPathKey".to_string(),
            AbstractValue::StringSet(BTreeSet::from(["path".to_string()])),
        ),
    ]);
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = empty_context(&analysis_db);
    let mut seen = HashSet::new();

    assert!(apply_local_set_mutations(
        r#"{{- $_ := set $patch (printf "%sKey" $opPathKey) "derived" -}}"#,
        &mut locals,
        None,
        context,
        &mut seen,
    ));

    sim_assert_eq!(
        have: locals
            .get("patch")
            .and_then(|patch| patch.apply_to_path(&["pathKey".to_string()])),
        want: Some(AbstractValue::StringSet(BTreeSet::from([
            "derived".to_string()
        ])))
    );
}
