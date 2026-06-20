use helm_schema_ast::{TemplateExpr, parse_action_expressions};

pub use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody};

/// Decode an if-condition string into a typed [`CapabilityGuard`].
pub(crate) fn decode_guard(cond: &str) -> CapabilityGuard {
    let trimmed = cond.trim();
    let wrapped = format!("{{{{ {trimmed} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    for expr in &exprs {
        if let Some(guard) = decode_guard_expr(expr, trimmed) {
            return guard;
        }
    }
    CapabilityGuard::Opaque {
        text: cond.trim().to_string(),
    }
}

pub(crate) fn decode_guard_expr(expr: &TemplateExpr, raw: &str) -> Option<CapabilityGuard> {
    find_capability_has(expr, false)
        .map(|(negated, api)| {
            if negated {
                CapabilityGuard::NotHas { api }
            } else {
                CapabilityGuard::Has { api }
            }
        })
        .or_else(|| {
            matches!(expr, TemplateExpr::Unknown(_)).then(|| CapabilityGuard::Opaque {
                text: raw.trim().to_string(),
            })
        })
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_core::{ApiPresenceQuery, CapabilityPresencePredicate};
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn decode_guard_recognises_capability_has() {
        sim_assert_eq!(
            decode_guard(".Capabilities.APIVersions.Has \"policy/v1\""),
            CapabilityGuard::Has {
                api: "policy/v1".to_string(),
            }
        );
        sim_assert_eq!(
            decode_guard("$.Capabilities.APIVersions.Has \"networking.k8s.io/v1/Ingress\""),
            CapabilityGuard::Has {
                api: "networking.k8s.io/v1/Ingress".to_string(),
            }
        );
    }

    #[test]
    fn decode_guard_recognises_negated_capability_has() {
        sim_assert_eq!(
            decode_guard("not .Capabilities.APIVersions.Has \"extensions/v1beta1\""),
            CapabilityGuard::NotHas {
                api: "extensions/v1beta1".to_string(),
            }
        );
    }

    #[test]
    fn decode_guard_falls_back_to_opaque_for_values_refs() {
        let guard = decode_guard("$.Values.podDisruptionBudget.apiVersion");
        assert!(matches!(guard, CapabilityGuard::Opaque { .. }));
    }

    #[test]
    fn presence_predicate_uses_core_query_parser() {
        let guard = CapabilityGuard::Has {
            api: "policy/v1/PodDisruptionBudget".to_string(),
        };
        sim_assert_eq!(
            guard.presence_predicate(),
            Some(CapabilityPresencePredicate::Has(
                ApiPresenceQuery::Resource {
                    api_version: "policy/v1".to_string(),
                    kind: "PodDisruptionBudget".to_string(),
                }
            ))
        );
    }
}
