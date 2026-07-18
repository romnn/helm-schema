//! Literal-dispatch helper analysis: a helper whose body is ONE
//! `if`/`else if`/`else` chain rendering only static text per arm (the
//! oauth2-proxy `legacy-config.mode` shape). Conditions comparing the
//! helper's output against a literal (`eq (include "mode" .) "x"`)
//! decode into the matching arms' branch conditions.

use helm_schema_ast::{TemplateHeader, children_with_field};

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
            NodeAction::Assignment(_)
            | NodeAction::With(_)
            | NodeAction::Range(_)
            | NodeAction::Output(_) => return false,
        }
    }
    true
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
