//! Checks contextual inheritance across equivalent array and map locations.

use std::fs;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_k8s::cache::{CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, k8s_cache_path};
use helm_schema_test_support::helm::adjudication::{
    KUBERNETES_RELEASE, KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart,
};
use helm_schema_test_support::helm::invocation::HelmRunner;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

fn write_chart(root: &Path, data: &Value) -> eyre::Result<()> {
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: canonical-assertions
            version: 1.0.0
        "},
    )?;
    fs::write(
        root.join("values.yaml"),
        serde_json::to_vec(&json!({"data": data}))?,
    )?;
    fs::write(
        root.join("templates/resource.yaml"),
        indoc! {r"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: assertions
            data: {{ .Values.data | toJson }}
        "},
    )?;
    Ok(())
}

fn write_schema(cache: &Path, schema: &Value) -> eyre::Result<()> {
    let mut schema = schema.clone();
    schema
        .as_object_mut()
        .ok_or_eyre("synthetic schema is not an object")?
        .insert(
            "x-kubernetes-group-version-kind".into(),
            json!([{"group": "", "version": "v1", "kind": "ConfigMap"}]),
        );
    fs::write(
        cache.join(LAYOUT_MARKER_FILENAME),
        CACHE_LAYOUT_VERSION.to_string(),
    )?;
    let path = k8s_cache_path(
        cache,
        helm_schema_k8s::default_source_id(),
        KUBERNETES_RELEASE,
        "configmap-v1.json",
    );
    fs::create_dir_all(path.parent().ok_or_eyre("schema has no parent")?)?;
    fs::write(path, serde_json::to_vec(&schema)?)?;
    Ok(())
}

#[test]
fn homogeneous_array_relocation_inherits_once_and_keeps_additional_failures_new() -> eyre::Result<()>
{
    let root = ScratchDir::new("canonical-array")?;
    let cache = ScratchDir::new("canonical-array")?;
    write_chart(root.path(), &json!([{"name": "bad", "image": 42}]))?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"data": {"type": "array", "items": {
                "type": "object", "properties": {"image": {"type": "string"}}
            }}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let relocation = json!({"data": [
        {"name": "good", "image": "busybox"}, {"name": "bad", "image": 42}
    ]});
    let payload = chart.adjudicate(&relocation)?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

    let payload = chart.adjudicate(&json!({"data": [
        {"name": "good", "image": "busybox"},
        {"name": "bad", "image": 42},
        {"name": "also-bad", "image": 43}
    ]}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.uncertain.is_empty());

    // Legacy attribution retains its existing exact-position meaning.
    let payload = chart.adjudicate(&relocation)?;
    let legacy = validator.compare_with_defaults(&chart, &payload)?;
    sim_assert_eq!(have: legacy.new_violations.len(), want: 1);
    assert!(legacy.inherited_violations.is_empty() && legacy.uncertain.is_empty());
    Ok(())
}

#[test]
fn map_key_renaming_uses_only_additional_and_pattern_property_positions() -> eyre::Result<()> {
    struct Case {
        defaults: Value,
        payload: Value,
        new_count: usize,
        inherited_count: usize,
    }

    let cache = ScratchDir::new("canonical-map")?;
    // All three routes reach one canonical type assertion; fixed names still identify a field.
    write_schema(
        cache.path(),
        &json!({
            "definitions": {"token": {"type": "string"}},
            "type": "object",
            "properties": {"data": {
                "type": "object",
                "properties": {"fixed": {"$ref": "#/definitions/token"}},
                "patternProperties": {"^mapped/": {"$ref": "#/definitions/token"}},
                "additionalProperties": {"$ref": "#/definitions/token"}
            }}
        }),
    )?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    for case in [
        Case {
            defaults: json!({"old/~": []}),
            payload: json!({"old/~": null, "new/~": []}),
            new_count: 0,
            inherited_count: 1,
        },
        Case {
            defaults: json!({"mapped/old": []}),
            payload: json!({"mapped/old": null, "mapped/new": []}),
            new_count: 0,
            inherited_count: 1,
        },
        Case {
            defaults: json!({"old": []}),
            payload: json!({"old": null, "fixed": []}),
            new_count: 1,
            inherited_count: 0,
        },
        Case {
            defaults: json!({"old": []}),
            payload: json!({"new": []}),
            new_count: 1,
            inherited_count: 1,
        },
    ] {
        let root = ScratchDir::new("canonical-map")?;
        write_chart(root.path(), &case.defaults)?;
        let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
        let defaults = chart.adjudicate(&json!({}))?;
        let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
        let payload = chart.adjudicate(&json!({"data": case.payload}))?;
        let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
        let comparison = comparisons
            .first()
            .ok_or_eyre("missing context comparison")?;
        sim_assert_eq!(have: comparison.new_violations.len(), want: case.new_count);
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: case.inherited_count);
        assert!(comparison.uncertain.is_empty());
    }
    Ok(())
}

#[test]
fn distinct_positional_constraints_do_not_share_array_allowances() -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-positions")?;
    let cache = ScratchDir::new("canonical-positions")?;
    write_chart(root.path(), &json!([[], 1]))?;
    write_schema(
        cache.path(),
        &json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"data": {"type": "array", "items": [
                {"type": "string"}, {"type": "integer"}
            ]}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let payload = chart.adjudicate(&json!({"data": ["valid", []]}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn empty_numeric_and_keyword_property_names_follow_their_schema_position() -> eyre::Result<()> {
    for fixed in [false, true] {
        let cache = ScratchDir::new("canonical-special-names")?;
        let properties = if fixed {
            json!({
                "": {"$ref": "#/definitions/token"},
                "0": {"$ref": "#/definitions/token"},
                "properties": {"$ref": "#/definitions/token"},
                "items": {"$ref": "#/definitions/token"},
                "*": {"$ref": "#/definitions/token"}
            })
        } else {
            json!({})
        };
        write_schema(
            cache.path(),
            &json!({
                "definitions": {"token": {"type": "string"}},
                "type": "object",
                "properties": {"data": {
                    "type": "object",
                    "properties": properties,
                    "additionalProperties": {"$ref": "#/definitions/token"}
                }}
            }),
        )?;
        let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
        for property in ["", "0", "properties", "items", "*"] {
            let root = ScratchDir::new("canonical-special-names")?;
            write_chart(root.path(), &json!({(property): []}))?;
            let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
            let defaults = chart.adjudicate(&json!({}))?;
            let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
            let payload = chart.adjudicate(&json!({"data": {(property): null, "moved": []}}))?;
            let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
            let comparison = comparisons
                .first()
                .ok_or_eyre("missing context comparison")?;

            // Literal property spelling cannot turn a fixed field into a collection wildcard.
            sim_assert_eq!(have: comparison.new_violations.len(), want: usize::from(fixed));
            sim_assert_eq!(have: comparison.inherited_violations.len(), want: usize::from(!fixed));
            sim_assert_eq!(
                have: comparison.new_violations.iter().chain(&comparison.inherited_violations).map(|violation| violation.key.instance_path.as_str()).collect::<Vec<_>>(),
                want: vec!["/data/moved"],
            );
            assert!(comparison.uncertain.is_empty());
        }
    }
    Ok(())
}

#[test]
fn nested_arrays_of_maps_retain_concrete_locations_and_failure_multiplicity() -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-nested-collections")?;
    let cache = ScratchDir::new("canonical-nested-collections")?;
    write_chart(root.path(), &json!([{"original": []}]))?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"data": {"type": "array", "items": {
                "type": "object", "additionalProperties": {"type": "string"}
            }}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let payload = chart.adjudicate(&json!({"data": [{"valid": "yes"}, {"renamed/~": []}]}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.inherited_violations.iter().map(|violation| violation.key.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/data/1/renamed~1~0"],
    );
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

    let payload =
        chart.adjudicate(&json!({"data": [{"valid": "yes"}, {"first": [], "second": []}]}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn prefix_item_constraints_remain_distinct_after_array_normalization() -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-prefix-positions")?;
    let cache = ScratchDir::new("canonical-prefix-positions")?;
    write_chart(root.path(), &json!([[], 1]))?;
    write_schema(
        cache.path(),
        &json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {"data": {"type": "array", "prefixItems": [
                {"type": "string"}, {"type": "integer"}
            ]}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let payload = chart.adjudicate(&json!({"data": ["valid", []]}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.key.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/data/1"],
    );
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn changing_an_invalid_values_json_type_preserves_the_canonical_assertion() -> eyre::Result<()> {
    struct Case {
        defaults: Value,
        payload: Value,
    }

    let cache = ScratchDir::new("canonical-value-types")?;
    write_schema(
        cache.path(),
        &json!({"type": "object", "properties": {"data": {"type": "string"}}}),
    )?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    for case in [
        Case {
            defaults: json!(42),
            payload: json!(true),
        },
        Case {
            defaults: json!([]),
            payload: json!({}),
        },
    ] {
        let root = ScratchDir::new("canonical-value-types")?;
        write_chart(root.path(), &case.defaults)?;
        let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
        let defaults = chart.adjudicate(&json!({}))?;
        let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
        let payload = chart.adjudicate(&json!({"data": case.payload}))?;
        let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
        let comparison = comparisons
            .first()
            .ok_or_eyre("missing context comparison")?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());
    }
    Ok(())
}

#[test]
fn dependent_required_triggers_keep_their_canonical_constraint_identity() -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-dependent-required")?;
    let cache = ScratchDir::new("canonical-dependent-required")?;
    write_chart(root.path(), &json!({"first": true}))?;
    write_schema(
        cache.path(),
        &json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {"data": {
                "type": "object",
                "allOf": [
                    {"dependentRequired": {"first": ["token"]}},
                    {"dependentRequired": {"second": ["token"]}}
                ]
            }}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let payload = chart.adjudicate(&json!({"data": {"first": false}}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

    // Equal missing-member names do not merge different triggering assertions.
    let payload = chart.adjudicate(&json!({"data": {"first": null, "second": true}}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn ambiguous_unevaluated_members_block_attribution_despite_a_known_fixed_failure()
-> eyre::Result<()> {
    // The validator groups forbidden names and invalid dynamic values into the same error kind.
    for unevaluated in [json!(false), json!({"type": "string"})] {
        let root = ScratchDir::new("canonical-unevaluated-ambiguity")?;
        let cache = ScratchDir::new("canonical-unevaluated-ambiguity")?;
        write_chart(root.path(), &json!({"fixed": "valid"}))?;
        write_schema(
            cache.path(),
            &json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {"data": {
                    "type": "object",
                    "properties": {"fixed": {"type": "string"}},
                    "unevaluatedProperties": unevaluated
                }}
            }),
        )?;
        let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
        let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
        let defaults = chart.adjudicate(&json!({}))?;
        let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
        let overlay = json!({"data": {"fixed": [], "dynamic": []}});
        let payload = chart.adjudicate(&overlay)?;
        let document: Value = serde_yaml::from_slice(&payload.rendered.stdout)?;
        let KubernetesVerdict::Invalid(violations) = validator.validate_documents(&[document])
        else {
            eyre::bail!("legacy validation did not prove the fixture violations");
        };
        assert!(violations.iter().any(|violation| {
            violation.key.instance_path == "/data/fixed" && violation.key.keyword == "type"
        }));
        assert!(
            violations
                .iter()
                .any(|violation| violation.key.keyword == "unevaluatedProperties")
        );
        let err = validator
            .compare_with_controls(&chart, &payload, &checked)
            .err()
            .ok_or_eyre("a known fixed failure concealed ambiguous member correspondence")?;
        assert!(
            err.to_string()
                .contains("unresolved contextual assertion identity"),
            "{err:?}"
        );
        assert!(err.to_string().contains("unevaluatedProperties"), "{err:?}");

        let defaults = chart.adjudicate(&json!({}))?;
        let context = chart.adjudicate(&overlay)?;
        let err = validator
            .check_controls(&chart, &defaults, &[&context])
            .err()
            .ok_or_eyre("ambiguous invalid context passed usability checking")?;
        assert!(
            err.to_string()
                .contains("unresolved contextual assertion identity"),
            "{err:?}"
        );
        assert!(err.to_string().contains("unevaluatedProperties"), "{err:?}");
    }
    Ok(())
}
