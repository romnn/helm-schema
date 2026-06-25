//! Static evaluation of Helm helpers to literal output candidates.
//!
//! Targeted at apiVersion-shaped helpers that vendored charts use to
//! emit a single literal apiVersion (or a finite if/else set of them)
//! based on `Capabilities.APIVersions.Has` checks. The detector calls
//! this when it sees `apiVersion: {{ template "X" . }}` or
//! `apiVersion: {{ include "X" . }}` in a document header.
//!
//! This is intentionally NOT a general Helm renderer:
//! - only accepts exact static string outputs from the shared expression
//!   interpreter and nested `{{ template/include "Y" . }}` calls;
//! - dives into `if` / `with` branches to collect alternatives;
//! - skips Field / Variable references (returns nothing for those —
//!   the literal-only output set is the contract).
//!
//! Output is typed so the common `if Capabilities.APIVersions.Has … else …`
//! shape stays branch-aware for Kubernetes lookup.

pub(super) use helm_schema_ast::HelperOutputEvaluator;

#[cfg(test)]
#[path = "../../tests/resource_identity/helper_output.rs"]
mod tests;
