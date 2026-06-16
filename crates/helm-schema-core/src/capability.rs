use std::collections::HashSet;

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
            Self::Literals { values } => values.is_empty(),
            Self::Nested { branches } => branches.iter().all(|branch| branch.body.is_empty()),
        }
    }

    #[must_use]
    pub fn all_literals(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        self.append_all_literals(&mut out, &mut seen);
        out
    }

    pub fn append_all_literals(&self, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match self {
            Self::Literals { values } => {
                for value in values {
                    if seen.insert(value.clone()) {
                        out.push(value.clone());
                    }
                }
            }
            Self::Nested { branches } => {
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
    #[must_use]
    pub fn canonical_helm_literal(&self) -> String {
        match self {
            Self::Resource { api_version, kind } => format!("{api_version}/{kind}"),
            Self::GroupVersion { api_version } => api_version.clone(),
        }
    }
}

fn is_k8s_api_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digit_count = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return false;
    }
    let suffix = &rest[digit_count..];
    if suffix.is_empty() {
        return true;
    }
    for qualifier in ["alpha", "beta"] {
        if let Some(number) = suffix.strip_prefix(qualifier) {
            return !number.is_empty()
                && number.chars().all(|character| character.is_ascii_digit());
        }
    }
    false
}
