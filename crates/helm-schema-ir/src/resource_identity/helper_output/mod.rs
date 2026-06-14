//! Static evaluation of Helm helpers to literal output candidates.
//!
//! Targeted at apiVersion-shaped helpers that vendored charts use to
//! emit a single literal apiVersion (or a finite if/else set of them)
//! based on `Capabilities.APIVersions.Has` checks. The detector calls
//! this when it sees `apiVersion: {{ template "X" . }}` or
//! `apiVersion: {{ include "X" . }}` in a document header.
//!
//! This is intentionally NOT a general Helm renderer:
//! - only handles `{{ print … }}`, `{{ printf "%s" "X" }}`, bare string
//!   literals, and nested `{{ template/include "Y" . }}` calls;
//! - dives into `if` / `with` branches to collect alternatives;
//! - skips Field / Variable references (returns nothing for those —
//!   the literal-only output set is the contract).
//!
//! Output is typed so the common `if Capabilities.APIVersions.Has … else …`
//! shape stays branch-aware for Kubernetes lookup.

#[cfg(test)]
use std::collections::HashSet;

use helm_schema_ast::DefineIndex;

use crate::capability_branch::HelperBranch;

use self::evaluator::HelperOutputEvaluator;

mod evaluator;

const MAX_RECURSION_DEPTH: usize = 6;

/// Typed output of helper evaluation.
///
/// Preserves branch structure (guard + literals) for if/elif/else
/// chains so callers downstream — specifically the `Chain` lookup layer
/// — can evaluate `Capabilities.APIVersions.Has` guards against the
/// actual K8s version cache and pick the live branch instead of
/// guessing between mutually-exclusive alternatives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum HelperOutput {
    /// Helper body is linear (no top-level branching). The vector
    /// holds the deduplicated literal outputs in first-seen order.
    /// Empty = could not be resolved statically.
    Literals(Vec<String>),
    /// Helper body has a single top-level if/elif/else chain. Each
    /// branch carries its guard (when decodable) and the literals it
    /// can produce.
    Branched { branches: Vec<HelperBranch> },
}

impl HelperOutput {
    /// Flatten branches into a single deduplicated literal list (in
    /// first-seen order, depth-first through nested branches).
    /// Test helper for assertions that do not need branch structure.
    #[cfg(test)]
    #[must_use]
    fn all_literals(&self) -> Vec<String> {
        match self {
            HelperOutput::Literals(l) => l.clone(),
            HelperOutput::Branched { branches } => {
                let mut out: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for branch in branches {
                    branch.body.append_all_literals(&mut out, &mut seen);
                }
                out
            }
        }
    }
}

/// Resolve a helper name to its typed output.
#[must_use]
pub(crate) fn helper_evaluate(name: &str, helpers: &DefineIndex) -> HelperOutput {
    HelperOutputEvaluator::new().evaluate(name, helpers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_branch::{CapabilityGuard, HelperBranch, HelperBranchBody};
    use helm_schema_ast::{DefineIndex, TreeSitterParser};
    use indoc::indoc;

    /// Test helper: extract literals from a Literals-bodied
    /// `HelperBranch`. Panics if the branch is unexpectedly Nested
    /// (tests that need to walk nested structure should match the
    /// `body` field directly instead of using this helper).
    fn literals_of(b: &HelperBranch) -> &[String] {
        match &b.body {
            HelperBranchBody::Literals { values } => values.as_slice(),
            HelperBranchBody::Nested { .. } => panic!("expected Literals-bodied branch; got {b:?}"),
        }
    }

    fn index_with(src: &str) -> DefineIndex {
        let mut idx = DefineIndex::new();
        idx.add_source(&TreeSitterParser, src)
            .expect("parse helpers");
        idx
    }

    #[test]
    fn single_literal_helper_resolves() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("x.apiVersion", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    #[test]
    fn if_else_helper_returns_both_branches() {
        let helpers = index_with(indoc! {r#"
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let outs = helper_evaluate("rbac.apiVersion", &helpers).all_literals();
        assert!(
            outs.contains(&"rbac.authorization.k8s.io/v1".to_string()),
            "must include modern; got {outs:?}"
        );
        assert!(
            outs.contains(&"rbac.authorization.k8s.io/v1beta1".to_string()),
            "must include legacy; got {outs:?}"
        );
    }

    #[test]
    fn helper_with_values_reference_is_silent_about_dynamic_branch() {
        // grafana podDisruptionBudget shape: first branch is
        // Values-driven (unresolvable), other branches are literal.
        // We collect the literal branches and skip the dynamic one.
        let helpers = index_with(indoc! {r#"
            {{- define "grafana.podDisruptionBudget.apiVersion" -}}
            {{- if $.Values.podDisruptionBudget.apiVersion }}
            {{- print $.Values.podDisruptionBudget.apiVersion }}
            {{- else if $.Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
            {{- print "policy/v1" }}
            {{- else }}
            {{- print "policy/v1beta1" }}
            {{- end }}
            {{- end }}
        "#});
        let outs =
            helper_evaluate("grafana.podDisruptionBudget.apiVersion", &helpers).all_literals();
        assert!(
            outs.contains(&"policy/v1".to_string()),
            "must include policy/v1 literal branch; got {outs:?}"
        );
        assert!(
            outs.contains(&"policy/v1beta1".to_string()),
            "must include policy/v1beta1 literal branch; got {outs:?}"
        );
    }

    #[test]
    fn unknown_helper_returns_empty() {
        let helpers = DefineIndex::new();
        assert_eq!(
            helper_evaluate("nope", &helpers).all_literals(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nested_helper_recurses_one_level() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- template "inner" . -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("outer", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    #[test]
    fn cyclic_helper_does_not_stack_overflow() {
        let helpers = index_with(indoc! {r#"
            {{- define "a" -}}
            {{- template "b" . -}}
            {{- end -}}
            {{- define "b" -}}
            {{- template "a" . -}}
            {{- end -}}
        "#});
        // Either empty (cycle suppressed) — must NOT panic / overflow.
        let outs = helper_evaluate("a", &helpers).all_literals();
        assert!(
            outs.is_empty(),
            "cyclic helper must return empty, not infinite recursion; got {outs:?}"
        );
    }

    #[test]
    fn typed_output_preserves_guard_and_branch_literals() {
        // The vendored RBAC-shaped helper: stable variant gated by
        // Capabilities.APIVersions.Has, legacy as else. The typed
        // output must split into two branches; one carrying the guard,
        // one unguarded (the else fallback).
        let helpers = index_with(indoc! {r#"
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("rbac.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("expected Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2, "expected 2 branches; got {branches:?}");
        // First branch carries the CapabilityHas guard for the v1 API
        // and yields the modern literal.
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "rbac.authorization.k8s.io/v1".to_string(),
            }),
            "branch[0] guard mismatch"
        );
        assert_eq!(
            literals_of(&branches[0]),
            vec!["rbac.authorization.k8s.io/v1".to_string()]
        );
        // Second branch is the unguarded fallback yielding the legacy
        // literal.
        assert_eq!(branches[1].guard, None, "branch[1] should be unguarded");
        assert_eq!(
            literals_of(&branches[1]),
            vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
        );
    }

    /// Typed branch structure survives through a wrapper helper that only
    /// delegates via `{{ include "branched_inner" . }}`.
    #[test]
    fn typed_output_preserves_branches_through_wrapper_include() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer.apiVersion" -}}
            {{- include "rbac.apiVersion" . -}}
            {{- end -}}
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!(
                "wrapper helper must preserve branched typed output from delegated callee; got {out:?}"
            );
        };
        assert_eq!(branches.len(), 2, "expected 2 branches; got {branches:?}");
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "rbac.authorization.k8s.io/v1".to_string(),
            }),
            "branch[0] guard must carry the CapabilityHas decoded from the inner helper"
        );
        assert_eq!(
            literals_of(&branches[0]),
            vec!["rbac.authorization.k8s.io/v1".to_string()]
        );
        assert_eq!(branches[1].guard, None);
        assert_eq!(
            literals_of(&branches[1]),
            vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
        );
    }

    /// Same shape, but with `template` (the bare-string variant of
    /// the delegation keyword) instead of `include`. Both must
    /// preserve branches identically.
    #[test]
    fn typed_output_preserves_branches_through_wrapper_template() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer.apiVersion" -}}
            {{- template "rbac.apiVersion" . -}}
            {{- end -}}
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer.apiVersion", &helpers);
        assert!(
            matches!(out, HelperOutput::Branched { .. }),
            "wrapper via template must preserve Branched output; got {out:?}"
        );
    }

    /// Multi-level delegation chain: outer → middle → branched inner.
    /// Branches must propagate through arbitrary wrapper depth (up to
    /// the recursion guard).
    #[test]
    fn typed_output_preserves_branches_through_multi_level_wrapper() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- include "middle" . -}}
            {{- end -}}
            {{- define "middle" -}}
            {{- include "inner" . -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "policy/v1" }}
            {{- print "policy/v1" -}}
            {{- else -}}
            {{- print "policy/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("multi-level wrapper must preserve branched output; got {out:?}");
        };
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1".to_string()
            })
        );
    }

    /// Typed branch structure composes through branch bodies, not just at the
    /// top level. The shape:
    ///
    /// ```text
    /// {{- define "outer" -}}
    /// {{- if .Capabilities.APIVersions.Has "A" -}}
    /// {{- include "branched_inner" . -}}    {{- /* nested-branched delegation */ -}}
    /// {{- else -}}
    /// fallback
    /// {{- end -}}
    /// {{- end -}}
    /// ```
    ///
    /// must yield `HelperOutput::Branched` whose first branch carries
    /// a `Nested` body holding the inner helper's typed branches, NOT
    /// flatten the inner branches to a `Literals` body. This
    /// preserves the inner Has-B guard so the chain can recurse
    /// through both A and B at evaluation time.
    #[test]
    fn typed_output_preserves_nested_branches_through_branch_body_delegation() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- if .Capabilities.APIVersions.Has "A" -}}
            {{- include "inner" . -}}
            {{- else -}}
            {{- print "fallback" -}}
            {{- end -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "B" -}}
            {{- print "b" -}}
            {{- else -}}
            {{- print "b_legacy" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("outer must be Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2, "expected 2 outer branches");

        // First branch: Has A guard + Nested body (the inner helper's branches).
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "A".to_string()
            }),
        );
        let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
            panic!(
                "branch[0].body must be Nested to preserve inner Has-B guard; got {:?}",
                branches[0].body
            );
        };
        assert_eq!(nested.len(), 2, "inner helper should contribute 2 branches");
        assert_eq!(
            nested[0].guard,
            Some(CapabilityGuard::Has {
                api: "B".to_string()
            }),
            "nested branch[0] must preserve the inner Has-B guard"
        );
        assert_eq!(literals_of(&nested[0]), vec!["b".to_string()]);
        assert_eq!(nested[1].guard, None);
        assert_eq!(literals_of(&nested[1]), vec!["b_legacy".to_string()]);

        // Second branch: unguarded else + flat literal payload.
        assert_eq!(branches[1].guard, None);
        assert_eq!(literals_of(&branches[1]), vec!["fallback".to_string()]);
    }

    /// The same nested-branch structure is preserved when the nested branch is
    /// inline rather than delegated through `include`.
    #[test]
    fn typed_output_preserves_nested_branches_through_inline_nested_if() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- if .Capabilities.APIVersions.Has "A" -}}
            {{- if .Capabilities.APIVersions.Has "B" -}}
            {{- print "b" -}}
            {{- else -}}
            {{- print "b_legacy" -}}
            {{- end -}}
            {{- else -}}
            {{- print "fallback" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("outer must be Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2);
        let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
            panic!(
                "inline nested if must produce Nested body; got {:?}",
                branches[0].body
            );
        };
        assert_eq!(nested.len(), 2);
        assert_eq!(
            nested[0].guard,
            Some(CapabilityGuard::Has {
                api: "B".to_string()
            })
        );
    }

    /// Wrapper helper that mixes a delegation with other content must
    /// NOT promote the callee's branches — the wrapper's output isn't
    /// equivalent to the callee's output any more (the prefix changes
    /// the rendered string). Fall through to the flat path, which
    /// already conservatively collects literals as candidates.
    #[test]
    fn wrapper_with_mixed_content_does_not_promote_branches() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            prefix-{{ include "inner" . }}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "X" }}
            {{- print "X" -}}
            {{- else -}}
            {{- print "Y" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        // The mixed-content wrapper is NOT a pure delegation — the
        // typed `Branched` form doesn't fit. Fall back to flat.
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "mixed-content wrapper must fall through to flat literals; got {out:?}"
        );
    }

    /// Wrapper indirection must respect the same cycle guard the
    /// flat-literal recursion uses — a cyclic helper graph must not
    /// stack-overflow the typed-branch extractor.
    #[test]
    fn wrapper_cycle_falls_through_safely() {
        let helpers = index_with(indoc! {r#"
            {{- define "a" -}}
            {{- include "b" . -}}
            {{- end -}}
            {{- define "b" -}}
            {{- include "a" . -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("a", &helpers);
        // Cycle → no branches discoverable → fall through to
        // Literals (which will also be empty per the existing
        // cycle guard in collect_literals).
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "cyclic wrapper chain must fall through cleanly; got {out:?}"
        );
    }

    #[test]
    fn typed_output_preserves_elif_chain() {
        // Three-way chain: Values guard (opaque), CapabilityHas guard
        // (decoded), unguarded fallback.
        let helpers = index_with(indoc! {r#"
            {{- define "grafana.pdb.apiVersion" -}}
            {{- if $.Values.podDisruptionBudget.apiVersion }}
            {{- print $.Values.podDisruptionBudget.apiVersion }}
            {{- else if $.Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
            {{- print "policy/v1" }}
            {{- else }}
            {{- print "policy/v1beta1" }}
            {{- end }}
            {{- end }}
        "#});
        let out = helper_evaluate("grafana.pdb.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("expected Branched; got {out:?}");
        };
        // Values-guarded output is not a literal apiVersion candidate, but
        // later capability branches still keep their structural guards.
        let has_branch = branches.iter().find(|b| {
            matches!(
                &b.guard,
                Some(CapabilityGuard::Has { api }) if api == "policy/v1/PodDisruptionBudget"
            )
        });
        assert!(
            has_branch.is_some(),
            "expected CapabilityHas branch; got {branches:?}"
        );
        assert_eq!(
            literals_of(has_branch.unwrap()),
            vec!["policy/v1".to_string()]
        );
        // Final unguarded branch carries the legacy fallback.
        let else_branch = branches.iter().find(|b| b.guard.is_none());
        assert!(
            else_branch.is_some(),
            "expected unguarded else branch; got {branches:?}"
        );
        assert_eq!(
            literals_of(else_branch.unwrap()),
            vec!["policy/v1beta1".to_string()]
        );
    }

    /// `printf "%s/%s" "apps" "v1"` is compositional formatting we do not
    /// statically model, so it must not emit its format or arguments as
    /// literal apiVersion candidates.
    #[test]
    fn printf_compositional_format_emits_no_bogus_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%s/%s" "apps" "v1" -}}
            {{- end -}}
        "#});
        let outs = helper_evaluate("x.apiVersion", &helpers).all_literals();
        assert!(
            outs.is_empty(),
            "compositional printf must emit no literal candidates; got {outs:?}"
        );
    }

    /// `printf "%s" "X"` is the one substitution shape we DO model
    /// exactly: a single `%s` placeholder + a single string-literal
    /// arg evaluates to the arg.
    #[test]
    fn printf_single_substitution_resolves_to_arg() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%s" "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("x.apiVersion", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    /// `printf "X"` with no substitutions evaluates to the literal.
    #[test]
    fn printf_no_substitution_resolves_to_format() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("x.apiVersion", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    /// `printf "%d" .x` uses a non-`%s` directive — refuse to model.
    #[test]
    fn printf_non_string_directive_emits_no_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%d" 1 -}}
            {{- end -}}
        "#});
        let outs = helper_evaluate("x.apiVersion", &helpers).all_literals();
        assert!(
            outs.is_empty(),
            "non-%s printf directive must emit no candidates; got {outs:?}"
        );
    }

    /// `quote "X"` should produce the inner literal "X" (without the
    /// added quote wrapping — for apiVersion resolution we want the
    /// raw value).
    #[test]
    fn quote_with_single_string_arg_resolves() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- quote "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("x.apiVersion", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    /// `print` with multiple args is unusual and not the apiVersion
    /// shape we model; refuse rather than emit partial candidates.
    #[test]
    fn print_with_multiple_args_emits_no_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps" "v1" -}}
            {{- end -}}
        "#});
        let outs = helper_evaluate("x.apiVersion", &helpers).all_literals();
        assert!(
            outs.is_empty(),
            "print with multiple args must emit no candidates; got {outs:?}"
        );
    }

    /// `"X" | quote` pipeline: the seed literal is passed through the
    /// identity-shaped `quote` stage; result is the seed.
    #[test]
    fn pipeline_seed_literal_then_quote_resolves_to_seed() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- "apps/v1" | quote -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_evaluate("x.apiVersion", &helpers).all_literals(),
            vec!["apps/v1"]
        );
    }

    #[test]
    fn single_literal_helper_is_not_branched() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("x.apiVersion", &helpers);
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "single-literal helper must not be Branched; got {out:?}"
        );
    }
}
