/// Configuration for the in-provider K8s version chain.
#[derive(Debug, Clone)]
pub struct K8sVersionChain {
    ordered: Vec<String>,
}

impl K8sVersionChain {
    /// The auto-fallback window: how many older minors `--k8s-version-fallback=auto` appends
    /// below a single explicit version (ADR 0032).
    ///
    /// Sized to the realistic deprecation horizon: charts in the wild still ship
    /// `policy/v1beta1` (PSP / PDB) and `networking.k8s.io/v1beta1` (Ingress), both removed in
    /// v1.25, so the default primary (`v1.35.0`) must reach at least `v1.24.0`. 15 leaves
    /// headroom for the next few Kubernetes releases.
    pub const AUTO_FALLBACK_WINDOW: u32 = 15;

    /// Build a chain from explicit user-ordered versions and an
    /// optional auto-fallback window. The two are combined as:
    ///   `[explicit..., auto_fallback...]`
    /// where the auto-fallback list is a descending window of `n` minors
    /// below the smallest explicit version. Auto-extension is only
    /// valid when `explicit.len() == 1` — the chain falls back to
    /// "explicit only" for any other shape.
    #[must_use]
    pub fn new(explicit: Vec<String>, auto_fallback_window: Option<u32>) -> Self {
        let mut ordered = explicit;
        if let Some(window) = auto_fallback_window
            && ordered.len() == 1
            && let Some((major, minor)) = ordered.first().and_then(|version| parse_minor(version))
        {
            for offset in 1..=window {
                let Some(next_minor) = minor.checked_sub(offset) else {
                    break;
                };
                ordered.push(format!("v{major}.{next_minor}.0"));
            }
        }
        Self { ordered }
    }

    /// Returns the materialized `version_dirs` in probe order.
    #[must_use]
    pub fn ordered(&self) -> &[String] {
        &self.ordered
    }

    /// The primary (first explicit) version, if any.
    #[must_use]
    pub fn primary(&self) -> Option<&str> {
        self.ordered.first().map(String::as_str)
    }
}

fn parse_minor(version_dir: &str) -> Option<(u32, u32)> {
    let trimmed = version_dir.trim().trim_start_matches('v');
    let trimmed = trimmed.split('-').next().unwrap_or(trimmed);
    let mut parts = trimmed.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
#[path = "tests/version_chain.rs"]
mod tests;
