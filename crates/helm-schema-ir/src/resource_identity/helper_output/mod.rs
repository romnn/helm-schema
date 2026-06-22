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

/// Resolve a helper name to its typed output.
#[must_use]
pub(crate) fn helper_evaluate(name: &str, helpers: &DefineIndex) -> HelperOutput {
    HelperOutputEvaluator::new().evaluate(name, helpers)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
