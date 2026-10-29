//! Offline coverage for the exact custom resources rendered by the corpus controls.

use std::fs;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::adjudication::{
    KUBERNETES_RELEASE, KubernetesVerdict, OfflineKubernetesValidator,
};
use helm_schema_test_support::source_digest::sha256_hex;
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    schema: String,
    source: String,
    source_revision: String,
    source_sha256: String,
    pointer: String,
    document_index: usize,
    schema_sha256: String,
    upstream_url: Option<String>,
    #[serde(default)]
    normalize_pattern_dialects: bool,
}

#[test]
fn catalog_additions_equal_the_complete_pinned_source_schemas() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let bundle = root.join("testdata/provider-bundle");
    let entries: Vec<Provenance> =
        serde_json::from_slice(&fs::read(bundle.join("oracle-coverage-provenance.json"))?)?;
    sim_assert_eq!(have: entries.len(), want: 12);
    for Provenance {
        schema,
        source,
        source_revision,
        source_sha256,
        pointer,
        document_index,
        schema_sha256,
        upstream_url,
        normalize_pattern_dialects,
    } in entries
    {
        if let Some(url) = upstream_url {
            sim_assert_eq!(have: source_revision, want: "db255c554b929afd73552fd3ac81d691107a1607");
            let filename = std::path::Path::new(&source)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_eyre("missing source filename")?;
            sim_assert_eq!(
                have: url,
                want: format!("https://raw.githubusercontent.com/projectcalico/calico/{source_revision}/charts/crd.projectcalico.org.v1/templates/{filename}"),
            );
        } else {
            sim_assert_eq!(have: source_revision, want: "8900ce9a");
        }
        let source = fs::read(root.join(source))?;
        sim_assert_eq!(have: sha256_hex(&source), want: source_sha256);
        let schema = fs::read(bundle.join(schema))?;
        sim_assert_eq!(have: sha256_hex(&schema), want: schema_sha256);
        let schema: Value = serde_json::from_slice(&schema)?;
        let document = serde_yaml::Deserializer::from_slice(&source)
            .nth(document_index)
            .ok_or_eyre("missing source document")?;
        let mut document = serde_yaml::Value::deserialize(document)?;
        document.apply_merge()?;
        let document = serde_json::to_value(document)?;
        let mut expected = document
            .pointer(&pointer)
            .ok_or_eyre("missing source schema")?
            .clone();
        if normalize_pattern_dialects {
            helm_schema_core::normalize_schema_pattern_dialects(&mut expected);
        }
        sim_assert_eq!(
            have: schema,
            want: expected,
        );
    }
    Ok(())
}

#[test]
fn pinned_custom_kinds_decide_valid_and_invalid_resources() -> eyre::Result<()> {
    let validator = pinned_validator()?;
    check_original_kinds(&validator)?;
    sim_assert_eq!(
        have: validator.validate_documents(&[json!({
            "apiVersion": "apiregistration.k8s.io/v1", "kind": "APIService",
            "metadata": {"name": "v1.coverage.example"},
            "spec": {"groupPriorityMinimum": 100, "versionPriority": 100}
        })]),
        want: KubernetesVerdict::Valid,
    );
    for (api, kind) in [
        ("unknown.example/v1", "Unknown"),
        ("nats.io/v999", "NatsCluster"),
        ("apiregistration.k8s.io/v1beta1", "APIService"),
    ] {
        let verdict = validator.validate_documents(&[json!({
            "apiVersion": api, "kind": kind,
            "metadata": {"name": "coverage"}, "spec": {}
        })]);
        assert!(
            matches!(verdict, KubernetesVerdict::Uncertain(_)),
            "{api}/{kind}: {verdict:?}"
        );
    }
    validator.verify_bundles_unchanged()?;
    Ok(())
}

fn pinned_validator() -> eyre::Result<OfflineKubernetesValidator> {
    let bundle = test_util::workspace_testdata().join("provider-bundle");
    OfflineKubernetesValidator::with_crd_catalog(
        &bundle.join("kubernetes-json-schema-cache"),
        &bundle.join("crds-catalog-cache"),
        KUBERNETES_RELEASE,
    )
}

fn check_original_kinds(validator: &OfflineKubernetesValidator) -> eyre::Result<()> {
    for (api, kind, property) in [
        ("monitoring.coreos.com/v1", "Alertmanager", "replicas"),
        ("monitoring.coreos.com/v1", "Prometheus", "replicas"),
        ("nats.io/v1alpha2", "NatsCluster", "size"),
        (
            "clickhouse.altinity.com/v1",
            "ClickHouseInstallation",
            "stop",
        ),
    ] {
        let mut document = json!({
            "apiVersion": api, "kind": kind,
            "metadata": {"name": "coverage"}, "spec": {}
        });
        sim_assert_eq!(
            have: validator.validate_documents(&[document.clone()]),
            want: KubernetesVerdict::Valid,
            "{api}/{kind}",
        );
        document
            .pointer_mut("/spec")
            .ok_or_eyre("missing spec")?
            .as_object_mut()
            .ok_or_eyre("spec is not an object")?
            .insert(property.to_string(), json!([]));
        let verdict = validator.validate_documents(&[document]);
        assert!(
            matches!(verdict, KubernetesVerdict::Invalid(_)),
            "{api}/{kind}: {verdict:?}"
        );
    }
    Ok(())
}

#[test]
fn remaining_pinned_kinds_decide_valid_and_invalid_resources() -> eyre::Result<()> {
    let validator = pinned_validator()?;
    for (api, kind, field, body) in [
        ("starrocks.com/v1", "StarRocksCluster", "spec", json!({})),
        (
            "acid.zalan.do/v1",
            "OperatorConfiguration",
            "configuration",
            json!({}),
        ),
        (
            "velero.io/v1",
            "BackupStorageLocation",
            "spec",
            json!({"provider":"aws","objectStorage":{"bucket":"coverage"}}),
        ),
        (
            "velero.io/v1",
            "VolumeSnapshotLocation",
            "spec",
            json!({"provider":"aws"}),
        ),
        (
            "velero.io/v1",
            "Schedule",
            "spec",
            json!({"schedule":"0 0 * * *","template":{}}),
        ),
        (
            "aquasecurity.github.io/v1alpha1",
            "ClusterComplianceReport",
            "spec",
            json!({
                "compliance": {
                    "controls": [], "description": "coverage", "id": "coverage",
                    "platform": "k8s", "relatedResources": [], "title": "coverage",
                    "type": "coverage", "version": "1"
                },
                "cron": "* * * * *", "reportType": "summary"
            }),
        ),
        ("operator.tigera.io/v1", "Goldmane", "spec", json!({})),
        ("operator.tigera.io/v1", "Whisker", "spec", json!({})),
    ] {
        let mut document = json!({
            "apiVersion": api, "kind": kind, "metadata": {"name": "coverage"},
            field: body
        });
        sim_assert_eq!(
            have: validator.validate_documents(&[document.clone()]),
            want: KubernetesVerdict::Valid,
            "{api}/{kind}",
        );
        document
            .as_object_mut()
            .ok_or_eyre("resource is not an object")?
            .insert(field.to_string(), json!([]));
        let verdict = validator.validate_documents(&[document]);
        assert!(
            matches!(verdict, KubernetesVerdict::Invalid(_)),
            "{api}/{kind}: {verdict:?}"
        );
    }
    validator.verify_bundles_unchanged()?;
    Ok(())
}
