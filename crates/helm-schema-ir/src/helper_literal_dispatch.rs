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
            match node_action(source, child) {
                NodeAction::Text => {
                    literal.push_str(child.utf8_text(source.as_bytes()).ok()?);
                }
                NodeAction::Suppressed => {}
                _ => return None,
            }
        }
        out.push(LiteralDispatchArm {
            header: arm_header,
            literal: literal.trim().to_string(),
        });
    }
    Some(out)
}
