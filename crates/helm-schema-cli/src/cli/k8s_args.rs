use std::path::PathBuf;

use clap::Args;

/// `--k8s-version-fallback` accepts either `auto` or a window size `<n>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum K8sVersionFallback {
    /// Auto-extend the single explicit `--k8s-version` with a default
    /// sliding window of older minors below it.
    Auto,
    /// Explicit window size. `0` is equivalent to no fallback.
    Window(u32),
}

impl std::str::FromStr for K8sVersionFallback {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(K8sVersionFallback::Auto)
        } else {
            s.parse::<u32>()
                .map(K8sVersionFallback::Window)
                .map_err(|err| format!("expected `auto` or a non-negative integer: {err}"))
        }
    }
}

/// Kubernetes schema versions, mirrors, cache, and network options.
#[derive(Args, Debug, Clone)]
pub struct K8sArgs {
    /// Kubernetes minor version directory(s) to consult, in
    /// user-supplied priority order. The first value is the primary;
    /// any further values are explicit fallbacks.
    #[arg(long = "k8s-version", default_values_t = vec![String::from("v1.35.0")])]
    pub k8s_version: Vec<String>,

    /// Auto-extend the (single explicit) `--k8s-version` with older
    /// minors. `auto` uses the default window; `<n>` selects an
    /// explicit window size.
    #[arg(long = "k8s-version-fallback", conflicts_with = "strict_k8s_version")]
    pub k8s_version_fallback: Option<K8sVersionFallback>,

    /// Additional upstream K8s schema mirror URL. Repeatable. Per-source
    /// cache namespacing keeps mirror entries from masking the default.
    #[arg(long = "k8s-schema-mirror")]
    pub k8s_schema_mirror: Vec<String>,

    /// Managed cache root for K8s schemas. Subject to the cache
    /// invalidation contract.
    #[arg(long = "k8s-schema-cache-dir")]
    pub k8s_schema_cache_dir: Option<PathBuf>,

    /// Bypass K8s schema cache reads and re-check upstream directly.
    ///
    /// Successful responses and authoritative 404s still refresh cache
    /// state, so this can repair stale local entries.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Suppress auto-fallback version semantics. Conflicts only with
    /// `--k8s-version-fallback`; orthogonal to `--k8s-schema-mirror`.
    #[arg(long = "strict-k8s-version")]
    pub strict_k8s_version: bool,

    /// Force offline. Equivalent to setting `HELM_SCHEMA_ALLOW_NET=0`.
    #[arg(long)]
    pub offline: bool,

    /// Skip K8s upstream schemas entirely.
    #[arg(long = "no-k8s-schemas")]
    pub no_k8s_schemas: bool,
}

impl K8sArgs {
    /// Resolved fallback window after applying strict mode + sanity
    /// checks. Returns `None` when no auto-fallback should apply (either
    /// because strict is set, the flag is absent, or `Window(0)`).
    ///
    /// # Errors
    ///
    /// Returns an error when automatic fallback is combined with more than
    /// one explicitly configured Kubernetes version.
    pub fn resolved_fallback_window(&self) -> Result<Option<u32>, String> {
        if self.strict_k8s_version {
            return Ok(None);
        }
        match &self.k8s_version_fallback {
            None | Some(K8sVersionFallback::Window(0)) => Ok(None),
            Some(K8sVersionFallback::Window(n)) => Ok(Some(*n)),
            Some(K8sVersionFallback::Auto) => {
                if self.k8s_version.len() != 1 {
                    return Err(
                        "--k8s-version-fallback=auto requires exactly one --k8s-version"
                            .to_string(),
                    );
                }
                Ok(Some(DEFAULT_AUTO_WINDOW))
            }
        }
    }
}

/// Default auto-fallback reach when `--k8s-version-fallback=auto` is set.
///
/// Sized to cover the realistic K8s deprecation horizon: charts in the
/// wild still ship `policy/v1beta1` (PSP / PDB) and
/// `networking.k8s.io/v1beta1` (Ingress) — both removed in v1.25 — so
/// a primary of the current default (`v1.35.0`) must be able to fall
/// back at least to `v1.24.0` to find a schema. 15 leaves headroom for
/// the next few K8s releases without churning this constant.
pub const DEFAULT_AUTO_WINDOW: u32 = 15;
