use std::path::Path;

use crate::cache::{NegativeCache, read_cached_json_doc};
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
