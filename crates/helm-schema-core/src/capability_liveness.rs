use crate::{ApiPresenceQuery, CapabilityGuard, HelperBranch, HelperBranchBody};

/// Authoritative answer to a parsed `.Capabilities.APIVersions.Has` query for
/// a specific Kubernetes version.
pub trait CapabilityOracle: Send + Sync {
    /// Returns an authoritative presence answer, or `None` when uncertain.
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardLiveness {
    Live,
    Dead,
    Unknown,
}

/// Resolve a typed-branch chain to the literal alternatives the chart would
/// emit at runtime for the target K8s version. If a non-empty branch is not
/// statically decidable, return no chosen branch so callers preserve all
/// candidates instead of collapsing ambiguity to source order.
#[must_use]
pub fn live_literals<O: CapabilityOracle + ?Sized>(
    branches: &[HelperBranch],
    oracle: &O,
) -> Vec<String> {
    for branch in branches {
        if branch.body.is_empty() {
            continue;
        }
        match guard_liveness(branch.guard.as_ref(), oracle) {
            GuardLiveness::Dead => continue,
            GuardLiveness::Unknown => return Vec::new(),
            GuardLiveness::Live => {}
        }
        match &branch.body {
            HelperBranchBody::Literals { values } => return values.clone(),
            HelperBranchBody::Nested { branches: nested } => {
                let inner = live_literals(nested, oracle);
                if !inner.is_empty() {
                    return inner;
                }
            }
        }
    }
    Vec::new()
}

fn guard_liveness<O: CapabilityOracle + ?Sized>(
    guard: Option<&CapabilityGuard>,
    oracle: &O,
) -> GuardLiveness {
    match guard {
        None => GuardLiveness::Live,
        Some(CapabilityGuard::Opaque { .. }) => GuardLiveness::Unknown,
        Some(CapabilityGuard::Has { api }) => ApiPresenceQuery::parse_helm_literal(api)
            .and_then(|query| oracle.capability_has_query(&query))
            .map_or(GuardLiveness::Unknown, bool_liveness),
        Some(CapabilityGuard::NotHas { api }) => ApiPresenceQuery::parse_helm_literal(api)
            .and_then(|query| oracle.capability_has_query(&query))
            .map_or(GuardLiveness::Unknown, |has| bool_liveness(!has)),
    }
}

fn bool_liveness(live: bool) -> GuardLiveness {
    if live {
        GuardLiveness::Live
    } else {
        GuardLiveness::Dead
    }
}
