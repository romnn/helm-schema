use std::path::PathBuf;

use clap::{Args, ValueEnum};

/// Policy for resolving CRD versions missing from the configured catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CrdVersionLookup {
    /// Consult only the exact group, kind, and version.
    #[default]
    Strict,
    /// Scan other versions and emit informational alternatives.
    Loose,
}

/// CRD catalog, override, cache, and version-resolution options.
#[derive(Args, Debug, Clone)]
pub struct CrdArgs {
    /// CRD version lookup mode. Default `strict`: only the exact
    /// `(group, kind, version)` is consulted. `loose` enables cross-scan
    /// + informational hints; mirrors are available in BOTH modes.
    #[arg(long = "crd-version-lookup", value_enum, default_value_t = CrdVersionLookup::Strict)]
    pub crd_version_lookup: CrdVersionLookup,

    /// Short alias for `--crd-version-lookup=strict`. Kept for
    /// symmetry with `--strict-k8s-version` / `--strict-api-versions`
    /// and to keep CI opt-out flags short.
    #[arg(long = "strict-crd-version")]
    pub strict_crd_version: bool,

    /// Additional upstream CRD catalog mirror URL. Repeatable.
    /// Per-source cache namespacing keeps mirror entries from masking
    /// the default.
    #[arg(long = "crd-catalog-mirror")]
    pub crd_catalog_mirror: Vec<String>,

    /// Managed cache root for CRD schemas. Subject to the cache
    /// invalidation contract.
    #[arg(long = "crd-catalog-cache-dir")]
    pub crd_catalog_cache_dir: Option<PathBuf>,

    /// Hand-maintained CRD schema overrides. Never wiped, never
    /// subject to the cache invalidation contract.
    #[arg(long = "crd-override-dir")]
    pub crd_override_dir: Option<PathBuf>,

    /// Write a `<schema>.json.meta` sidecar alongside every CRD cache
    /// entry recording the fetch URL and timestamp.
    #[arg(long = "crd-cache-record-source")]
    pub crd_cache_record_source: bool,

    /// Removed in this alpha — use `--crd-override-dir` and/or
    /// `--crd-catalog-cache-dir` instead.
    #[arg(long = "crd-catalog-dir", hide = true)]
    pub crd_catalog_dir_removed: Option<PathBuf>,
}

impl CrdArgs {
    /// Resolved version-lookup mode after considering the alias flag.
    #[must_use]
    pub fn lookup_mode(&self) -> CrdVersionLookup {
        if self.strict_crd_version {
            CrdVersionLookup::Strict
        } else {
            self.crd_version_lookup
        }
    }

    /// Validate CRD-related flags. Returns a user-facing error string
    /// on invalid combinations.
    ///
    /// # Errors
    ///
    /// Returns an error for removed flags or overlapping managed and
    /// hand-maintained cache directories.
    pub fn validate(&self) -> Result<(), String> {
        if self.crd_catalog_dir_removed.is_some() {
            return Err(
                "--crd-catalog-dir is removed; use --crd-override-dir (hand-maintained schemas, never wiped) and/or --crd-catalog-cache-dir (managed cache root)".to_string(),
            );
        }
        if let (Some(o), Some(c)) = (&self.crd_override_dir, &self.crd_catalog_cache_dir)
            && o == c
        {
            return Err(
                "--crd-override-dir and --crd-catalog-cache-dir must point at different paths (override is hand-maintained, cache is wipe-eligible)".to_string(),
            );
        }
        Ok(())
    }
}
