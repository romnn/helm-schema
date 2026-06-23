use super::*;
use crate::{CapabilityGuard, HelperBranch, HelperBranchBody};
use helm_schema_ast::{DefineIndex, TreeSitterParser};
use indoc::indoc;
use std::collections::HashSet;
use test_util::prelude::sim_assert_eq;

trait HelperOutputTestExt {
    fn all_literals(&self) -> Vec<String>;
}

impl HelperOutputTestExt for HelperOutput {
    fn all_literals(&self) -> Vec<String> {
        match self {
            HelperOutput::Literals(literals) => literals.clone(),
            HelperOutput::Branched { branches } => {
                let mut out = Vec::new();
                let mut seen = HashSet::new();
                for branch in branches {
                    branch.body.append_all_literals(&mut out, &mut seen);
                }
                out
            }
        }
    }
}

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

fn evaluate_helper(name: &str, helpers: &DefineIndex) -> HelperOutput {
    HelperOutputEvaluator::new().evaluate(name, helpers)
}

#[test]
fn single_literal_helper_resolves() {
    let helpers = index_with(indoc! {r#"
        {{- define "x.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#});
    sim_assert_eq!(
        have: evaluate_helper("x.apiVersion", &helpers).all_literals(),
        want: vec!["apps/v1"]
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
    let outs = evaluate_helper("rbac.apiVersion", &helpers).all_literals();
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
    let outs = evaluate_helper("grafana.podDisruptionBudget.apiVersion", &helpers).all_literals();
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
    sim_assert_eq!(
        have: evaluate_helper("nope", &helpers).all_literals(),
        want: Vec::<String>::new()
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
    sim_assert_eq!(
        have: evaluate_helper("outer", &helpers).all_literals(),
        want: vec!["apps/v1"]
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
    let outs = evaluate_helper("a", &helpers).all_literals();
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
    let out = evaluate_helper("rbac.apiVersion", &helpers);
    let HelperOutput::Branched { branches } = out else {
        panic!("expected Branched; got {out:?}");
    };
    sim_assert_eq!(have: branches.len(), want: 2, "expected 2 branches; got {branches:?}");
    // First branch carries the CapabilityHas guard for the v1 API
    // and yields the modern literal.
    sim_assert_eq!(
        have: branches[0].guard,
        want: Some(CapabilityGuard::Has {
            api: "rbac.authorization.k8s.io/v1".to_string(),
        }),
        "branch[0] guard mismatch"
    );
    sim_assert_eq!(
        have: literals_of(&branches[0]),
        want: vec!["rbac.authorization.k8s.io/v1".to_string()]
    );
    // Second branch is the unguarded fallback yielding the legacy
    // literal.
    sim_assert_eq!(have: branches[1].guard, want: None, "branch[1] should be unguarded");
    sim_assert_eq!(
        have: literals_of(&branches[1]),
        want: vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
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
    let out = evaluate_helper("outer.apiVersion", &helpers);
    let HelperOutput::Branched { branches } = out else {
        panic!(
            "wrapper helper must preserve branched typed output from delegated callee; got {out:?}"
        );
    };
    sim_assert_eq!(have: branches.len(), want: 2, "expected 2 branches; got {branches:?}");
    sim_assert_eq!(
        have: branches[0].guard,
        want: Some(CapabilityGuard::Has {
            api: "rbac.authorization.k8s.io/v1".to_string(),
        }),
        "branch[0] guard must carry the CapabilityHas decoded from the inner helper"
    );
    sim_assert_eq!(
        have: literals_of(&branches[0]),
        want: vec!["rbac.authorization.k8s.io/v1".to_string()]
    );
    sim_assert_eq!(have: branches[1].guard, want: None);
    sim_assert_eq!(
        have: literals_of(&branches[1]),
        want: vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
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
    let out = evaluate_helper("outer.apiVersion", &helpers);
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
    let out = evaluate_helper("outer", &helpers);
    let HelperOutput::Branched { branches } = out else {
        panic!("multi-level wrapper must preserve branched output; got {out:?}");
    };
    sim_assert_eq!(have: branches.len(), want: 2);
    sim_assert_eq!(
        have: branches[0].guard,
        want: Some(CapabilityGuard::Has {
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
    let out = evaluate_helper("outer", &helpers);
    let HelperOutput::Branched { branches } = out else {
        panic!("outer must be Branched; got {out:?}");
    };
    sim_assert_eq!(have: branches.len(), want: 2, "expected 2 outer branches");

    // First branch: Has A guard + Nested body (the inner helper's branches).
    sim_assert_eq!(
        have: branches[0].guard,
        want: Some(CapabilityGuard::Has {
            api: "A".to_string()
        }),
    );
    let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
        panic!(
            "branch[0].body must be Nested to preserve inner Has-B guard; got {:?}",
            branches[0].body
        );
    };
    sim_assert_eq!(have: nested.len(), want: 2, "inner helper should contribute 2 branches");
    sim_assert_eq!(
        have: nested[0].guard,
        want: Some(CapabilityGuard::Has {
            api: "B".to_string()
        }),
        "nested branch[0] must preserve the inner Has-B guard"
    );
    sim_assert_eq!(have: literals_of(&nested[0]), want: vec!["b".to_string()]);
    sim_assert_eq!(have: nested[1].guard, want: None);
    sim_assert_eq!(have: literals_of(&nested[1]), want: vec!["b_legacy".to_string()]);

    // Second branch: unguarded else + flat literal payload.
    sim_assert_eq!(have: branches[1].guard, want: None);
    sim_assert_eq!(have: literals_of(&branches[1]), want: vec!["fallback".to_string()]);
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
    let out = evaluate_helper("outer", &helpers);
    let HelperOutput::Branched { branches } = out else {
        panic!("outer must be Branched; got {out:?}");
    };
    sim_assert_eq!(have: branches.len(), want: 2);
    let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
        panic!(
            "inline nested if must produce Nested body; got {:?}",
            branches[0].body
        );
    };
    sim_assert_eq!(have: nested.len(), want: 2);
    sim_assert_eq!(
        have: nested[0].guard,
        want: Some(CapabilityGuard::Has {
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
    let out = evaluate_helper("outer", &helpers);
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
    let out = evaluate_helper("a", &helpers);
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
    let out = evaluate_helper("grafana.pdb.apiVersion", &helpers);
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
    sim_assert_eq!(
        have: literals_of(has_branch.unwrap()),
        want: vec!["policy/v1".to_string()]
    );
    // Final unguarded branch carries the legacy fallback.
    let else_branch = branches.iter().find(|b| b.guard.is_none());
    assert!(
        else_branch.is_some(),
        "expected unguarded else branch; got {branches:?}"
    );
    sim_assert_eq!(
        have: literals_of(else_branch.unwrap()),
        want: vec!["policy/v1beta1".to_string()]
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
    let outs = evaluate_helper("x.apiVersion", &helpers).all_literals();
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
    sim_assert_eq!(
        have: evaluate_helper("x.apiVersion", &helpers).all_literals(),
        want: vec!["apps/v1"]
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
    sim_assert_eq!(
        have: evaluate_helper("x.apiVersion", &helpers).all_literals(),
        want: vec!["apps/v1"]
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
    let outs = evaluate_helper("x.apiVersion", &helpers).all_literals();
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
    sim_assert_eq!(
        have: evaluate_helper("x.apiVersion", &helpers).all_literals(),
        want: vec!["apps/v1"]
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
    let outs = evaluate_helper("x.apiVersion", &helpers).all_literals();
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
    sim_assert_eq!(
        have: evaluate_helper("x.apiVersion", &helpers).all_literals(),
        want: vec!["apps/v1"]
    );
}

#[test]
fn single_literal_helper_is_not_branched() {
    let helpers = index_with(indoc! {r#"
        {{- define "x.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#});
    let out = evaluate_helper("x.apiVersion", &helpers);
    assert!(
        matches!(out, HelperOutput::Literals(_)),
        "single-literal helper must not be Branched; got {out:?}"
    );
}
