use std::fs;
use std::path::Path;
use std::sync::Mutex;

use crate::diagnostic::{Diagnostic, DiagnosticSink};

use super::layout::{CACHE_LAYOUT_VERSION, layout_marker_path};

/// Outcome of a per-root layout check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutCheckOutcome {
    /// Root was empty or had a matching marker — safe to use as-is.
    Ok,
    /// Root was wiped (legacy layout or older marker). Repopulate.
    Invalidated,
    /// Root carries a marker newer than this binary. Untouched; caller
    /// must skip writes and treat the cache as read-only.
    ForwardIncompatible,
}

/// Per-process gate that ensures a given cache root is only checked
/// once per invocation, even when multiple providers share it.
#[derive(Debug, Default)]
pub struct LayoutChecker {
    seen: Mutex<std::collections::HashMap<std::path::PathBuf, LayoutCheckOutcome>>,
}

impl LayoutChecker {
    /// Creates an empty per-process layout-check gate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a managed cache root against [`CACHE_LAYOUT_VERSION`].
    ///
    /// On first call for a given `root`, applies the
    /// [Cache compatibility policy (alpha)]:
    ///   - missing marker AND empty/non-existent root → `Ok`, no wipe;
    ///   - missing marker AND populated subtree → wipe, then `Invalidated`;
    ///   - marker matches compiled → `Ok`;
    ///   - marker < compiled → wipe + `Invalidated`;
    ///   - marker > compiled → `ForwardIncompatible` (no mutation).
    ///
    /// Emits [`Diagnostic::CacheLayoutInvalidated`] or
    /// [`Diagnostic::CacheLayoutForwardIncompatible`] exactly once per
    /// root per process.
    ///
    /// `populated_subtree_check` is a callback the caller supplies to
    /// determine whether the root contains anything that would qualify
    /// as a "populated managed subtree" — the K8s provider checks for
    /// version dirs, the CRD provider checks for source-namespace dirs
    /// or group dirs. Returning `true` from this callback triggers a
    /// wipe when the marker is missing.
    pub(crate) fn check_and_prepare<F: FnOnce(&Path) -> bool>(
        &self,
        root: &Path,
        sink: Option<&DiagnosticSink>,
        populated_subtree_check: F,
    ) -> LayoutCheckOutcome {
        if let Ok(guard) = self.seen.lock()
            && let Some(prev) = guard.get(root)
        {
            return *prev;
        }

        let outcome = perform_check(root, sink, populated_subtree_check);

        if let Ok(mut guard) = self.seen.lock() {
            guard.insert(root.to_path_buf(), outcome);
        }
        outcome
    }
}

fn perform_check<F: FnOnce(&Path) -> bool>(
    root: &Path,
    sink: Option<&DiagnosticSink>,
    populated_subtree_check: F,
) -> LayoutCheckOutcome {
    let marker_path = layout_marker_path(root);
    let on_disk = read_marker(&marker_path);

    match on_disk {
        Some(value) if value == CACHE_LAYOUT_VERSION => LayoutCheckOutcome::Ok,
        Some(value) if value > CACHE_LAYOUT_VERSION => {
            if let Some(sink) = sink {
                sink.push(Diagnostic::CacheLayoutForwardIncompatible {
                    cache_root: root.display().to_string(),
                    on_disk_marker: value,
                    compiled_marker: CACHE_LAYOUT_VERSION,
                });
            }
            LayoutCheckOutcome::ForwardIncompatible
        }
        Some(value) => {
            wipe_and_write_marker(root, &marker_path, sink, Some(value));
            LayoutCheckOutcome::Invalidated
        }
        None => {
            let exists = root.exists();
            let populated = exists && populated_subtree_check(root);
            if populated {
                wipe_and_write_marker(root, &marker_path, sink, None);
                LayoutCheckOutcome::Invalidated
            } else {
                // Empty or non-existent. First-populate path: ensure the
                // directory exists and write the marker. No diagnostic.
                let _ = fs::create_dir_all(root);
                let _ = fs::write(&marker_path, format!("{CACHE_LAYOUT_VERSION}\n"));
                LayoutCheckOutcome::Ok
            }
        }
    }
}

fn read_marker(path: &Path) -> Option<u32> {
    let bytes = fs::read(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    text.trim().parse::<u32>().ok()
}

fn wipe_and_write_marker(
    root: &Path,
    marker_path: &Path,
    sink: Option<&DiagnosticSink>,
    previous_marker: Option<u32>,
) {
    // Best-effort: read the directory, remove each child individually
    // so we never accidentally delete the root itself (preserves
    // symlinks pointing into the root, lets parent permissions stay
    // intact). The marker is recreated immediately afterwards.
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
    let _ = fs::create_dir_all(root);
    let _ = fs::write(marker_path, format!("{CACHE_LAYOUT_VERSION}\n"));

    if let Some(sink) = sink {
        sink.push(Diagnostic::CacheLayoutInvalidated {
            cache_root: root.display().to_string(),
            previous_marker,
            current_marker: CACHE_LAYOUT_VERSION,
        });
    }
}
