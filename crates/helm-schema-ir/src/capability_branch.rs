use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use serde::{Deserialize, Serialize};

/// One branch of an if/elif/else chain in a helper or manifest header.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HelperBranch {
    /// `None` = unguarded trailing `else`; `Some` = structurally decoded guard.
    pub guard: Option<CapabilityGuard>,
    /// The apiVersion literals or nested branch chain produced by this branch.
    pub body: HelperBranchBody,
}

/// What a `HelperBranch` produces when its guard is live.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelperBranchBody {
    /// Flat list of literal alternatives. Empty means the branch resolves to
    /// no statically-known literal.
    Literals { values: Vec<String> },
    /// Branch body is itself a typed if/else chain.
    Nested { branches: Vec<HelperBranch> },
}

impl HelperBranchBody {
    /// Build a literal-bodied branch payload.
    #[must_use]
    pub fn literals(values: Vec<String>) -> Self {
        Self::Literals { values }
    }

    /// True when the body carries no statically-reachable literal.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            HelperBranchBody::Literals { values } => values.is_empty(),
            HelperBranchBody::Nested { branches } => branches.iter().all(|b| b.body.is_empty()),
        }
    }

    pub(crate) fn all_literals(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        self.append_all_literals(&mut out, &mut seen);
        out
    }

    pub(crate) fn append_all_literals(&self, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match self {
            HelperBranchBody::Literals { values } => {
                for value in values {
                    if seen.insert(value.clone()) {
                        out.push(value.clone());
                    }
                }
            }
            HelperBranchBody::Nested { branches } => {
                for branch in branches {
                    branch.body.append_all_literals(out, seen);
                }
            }
        }
    }
}

impl HelperBranch {
    /// Build a literal-bodied branch.
    #[must_use]
    pub fn with_literals(guard: Option<CapabilityGuard>, values: Vec<String>) -> Self {
        Self {
            guard,
            body: HelperBranchBody::Literals { values },
        }
    }

    /// Build a nested branch.
    #[must_use]
    pub fn with_nested(guard: Option<CapabilityGuard>, branches: Vec<HelperBranch>) -> Self {
        Self {
            guard,
            body: HelperBranchBody::Nested { branches },
        }
    }
}

/// Structurally-decoded capability guard for an `if` action.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CapabilityGuard {
    /// `.Capabilities.APIVersions.Has "X"`.
    Has { api: String },
    /// `not .Capabilities.APIVersions.Has "X"`.
    NotHas { api: String },
    /// Any guard the static decoder cannot structurally evaluate.
    Opaque { text: String },
}

/// Decode an if-condition string into a typed [`CapabilityGuard`].
pub(crate) fn decode_guard(cond: &str) -> CapabilityGuard {
    let trimmed = cond.trim();
    let wrapped = format!("{{{{ {trimmed} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    for expr in &exprs {
        if let Some((negated, api)) = find_capability_has(expr, false) {
            return if negated {
                CapabilityGuard::NotHas { api }
            } else {
                CapabilityGuard::Has { api }
            };
        }
    }
    CapabilityGuard::Opaque {
        text: cond.trim().to_string(),
    }
}

fn is_capabilities_has(function: &str) -> bool {
    function == ".Capabilities.APIVersions.Has"
        || function == "$.Capabilities.APIVersions.Has"
        || function.ends_with(".Capabilities.APIVersions.Has")
}

fn find_capability_has(expr: &TemplateExpr, negated: bool) -> Option<(bool, String)> {
    match expr {
        TemplateExpr::Call { function, args } if function == "not" => {
            for arg in args {
                if let Some((negated, api)) = find_capability_has(arg, !negated) {
                    return Some((negated, api));
                }
            }
            let field_ends_in_has = args.iter().any(|arg| {
                matches!(
                    arg,
                    TemplateExpr::Field(path)
                        if path.last().map(String::as_str) == Some("Has")
                            && path.iter().rev().nth(1).map(String::as_str) == Some("APIVersions")
                            && path.iter().rev().nth(2).map(String::as_str) == Some("Capabilities")
                )
            });
            if field_ends_in_has {
                return args.iter().find_map(|arg| match arg {
                    TemplateExpr::Literal(lit) => {
                        lit.as_string().map(|api| (!negated, api.to_string()))
                    }
                    _ => None,
                });
            }
            None
        }
        TemplateExpr::Call { function, args } if is_capabilities_has(function) => {
            args.iter().find_map(|arg| match arg {
                TemplateExpr::Literal(lit) => lit.as_string().map(|api| (negated, api.to_string())),
                _ => None,
            })
        }
        TemplateExpr::Call { args, .. } => args
            .iter()
            .find_map(|arg| find_capability_has(arg, negated)),
        TemplateExpr::Pipeline(stages) => stages
            .iter()
            .find_map(|stage| find_capability_has(stage, negated)),
        TemplateExpr::Parenthesized(inner) => find_capability_has(inner, negated),
        TemplateExpr::Selector { operand, .. } => find_capability_has(operand, negated),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            find_capability_has(value, negated)
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_guard_recognises_capability_has() {
        assert_eq!(
            decode_guard(".Capabilities.APIVersions.Has \"policy/v1\""),
            CapabilityGuard::Has {
                api: "policy/v1".to_string()
            }
        );
        assert_eq!(
            decode_guard("$.Capabilities.APIVersions.Has \"networking.k8s.io/v1/Ingress\""),
            CapabilityGuard::Has {
                api: "networking.k8s.io/v1/Ingress".to_string()
            }
        );
    }

    #[test]
    fn decode_guard_recognises_negated_capability_has() {
        assert_eq!(
            decode_guard("not .Capabilities.APIVersions.Has \"extensions/v1beta1\""),
            CapabilityGuard::NotHas {
                api: "extensions/v1beta1".to_string()
            }
        );
    }

    #[test]
    fn decode_guard_falls_back_to_opaque_for_values_refs() {
        let guard = decode_guard("$.Values.podDisruptionBudget.apiVersion");
        assert!(
            matches!(guard, CapabilityGuard::Opaque { .. }),
            "expected Opaque; got {guard:?}"
        );
    }
}
