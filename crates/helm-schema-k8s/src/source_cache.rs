use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use crate::cache::{
    NegativeCache, SourceDocCache, not_found_marker_exists, not_found_marker_path,
    read_cached_json_doc, write_not_found_marker,
};
use crate::cache_write::write_fetched_schema_doc;
use crate::fetch::HttpFetcher;
use crate::schema_doc::SchemaDoc;

pub(crate) struct CachedSchemaDocRequest<'a> {
    pub(crate) local: PathBuf,
    pub(crate) url: String,
    pub(crate) source_id: &'a str,
    pub(crate) cache_namespace: &'a str,
    pub(crate) cache_key: &'a str,
    pub(crate) allow_download: bool,
    pub(crate) use_cache: bool,
    pub(crate) record_source: bool,
    /// When set, an authoritative upstream 404 is persisted as a
    /// `<local>.not-found` sidecar so absence survives across processes
    /// (the in-process [`NegativeCache`] does not).
    pub(crate) use_not_found_marker: bool,
    pub(crate) fetcher: &'a dyn HttpFetcher,
    pub(crate) negative_cache: &'a NegativeCache,
}

/// Tri-state outcome of one `(source, cache slot)` schema-doc probe.
/// The authoritative-vs-uncertain distinction is the heart of the
/// capability oracle's offline-safety contract:
///   - `Found`: schema is loadable (mem cache, disk cache, or
///     successful fetch).
///   - `AuthoritativelyAbsent`: the fetcher confirmed the schema
///     does not exist upstream (recorded in negative cache).
///     Includes negative-cache hits from a prior online run, since
///     those represent a confirmed past 404 — still authoritative.
///   - `Uncertain`: no cache hit, no fetch attempted (offline AND
///     no negative-cache record), or fetch failed with a network
///     error. The probe gives no information either way.
pub(crate) enum SourceDocOutcome {
    Found(SchemaDoc),
    AuthoritativelyAbsent,
    Uncertain,
}

/// Single upstream-first cache-then-fetch sequence shared by resource
/// lookup ([`load_source_schema_doc`]) and the capability oracle's
/// source probes.
pub(crate) fn probe_source_schema_doc<K>(
    request: &CachedSchemaDocRequest<'_>,
    mem: &SourceDocCache<K>,
    mem_key: K,
) -> SourceDocOutcome
where
    K: Eq + Hash,
{
    if request.use_cache {
        if let Some(doc) = mem.read(&mem_key) {
            return SourceDocOutcome::Found(doc);
        }
        if request.local.exists()
            && let Some(doc) = read_cached_json_doc(&request.local)
        {
            mem.write(mem_key, doc.clone());
            return SourceDocOutcome::Found(doc);
        }
        // Negative cache / not-found marker is set ONLY when the fetcher
        // returned a clean "not found" — treat as authoritative even
        // offline. A prior online run already proved upstream doesn't
        // have this file.
        if request.negative_cache.contains(
            request.source_id,
            request.cache_namespace,
            request.cache_key,
        ) || (request.use_not_found_marker && not_found_marker_exists(&request.local))
        {
            return SourceDocOutcome::AuthoritativelyAbsent;
        }
    }

    if !request.allow_download {
        // Offline + no cache + no negative-cache record: nothing to
        // base an answer on.
        return SourceDocOutcome::Uncertain;
    }

    match request.fetcher.fetch(&request.url) {
        Ok(Some(bytes)) => {
            let Some(doc) = write_fetched_schema_doc(
                &request.local,
                &request.url,
                &bytes,
                request.record_source,
            ) else {
                // Couldn't persist or parse — we still proved the
                // schema exists upstream, but treat as Uncertain so
                // a later run probes again rather than locking in a
                // cache miss.
                return SourceDocOutcome::Uncertain;
            };
            if request.use_not_found_marker {
                remove_cache_file_if_present(
                    &not_found_marker_path(&request.local),
                    "failed to remove stale schema not-found marker",
                );
            }
            mem.write(mem_key, doc.clone());
            SourceDocOutcome::Found(doc)
        }
        Ok(None) => {
            request.negative_cache.record(
                request.source_id,
                request.cache_namespace,
                request.cache_key,
            );
            if request.use_not_found_marker {
                remove_cache_file_if_present(
                    &request.local,
                    "failed to remove stale schema cache file",
                );
                if let Err(err) = write_not_found_marker(&request.local) {
                    tracing::debug!(?err, "failed to write schema not-found marker");
                }
            }
            SourceDocOutcome::AuthoritativelyAbsent
        }
        // Network error: uncertain. Don't pollute the negative cache,
        // since the failure isn't proof of absence.
        Err(_) => SourceDocOutcome::Uncertain,
    }
}

pub(crate) fn load_source_schema_doc<K>(
    request: &CachedSchemaDocRequest<'_>,
    mem: &SourceDocCache<K>,
    mem_key: K,
) -> Option<SchemaDoc>
where
    K: Eq + Hash,
{
    match probe_source_schema_doc(request, mem, mem_key) {
        SourceDocOutcome::Found(doc) => Some(doc),
        SourceDocOutcome::AuthoritativelyAbsent | SourceDocOutcome::Uncertain => None,
    }
}

fn remove_cache_file_if_present(path: &Path, message: &'static str) {
    if let Err(err) = fs::remove_file(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!(?err, message);
    }
}

/// Default download policy for providers whose caller didn't configure
/// one: `HELM_SCHEMA_ALLOW_NET=1` (or `true`) enables network fetches.
pub(crate) fn allow_download_from_env() -> bool {
    std::env::var("HELM_SCHEMA_ALLOW_NET")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

pub(crate) fn source_url(base_url: &str, relative_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        relative_path.trim_start_matches('/')
    )
}
