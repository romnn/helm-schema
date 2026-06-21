use std::collections::HashSet;
use std::fs;
use std::hash::Hash;
use std::path::Path;

use crate::cache::{
    NegativeCache, SourceDocCache, not_found_marker_exists, not_found_marker_path,
    read_cached_json_doc, write_not_found_marker,
};
use crate::cache_write::write_fetched_schema_doc;
use crate::fetch::HttpFetcher;
use crate::schema_doc::SchemaDoc;

pub(crate) struct CachedSchemaDocRequest<'a> {
    pub(crate) local: &'a Path,
    pub(crate) url: &'a str,
    pub(crate) source_id: &'a str,
    pub(crate) cache_namespace: &'a str,
    pub(crate) cache_key: &'a str,
    pub(crate) allow_download: bool,
    pub(crate) use_cache: bool,
    pub(crate) record_source: bool,
    pub(crate) fetcher: &'a dyn HttpFetcher,
    pub(crate) negative_cache: &'a NegativeCache,
}

pub(crate) enum AuthoritativeAbsence<'a> {
    None,
    MarkerPath(&'a Path),
}

impl AuthoritativeAbsence<'_> {
    pub(crate) fn exists(&self) -> bool {
        match self {
            Self::None => false,
            Self::MarkerPath(local) => not_found_marker_exists(local),
        }
    }

    pub(crate) fn clear(&self) {
        let Self::MarkerPath(local) = self else {
            return;
        };
        remove_cache_file_if_present(
            &not_found_marker_path(local),
            "failed to remove stale schema not-found marker",
        );
    }

    pub(crate) fn record(&self) {
        let Self::MarkerPath(local) = self else {
            return;
        };
        remove_cache_file_if_present(local, "failed to remove stale schema cache file");
        if let Err(err) = write_not_found_marker(local) {
            tracing::debug!(?err, "failed to write schema not-found marker");
        }
    }
}

fn remove_cache_file_if_present(path: &Path, message: &'static str) {
    if let Err(err) = fs::remove_file(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!(?err, message);
    }
}

pub(crate) fn load_cached_schema_doc(
    request: CachedSchemaDocRequest<'_>,
    read_mem: impl FnOnce() -> Option<SchemaDoc>,
    mut write_mem: impl FnMut(SchemaDoc),
    has_authoritative_absence_marker: impl FnOnce() -> bool,
    clear_authoritative_absence_marker: impl FnOnce(),
    record_authoritative_absence_marker: impl FnOnce(),
) -> Option<SchemaDoc> {
    if request.use_cache {
        if let Some(doc) = read_mem() {
            return Some(doc);
        }
        if request.local.exists()
            && let Some(doc) = read_cached_json_doc(request.local)
        {
            write_mem(doc.clone());
            return Some(doc);
        }
        if request.negative_cache.contains(
            request.source_id,
            request.cache_namespace,
            request.cache_key,
        ) || has_authoritative_absence_marker()
        {
            return None;
        }
    }

    if !request.allow_download {
        return None;
    }

    match request.fetcher.fetch(request.url) {
        Ok(Some(bytes)) => {
            let doc = write_fetched_schema_doc(
                request.local,
                request.url,
                &bytes,
                request.record_source,
            )?;
            clear_authoritative_absence_marker();
            write_mem(doc.clone());
            Some(doc)
        }
        Ok(None) => {
            request.negative_cache.record(
                request.source_id,
                request.cache_namespace,
                request.cache_key,
            );
            record_authoritative_absence_marker();
            None
        }
        Err(_) => None,
    }
}

pub(crate) fn load_source_schema_doc<K>(
    request: CachedSchemaDocRequest<'_>,
    mem: &SourceDocCache<K>,
    mem_key: K,
    absence: AuthoritativeAbsence<'_>,
) -> Option<SchemaDoc>
where
    K: Eq + Hash + Clone,
{
    load_cached_schema_doc(
        request,
        || mem.read(&mem_key),
        |doc| mem.write(mem_key.clone(), doc),
        || absence.exists(),
        || absence.clear(),
        || absence.record(),
    )
}

pub(crate) fn source_url(base_url: &str, relative_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        relative_path.trim_start_matches('/')
    )
}

pub(crate) fn locations_tried<S>(
    sources: &[S],
    relative_path: &str,
    base_url: impl Fn(&S) -> &str,
) -> Vec<String> {
    sources
        .iter()
        .map(|source| source_url(base_url(source), relative_path))
        .collect()
}

pub(crate) fn configured_source_ids<S>(
    sources: &[S],
    source_id: impl Fn(&S) -> &str,
) -> HashSet<String> {
    sources
        .iter()
        .map(|source| source_id(source).to_string())
        .collect()
}
