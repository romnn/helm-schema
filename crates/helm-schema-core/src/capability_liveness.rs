use crate::{
    ApiPresenceQuery, CapabilityGuard, CapabilityPresencePredicate, HelperBranch, HelperBranchBody,
};

/// Authoritative answer to a parsed `.Capabilities.APIVersions.Has` query for
/// a specific Kubernetes version.
pub trait CapabilityOracle: Send + Sync {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool>;

    fn kube_version(&self) -> Option<&str> {
        None
    }
}

/// Test oracle: returns whatever was inserted for each api literal.
#[derive(Debug, Default, Clone)]
pub struct StaticOracle {
    answers: std::collections::BTreeMap<String, bool>,
}

impl StaticOracle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, api: &str, has: bool) -> Self {
        if let Some(query) = ApiPresenceQuery::parse_helm_literal(api) {
            self.answers.insert(query.canonical_helm_literal(), has);
        }
        self
    }
}

impl CapabilityOracle for StaticOracle {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.answers.get(&query.canonical_helm_literal()).copied()
    }
}

/// Evaluate a single guard against the oracle.
#[must_use]
pub fn evaluate_guard<O: CapabilityOracle + ?Sized>(
    guard: Option<&CapabilityGuard>,
    oracle: &O,
) -> bool {
    match guard {
        None => true,
        Some(guard) => match guard.presence_predicate() {
            Some(CapabilityPresencePredicate::Has(query)) => {
                oracle.capability_has_query(&query).unwrap_or(true)
            }
            Some(CapabilityPresencePredicate::NotHas(query)) => {
                oracle.capability_has_query(&query).is_none_or(|has| !has)
            }
            None => true,
        },
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
        if !evaluate_guard(branch.guard.as_ref(), oracle) {
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

/// Return the deepest live literal-bearing branch, if any.
#[must_use]
pub fn select_live_branch<'a, O: CapabilityOracle + ?Sized>(
    branches: &'a [HelperBranch],
    oracle: &O,
) -> Option<&'a HelperBranch> {
    for branch in branches {
        if branch.body.is_empty() {
            continue;
        }
        if !evaluate_guard(branch.guard.as_ref(), oracle) {
            continue;
        }
        match &branch.body {
            HelperBranchBody::Literals { values } if !values.is_empty() => return Some(branch),
            HelperBranchBody::Literals { .. } => continue,
            HelperBranchBody::Nested { branches: nested } => {
                if let Some(inner) = select_live_branch(nested, oracle) {
                    return Some(inner);
                }
            }
        }
    }
    None
}
