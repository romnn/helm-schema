use helm_schema_core::CapabilityGuard;

use crate::{TemplateExpr, TemplateHeader};

/// Decodes a parsed control header into a capability guard, preserving
/// unsupported conditions as opaque guards over their source text.
#[must_use]
pub fn decode_header_guard(header: &TemplateHeader) -> CapabilityGuard {
    decode_guard_expr(header.expr(), header.raw()).unwrap_or_else(|| CapabilityGuard::Opaque {
        text: header.raw().trim().to_string(),
    })
}

/// Extracts a capability guard from one typed expression when possible.
#[must_use]
pub fn decode_guard_expr(expr: &TemplateExpr, raw: &str) -> Option<CapabilityGuard> {
    match expr {
        TemplateExpr::Call { function, args } if function == "not" => {
            if let [arg] = args.as_slice() {
                return match decode_guard_expr(arg, raw)? {
                    CapabilityGuard::Has { api } => Some(CapabilityGuard::NotHas { api }),
                    CapabilityGuard::NotHas { api } => Some(CapabilityGuard::Has { api }),
                    opaque @ CapabilityGuard::Opaque { .. } => Some(opaque),
                };
            }
            let [TemplateExpr::Field(path), TemplateExpr::Literal(literal)] = args.as_slice()
            else {
                return None;
            };
            let field_ends_in_has = path.last().map(String::as_str) == Some("Has")
                && path.iter().rev().nth(1).map(String::as_str) == Some("APIVersions")
                && path.iter().rev().nth(2).map(String::as_str) == Some("Capabilities");
            if !field_ends_in_has {
                return None;
            }
            Some(CapabilityGuard::NotHas {
                api: literal.as_string()?.to_string(),
            })
        }
        TemplateExpr::Call { function, args }
            if function.ends_with(".Capabilities.APIVersions.Has") =>
        {
            let [TemplateExpr::Literal(literal)] = args.as_slice() else {
                return None;
            };
            Some(CapabilityGuard::Has {
                api: literal.as_string()?.to_string(),
            })
        }
        TemplateExpr::Pipeline(stages) => {
            let [stage] = stages.as_slice() else {
                return None;
            };
            decode_guard_expr(stage, raw)
        }
        TemplateExpr::Parenthesized(inner) => decode_guard_expr(inner, raw),
        TemplateExpr::Unknown(_) => Some(CapabilityGuard::Opaque {
            text: raw.trim().to_string(),
        }),
        // A nested capability call does not establish a compound predicate's truth.
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/capability_branch.rs"]
mod tests;
