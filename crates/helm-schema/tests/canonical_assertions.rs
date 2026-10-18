//! Checks contextual inheritance across equivalent array and map locations.

use std::fs;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_k8s::cache::{CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, k8s_cache_path};
use helm_schema_test_support::helm::adjudication::{
    DefaultsComparison, KUBERNETES_RELEASE, KubernetesVerdict, OfflineKubernetesValidator,
    PinnedHelmChart,
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

    Ok(())
}

#[test]
fn defaults_comparison_inherits_failure_after_valid_array_prepend() -> eyre::Result<()> {
    let root = ScratchDir::new("defaults-array-prepend")?;
    let cache = ScratchDir::new("defaults-array-prepend")?;
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
    let payload = chart.adjudicate(&json!({"data": [
        {"name": "good", "image": "busybox"}, {"name": "bad", "image": 42}
    ]}))?;
    let comparison = validator.compare_with_defaults(&chart, &payload)?;
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    assert!(comparison.new_violations.is_empty() && comparison.uncertain.is_empty());
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
                have: comparison.new_violations.iter().chain(&comparison.inherited_violations).map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
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
        have: comparison.inherited_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
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
        have: comparison.new_violations.iter().map(|violation| violation.instance_path.as_str()).collect::<Vec<_>>(),
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
        let mut fixed_only = document.clone();
        fixed_only
            .pointer_mut("/data")
            .and_then(Value::as_object_mut)
            .ok_or_eyre("fixture data is not an object")?
            .remove("dynamic")
            .ok_or_eyre("fixture has no dynamic member")?;
        let KubernetesVerdict::Invalid(violations) = validator.validate_documents(&[fixed_only])
        else {
            eyre::bail!("resource validation did not prove the fixed-member violation");
        };
        assert!(violations.iter().any(|violation| {
            violation.instance_path == "/data/fixed" && violation.key.keyword == "type"
        }));

        let KubernetesVerdict::Uncertain(reasons) = validator.validate_documents(&[document])
        else {
            eyre::bail!(
                "known fixed-member failure concealed ambiguous identity during validation"
            );
        };
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("unevaluatedProperties"))
        );
        let err = validator
            .compare_with_controls(&chart, &payload, &checked)
            .err()
            .ok_or_eyre("a known fixed failure concealed ambiguous member correspondence")?;
        assert!(
            err.to_string().contains("unresolved assertion identity"),
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
            err.to_string().contains("unresolved assertion identity"),
            "{err:?}"
        );
        assert!(err.to_string().contains("unevaluatedProperties"), "{err:?}");
    }
    Ok(())
}

fn assert_shared_reference_positions_stay_distinct(schema: &Value) -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-shared-position")?;
    let cache = ScratchDir::new("canonical-shared-position")?;
    write_chart(root.path(), &json!([42, "ok"]))?;
    write_schema(cache.path(), schema)?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let overlay = json!({"data": ["ok", false]});
    let payload = chart.adjudicate(&overlay)?;
    let comparisons = validator.compare_with_controls(&chart, &payload, &checked)?;
    let comparison = comparisons
        .first()
        .ok_or_eyre("missing context comparison")?;
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());

    let payload = chart.adjudicate(&overlay)?;
    let comparison = validator.compare_with_defaults(&chart, &payload)?;
    sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
    assert!(comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty());

    // Reusing a canonical constraint does not let a different tuple slot pass usability.
    let defaults = chart.adjudicate(&json!({}))?;
    let context = chart.adjudicate(&overlay)?;
    let err = validator
        .check_controls(&chart, &defaults, &[&context])
        .err()
        .ok_or_eyre("a different invalid tuple slot passed context usability")?;
    assert!(
        err.to_string()
            .contains("unusable context Kubernetes control"),
        "{err:?}"
    );
    Ok(())
}

#[test]
fn tuple_items_sharing_a_reference_retain_position() -> eyre::Result<()> {
    assert_shared_reference_positions_stay_distinct(&json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {"s": {"type": "string"}},
        "type": "object",
        "properties": {"data": {"type": "array", "items": [
            {"$ref": "#/definitions/s"}, {"$ref": "#/definitions/s"}
        ]}}
    }))
}

#[test]
fn prefix_items_sharing_a_reference_retain_position() -> eyre::Result<()> {
    assert_shared_reference_positions_stay_distinct(&json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {"s": {"type": "string"}},
        "type": "object",
        "properties": {"data": {"type": "array", "prefixItems": [
            {"$ref": "#/$defs/s"}, {"$ref": "#/$defs/s"}
        ]}}
    }))
}

fn assert_grouped_array_result(
    result: eyre::Result<DefaultsComparison>,
    keyword: &str,
) -> eyre::Result<()> {
    if keyword == "additionalItems" {
        let comparison = result?;
        sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
        sim_assert_eq!(have: comparison.new_violations.len(), want: 1);
        assert!(comparison.uncertain.is_empty());
    } else {
        sim_assert_eq!(have: keyword, want: "unevaluatedItems");
        let err = result
            .err()
            .ok_or_eyre("ambiguous grouped items produced a resolved comparison")?;
        assert!(
            err.to_string().contains("unresolved assertion identity"),
            "{err:?}"
        );
        assert!(err.to_string().contains(keyword), "{err:?}");
    }
    Ok(())
}

fn assert_grouped_array_addition_is_not_all_inherited(
    schema: &Value,
    defaults_data: &Value,
    payload_data: &Value,
    keyword: &str,
) -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-grouped-array")?;
    let cache = ScratchDir::new("canonical-grouped-array")?;
    write_chart(root.path(), defaults_data)?;
    write_schema(cache.path(), schema)?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let payload = chart.adjudicate(&json!({"data": payload_data}))?;
    assert_grouped_array_result(validator.compare_with_defaults(&chart, &payload), keyword)?;

    let defaults = chart.adjudicate(&json!({}))?;
    let context = chart.adjudicate(&json!({"context": true}))?;
    let comparison = match validator.check_controls(&chart, &defaults, &[&context]) {
        Ok(checked) => {
            let payload = chart.adjudicate(&json!({"data": payload_data, "context": true}))?;
            validator
                .compare_with_controls(&chart, &payload, &checked)
                .and_then(|comparisons| {
                    comparisons
                        .into_iter()
                        .next()
                        .ok_or_eyre("missing context comparison")
                })
        }
        Err(err) => Err(err),
    };
    assert_grouped_array_result(comparison, keyword)?;
    Ok(())
}

#[test]
fn grouped_additional_items_preserve_added_invalid_elements() -> eyre::Result<()> {
    assert_grouped_array_addition_is_not_all_inherited(
        &json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"data": {
                "type": "array",
                "items": [{"type": "string"}],
                "additionalItems": false
            }}
        }),
        &json!(["ok", 42]),
        &json!(["ok", 42, 43]),
        "additionalItems",
    )
}

#[test]
fn grouped_unevaluated_items_fail_unresolved() -> eyre::Result<()> {
    assert_grouped_array_addition_is_not_all_inherited(
        &json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {"data": {
                "type": "array",
                "unevaluatedItems": {"type": "string"}
            }}
        }),
        &json!([42]),
        &json!([42, 43]),
        "unevaluatedItems",
    )
}

#[test]
fn conflicting_fixed_and_pattern_roles_for_one_occurrence_fail_unresolved() -> eyre::Result<()> {
    let root = ScratchDir::new("canonical-conflicting-roles")?;
    let cache = ScratchDir::new("canonical-conflicting-roles")?;
    write_chart(root.path(), &json!({"token": "valid"}))?;
    write_schema(
        cache.path(),
        &json!({
            "definitions": {"s": {"type": "string"}},
            "properties": {"data": {
                "properties": {"token": {"$ref": "#/definitions/s"}},
                "patternProperties": {"^token$": {"$ref": "#/definitions/s"}}
            }}
        }),
    )?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, root.path())?;
    let validator = OfflineKubernetesValidator::new(cache.path(), KUBERNETES_RELEASE)?;
    let defaults = chart.adjudicate(&json!({}))?;
    let checked = validator.check_controls(&chart, &defaults, &[&defaults])?;
    let overlay = json!({"data": {"token": 42}});
    let payload = chart.adjudicate(&overlay)?;
    let document: Value = serde_yaml::from_slice(&payload.rendered.stdout)?;
    let KubernetesVerdict::Uncertain(reasons) = validator.validate_documents(&[document]) else {
        eyre::bail!(
            "validation resolved conflicting identities for the same constraint occurrence"
        );
    };
    assert!(
        reasons
            .iter()
            .any(|reason| reason.contains("conflicting schema roles"))
    );
    let err = validator
        .compare_with_controls(&chart, &payload, &checked)
        .err()
        .ok_or_eyre("context comparison resolved conflicting schema roles")?;
    assert!(
        err.to_string().contains("unresolved assertion identity"),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("conflicting schema roles"),
        "{err:?}"
    );

    let payload = chart.adjudicate(&overlay)?;
    let err = validator
        .compare_with_defaults(&chart, &payload)
        .err()
        .ok_or_eyre("defaults comparison resolved conflicting schema roles")?;
    assert!(
        err.to_string().contains("unresolved assertion identity"),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("conflicting schema roles"),
        "{err:?}"
    );
    Ok(())
}
