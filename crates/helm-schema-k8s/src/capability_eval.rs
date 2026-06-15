//! Static evaluator for `.Capabilities.APIVersions.Has` guards on
//! typed apiVersion branches.
//!
//! Vendored Helm charts use the pattern
//!
//! ```text
//! {{- if .Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
//! apiVersion: policy/v1
//! {{- else }}
//! apiVersion: policy/v1beta1
//! {{- end }}
//! ```
//!
//! to emit different `apiVersion` literals depending on what the
//! target Kubernetes cluster supports. The IR layer
//! (`helm_schema_ir`) decodes the structural shape into a list of
//! `HelperBranch { guard, body }` entries; the chain layer's job
//! is to ask, for the user's configured primary K8s version, *which
//! branch is live* — and then drive both schema resolution AND
//! MissingSchema attribution from that branch's literals.
//!
//! Design contract: the oracle is upstream-first. A live branch is
//! one whose guard is satisfied by the authoritative schema bundle
//! for the primary K8s version, not by whatever happened to be in
//! the cache from previous lookups. Cache is a speed optimisation;
//! the bundle is fetched on demand if cold.
//!
//! Module layout — single-responsibility split so this stays a clean,
//! small interpreter as more guard shapes are added:
//!   - [`CapabilityOracle`] trait: pluggable "is api X supported in
//!     the target K8s version" oracle. The chain provides the
//!     production implementation via its providers; tests can supply
//!     a [`StaticOracle`] with explicit yes/no answers.
//!   - [`select_live_branch`]: pure walker over a `&[HelperBranch]`,
//!     parametric in the oracle. Has no chain / provider dependencies.
//!   - [`evaluate_guard`]: the single-guard evaluator used by
//!     `select_live_branch`. Externally callable so it can be reused
//!     for other guard sites in the future (e.g. evaluating
//!     `if Has X` gates around whole document bodies, not just
//!     `apiVersion:` selection).

use helm_schema_ir::{CapabilityGuard, HelperBranch, HelperBranchBody};

use crate::api_presence::ApiPresenceQuery;

/// Authoritative answer to a parsed `.Capabilities.APIVersions.Has` query for
/// a specific Kubernetes version. Production implementation lives in the chain
/// layer and consults the configured providers; tests use [`StaticOracle`] with
/// explicit yes/no answers.
///
/// Returns:
///   - `Some(true)` — the api (and kind, if specified) exists in the
///     target K8s version's schema bundle.
///   - `Some(false)` — the bundle exists but does not contain the
///     api/kind.
///   - `None` — the oracle can't answer (unknown api shape, no
///     primary version configured, fetch failure). Callers treat
///     `None` as "potentially live" so unknown guards never silently
///     drop a branch.
pub trait CapabilityOracle {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool>;

    /// Compatibility adapter for callers that still hold Helm's literal
    /// argument text. Invalid literals return `None`, which callers treat as
    /// "potentially live".
    fn capability_has_literal(&self, api: &str) -> Option<bool> {
        let query = ApiPresenceQuery::parse_helm_literal(api)?;
        self.capability_has_query(&query)
    }
}

/// Test oracle: returns whatever was inserted for each api literal.
/// `None` queries unknown apis. Convenient for unit tests where the
/// real provider chain would be heavyweight (network, cache state).
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
///
/// `None` guards (the unguarded `else` branch) are always live;
/// `Opaque` guards are conservatively treated as live because we
/// can't prove the branch dead; `Has` / `NotHas` consult the oracle
/// and treat an `Option::None` answer as "potentially live".
///
/// Returns `true` when the branch this guard wraps should be
/// considered as a possible runtime outcome.
#[must_use]
pub fn evaluate_guard<O: CapabilityOracle + ?Sized>(
    guard: Option<&CapabilityGuard>,
    oracle: &O,
) -> bool {
    match guard {
        None => true,
        Some(CapabilityGuard::Has { api }) => oracle.capability_has_literal(api).unwrap_or(true),
        Some(CapabilityGuard::NotHas { api }) => {
            oracle.capability_has_literal(api).is_none_or(|has| !has)
        }
        Some(CapabilityGuard::Opaque { .. }) => true,
    }
}

/// Resolve a typed-branch chain to the literal alternatives the
/// chart would emit at runtime for the target K8s version. Walks
/// `branches` in source order; for the first branch whose guard is
/// live, returns its literal payload — recursing through
/// `HelperBranchBody::Nested` chains so guard structure composes
/// across delegation depth (round-12 Finding 1). Branches with
/// empty bodies (no literal reachable, even through nesting) are
/// skipped.
///
/// Returns the empty vector when no branch satisfies both
/// constraints (every reachable branch body is empty, or every
/// guard evaluates to false — which can only happen for `NotHas`
/// chains).
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
                // Inner had no live + literal-bearing branch — keep
                // looking at later peer branches of `branches`.
            }
        }
    }
    Vec::new()
}

/// Convenience: which branch (at any nesting depth) is the live
/// literal-bodied one? Used by tests / diagnostics that want a
/// pointer to the picked branch rather than just its literals.
/// Returns the deepest live branch with a non-empty literal body.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn branch_has(api: &str, lits: &[&str]) -> HelperBranch {
        HelperBranch::with_literals(
            Some(CapabilityGuard::Has {
                api: api.to_string(),
            }),
            lits.iter().map(|s| (*s).to_string()).collect(),
        )
    }

    fn branch_not_has(api: &str, lits: &[&str]) -> HelperBranch {
        HelperBranch::with_literals(
            Some(CapabilityGuard::NotHas {
                api: api.to_string(),
            }),
            lits.iter().map(|s| (*s).to_string()).collect(),
        )
    }

    fn branch_else(lits: &[&str]) -> HelperBranch {
        HelperBranch::with_literals(None, lits.iter().map(|s| (*s).to_string()).collect())
    }

    fn branch_nested_has(api: &str, nested: Vec<HelperBranch>) -> HelperBranch {
        HelperBranch::with_nested(
            Some(CapabilityGuard::Has {
                api: api.to_string(),
            }),
            nested,
        )
    }

    /// Test helper: extract the literal values from a Literals-bodied
    /// branch, panicking if the branch is unexpectedly Nested.
    /// `select_live_branch` is documented to return only
    /// Literals-bodied branches, so this is the natural unwrap.
    fn literals_of(b: &HelperBranch) -> &[String] {
        match &b.body {
            HelperBranchBody::Literals { values } => values.as_slice(),
            HelperBranchBody::Nested { .. } => {
                panic!("expected Literals-bodied branch; got Nested: {b:?}")
            }
        }
    }

    #[test]
    fn evaluate_unguarded_branch_is_always_live() {
        let oracle = StaticOracle::new();
        assert!(evaluate_guard(None, &oracle));
    }

    #[test]
    fn evaluate_has_consults_oracle_true() {
        let oracle = StaticOracle::new().with("autoscaling/v2", true);
        let g = CapabilityGuard::Has {
            api: "autoscaling/v2".to_string(),
        };
        assert!(evaluate_guard(Some(&g), &oracle));
    }

    #[test]
    fn evaluate_has_consults_oracle_false() {
        let oracle = StaticOracle::new().with("autoscaling/v2", false);
        let g = CapabilityGuard::Has {
            api: "autoscaling/v2".to_string(),
        };
        assert!(!evaluate_guard(Some(&g), &oracle));
    }

    #[test]
    fn evaluate_has_unknown_oracle_treats_as_potentially_live() {
        // Conservative default: unknown api is "potentially live" so
        // branches with semantic content we can't decode aren't
        // silently dropped.
        let oracle = StaticOracle::new();
        let g = CapabilityGuard::Has {
            api: "some.crd/v1".to_string(),
        };
        assert!(evaluate_guard(Some(&g), &oracle));
    }

    #[test]
    fn evaluate_not_has_negates_oracle() {
        let oracle = StaticOracle::new().with("extensions/v1beta1", false);
        let g = CapabilityGuard::NotHas {
            api: "extensions/v1beta1".to_string(),
        };
        assert!(evaluate_guard(Some(&g), &oracle));
    }

    #[test]
    fn evaluate_opaque_guard_is_always_live() {
        let oracle = StaticOracle::new();
        let g = CapabilityGuard::Opaque {
            text: "$.Values.foo".to_string(),
        };
        assert!(evaluate_guard(Some(&g), &oracle));
    }

    #[test]
    fn select_picks_if_branch_when_has_is_true() {
        // The grafana HPA shape: `if Has "autoscaling/v2" then v2
        // else v2beta2`. In K8s 1.35 (v2 GA), the if-branch is live.
        let oracle = StaticOracle::new().with("autoscaling/v2", true);
        let branches = vec![
            branch_has("autoscaling/v2", &["autoscaling/v2"]),
            branch_else(&["autoscaling/v2beta2"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["autoscaling/v2".to_string()]);
    }

    #[test]
    fn select_picks_else_branch_when_has_is_false() {
        // Same shape, but target is K8s 1.22 (v2 not yet GA). The
        // if-branch's guard evaluates false; the else branch wins.
        let oracle = StaticOracle::new().with("autoscaling/v2", false);
        let branches = vec![
            branch_has("autoscaling/v2", &["autoscaling/v2"]),
            branch_else(&["autoscaling/v2beta2"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["autoscaling/v2beta2".to_string()]);
    }

    #[test]
    fn select_picks_if_branch_when_capability_unknown() {
        // Unknown capability → treated as potentially-live → if-branch
        // wins (it's first in source order and has a literal). This
        // preserves the "conservative default" property: branches with
        // semantic content we can't decode aren't silently dropped.
        let oracle = StaticOracle::new();
        let branches = vec![
            branch_has("some.crd/v1", &["some.crd/v1"]),
            branch_else(&["some.crd/v1beta1"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["some.crd/v1".to_string()]);
    }

    #[test]
    fn select_handles_elif_chain_picking_middle_branch() {
        // grafana PDB shape: `if $.Values.X then opaque elif Has Y then
        // policy/v1 else policy/v1beta1`. Values-guard branch is
        // opaque + literals empty (we can't statically resolve the
        // Values expression) so it's skipped; elif's CapabilityHas
        // guard evaluates true → that branch is live.
        let oracle = StaticOracle::new().with("policy/v1/PodDisruptionBudget", true);
        let branches = vec![
            // Values-driven branch has no decodable literal.
            HelperBranch::with_literals(
                Some(CapabilityGuard::Opaque {
                    text: "$.Values.podDisruptionBudget.apiVersion".to_string(),
                }),
                vec![],
            ),
            branch_has("policy/v1/PodDisruptionBudget", &["policy/v1"]),
            branch_else(&["policy/v1beta1"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["policy/v1".to_string()]);
    }

    #[test]
    fn select_skips_branches_with_empty_literals() {
        // A branch with no literals isn't a viable runtime outcome —
        // skip it even if its guard is live.
        let oracle = StaticOracle::new().with("policy/v1", true);
        let branches = vec![
            HelperBranch::with_literals(
                Some(CapabilityGuard::Has {
                    api: "policy/v1".to_string(),
                }),
                vec![],
            ),
            branch_else(&["policy/v1beta1"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["policy/v1beta1".to_string()]);
    }

    #[test]
    fn select_returns_none_when_all_branches_unusable() {
        // Every branch's literals are empty, or every NotHas guard is
        // false. There is no live branch with a literal output.
        let oracle = StaticOracle::new().with("X", true);
        let branches = vec![
            branch_not_has("X", &["unreachable"]),
            HelperBranch::with_literals(None, vec![]),
        ];
        assert!(select_live_branch(&branches, &oracle).is_none());
    }

    #[test]
    fn select_evaluates_in_source_order() {
        // First live branch wins, even if a later branch's guard is
        // also true. This mirrors Helm's runtime semantics where the
        // first matching `if` / `else if` is taken.
        let oracle = StaticOracle::new().with("X", true).with("Y", true);
        let branches = vec![
            branch_has("X", &["x-version"]),
            branch_has("Y", &["y-version"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["x-version".to_string()]);
    }

    /// Round-12: nested-branch composition. An outer if/else where
    /// the if-branch delegates to a branched inner helper:
    /// `if Has A then (Nested if Has B then "b" else "b_legacy")
    ///  else "y"`.
    /// With both guards live in the oracle, the selector must
    /// recurse through the Nested body and return the inner if-branch.
    #[test]
    fn select_recurses_through_nested_body() {
        let oracle = StaticOracle::new().with("A", true).with("B", true);
        let nested = vec![branch_has("B", &["b"]), branch_else(&["b_legacy"])];
        let branches = vec![branch_nested_has("A", nested), branch_else(&["y"])];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(
            literals_of(picked),
            vec!["b".to_string()],
            "must recurse through Nested body and pick inner if-branch when both guards are true"
        );
        assert_eq!(
            live_literals(&branches, &oracle),
            vec!["b".to_string()],
            "live_literals must agree with select_live_branch on the chosen literal"
        );
    }

    /// Same nested shape, but the inner Has B guard is false. The
    /// selector must pick the inner else branch's literal, NOT fall
    /// back to the outer else branch — Has A is still live, so we're
    /// committed to the outer if-branch's nested subtree.
    #[test]
    fn select_picks_inner_else_when_outer_has_true_inner_has_false() {
        let oracle = StaticOracle::new().with("A", true).with("B", false);
        let nested = vec![branch_has("B", &["b"]), branch_else(&["b_legacy"])];
        let branches = vec![branch_nested_has("A", nested), branch_else(&["y"])];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["b_legacy".to_string()]);
    }

    /// When the outer Has A is false, the entire nested subtree is
    /// skipped and the outer else branch's literal wins.
    #[test]
    fn select_skips_nested_subtree_when_outer_has_false() {
        let oracle = StaticOracle::new().with("A", false).with("B", true);
        let nested = vec![branch_has("B", &["b"]), branch_else(&["b_legacy"])];
        let branches = vec![branch_nested_has("A", nested), branch_else(&["y"])];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(literals_of(picked), vec!["y".to_string()]);
    }

    /// The elasticsearch PSP regression: the chart's inline `if Has
    /// "policy/v1"` guard evaluates true in K8s 1.35 (PDB is at
    /// policy/v1), so the chart emits `apiVersion: policy/v1`. PSP
    /// doesn't actually exist at policy/v1 — but that's a real chart
    /// bug for the diagnostic layer to surface, not something the
    /// branch selector should mask by silently preferring the else
    /// branch.
    #[test]
    fn select_picks_if_branch_even_when_kind_does_not_exist_there() {
        let oracle = StaticOracle::new().with("policy/v1", true);
        let branches = vec![
            branch_has("policy/v1", &["policy/v1"]),
            branch_else(&["policy/v1beta1"]),
        ];
        let picked = select_live_branch(&branches, &oracle).expect("a branch");
        assert_eq!(
            literals_of(picked),
            vec!["policy/v1".to_string()],
            "structural branch eval must reflect what the chart emits at runtime, even if that apiVersion has no schema for this kind"
        );
    }
}
