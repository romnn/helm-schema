use std::collections::BTreeMap;
use std::path::Path;

use helm_schema_json_schema_minify::{DefinitionOrigin, minimize_schema, name_definitions};
use serde_json::Value;

use crate::error::EngineResult;
use crate::flatten;
use crate::output_pipeline::annotation::{FinalOutputPolicy, annotate_final_schema};
use crate::output_pipeline::descriptions::strip_schema_descriptions;
use crate::output_pipeline::reachability::{OwnedDefinitions, prune_unreachable_owned_definitions};
use crate::output_pipeline::{OutputPipelineOptions, PreparedEmitRequest, ReferencePolicy};
use crate::schema_override;

/// Applies overrides, reference policy, final minimization, and the readable
/// names of anonymous definitions, the generator's private handles among them.
///
/// # Errors
///
/// Returns an error when prepared references cannot be validated, bundled, or
/// fully inlined under the requested output policy.
#[tracing::instrument(
    skip_all,
    fields(
        override_count = prepared.override_count(),
        reference_policy = ?prepared.request.reference_policy,
        strip_descriptions = prepared.request.output.strip_descriptions,
        minimize = prepared.request.output.minimize,
    )
)]
pub(crate) fn apply_schema_output_pipeline(
    mut schema: Value,
    definition_origins: &BTreeMap<String, Vec<DefinitionOrigin>>,
    prepared: PreparedEmitRequest,
    base_dir: &Path,
    policy: FinalOutputPolicy,
) -> EngineResult<Value> {
    let options = prepared.request.output;
    let reference_policy = prepared.request.reference_policy;
    let generated_definitions = OwnedDefinitions::capture(&schema);
    let override_identity = prepared.identity();
    for override_schema in prepared.into_prepared_override_schemas() {
        schema = schema_override::apply_prepared_schema_override(schema, override_schema);
    }
    let generated_definitions = generated_definitions.retain_unchanged(&schema);
    // An override that replaced a handle's body owns that definition now.
    let mut anonymous = BTreeMap::new();
    for (handle, origins) in definition_origins {
        if generated_definitions.owns_root_definition(handle) {
            anonymous.insert(handle.clone(), origins.clone());
        }
    }

    schema = apply_output_transforms(
        schema,
        base_dir,
        reference_policy,
        options,
        &generated_definitions,
        &anonymous,
    )?;
    annotate_final_schema(schema, policy, &override_identity, reference_policy)
}

#[tracing::instrument(
    skip_all,
    fields(
        reference_policy = ?reference_policy,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
fn apply_output_transforms(
    mut schema: Value,
    base_dir: &Path,
    reference_policy: ReferencePolicy,
    options: OutputPipelineOptions,
    generated_definitions: &OwnedDefinitions,
    anonymous: &BTreeMap<String, Vec<DefinitionOrigin>>,
) -> EngineResult<Value> {
    match reference_policy {
        ReferencePolicy::SelfContained => schema = flatten::bundle_prepared_refs(schema, base_dir)?,
        ReferencePolicy::FullyInlinedExport => {
            schema = flatten::flatten_prepared_refs(&schema, base_dir)?;
        }
        ReferencePolicy::PreserveRefs => {}
    }

    if options.strip_descriptions {
        strip_schema_descriptions(&mut schema);
    }

    // Remove dead owned bodies before minimization can extract new definitions
    // from them and lose their original ownership.
    prune_unreachable_owned_definitions(&mut schema, generated_definitions);
    if options.minimize {
        schema = minimize_schema(schema, anonymous, options.definition_names);
    } else {
        name_definitions(&mut schema, anonymous, options.definition_names);
    }

    Ok(schema)
}

#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;
