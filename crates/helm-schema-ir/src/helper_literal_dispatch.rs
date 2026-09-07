//! Literal-dispatch helper analysis: a helper whose body is ONE
//! `if`/`else if`/`else` chain rendering only static text per arm (the
//! oauth2-proxy `legacy-config.mode` shape). Conditions comparing the
//! helper's output against a literal (`eq (include "mode" .) "x"`)
//! decode into the matching arms' branch conditions.

use helm_schema_ast::{TemplateExpr, TemplateHeader, children_with_field};

use crate::analysis_db::IrAnalysisDb;
use crate::node_eval::{NodeAction, else_if_pairs, node_action};

/// One ordered arm; `header: None` is the `else` arm. A chain without an
/// `else` gets an implicit empty-literal arm (the helper renders nothing
/// when no condition holds).
pub(crate) struct LiteralDispatchArm {
    pub(crate) header: Option<TemplateHeader>,
    pub(crate) literal: String,
    /// Whether the arm collected NO content at all. `literal` is trimmed (an
    /// approximation of the chain's trim markers), so an empty `literal`
    /// alone cannot distinguish "renders nothing" from "renders only
    /// whitespace" — which differ under Helm truthiness.
    pub(crate) raw_empty: bool,
}

pub(crate) fn helper_literal_dispatch(
    db: &IrAnalysisDb,
    name: &str,
) -> Option<Vec<LiteralDispatchArm>> {
    let body = db.parsed_helper_body(name)?;
    let mut dispatch = None;
    if !collect_dispatch(body.source, body.tree.root_node(), &mut dispatch) {
        return None;
    }
    dispatch
}

/// Whitespace and suppressed nodes may surround the single dispatch `if`;
/// anything else means the helper's output is not a pure literal dispatch.
fn collect_dispatch(
    source: &str,
    node: tree_sitter::Node<'_>,
    dispatch: &mut Option<Vec<LiteralDispatchArm>>,
) -> bool {
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        match node_action(source, child) {
            NodeAction::Text => {
                if child
                    .utf8_text(source.as_bytes())
                    .is_ok_and(|text| !text.trim().is_empty())
                {
                    return false;
                }
            }
            NodeAction::Suppressed => {}
            NodeAction::If(header) => {
                if dispatch.is_some() {
                    return false;
                }
                let Some(arms) = dispatch_arms(source, child, header) else {
                    return false;
                };
                *dispatch = Some(arms);
            }
            NodeAction::Descend => {
                if !collect_dispatch(source, child, dispatch) {
                    return false;
                }
            }
            // A body that is ONE boolean-valued expression renders exactly
            // `true` or `false`, which is the two-arm dispatch
            // `if <expr>` → "true" / else → "false" (oauth2-proxy's
            // `redis.enabled` helper is `eq (index .Values "redis-ha"
            // "enabled") true`).
            NodeAction::Output(exprs) => {
                if dispatch.is_some() {
                    return false;
                }
                let Some(arms) = boolean_output_arms(source, child, exprs.as_deref()) else {
                    return false;
                };
                *dispatch = Some(arms);
            }
            NodeAction::Assignment(_) | NodeAction::With(_) | NodeAction::Range(_) => {
                return false;
            }
        }
    }
    true
}

/// The synthetic two-arm dispatch for a single boolean-valued output body.
/// The header re-parses the expression's own source text, so the arm's
/// condition is exactly the rendered test.
fn boolean_output_arms(
    source: &str,
    node: tree_sitter::Node<'_>,
    exprs: Option<&[TemplateExpr]>,
) -> Option<Vec<LiteralDispatchArm>> {
    let [expr] = exprs? else {
        return None;
    };
    if !boolean_valued(expr) {
        return None;
    }
    let raw = node.utf8_text(source.as_bytes()).ok()?;
    Some(vec![
        LiteralDispatchArm {
            header: Some(TemplateHeader::parse_control(raw.trim())),
            literal: "true".to_string(),
            raw_empty: false,
        },
        LiteralDispatchArm {
            header: None,
            literal: "false".to_string(),
            raw_empty: false,
        },
    ])
}

/// Whether the expression's VALUE is a Go/Sprig boolean, so its `%v`
/// rendering is exactly `true`/`false`. `and`/`or` return one of their
/// arguments rather than a coerced bool, hence the recursive requirement.
fn boolean_valued(expr: &TemplateExpr) -> bool {
    use helm_schema_ast::Literal;
    match expr.deparen() {
        TemplateExpr::Literal(Literal::Bool(_)) => true,
        TemplateExpr::Call { function, args } => match function.as_str() {
            "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "not" | "hasKey" | "has" | "contains"
            | "empty" | "kindIs" | "typeIs" | "regexMatch" | "mustRegexMatch" | "hasPrefix"
            | "hasSuffix" => true,
            "and" | "or" => !args.is_empty() && args.iter().all(boolean_valued),
            _ => false,
        },
        _ => false,
    }
}

fn dispatch_arms(
    source: &str,
    node: tree_sitter::Node<'_>,
    header: Option<TemplateHeader>,
) -> Option<Vec<LiteralDispatchArm>> {
    // An unparsable `if` or `else if` header disqualifies the chain: the
    // arm negations below would be built over the wrong conditions.
    let mut arms = vec![(Some(header?), children_with_field(node, "consequence"))];
    for (arm_header, children) in else_if_pairs(node, source) {
        arms.push((Some(arm_header?), children));
    }
    arms.push((None, children_with_field(node, "alternative")));

    let mut out = Vec::new();
    for (arm_header, children) in arms {
        let mut literal = String::new();
        for child in children {
            // Trim-marker delimiter tokens surface as named children of
            // the consequence; they render nothing.
            if matches!(child.kind(), "{{" | "{{-" | "}}" | "-}}") {
                continue;
            }
            match node_action(source, child) {
                NodeAction::Text => {
                    literal.push_str(child.utf8_text(source.as_bytes()).ok()?);
                }
                NodeAction::Suppressed => {}
                // A bare literal output (`{{- true -}}`, `{{ "text" }}`)
                // renders static text just like a text node (redis'
                // `createConfigmap` gate spells its `true` this way).
                NodeAction::Output(exprs) => {
                    let [expr] = exprs.as_deref()? else {
                        return None;
                    };
                    literal.push_str(&literal_output_text(expr)?);
                }
                _ => return None,
            }
        }
        out.push(LiteralDispatchArm {
            header: arm_header,
            raw_empty: literal.is_empty(),
            literal: literal.trim().to_string(),
        });
    }
    Some(out)
}

/// The exact text a plain scalar literal renders. Floats abstain: Go's
/// formatting of float64 output is not worth modeling here.
fn literal_output_text(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    use helm_schema_ast::{Literal, TemplateExpr};
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text)) => {
            Some(text.clone())
        }
        TemplateExpr::Literal(Literal::Bool(value)) => Some(value.to_string()),
        TemplateExpr::Literal(Literal::Int(value)) => Some(value.to_string()),
        // `print` of one literal renders it verbatim (oauth2-proxy's
        // capability helpers spell their arms `{{- print "…" -}}`).
        TemplateExpr::Call { function, args } if function == "print" => match args.as_slice() {
            [argument] => literal_output_text(argument),
            _ => None,
        },
        _ => None,
    }
}
