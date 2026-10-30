use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use super::{State, eval::local_name, namespace::Namespace};

/// Certifies heap effects only; returned values still come from execution.
/// Every writer must target a uniquely bound fresh dictionary. Recursive calls
/// may use the same proof because they cannot write their arguments either.
#[tracing::instrument(skip_all)]
pub(super) fn local_targets(
    source: &str,
    syntax: &crate::analysis_db::ParsedProgramSyntax,
    namespace: &Namespace,
) -> Option<BTreeSet<String>> {
    if syntax.actions.len() > 128 || syntax.tree.root_node().has_error() {
        return None;
    }
    let expressions = syntax
        .expressions
        .starting_in(helm_schema_syntax::Span::new(0, source.len()));
    let mut definitions = BTreeMap::new();
    let mut complete = true;
    for action in expressions {
        for expression in action.expressions.iter() {
            expression.walk(|expression| match expression {
                TemplateExpr::VariableDefinition { name, value } => {
                    let name = local_name(name);
                    if name != "_" {
                        let fresh = matches!(value.deparen(), TemplateExpr::Call { function, args } if function == "dict" && args.is_empty());
                        complete &= definitions.insert(name.to_string(), fresh).is_none();
                    }
                }
                TemplateExpr::Assignment { .. } | TemplateExpr::Unknown(_) => complete = false,
                TemplateExpr::Literal(_)
                | TemplateExpr::Field(_)
                | TemplateExpr::Selector { .. }
                | TemplateExpr::Variable(_)
                | TemplateExpr::Call { .. }
                | TemplateExpr::Pipeline(_)
                | TemplateExpr::Parenthesized(_) => {}
            });
        }
    }
    let mut targets = BTreeSet::new();
    for action in expressions {
        for expression in action.expressions.iter() {
            expression.walk(|expression| {
                let TemplateExpr::Call { function, args } = expression else {
                    return;
                };
                match function.as_str() {
                    "set" | "unset" => {
                        let Some(TemplateExpr::Variable(name)) = args.first().map(TemplateExpr::deparen) else {
                            complete = false;
                            return;
                        };
                        let name = local_name(name);
                        complete &= definitions.get(name) == Some(&true);
                        targets.insert(name.to_string());
                    }
                    "include" | "template" => {
                        complete &= matches!(args.first().map(TemplateExpr::deparen), Some(TemplateExpr::Literal(Literal::String(name))) if namespace.program_source(name) == Some(source));
                    }
                    _ => {
                        complete &= matches!(crate::function_semantics::function_semantics(function).heap_effect, crate::function_semantics::HeapEffect::ReadOnly);
                    }
                }
            });
        }
    }
    let mut nodes = vec![syntax.tree.root_node()];
    while let Some(node) = nodes.pop() {
        if node.kind() == "range_action" {
            complete &= !helm_schema_ast::range_uses_assignment(node);
            for name in [
                helm_schema_ast::range_destructured_key_variable(node, source),
                helm_schema_ast::range_destructured_value_variable(node, source),
            ]
            .into_iter()
            .flatten()
            {
                complete &= !definitions.contains_key(&name);
            }
        }
        if node.kind() == "range_variable_definition" {
            let mut cursor = node.walk();
            for variable in node
                .children(&mut cursor)
                .take_while(|child| !matches!(child.kind(), ":=" | "="))
                .filter(|child| child.kind() == "variable")
            {
                let name = variable.utf8_text(source.as_bytes()).ok()?;
                complete &= !definitions.contains_key(local_name(name));
            }
        }
        let mut cursor = node.walk();
        nodes.extend(node.named_children(&mut cursor));
    }
    complete.then_some(targets)
}

pub(super) fn widen_targets(
    state: &mut State,
    targets: Option<&BTreeSet<String>>,
    influences: &BTreeSet<helm_schema_core::ValuesPath>,
) {
    let Some(targets) = targets else {
        super::operations::invalidate(state);
        return;
    };
    for name in targets {
        let Some(value) = state.read_local(name).cloned() else {
            continue;
        };
        let Some(target) = state.map_target(&value) else {
            super::operations::invalidate(state);
            return;
        };
        super::operations::widen_map(&target, influences, state);
    }
}
