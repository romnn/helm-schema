use std::fmt::Write;

use super::diagnostic::Diagnostic;

/// Render a [`Diagnostic`] as a human-readable line. Returns
/// `"warning: …"` for diagnostics representing problems, `"info: …"`
/// for diagnostics describing successful but noteworthy outcomes.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps every diagnostic's user-facing rendering auditable"
)]
pub fn format_diagnostic_text(diagnostic: &Diagnostic) -> String {
    match diagnostic {
        Diagnostic::MissingSchema {
            kind,
            api_version,
            k8s_versions_tried,
            available_in_cache_versions,
            suggested_k8s_version,
            hint,
            ..
        } => {
            let mut out = String::new();
            let _ = if api_version.trim().is_empty() {
                write!(
                    out,
                    "warning: no upstream Kubernetes JSON schema found for {kind} (apiVersion unknown)"
                )
            } else {
                write!(
                    out,
                    "warning: no upstream Kubernetes JSON schema found for {kind} ({api_version})"
                )
            };
            if !k8s_versions_tried.is_empty() {
                let _ = write!(out, " in {}", k8s_versions_tried.join(", "));
            }
            if !available_in_cache_versions.is_empty() {
                let _ = write!(
                    out,
                    "; available in local cache for: {}",
                    available_in_cache_versions.join(", ")
                );
            }
            if let Some(s) = suggested_k8s_version {
                let _ = write!(out, "; try --k8s-version {s}");
            }
            if let Some(h) = hint {
                let _ = write!(out, "; {h}");
            }
            out
        }
        Diagnostic::ResolvedFromFallbackVersion {
            kind,
            api_version,
            primary_version,
            resolved_version,
        } => format!(
            "info: {kind} ({api_version}) resolved from K8s {resolved_version} (not in primary {primary_version})"
        ),
        Diagnostic::InferredApiVersion {
            kind,
            inferred_api_version,
            source,
            ..
        } => format!(
            "info: inferred apiVersion {inferred_api_version} for {kind} (source: {source:?})"
        ),
        Diagnostic::AmbiguousApiVersion { kind, candidates } => {
            let list = candidates
                .iter()
                .map(|c| c.api_version.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("warning: ambiguous apiVersion inference for {kind}: candidates [{list}]")
        }
        Diagnostic::CrdVersionNotFound {
            group,
            kind,
            requested_version,
            locations_tried,
        } => {
            let mut out = format!(
                "warning: CRD schema not found for {group}/{kind} version {requested_version}"
            );
            if !locations_tried.is_empty() {
                let _ = write!(out, "; tried: {}", locations_tried.join(", "));
            }
            out
        }
        Diagnostic::CrdVersionAvailableAtOtherVersions {
            group,
            kind,
            requested_version,
            available_versions,
        } => format!(
            "info: {group}/{kind} {requested_version} not found, but cached at versions: {}",
            available_versions.join(", ")
        ),
        Diagnostic::LocalOverrideUnreadable {
            kind,
            api_version,
            override_path,
            io_error,
        } => format!(
            "warning: local override for {kind} ({api_version}) at {override_path} is unreadable: {io_error}"
        ),
        Diagnostic::CacheLayoutInvalidated {
            cache_root,
            previous_marker,
            current_marker,
        } => {
            let prev = previous_marker
                .map(|n| n.to_string())
                .unwrap_or_else(|| "none".to_string());
            format!(
                "info: cache layout for {cache_root} invalidated (previous: {prev}, current: {current_marker})"
            )
        }
        Diagnostic::CacheLayoutForwardIncompatible {
            cache_root,
            on_disk_marker,
            compiled_marker,
        } => format!(
            "warning: cache at {cache_root} was written by a newer helm-schema (on-disk: {on_disk_marker}, this binary: {compiled_marker}); refusing to mutate"
        ),
        Diagnostic::InputChannelNumericRangeAmbiguity { value_path } => format!(
            "warning: {value_path} has input-channel-dependent integer range semantics: Helm can iterate an integer from --set but rejects the same JSON number from a values file or --set-json; JSON Schema cannot distinguish those inputs"
        ),
    }
}
