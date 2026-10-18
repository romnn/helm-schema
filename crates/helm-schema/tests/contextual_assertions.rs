//! Checks structural assertion identity and independent contextual inheritance.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_k8s::cache::{CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, k8s_cache_path};
use helm_schema_test_support::helm::adjudication::{
    KUBERNETES_RELEASE, OfflineKubernetesValidator, PinnedHelmChart,
};
use helm_schema_test_support::helm::invocation::HelmRunner;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

fn write_chart(root: &Path, values: &Value, template: &str) -> eyre::Result<()> {
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: contextual-assertions
            version: 1.0.0
        "},
    )?;
    fs::write(root.join("values.yaml"), serde_json::to_vec(values)?)?;
    fs::write(root.join("templates/resource.yaml"), template)?;
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
    write_cached_schema(cache, "configmap-v1.json", &schema)
}

fn write_cached_schema(cache: &Path, filename: &str, schema: &Value) -> eyre::Result<()> {
    fs::write(
        cache.join(LAYOUT_MARKER_FILENAME),
        CACHE_LAYOUT_VERSION.to_string(),
    )?;
    let path = k8s_cache_path(
        cache,
        helm_schema_k8s::default_source_id(),
        KUBERNETES_RELEASE,
        filename,
    );
    fs::create_dir_all(path.parent().ok_or_eyre("schema has no parent")?)?;
    fs::write(path, serde_json::to_vec(schema)?)?;
    Ok(())
}

const DATA_TEMPLATE: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: assertions
    data: {{ .Values.data | toJson }}
"};

#[test]
fn forbidden_members_keep_escaped_paths_and_comma_names() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-members")?;
    let cache = ScratchDir::new("contextual-members")?;
    write_chart(
        root.path(),
        &json!({"members": {"a, b": 0, "existing/~": 0}}),
        indoc! {r"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: assertions
            {{ range $name, $value := .Values.members }}
            {{ $name | quote }}: {{ $value | toJson }}
            {{ end }}
        "},
    )?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"apiVersion": {}, "kind": {}, "metadata": {}},
            "additionalProperties": false
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let payload = chart.adjudicate(&json!({
        "members": {"a, b": 1, "existing/~": 1, "added/~": 2}
    }))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;

    // One grouped validator error contains both inherited and newly forbidden members.
    sim_assert_eq!(have: comparisons.len(), want: 1);
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/added~1~0"],
    );
    sim_assert_eq!(
        have: comparison.inherited_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<BTreeSet<_>>(),
        want: BTreeSet::from(["/a, b", "/existing~1~0"]),
    );
    assert!(comparison.uncertain.is_empty());

    // A comma inside one member name cannot identify two different members.
    let payload = chart.adjudicate(&json!({"members": {"a, b": null, "a": 2, "b": 2}}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<BTreeSet<_>>(),
        want: BTreeSet::from(["/a", "/b"]),
    );
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn missing_required_members_do_not_exchange_allowances() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-required")?;
    let cache = ScratchDir::new("contextual-required")?;
    write_chart(
        root.path(),
        &json!({"data": {"name": "role"}}),
        DATA_TEMPLATE,
    )?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"data": {"type": "object", "required": ["apiGroup", "name"]}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let changed = chart.adjudicate(&json!({"data": {"apiGroup": "", "name": null}}))?;

    // Both errors share the required assertion location, but require different members.
    let err = validator
        .check_controls(&chart, &defaults, &[&changed])
        .err()
        .ok_or_eyre("a different missing member passed context usability")?;
    assert!(
        err.to_string()
            .contains("unusable context Kubernetes control"),
        "{err:?}"
    );
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let changed = chart.adjudicate(&json!({"data": {"apiGroup": "", "name": null}}))?;
    let comparisons = validator.compare_with_controls(&chart, &changed, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.key.detail.as_str()).collect::<Vec<_>>(),
        want: vec![r#""name""#],
    );
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn unidentified_list_members_retain_each_missing_identity_field() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-identity")?;
    let cache = ScratchDir::new("contextual-identity")?;
    write_chart(
        root.path(),
        &json!({"includeApiVersion": true, "includeKind": false}),
        indoc! {r"
            apiVersion: v1
            kind: List
            items:
              - metadata:
                  name: child
                {{ if .Values.includeApiVersion }}
                apiVersion: v1
                {{ end }}
                {{ if .Values.includeKind }}
                kind: ConfigMap
                {{ end }}
        "},
    )?;
    write_cached_schema(
        cache.path(),
        "list-v1.json",
        &json!({
            "type": "object",
            "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "List"}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let changed = chart.adjudicate(&json!({"includeApiVersion": false, "includeKind": true}))?;
    let err = validator
        .check_controls(&chart, &defaults, &[&changed])
        .err()
        .ok_or_eyre("a different missing identity field passed context usability")?;
    assert!(
        err.to_string()
            .contains("unusable context Kubernetes control"),
        "{err:?}"
    );
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let changed = chart.adjudicate(&json!({"includeApiVersion": false, "includeKind": true}))?;
    let comparisons = validator.compare_with_controls(&chart, &changed, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/apiVersion"],
    );
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());

    // Removing the remaining identity field preserves the old failure and adds another.
    let neither = chart.adjudicate(&json!({"includeApiVersion": false, "includeKind": false}))?;
    let comparisons = validator.compare_with_controls(&chart, &neither, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/apiVersion"],
    );
    sim_assert_eq!(
        have: comparison.inherited_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/kind"],
    );
    assert!(comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn property_names_remain_structural_even_when_the_validator_reports_a_string() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-property-names")?;
    let cache = ScratchDir::new("contextual-property-names")?;
    write_chart(root.path(), &json!({"data": {"old/~": 0}}), DATA_TEMPLATE)?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"data": {"type": "object", "propertyNames": {"pattern": "^valid"}}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let changed = chart.adjudicate(&json!({"data": {"old/~": null, "new/~": 1}}))?;
    let err = validator
        .check_controls(&chart, &defaults, &[&changed])
        .err()
        .ok_or_eyre("a different invalid member name passed context usability")?;
    assert!(
        err.to_string()
            .contains("unusable context Kubernetes control"),
        "{err:?}"
    );
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let changed = chart.adjudicate(&json!({"data": {"old/~": null, "new/~": 1}}))?;
    let comparisons = validator.compare_with_controls(&chart, &changed, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
        want: vec!["/data/new~1~0"],
    );
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn every_applicable_context_retains_its_own_assertion_count() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-counts")?;
    let cache = ScratchDir::new("contextual-counts")?;
    write_chart(
        root.path(),
        &json!({"copies": 2, "token": 0}),
        indoc! {r"
            {{ range $index := until (int .Values.copies) }}
            ---
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: assertion-{{ $index }}
            data:
              token: {{ $.Values.token }}
            {{ end }}
        "},
    )?;
    write_schema(
        cache.path(),
        &json!({
            "type": "object",
            "properties": {"data": {"type": "object", "additionalProperties": {"type": "string"}}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let one = chart.adjudicate(&json!({"copies": 1, "token": 1}))?;
    let two = chart.adjudicate(&json!({"copies": 2, "token": 2}))?;
    let payload = chart.adjudicate(&json!({"copies": 2, "token": 3}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&one, &two])?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    sim_assert_eq!(
        have: comparisons.iter().map(|comparison| comparison.new_violations.len()).collect::<Vec<_>>(),
        want: vec![1, 0],
    );
    sim_assert_eq!(
        have: comparisons.iter().map(|comparison| comparison.inherited_violations.len()).collect::<Vec<_>>(),
        want: vec![1, 2],
    );
    assert!(
        comparisons
            .iter()
            .all(|comparison| comparison.uncertain.is_empty())
    );

    // Defaults are an applicable control only when explicitly supplied alongside this context.
    let defaults = chart.adjudicate(&json!({}))?;
    let one = chart.adjudicate(&json!({"copies": 1, "token": 1}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults, &one])?;
    let payload = chart.adjudicate(&json!({"copies": 2, "token": 3}))?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    sim_assert_eq!(
        have: comparisons.iter().map(|comparison| comparison.new_violations.len()).collect::<Vec<_>>(),
        want: vec![0, 1],
    );
    sim_assert_eq!(
        have: comparisons.iter().map(|comparison| comparison.inherited_violations.len()).collect::<Vec<_>>(),
        want: vec![2, 1],
    );
    assert!(
        comparisons
            .iter()
            .all(|comparison| comparison.uncertain.is_empty())
    );
    Ok(())
}

#[test]
fn references_preserve_the_canonical_assertion_while_scalar_values_normalize() -> eyre::Result<()> {
    let root = ScratchDir::new("contextual-references")?;
    let cache = ScratchDir::new("contextual-references")?;
    write_chart(
        root.path(),
        &json!({"data": {"branch": 0, "token": 0}}),
        DATA_TEMPLATE,
    )?;
    write_schema(
        cache.path(),
        &json!({
            "definitions": {"token": {"type": "string"}},
            "type": "object",
            "properties": {
                "data": {
                    "type": "object",
                    "if": {"properties": {"branch": {"const": 0}}},
                    "then": {"properties": {"token": {"$ref": "#/definitions/token"}}},
                    "else": {"properties": {"token": {"$ref": "#/definitions/token"}}}
                }
            }
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let same_assertion = chart.adjudicate(&json!({"data": {"token": 1}}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&same_assertion])?;
    let scalar_change = chart.adjudicate(&json!({"data": {"token": 2}}))?;
    let comparisons = validator.compare_with_controls(&chart, &scalar_change, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

    // Both routes apply the same canonical constraint to the same member.
    let defaults = chart.adjudicate(&json!({}))?;
    let other_route = chart.adjudicate(&json!({"data": {"branch": 1, "token": 2}}))?;
    validator.check_controls(&chart, &defaults, &[&other_route])?;
    let other_route = chart.adjudicate(&json!({"data": {"branch": 1, "token": 2}}))?;
    let comparisons = validator.compare_with_controls(&chart, &other_route, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());
    Ok(())
}

#[test]
fn distinct_schema_resources_preserve_integer_and_string_constraints() -> eyre::Result<()> {
    assert_distinct_schema_resources(
        "https://review.test/",
        &json!("integer"),
        &json!("string"),
        &json!(false),
    )
}

#[test]
fn distinct_schema_resources_preserve_overlapping_union_constraints() -> eyre::Result<()> {
    assert_distinct_schema_resources(
        "https://example.test/",
        &json!(["string", "integer"]),
        &json!(["integer", "boolean"]),
        &json!([]),
    )
}

fn assert_distinct_schema_resources(
    resource_base: &str,
    first_type: &Value,
    second_type: &Value,
    payload: &Value,
) -> eyre::Result<()> {
    let root = ScratchDir::new("distinct-schema-resources")?;
    let cache = ScratchDir::new("distinct-schema-resources")?;
    write_chart(root.path(), &json!({"data": "ok"}), DATA_TEMPLATE)?;
    write_schema(
        cache.path(),
        &json!({
            "$defs": {
                "a": {"$id": format!("{resource_base}a"), "type": first_type},
                "b": {"$id": format!("{resource_base}b"), "type": second_type}
            },
            "properties": {"data": {"allOf": [
                {"$ref": format!("{resource_base}a")},
                {"$ref": format!("{resource_base}b")}
            ]}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let overlay = json!({"data": payload});
    let payload = chart.adjudicate(&overlay)?;
    let mut comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let payload = chart.adjudicate(&overlay)?;
    comparisons.push(validator.compare_with_defaults(&chart, &payload)?);

    // Both constraints use /type at /data, but belong to different schema resources.
    sim_assert_eq!(
        have: comparisons.iter().map(|comparison| (
            comparison.new_violations.len(), comparison.inherited_violations.len()
        )).collect::<Vec<_>>(),
        want: vec![(1, 1), (1, 1)],
    );
    assert!(
        comparisons
            .iter()
            .all(|comparison| comparison.uncertain.is_empty())
    );
    let defaults = chart.adjudicate(&json!({}))?;
    let payload = chart.adjudicate(&overlay)?;
    assert!(
        validator
            .check_controls(&chart, &defaults, &[&payload])
            .is_err()
    );
    Ok(())
}

#[test]
fn unproved_schema_resource_correspondence_fails_in_both_lanes() -> eyre::Result<()> {
    for schema in [
        json!({
            "$defs": {"token": {"$dynamicAnchor": "token", "type": "string"}},
            "properties": {"data": {"$dynamicRef": "#token"}}
        }),
        json!({"properties": {"data": {"allOf": [
            {"$id": "", "type": "string"},
            {"$id": "", "type": "string"}
        ]}}}),
        json!({"properties": {"data": {"allOf": [
            {"$id": "https://helm-schema.invalid/resource", "type": "string"},
            {"$id": "https://helm-schema.invalid/resource", "type": "string"}
        ]}}}),
    ] {
        let root = ScratchDir::new("contextual-unproved-reference")?;
        let cache = ScratchDir::new("contextual-unproved-reference")?;
        write_chart(root.path(), &json!({"data": "ok"}), DATA_TEMPLATE)?;
        write_schema(cache.path(), &schema)?;
        let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
        let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
        let defaults = chart.adjudicate(&json!({}))?;
        let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
        let payload = chart.adjudicate(&json!({"data": false}))?;
        let contextual = validator.compare_with_controls(&chart, &payload, &checked);
        let payload = chart.adjudicate(&json!({"data": false}))?;
        let defaults_only = validator.compare_with_defaults(&chart, &payload);
        for error in [contextual.err(), defaults_only.err()] {
            let error = error.ok_or_eyre("unproved schema correspondence reached comparison")?;
            assert!(
                error.to_string().contains("unresolved assertion identity"),
                "{error:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn repeated_references_to_one_constraint_count_one_occurrence() -> eyre::Result<()> {
    // Pointer, URI and anchor aliases must prove the same actual constraint node.
    for (token_schema, token_ref) in [
        (json!({"type": "string"}), "#/definitions/token"),
        (
            json!({"$id": "https://review.test/shared", "type": "string"}),
            "https://review.test/shared",
        ),
        (json!({"$id": "shared", "type": "string"}), "shared"),
        (json!({"$anchor": "token", "type": "string"}), "#token"),
        (
            json!({"type": "string"}),
            "https://helm-schema.invalid/resource#/definitions/token",
        ),
    ] {
        let root = ScratchDir::new("contextual-repeated-references")?;
        let cache = ScratchDir::new("contextual-repeated-references")?;
        write_chart(
            root.path(),
            &json!({"data": {"branch": 0, "token": 0}}),
            DATA_TEMPLATE,
        )?;
        write_schema(
            cache.path(),
            &json!({
                "$id": "https://review.test/root",
                "definitions": {"token": token_schema},
                "properties": {"data": {
                    "if": {"properties": {"branch": {"const": 0}}},
                    "then": {"properties": {"token": {"$ref": "#/definitions/token"}}},
                    "else": {"properties": {"token": {"allOf": [
                        {"$ref": token_ref}, {"$ref": token_ref}
                    ]}}}
                }}
            }),
        )?;
        let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
        let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
        let defaults = chart.adjudicate(&json!({}))?;
        let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
        let payload = chart.adjudicate(&json!({"data": {"branch": 1, "token": 2}}))?;
        let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
        let comparison = comparisons
            .first()
            .ok_or_eyre("missing context comparison")?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

        let payload = chart.adjudicate(&json!({"data": {"branch": 1, "token": 2}}))?;
        let comparison = validator.compare_with_defaults(&chart, &payload)?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());

        let defaults = chart.adjudicate(&json!({}))?;
        let context = chart.adjudicate(&json!({"data": {"branch": 1, "token": 2}}))?;
        validator.check_controls(&chart, &defaults, &[&context])?;
    }
    Ok(())
}

#[test]
fn duplicate_reports_preserve_occurrences_at_distinct_positions_and_resources() -> eyre::Result<()>
{
    let root = ScratchDir::new("distinct-repeated-references")?;
    let cache = ScratchDir::new("distinct-repeated-references")?;
    write_chart(
        root.path(),
        &json!({"copies": 1, "data": {"token": 0}}),
        indoc! {r"
            {{ range $index := until (int .Values.copies) }}
            ---
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: occurrence-{{ $index }}
            data: {{ $.Values.data | toJson }}
            {{ end }}
        "},
    )?;
    write_schema(
        cache.path(),
        &json!({
            "definitions": {"token": {"type": "string"}},
            "properties": {"data": {"additionalProperties": {"allOf": [
                {"$ref": "#/definitions/token"}, {"$ref": "#/definitions/token"}
            ]}}}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    for overlay in [json!({"data": {"extra": 1}}), json!({"copies": 2})] {
        let payload = chart.adjudicate(&overlay)?;
        let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
        let comparison = comparisons
            .first()
            .ok_or_eyre("missing context comparison")?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
        assert!(comparison.uncertain.is_empty());

        let payload = chart.adjudicate(&overlay)?;
        let comparison = validator.compare_with_defaults(&chart, &payload)?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
        assert!(comparison.uncertain.is_empty());
    }
    Ok(())
}
