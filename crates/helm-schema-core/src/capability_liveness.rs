use crate::{ApiPresenceQuery, CapabilityGuard, HelperBranch, HelperBranchBody};

/// Authoritative answer to a parsed `.Capabilities.APIVersions.Has` query for
/// a specific Kubernetes version.
pub trait CapabilityOracle: Send + Sync {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool>;

    fn kube_version(&self) -> Option<&str> {
        None
    }
}

/// Resolve a typed-branch chain to the literal alternatives the chart would
/// emit at runtime for the target K8s version.
#[must_use]
pub fn live_literals<O: CapabilityOracle + ?Sized>(
    branches: &[HelperBranch],
    oracle: &O,
) -> Vec<String> {
    for branch in branches {
        if branch.body.is_empty() {
            continue;
        }
        if !guard_is_live(branch.guard.as_ref(), oracle) {
            continue;
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

fn guard_is_live<O: CapabilityOracle + ?Sized>(
    guard: Option<&CapabilityGuard>,
    oracle: &O,
) -> bool {
    match guard {
        None | Some(CapabilityGuard::Opaque { .. }) => true,
        Some(CapabilityGuard::Has { api }) => ApiPresenceQuery::parse_helm_literal(api)
            .and_then(|query| oracle.capability_has_query(&query))
            .unwrap_or(true),
        Some(CapabilityGuard::NotHas { api }) => ApiPresenceQuery::parse_helm_literal(api)
            .and_then(|query| oracle.capability_has_query(&query))
            .is_none_or(|has| !has),
    }
}
