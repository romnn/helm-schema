use crate::inference::candidate::ApiVersionCandidate;

/// Sort a `Vec<String>` lexicographically and remove duplicates so the
/// same logical event always produces the same payload regardless of
/// the order in which probes ran.
pub(super) fn canonicalise_strings(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

/// Sort a `Vec<ApiVersionCandidate>` deterministically and dedupe.
pub(super) fn canonicalise_candidates(v: &mut Vec<ApiVersionCandidate>) {
    v.sort_by(|a, b| {
        a.api_version
            .cmp(&b.api_version)
            .then_with(|| format!("{:?}", a.source).cmp(&format!("{:?}", b.source)))
            .then_with(|| format!("{:?}", a.origin).cmp(&format!("{:?}", b.origin)))
    });
    v.dedup_by(|a, b| {
        a.api_version == b.api_version && a.source == b.source && a.origin == b.origin
    });
}
