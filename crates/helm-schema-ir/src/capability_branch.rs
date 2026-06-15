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

/// Typed capability-presence predicate carried by a decoded guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityPresencePredicate {
    Has(ApiPresenceQuery),
    NotHas(ApiPresenceQuery),
}

impl CapabilityGuard {
    #[must_use]
    pub fn presence_predicate(&self) -> Option<CapabilityPresencePredicate> {
        match self {
            Self::Has { api } => {
                ApiPresenceQuery::parse_helm_literal(api).map(CapabilityPresencePredicate::Has)
            }
            Self::NotHas { api } => {
                ApiPresenceQuery::parse_helm_literal(api).map(CapabilityPresencePredicate::NotHas)
            }
            Self::Opaque { .. } => None,
        }
    }
}

/// A typed `.Capabilities.APIVersions.Has ...` query.
///
/// Helm accepts both api-version-only literals (`policy/v1`, `v1`) and
/// resource-qualified literals (`policy/v1/PodDisruptionBudget`, `v1/Secret`).
/// Keeping that distinction explicit lets knowledge providers probe exact
/// resources directly and confines api-version-only probing to the provider
/// layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApiPresenceQuery {
    Resource { api_version: String, kind: String },
    GroupVersion { api_version: String },
}

impl ApiPresenceQuery {
    #[must_use]
    pub fn parse_helm_literal(api: &str) -> Option<Self> {
        let parts: Vec<&str> = api.split('/').collect();
        match parts.as_slice() {
            [group, version, kind]
                if !group.is_empty() && !version.is_empty() && !kind.is_empty() =>
            {
                Some(Self::Resource {
                    api_version: format!("{group}/{version}"),
                    kind: (*kind).to_string(),
                })
            }
            [version, kind] if is_k8s_api_version_segment(version) && !kind.is_empty() => {
                Some(Self::Resource {
                    api_version: (*version).to_string(),
                    kind: (*kind).to_string(),
                })
            }
            [api_version] if !api_version.is_empty() => Some(Self::GroupVersion {
                api_version: (*api_version).to_string(),
            }),
            [group, version] if !group.is_empty() && !version.is_empty() => {
                Some(Self::GroupVersion {
                    api_version: format!("{group}/{version}"),
                })
            }
            _ => None,
        }
    }

    /// Canonical Helm literal for this query.
    ///
    /// Resource queries intentionally use only apiVersion and kind because
    /// capability presence is independent of schema-resolution candidates.
    #[must_use]
    pub fn canonical_helm_literal(&self) -> String {
        match self {
            ApiPresenceQuery::Resource { api_version, kind } => format!("{api_version}/{kind}"),
            ApiPresenceQuery::GroupVersion { api_version } => api_version.clone(),
        }
    }
}

fn is_k8s_api_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let suffix = &rest[digit_count..];
    if suffix.is_empty() {
        return true;
    }
    for qualifier in ["alpha", "beta"] {
        if let Some(number) = suffix.strip_prefix(qualifier) {
            return !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
        }
    }
    false
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

    #[test]
    fn capability_guard_projects_typed_presence_predicate() {
        let guard = CapabilityGuard::Has {
            api: "policy/v1/PodDisruptionBudget".to_string(),
        };

        assert_eq!(
            guard.presence_predicate(),
            Some(CapabilityPresencePredicate::Has(
                ApiPresenceQuery::Resource {
                    api_version: "policy/v1".to_string(),
                    kind: "PodDisruptionBudget".to_string(),
                }
            ))
        );
    }

    fn parse(api: &str) -> Option<ApiPresenceQuery> {
        ApiPresenceQuery::parse_helm_literal(api)
    }

    #[test]
    fn parses_group_version_query() {
        assert_eq!(
            parse("policy/v1"),
            Some(ApiPresenceQuery::GroupVersion {
                api_version: "policy/v1".to_string(),
            })
        );
    }

    #[test]
    fn parses_core_version_query() {
        assert_eq!(
            parse("v1"),
            Some(ApiPresenceQuery::GroupVersion {
                api_version: "v1".to_string(),
            })
        );
    }

    #[test]
    fn parses_resource_qualified_group_version_query() {
        assert_eq!(
            parse("policy/v1/PodSecurityPolicy"),
            Some(ApiPresenceQuery::Resource {
                api_version: "policy/v1".to_string(),
                kind: "PodSecurityPolicy".to_string(),
            })
        );
    }

    #[test]
    fn parses_resource_qualified_core_version_query() {
        assert_eq!(
            parse("v1/Secret"),
            Some(ApiPresenceQuery::Resource {
                api_version: "v1".to_string(),
                kind: "Secret".to_string(),
            })
        );
    }

    #[test]
    fn rejects_malformed_resource_queries() {
        assert!(parse("policy/v1/").is_none());
        assert!(parse("v1/").is_none());
        assert!(parse("policy/v1/Pod/extra").is_none());
    }

    #[test]
    fn api_version_segment_parser_accepts_stable_and_prerelease_versions() {
        assert!(is_k8s_api_version_segment("v1"));
        assert!(is_k8s_api_version_segment("v2beta1"));
        assert!(is_k8s_api_version_segment("v3alpha2"));
    }

    #[test]
    fn api_version_segment_parser_rejects_group_names_and_incomplete_versions() {
        assert!(!is_k8s_api_version_segment("policy"));
        assert!(!is_k8s_api_version_segment("v"));
        assert!(!is_k8s_api_version_segment("v1gamma1"));
    }
}
