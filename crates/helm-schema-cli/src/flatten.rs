//! Inline all `$ref`s in a generated schema before it's written to disk.
//!
//! The flattening pass is delegated to the [`jsonschema`] crate's
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
//! HTTP via `ureq` (gated by `--offline`).
//!
//! For tests, the lower-level [`flatten_with_retriever`] accepts any
//! `Retrieve` impl so callers can wire in an in-memory map keyed by URI
//! and avoid disk I/O entirely.

use std::io::Read;
use std::path::Path;

use jsonschema::{Retrieve, Uri};
use serde_json::Value;
use tracing::instrument;

use crate::error::CliResult;

/// Knobs for [`flatten_refs`].
#[derive(Debug, Clone)]
pub struct FlattenOptions {
    /// When true, URL `$ref`s (http/https) are fetched; when false they
    /// produce [`CliError::RefNetworkDisabled`] rather than silently
    /// leaving a dangling reference in the output.
    pub allow_net: bool,
}

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
/// can't break, network ref under `--offline`, …). The underlying error
/// is wrapped with enough detail for an operator to find the bad ref.
#[instrument(skip_all)]
pub fn flatten_refs(schema: Value, base_dir: &Path, options: &FlattenOptions) -> CliResult<Value> {
    let base_uri = path_to_file_uri(base_dir);
    let retriever = FsHttpRetrieve::new(options.allow_net);
    flatten_with_retriever(schema, &base_uri, retriever)
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

/// Production [`Retrieve`]: file URIs go through `std::fs`; HTTP/HTTPS
/// URIs go through a single shared `ureq` agent (gated by
/// `allow_net`).
struct FsHttpRetrieve {
    allow_net: bool,
    agent: ureq::Agent,
}

impl FsHttpRetrieve {
    fn new(allow_net: bool) -> Self {
        Self {
            allow_net,
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
                // `file://host/path/to/foo.json` — the path component is
                // what we want. Empty host is the standard
                // `file:///path` shape.
                let path = uri.path().as_str();
                let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
                let value: Value =
                    serde_json::from_slice(&bytes).map_err(|e| format!("parse {path}: {e}"))?;
                Ok(value)
            }
            "http" | "https" => {
                if !self.allow_net {
                    return Err(format!(
                        "$ref to {uri} but network access is disabled (--offline)"
                    )
                    .into());
                }
                let resp = self
                    .agent
                    .get(uri.as_str())
                    .call()
                    .map_err(|e| format!("fetch {uri}: {e}"))?;
                let mut body = resp.into_body();
                let mut text = String::new();
                body.as_reader()
                    .read_to_string(&mut text)
                    .map_err(|e| format!("read body {uri}: {e}"))?;
                let value: Value =
                    serde_json::from_str(&text).map_err(|e| format!("parse {uri}: {e}"))?;
                Ok(value)
            }
            other => Err(format!("unsupported $ref scheme: {other} (uri={uri})").into()),
        }
    }
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
