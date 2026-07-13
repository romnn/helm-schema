/// Configuration for the in-provider K8s version chain.
#[derive(Debug, Clone)]
pub struct K8sVersionChain {
    /// User-supplied versions in their literal CLI order. The first is
    /// the primary, the rest are explicit fallbacks.
    pub explicit: Vec<String>,
    /// Auto-extension policy: `None` = no auto-fallback;
    /// `Some(n)` = append `n` minors below the smallest
    /// explicit version, monotonically descending.
    pub auto_fallback_window: Option<u32>,
}

impl K8sVersionChain {
    /// Build a chain from explicit user-ordered versions and an
    /// optional auto-fallback window. The two are combined as:
    ///   `[explicit..., auto_fallback...]`
    /// where the auto-fallback list is a descending window of `n` minors
    /// below the smallest explicit version. Auto-extension is only
    /// valid when `explicit.len() == 1` — the chain falls back to
    /// "explicit only" for any other shape.
    #[must_use]
    pub fn new(explicit: Vec<String>, auto_fallback_window: Option<u32>) -> Self {
        Self {
            explicit,
            auto_fallback_window,
        }
    }

    /// Materialise the ordered list of version_dirs to probe.
    #[must_use]
    pub fn ordered(&self) -> Vec<String> {
        let mut out: Vec<String> = self.explicit.clone();
        if let Some(window) = self.auto_fallback_window
            && self.explicit.len() == 1
            && let Some(primary) = self.explicit.first().and_then(|v| parse_minor(v))
        {
            for offset in 1..=window {
                let next_minor = match primary.1.checked_sub(offset) {
                    Some(m) => m,
                    None => break,
                };
                out.push(format!("v{}.{next_minor}.0", primary.0));
            }
        }
        out
    }

    /// The primary (first explicit) version, if any.
    #[must_use]
    pub fn primary(&self) -> Option<&str> {
        self.explicit.first().map(String::as_str)
    }

    /// Versions that should participate in apiVersion-inference cache
    /// scanning. This is `explicit` only — auto-fallback versions are
    /// escape valves for legacy resources whose schemas exist only in
    /// older K8s minors; they do NOT represent "what the user intends
    /// to target". Including them in inference would surface
    /// historical apiVersions (`policy/v1beta1`, `extensions/v1beta1`,
    /// …) for kinds whose modern version lives at the primary
    /// version dir, producing spurious `AmbiguousApiVersion`
    /// diagnostics.
    #[must_use]
    pub fn inference_scan_versions(&self) -> Vec<String> {
        self.explicit.clone()
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
