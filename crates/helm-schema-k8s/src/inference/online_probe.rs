use std::sync::Arc;

use crate::fetch::HttpFetcher;
use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};
use super::shortlist::canonical_group_version_for_kind;

/// Probe one CRD catalog source (default or mirror) for a kind whose
/// `(canonical_group, canonical_version)` is in the extended shortlist.
///
/// Returns a single-element vec on 200, empty vec otherwise. The probe
/// is kind-scoped (no blind group sweep): without a shortlist entry
/// for the kind we abstain.
#[must_use]
pub fn probe_crd_catalog(
    fetcher: &Arc<dyn HttpFetcher>,
    base_url: &str,
    kind: &str,
) -> Vec<ApiVersionCandidate> {
    let Some((group, version)) = canonical_group_version_for_kind(kind) else {
        return Vec::new();
    };
    if group.is_empty() {
        return Vec::new();
    }
    let url = format!(
        "{}/{}/{}_{}.json",
        base_url.trim_end_matches('/'),
        group,
        kind.to_ascii_lowercase(),
        version
    );
    match fetcher.fetch(&url) {
        Ok(Some(_)) => vec![ApiVersionCandidate {
            api_version: format!("{group}/{version}"),
            source: InferenceSource::OnlineProbe,
            origin: ProviderOrigin::DefaultCatalog,
        }],
        Ok(None) | Err(_) => Vec::new(),
    }
}
