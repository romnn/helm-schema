//! Prepare `$ref`s in a generated schema before it's written to disk.
//!
//! Fully inlined export mode is delegated to the [`jsonschema`] crate's
//! [`dereference`](jsonschema::dereference) helper, which sits on top of
//! [`referencing`](::jsonschema::Retrieve) — the same ref-resolution
//! library that the broader JSON Schema validator ecosystem uses. This
//! gives us battle-tested behaviour for every `$ref` shape we care about:
//!
//! - file refs with or without `#/json/pointer` fragments
//! - URL refs with or without `#/json/pointer` fragments
//! - bare fragment refs (`#/$defs/foo`) against the current document
//! - RFC 6901 escapes (`~0`, `~1`) inside pointers
//! - relative-URI resolution against a base
//! - JSON Schema drafts 4 through 2020-12 (we ship draft-07)
//! - cycle detection (left in place as `$ref` strings rather than
//!   recursing forever)
//!
//! All we own is the [`Retrieve`] implementation that maps URIs back to
//! their content — files from the chart-local filesystem and URLs over
//! HTTP via `ureq`, both gated by an explicit fetch policy.
//!
//! Self-contained output mode keeps use sites as `$ref`s but re-homes
//! external documents under root-level `$defs`. For tests, the lower-level
//! entry points accept any `Retrieve` impl so callers can wire in an
//! in-memory map keyed by URI and avoid disk I/O entirely.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use json_schema_walk::try_visit_subschemas_mut;
use jsonschema::{Retrieve, Uri};
use referencing::uri;
use serde_json::{Map, Value};
use tracing::instrument;

use crate::error::{CliError, CliResult};
use crate::fetch_policy::FetchPolicy;
use crate::load_budget::{LoadBudget, read_to_end_capped};

/// Inline every `$ref` in `schema` against the filesystem rooted at
/// `base_dir`.
///
/// Relative refs in the schema (and in any document loaded transitively)
/// resolve against the directory each ref originates from — the standard
/// JSON Schema base-URI rule. The base URI is constructed from
/// `base_dir` as `file://<canonical-path>/`.
///
/// # Errors
///
/// Returns [`CliError::Referencing`] for any ref-resolution failure
/// (file not found, JSON parse error, cycle the underlying resolver
/// can't break, network/file refs denied by fetch policy, …). The underlying error
/// is wrapped with enough detail for an operator to find the bad ref.
#[instrument(skip_all)]
pub fn flatten_refs(
    schema: Value,
    base_dir: &Path,
    fetch_policy: FetchPolicy,
    load_budget: LoadBudget,
) -> CliResult<Value> {
    let base_uri = path_to_file_uri(base_dir);
    let retriever = FsHttpRetrieve::new(fetch_policy, load_budget);
    flatten_with_retriever(schema, &base_uri, retriever)
}

/// Fully inline already-prepared refs without allowing file/URL retrieval.
///
/// This is the final-output counterpart to [`flatten_refs`]. Input assembly is
/// responsible for fetching and preparing external refs. If an external ref
/// reaches this pass, the schema is not self-contained and the run fails.
#[instrument(skip_all)]
pub fn flatten_prepared_refs(schema: Value, base_dir: &Path) -> CliResult<Value> {
    let base_uri = path_to_file_uri(base_dir);
    flatten_with_retriever(schema, &base_uri, NoExternalRetrieve)
}

/// Resolve external `$ref`s into root-level `$defs` entries while preserving
/// internal refs as refs.
///
/// The result is self-contained: file and URL references are loaded through
/// the same [`Retrieve`] implementation used by [`flatten_refs`], but the
/// referenced schema is re-homed under `#/$defs/...` instead of being inlined
/// at each use site.
#[instrument(skip_all)]
pub fn bundle_refs(
    schema: Value,
    base_dir: &Path,
    fetch_policy: FetchPolicy,
    load_budget: LoadBudget,
) -> CliResult<Value> {
    let base_uri = path_to_file_uri(base_dir);
    let retriever = FsHttpRetrieve::new(fetch_policy, load_budget);
    bundle_with_retriever(schema, &base_uri, retriever)
}

/// Validate and normalize already-prepared refs without allowing file/URL
/// retrieval.
///
/// Internal refs are preserved. External refs fail, because input assembly
/// should have already re-homed them under root-level `$defs`.
#[instrument(skip_all)]
pub fn bundle_prepared_refs(schema: Value, base_dir: &Path) -> CliResult<Value> {
    let base_uri = path_to_file_uri(base_dir);
    bundle_with_retriever(schema, &base_uri, NoExternalRetrieve)
}

/// Low-level dereference entry point: lets callers (most importantly,
/// tests) plug in a custom [`Retrieve`] so they don't have to touch the
/// filesystem to exercise the ref-resolution behaviour.
///
/// `base_uri` is the URI relative refs resolve against. Use a synthetic
/// `file:///<something>/` for in-memory tests.
///
/// # Errors
///
/// Returns [`CliError::Referencing`] on any ref-resolution failure.
#[instrument(skip_all)]
pub fn flatten_with_retriever(
    schema: Value,
    base_uri: &str,
    retriever: impl Retrieve + 'static,
) -> CliResult<Value> {
    let dereferenced = jsonschema::options()
        .with_base_uri(base_uri.to_string())
        .with_retriever(retriever)
        .dereference(&schema)?;
    Ok(dereferenced)
}

/// Low-level bundling entry point for tests and custom retrievers.
///
/// `base_uri` is the URI relative refs resolve against. External refs are
/// fetched through `retriever`, rewritten to root-level `$defs`, and any refs
/// inside fetched schemas are interpreted relative to the document they came
/// from before being re-homed.
#[instrument(skip_all)]
pub fn bundle_with_retriever(
    mut schema: Value,
    base_uri: &str,
    retriever: impl Retrieve,
) -> CliResult<Value> {
    let root_document_uri = document_uri(&uri::from_str(base_uri)?)?;
    let root_base_uri = effective_base_uri(&schema, &root_document_uri)?;
    let root_document_uris = BTreeSet::from([
        root_document_uri.as_str().to_string(),
        root_base_uri.as_str().to_string(),
    ]);
    let existing_definition_names = existing_definition_names(&schema);
    let mut state = BundleState::new(retriever, root_document_uris, existing_definition_names);
    state.bundle_schema(&mut schema, &root_document_uri)?;
    state.insert_definitions(&mut schema)?;
    Ok(schema)
}

struct BundleState<R> {
    retriever: R,
    root_document_uris: BTreeSet<String>,
    names_by_target_uri: BTreeMap<String, String>,
    definitions: BTreeMap<String, Value>,
    existing_definition_names: BTreeSet<String>,
    next_definition_id: usize,
}

impl<R: Retrieve> BundleState<R> {
    fn new(
        retriever: R,
        root_document_uris: BTreeSet<String>,
        existing_definition_names: BTreeSet<String>,
    ) -> Self {
        Self {
            retriever,
            root_document_uris,
            names_by_target_uri: BTreeMap::new(),
            definitions: BTreeMap::new(),
            existing_definition_names,
            next_definition_id: 1,
        }
    }

    fn bundle_schema(
        &mut self,
        schema: &mut Value,
        current_document_uri: &Uri<String>,
    ) -> CliResult<()> {
        let current_document_uri = effective_base_uri(schema, current_document_uri)?;
        if let Some(reference) = schema_reference(schema) {
            let target_uri = uri::resolve_against(&current_document_uri.borrow(), &reference)?;
            if self.should_preserve_reference(&target_uri, &current_document_uri)? {
                return Ok(());
            }
            let definition_name = self.definition_name_for_target(&target_uri)?;
            *schema = definition_ref(&definition_name);
            return Ok(());
        }

        try_visit_subschemas_mut(schema, &mut |subschema| {
            self.bundle_schema(subschema, &current_document_uri)
        })
    }

    fn should_preserve_reference(
        &self,
        target_uri: &Uri<String>,
        current_document_uri: &Uri<String>,
    ) -> CliResult<bool> {
        let target_document_uri = document_uri(target_uri)?;
        Ok(self.is_root_document(&target_document_uri)
            && self.is_root_document(current_document_uri))
    }

    fn definition_name_for_target(&mut self, target_uri: &Uri<String>) -> CliResult<String> {
        let target_key = target_uri.as_str().to_string();
        if let Some(name) = self.names_by_target_uri.get(&target_key) {
            return Ok(name.clone());
        }

        let name = self.next_definition_name();
        self.names_by_target_uri.insert(target_key, name.clone());

        let target_document_uri = document_uri(target_uri)?;
        let mut target_schema = self.resolve_target_schema(target_uri, &target_document_uri)?;
        self.bundle_schema(&mut target_schema, &target_document_uri)?;
        self.definitions.insert(name.clone(), target_schema);

        Ok(name)
    }

    fn resolve_target_schema(
        &self,
        target_uri: &Uri<String>,
        target_document_uri: &Uri<String>,
    ) -> CliResult<Value> {
        if self.is_root_document(target_document_uri) {
            return Err(CliError::RefBundling(format!(
                "cannot bundle non-local ref back to root document: {target_uri}"
            )));
        }

        let document = self
            .retriever
            .retrieve(target_document_uri)
            .map_err(|err| {
                CliError::RefBundling(format!("retrieve {target_document_uri}: {err}"))
            })?;
        select_fragment(document, target_uri)
    }

    fn next_definition_name(&mut self) -> String {
        loop {
            let name = format!("schema{}", self.next_definition_id);
            self.next_definition_id += 1;
            if self.existing_definition_names.insert(name.clone()) {
                return name;
            }
        }
    }

    fn insert_definitions(self, schema: &mut Value) -> CliResult<()> {
        if self.definitions.is_empty() {
            return Ok(());
        }

        let Value::Object(root) = schema else {
            return Err(CliError::RefBundling(
                "cannot insert bundled definitions into non-object root schema".to_string(),
            ));
        };
        let entry = root
            .entry("$defs".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(existing) = entry else {
            return Err(CliError::RefBundling(
                "cannot insert bundled definitions because root $defs is not an object".to_string(),
            ));
        };
        for (name, definition) in self.definitions {
            existing.insert(name, definition);
        }
        Ok(())
    }

    fn is_root_document(&self, document_uri: &Uri<String>) -> bool {
        self.root_document_uris.contains(document_uri.as_str())
    }
}

/// Production [`Retrieve`]: file URIs go through `std::fs`; HTTP/HTTPS
/// URIs go through a single shared `ureq` agent, both gated by an explicit
/// [`FetchPolicy`].
struct FsHttpRetrieve {
    fetch_policy: FetchPolicy,
    load_budget: LoadBudget,
    agent: ureq::Agent,
}

impl FsHttpRetrieve {
    fn new(fetch_policy: FetchPolicy, load_budget: LoadBudget) -> Self {
        Self {
            fetch_policy,
            load_budget,
            agent: ureq::Agent::new_with_defaults(),
        }
    }
}

impl Retrieve for FsHttpRetrieve {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let scheme = uri.scheme().as_str().to_ascii_lowercase();
        match scheme.as_str() {
            "file" => {
                let host = uri
                    .authority()
                    .map(|authority| authority.host())
                    .unwrap_or("");
                self.fetch_policy
                    .validate_file_host(host)
                    .map_err(|err| format!("$ref to {uri} but {err}"))?;
                // `file://host/path/to/foo.json` — the path component is
                // what we want. Empty host is the standard
                // `file:///path` shape.
                let path = uri.path().as_str();
                if !std::path::Path::new(path).is_absolute() {
                    return Err(
                        format!("$ref to {uri} but file URI path is not absolute: {path}").into(),
                    );
                }
                let mut file =
                    std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
                let bytes = read_to_end_capped(
                    &mut file,
                    self.load_budget.max_schema_document_bytes,
                    path.to_string(),
                )
                .map_err(|e| e.to_string())?;
                let value: Value =
                    serde_json::from_slice(&bytes).map_err(|e| format!("parse {path}: {e}"))?;
                Ok(value)
            }
            "http" | "https" => {
                let host = uri.authority().map(|authority| authority.host());
                self.fetch_policy
                    .validate_network_host(host)
                    .map_err(|err| format!("$ref to {uri} but {err}"))?;
                let resp = self
                    .agent
                    .get(uri.as_str())
                    .call()
                    .map_err(|e| format!("fetch {uri}: {e}"))?;
                let mut body = resp.into_body();
                let mut reader = body.as_reader();
                let bytes = read_to_end_capped(
                    &mut reader,
                    self.load_budget.max_schema_document_bytes,
                    uri.as_str().to_string(),
                )
                .map_err(|e| e.to_string())?;
                let value: Value =
                    serde_json::from_slice(&bytes).map_err(|e| format!("parse {uri}: {e}"))?;
                Ok(value)
            }
            other => Err(format!("unsupported $ref scheme: {other} (uri={uri})").into()),
        }
    }
}

struct NoExternalRetrieve;

impl Retrieve for NoExternalRetrieve {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("external $ref remained after input preparation: {uri}").into())
    }
}

fn schema_reference(schema: &Value) -> Option<String> {
    let Value::Object(object) = schema else {
        return None;
    };
    object
        .get("$ref")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn document_uri(uri: &Uri<String>) -> CliResult<Uri<String>> {
    let document = uri.strip_fragment().as_str().to_string();
    Uri::parse(document)
        .map_err(|err| CliError::RefBundling(format!("parse document uri for {uri}: {err:?}")))
}

fn effective_base_uri(
    schema: &Value,
    current_document_uri: &Uri<String>,
) -> CliResult<Uri<String>> {
    let Some(id) = schema
        .as_object()
        .and_then(|object| object.get("$id"))
        .and_then(Value::as_str)
    else {
        return Ok(current_document_uri.clone());
    };

    let resolved = uri::resolve_against(&current_document_uri.borrow(), id)?;
    document_uri(&resolved)
}

fn select_fragment(document: Value, target_uri: &Uri<String>) -> CliResult<Value> {
    let Some(fragment) = target_uri.fragment() else {
        return Ok(document);
    };
    let pointer = fragment.decode().to_string().map_err(|_| {
        CliError::RefBundling(format!("decode json pointer fragment for {target_uri}"))
    })?;
    if pointer.is_empty() {
        return Ok(document);
    }
    if !pointer.starts_with('/') {
        return Err(CliError::RefBundling(format!(
            "unsupported non-json-pointer fragment in {target_uri}"
        )));
    }

    document.pointer(&pointer).cloned().ok_or_else(|| {
        CliError::RefBundling(format!("json pointer {pointer} not found in {target_uri}"))
    })
}

fn existing_definition_names(schema: &Value) -> BTreeSet<String> {
    schema
        .get("$defs")
        .and_then(Value::as_object)
        .map(|definitions| definitions.keys().cloned().collect())
        .unwrap_or_default()
}

fn definition_ref(name: &str) -> Value {
    Value::Object(Map::from_iter([(
        "$ref".to_string(),
        Value::String(format!("#/$defs/{name}")),
    )]))
}

/// Convert a filesystem path into a base `file://` URI suitable for
/// passing to `with_base_uri`. The trailing `/` ensures relative refs
/// resolve as *children* of the base, not as siblings replacing the last
/// path segment.
fn path_to_file_uri(p: &Path) -> String {
    // Canonicalise so `..` segments in the input don't end up in the
    // base URI (would otherwise interfere with ref resolution).
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let s = canonical.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    format!("file://{trimmed}/")
}

#[cfg(test)]
#[path = "tests/flatten.rs"]
mod tests;
