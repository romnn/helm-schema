use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::EngineResult;
use crate::flatten;
use crate::load_budget::read_to_end_capped;
use crate::output_pipeline::{EmitRequest, PolicyInputOptions, ReferencePolicy};
use crate::schema_override::{PreparedOverride, UnpreparedOverride};

/// Output policy inputs validated before chart generation begins.
#[derive(Debug)]
pub(crate) struct LoadedEmitRequest {
    loaded_overrides: Vec<LoadedOverride>,
    request: EmitRequest,
}

#[derive(Debug)]
struct LoadedOverride {
    path: PathBuf,
    schema: UnpreparedOverride,
}

/// Output policy inputs loaded from the filesystem before final schema
/// transforms run.
///
/// This keeps override IO and external `$ref` retrieval out of the pure output
/// transform path.
#[derive(Debug)]
pub(crate) struct PreparedEmitRequest {
    /// Override schema documents after file loading and output-mode
    /// preparation. The final output pipeline consumes these prepared
    /// documents as data, so override file IO and override merge policy
    /// stay separate.
    prepared_override_schemas: Vec<PreparedOverride>,
    pub(super) request: EmitRequest,
}

pub(super) struct PreparedOverridesIdentity {
    pub(super) count: usize,
    pub(super) digest: String,
}

impl PreparedEmitRequest {
    pub(crate) fn empty(request: EmitRequest) -> Self {
        Self {
            prepared_override_schemas: Vec::new(),
            request,
        }
    }

    pub(super) fn override_count(&self) -> usize {
        self.prepared_override_schemas.len()
    }

    pub(super) fn identity(&self) -> PreparedOverridesIdentity {
        let identities = Value::Array(
            self.prepared_override_schemas
                .iter()
                .map(PreparedOverride::identity)
                .collect(),
        );
        let canonical = helm_schema_json_schema_walk::canonical_json_string(&identities);
        let digest = Sha256::digest(canonical.as_bytes());
        let mut digest_hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(digest_hex, "{byte:02x}");
        }
        PreparedOverridesIdentity {
            count: self.prepared_override_schemas.len(),
            digest: digest_hex,
        }
    }

    pub(super) fn into_prepared_override_schemas(self) -> Vec<PreparedOverride> {
        self.prepared_override_schemas
    }
}

/// Loads override schemas before chart generation begins.
///
/// # Errors
///
/// Returns an error when an override exceeds its load budget, cannot be read
/// or decoded, or has a non-schema root.
#[tracing::instrument(skip_all, fields(override_count = paths.len()))]
pub(crate) fn load_emit_request(
    paths: &[PathBuf],
    options: &PolicyInputOptions,
    request: EmitRequest,
) -> EngineResult<LoadedEmitRequest> {
    let loaded_overrides = paths
        .iter()
        .map(|path| {
            Ok(LoadedOverride {
                path: path.clone(),
                schema: load_override_schema(path, options)?,
            })
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(LoadedEmitRequest {
        loaded_overrides,
        request,
    })
}

/// Prepares loaded override schemas according to reference and fetch policy.
///
/// Overrides must not reference generator-owned definitions in the inferred
/// document's `$defs`; their names and presence are private implementation
/// details. Prepared overrides form an ordered set: a later override may
/// reference a same-URI definition carried by an earlier override after
/// namespace deduplication. They must not be consumed independently or reordered.
///
/// # Errors
///
/// Returns an error when an override contains references that policy cannot
/// prepare.
#[tracing::instrument(skip_all, fields(override_count = loaded.loaded_overrides.len()))]
pub(crate) fn prepare_emit_request(
    loaded: LoadedEmitRequest,
    options: &PolicyInputOptions,
    base_schema: &Value,
) -> EngineResult<PreparedEmitRequest> {
    let LoadedEmitRequest {
        loaded_overrides,
        request,
    } = loaded;
    let mut namespace = flatten::BundleNamespace::default();
    namespace.reserve_schema(base_schema);
    for loaded in &loaded_overrides {
        namespace.reserve_schema(loaded.schema.schema());
    }
    let prepared_override_schemas = loaded_overrides
        .into_iter()
        .map(|loaded| {
            let prepared_schema = prepare_override_schema(
                loaded.schema.schema(),
                &loaded.path,
                options,
                request.reference_policy,
                &mut namespace,
            )?;
            Ok(loaded.schema.into_prepared(prepared_schema))
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(PreparedEmitRequest {
        prepared_override_schemas,
        request,
    })
}

#[tracing::instrument(skip_all)]
fn load_override_schema(
    path: &Path,
    options: &PolicyInputOptions,
) -> EngineResult<UnpreparedOverride> {
    let override_schema = load_json_file(path, options.load_budget.max_schema_document_bytes)?;
    if !override_schema.is_object() && !override_schema.is_boolean() {
        return Err(crate::error::CliError::InvalidOverrideRoot {
            path: path.to_path_buf(),
            kind: json_kind(&override_schema),
        });
    }
    Ok(UnpreparedOverride::capture(override_schema))
}

#[tracing::instrument(skip_all, fields(reference_policy = ?reference_policy))]
fn prepare_override_schema(
    schema: &Value,
    override_path: &Path,
    options: &PolicyInputOptions,
    reference_policy: ReferencePolicy,
    namespace: &mut flatten::BundleNamespace,
) -> EngineResult<Value> {
    let override_base = override_path.parent().unwrap_or_else(|| Path::new("."));

    match reference_policy {
        ReferencePolicy::SelfContained => flatten::bundle_refs_in_namespace(
            schema.clone(),
            override_base,
            options.fetch_policy,
            options.load_budget,
            namespace,
        ),
        ReferencePolicy::FullyInlinedExport => flatten::flatten_refs(
            schema,
            override_base,
            options.fetch_policy,
            options.load_budget,
        ),
        ReferencePolicy::PreserveRefs => Ok(schema.clone()),
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn load_json_file(path: &Path, max_bytes: usize) -> EngineResult<Value> {
    let mut file = std::fs::File::open(path)?;
    let bytes = read_to_end_capped(&mut file, max_bytes, path.display().to_string())?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

#[cfg(test)]
#[path = "tests/overrides.rs"]
mod tests;
