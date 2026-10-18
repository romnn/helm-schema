//! Exhaustive provenance preflight of the real pinned Kubernetes and CRD schemas.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre;
use jsonschema::Draft;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use test_util::prelude::sim_assert_eq;

use crate::helm::adjudication::{KUBERNETES_RELEASE, ResourceSchema};
use crate::helm::assertion_identity::{
    canonical_schema_location, verify_execution, verify_registry_integrity,
};

#[test]
fn execution_failure_check_descends_into_aggregate_and_property_name_errors() -> eyre::Result<()> {
    let pattern = json!({"pattern": "^(?:(a|b|ab)*(?=c)c|.*)$"});
    let text = "ab".repeat(64);
    for schema in [
        json!({"anyOf": [pattern, {"type": "integer"}]}),
        json!({"oneOf": [pattern, {"type": "integer"}]}),
        json!({"oneOf": [pattern, {"type": "string"}, {"minLength": 1}]}),
        json!({"anyOf": [{"oneOf": [pattern, {"type": "integer"}]}]}),
    ] {
        for document in [json!(text), json!({text.clone(): 0})] {
            let schema = if document.is_object() {
                json!({"propertyNames": schema})
            } else {
                schema.clone()
            };
            let wrapped = jsonschema::validator_for(&schema)?;
            let errors: Vec<_> = wrapped.iter_errors(&document).collect();
            eyre::ensure!(!errors.is_empty(), "missing execution failure control");
            for error in errors {
                assert!(verify_execution(&error).is_err(), "{error:?}");
            }
        }
    }
    for error in jsonschema::validator_for(&json!({"anyOf": [{"type": "integer"}]}))?
        .iter_errors(&json!("ordinary mismatch"))
    {
        verify_execution(&error)?;
    }
    Ok(())
}

#[test]
fn pinned_bundle_assertion_provenance_audit() -> eyre::Result<()> {
    let bundle = test_util::workspace_testdata().join("provider-bundle");
    let files = pinned_schema_files(&bundle)?;
    eyre::ensure!(!files.is_empty(), "pinned schema audit selected nothing");
    let mut digest = Sha256::new();
    let mut nodes = 0;
    let mut keywords = BTreeMap::<String, usize>::new();
    let mut errors_checked = 0;
    let mut pattern_pairs_checked = 0;
    let mut compilation_failures = BTreeMap::new();
    let mut kubernetes = 0;
    let mut crds = 0;
    for file in &files {
        let relative = file.strip_prefix(&bundle)?;
        if relative.starts_with("kubernetes-json-schema-cache") {
            kubernetes += 1;
        } else {
            crds += 1;
        }
        let bytes = fs::read(file)?;
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(&bytes);
        let schema: Value = serde_json::from_slice(&bytes)?;
        let base = "https://helm-schema.invalid/resource";
        verify_registry_integrity(&schema, Draft::Draft7, base)?;
        let survey = survey_schema(&schema, relative, &mut keywords)?;
        nodes += survey.nodes;
        let compiled = match ResourceSchema::compile(&schema, Draft::Draft7) {
            Ok(compiled) => compiled,
            Err(error) => {
                compilation_failures.insert(
                    relative.to_string_lossy().to_string(),
                    pattern_compilation_diagnostics(survey.patterns, &error),
                );
                continue;
            }
        };
        for document in [
            json!(null),
            json!(true),
            json!(0),
            json!(""),
            json!("ab".repeat(64)),
            json!([]),
            json!({}),
            json!({"metadata": false, "spec": false, "data": false, "unexpected": false}),
        ] {
            pattern_pairs_checked += compiled.verify_pattern_execution(&document)?;
            for error in compiled.validator.iter_errors(&document) {
                canonical_schema_location(&compiled.registry, base, &error)?;
                eyre::ensure!(
                    document.pointer(error.instance_path().as_str())
                        == Some(error.instance().as_ref()),
                    "reported instance mismatch in {}",
                    relative.display()
                );
                errors_checked += 1;
            }
        }
    }
    eyre::ensure!(
        kubernetes > 0 && crds > 0 && errors_checked > 0,
        "vacuous provenance audit"
    );
    // These upstream CRDs already fail the pinned validator's regex meta-validation.
    sim_assert_eq!(
        have: compilation_failures.keys().map(String::as_str).collect::<Vec<_>>(),
        want: vec![
            "crds-catalog-cache/default/cilium.io/ciliumnetworkpolicy_v2.json",
            "crds-catalog-cache/default/monitoring.coreos.com/prometheusrule_v1.json",
            "crds-catalog-cache/default/projectcalico.org/felixconfiguration_v3.json",
            "crds-catalog-cache/default/projectcontour.io/httpproxy_v1.json",
        ],
    );
    let mut bundle_sha256 = String::new();
    for byte in digest.finalize() {
        write!(bundle_sha256, "{byte:02x}")?;
    }
    eprintln!(
        "PINNED_ASSERTION_AUDIT {}",
        json!({
            "kubernetes_schemas": kubernetes, "crd_schemas": crds,
            "schema_nodes": nodes, "schema_keywords": keywords,
            "reported_errors_checked": errors_checked,
            "pattern_execution_pairs_checked": pattern_pairs_checked,
            "compilation_failures": compilation_failures,
            "registry_failures": 0, "unresolved_errors": 0,
            "potentially_new_unresolved_constraints": 0,
            "bundle_sha256": bundle_sha256,
        })
    );
    Ok(())
}

fn pattern_compilation_diagnostics(patterns: Vec<&Value>, error: &eyre::Report) -> Vec<String> {
    let mut failures = Vec::new();
    for pattern in patterns {
        if let Err(pattern_error) = jsonschema::options()
            .with_draft(Draft::Draft7)
            .build(&json!({"pattern": pattern}))
        {
            failures.push(pattern_error.to_string());
        }
    }
    if failures.is_empty() {
        failures.push(error.to_string());
    }
    failures.sort();
    failures.dedup();
    failures
}

struct SchemaSurvey<'a> {
    nodes: usize,
    patterns: Vec<&'a Value>,
}

fn survey_schema<'a>(
    schema: &'a Value,
    relative: &Path,
    keywords: &mut BTreeMap<String, usize>,
) -> eyre::Result<SchemaSurvey<'a>> {
    let mut schemas = vec![schema];
    let mut survey = SchemaSurvey {
        nodes: 0,
        patterns: Vec::new(),
    };
    while let Some(node) = schemas.pop() {
        survey.nodes += 1;
        if let Some(pattern) = node.get("pattern") {
            survey.patterns.push(pattern);
        }
        if let Some(object) = node.as_object() {
            for keyword in object.keys() {
                *keywords.entry(keyword.clone()).or_default() += 1;
            }
        }
        // These constructors omit provenance or member correspondence in the pinned validator.
        for keyword in [
            "propertyNames",
            "dependencies",
            "dependentRequired",
            "minContains",
            "maxContains",
            "unevaluatedItems",
            "unevaluatedProperties",
            "$dynamicRef",
            "$recursiveRef",
        ] {
            eyre::ensure!(
                node.get(keyword).is_none(),
                "{} contains {keyword}",
                relative.display()
            );
        }
        for child in Draft::Draft7.subresources_of(node) {
            if child == &Value::Bool(false) {
                // Named/patterned forbidden properties have exact member diagnostics.
                eyre::ensure!(
                    node.get("additionalProperties")
                        .is_some_and(|value| std::ptr::eq(value, child))
                        && ["properties", "patternProperties"]
                            .iter()
                            .any(|keyword| node
                                .get(keyword)
                                .and_then(Value::as_object)
                                .is_some_and(|properties| !properties.is_empty())),
                    "{} contains an unproved boolean schema",
                    relative.display()
                );
            } else {
                schemas.push(child);
            }
        }
    }
    Ok(survey)
}

fn pinned_schema_files(bundle: &Path) -> eyre::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![
        bundle
            .join("kubernetes-json-schema-cache/default")
            .join(KUBERNETES_RELEASE),
        bundle.join("crds-catalog-cache"),
    ];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
