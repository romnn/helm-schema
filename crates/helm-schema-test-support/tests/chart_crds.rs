//! Native oracle regression cases replay recorded Kubernetes responses without a Go process.

use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::adjudication::{
    KUBERNETES_RELEASE, KubernetesVerdict, OfflineKubernetesValidator,
};
use helm_schema_test_support::helm::chart_crds::{Checker, Decision, Installation};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Case {
    source: Value,
    installation: Installation,
    resource: Value,
    identity: String,
    key: String,
}

fn starrocks() -> eyre::Result<Case> {
    let source: Value =
        serde_json::from_slice(&fs::read(root().join("testdata/crdcheck/starrocks.json"))?)?;
    let installation: Installation = serde_json::from_value(
        source
            .get("installation")
            .ok_or_eyre("missing installation")?
            .clone(),
    )?;
    let index = source
        .get("index")
        .ok_or_eyre("missing index")?
        .as_u64()
        .ok_or_eyre("missing resource index")?;
    let resource = installation
        .resources()
        .get(usize::try_from(index)?)
        .ok_or_eyre("missing StarRocks resource")?
        .clone();
    let identity = source
        .get("identity")
        .ok_or_eyre("missing identity")?
        .as_str()
        .ok_or_eyre("missing identity")?
        .to_string();
    let key = source
        .get("request_key")
        .ok_or_eyre("missing request key")?
        .as_str()
        .ok_or_eyre("missing request key")?
        .to_string();
    Ok(Case {
        source,
        installation,
        resource,
        identity,
        key,
    })
}

#[test]
fn starrocks_defaults_become_decidable_from_recorded_native_evidence() -> eyre::Result<()> {
    let case = starrocks()?;
    let bundle = root().join("testdata/provider-bundle");
    let previous = OfflineKubernetesValidator::with_crd_catalog(
        &bundle.join("kubernetes-json-schema-cache"),
        &bundle.join("crds-catalog-cache"),
        KUBERNETES_RELEASE,
    )?;
    assert!(matches!(
        previous.validate_documents(std::slice::from_ref(&case.resource)),
        KubernetesVerdict::Uncertain(_)
    ));
    // Replay has no executable path: accepting this case cannot depend on Go being installed.
    let checker = Checker::replay(&root().join("testdata/crdcheck"));
    sim_assert_eq!(have:case.installation.decide(&checker,&case.resource,Some(&bundle.join("crds-catalog-cache")),&case.identity,&[]),want:Decision::Accepted);
    Ok(())
}

#[test]
fn original_inputs_and_policy_context_cannot_borrow_a_recorded_acceptance() -> eyre::Result<()> {
    let case = starrocks()?;
    let checker = Checker::replay(&root().join("testdata/crdcheck"));
    let mut changed = case.resource.clone();
    changed["spec"]["starRocksBeSpec"]["limits"]["cpu"] = json!("invalid quantity");
    for (resource, identity, namespaces) in [
        (&changed, case.identity.as_str(), vec![]),
        (&case.resource, "another chart identity", vec![]),
        (
            &case.resource,
            case.identity.as_str(),
            vec!["other".to_string()],
        ),
    ] {
        assert!(matches!(
            case.installation
                .decide(&checker, resource, None, identity, &namespaces),
            Decision::Uncertain(_)
        ));
    }
    Ok(())
}

#[test]
fn unsupported_apply_inputs_and_installation_conflicts_abstain_before_replay() -> eyre::Result<()> {
    let case = starrocks()?;
    let checker = Checker::replay(&root().join("testdata/crdcheck"));
    for status in [
        Value::Null,
        json!({}),
        json!({"count":"bad"}),
        json!({"count":2}),
    ] {
        let mut resource = case.resource.clone();
        resource["status"] = status;
        let decision = case
            .installation
            .decide(&checker, &resource, None, &case.identity, &[]);
        assert!(matches!(decision,Decision::Uncertain(reason) if reason.contains("status")));
    }
    for (key, value) in [
        ("uid", json!("uid")),
        ("resourceVersion", json!("1")),
        ("managedFields", json!([])),
        ("generateName", json!("generated-")),
        ("finalizers", json!([])),
        ("ownerReferences", json!([])),
    ] {
        let mut resource = case.resource.clone();
        resource["metadata"][key] = value;
        assert!(
            matches!(case.installation.decide(&checker,&resource,None,&case.identity,&[]),Decision::Uncertain(reason) if reason.contains("metadata"))
        );
    }
    let mut same_name = case.source["installation"].clone();
    let mut declaration = same_name["crds"][0]["documents"][0].clone();
    declaration["spec"]["scope"] = json!("Cluster");
    same_name["crds"][0]["documents"]
        .as_array_mut()
        .ok_or_eyre("declarations missing")?
        .push(declaration);
    let mut cross_name = case.source["installation"].clone();
    let mut declaration = cross_name["crds"][0]["documents"][0].clone();
    declaration["metadata"]["name"] = json!("other.starrocks.com");
    cross_name["crds"][0]["documents"]
        .as_array_mut()
        .ok_or_eyre("declarations missing")?
        .push(declaration);
    for (mut installation, reason) in [(same_name, "same-name"), (cross_name, "cross-name")] {
        let parsed: Installation = serde_json::from_value(installation.clone())?;
        assert!(
            matches!(parsed.decide(&checker,&case.resource,None,&case.identity,&[]),Decision::Uncertain(message) if message.contains(reason))
        );
        installation["problems"] = json!(["active install hook"]);
        let parsed: Installation = serde_json::from_value(installation)?;
        assert!(
            matches!(parsed.decide(&checker,&case.resource,None,&case.identity,&[]),Decision::Uncertain(message) if message.contains("installation veto"))
        );
    }
    let mut unrelated = case.source["installation"].clone();
    for plural in ["widgets", "otherwidgets"] {
        let mut declaration = unrelated["crds"][0]["documents"][0].clone();
        declaration["metadata"]["name"] = json!(format!("{plural}.witness.example"));
        declaration["spec"]["group"] = json!("witness.example");
        declaration["spec"]["names"] = json!({"kind":"Widget","plural":plural});
        unrelated["crds"][0]["documents"]
            .as_array_mut()
            .ok_or_eyre("declarations missing")?
            .push(declaration);
    }
    let parsed: Installation = serde_json::from_value(unrelated)?;
    assert!(
        matches!(parsed.decide(&checker,&case.resource,None,&case.identity,&[]),Decision::Uncertain(reason) if reason.contains("cross-name"))
    );
    Ok(())
}

#[test]
fn corrupt_or_mismatched_responses_never_replay() -> eyre::Result<()> {
    let case = starrocks()?;
    let records = root().join("testdata/crdcheck");
    let original: Value =
        serde_json::from_slice(&fs::read(records.join(format!("{}.json", case.key)))?)?;
    for mutation in [
        "protocol",
        "request_sha256",
        "effective_sha256",
        "effective_json",
        "effective_identity",
        "inputs",
        "truncated",
        "extra-document",
    ] {
        let scratch = ScratchDir::new("native-replay-corrupt")?;
        fs::copy(
            records.join("binary.sha256"),
            scratch.path().join("binary.sha256"),
        )?;
        let mut record = original.clone();
        match mutation {
            "inputs" => record["inputs"][1] = json!("{}"),
            "effective_identity" => {
                let wrong = "{\"kind\":\"AnotherResource\"}";
                record["response"]["effective_json"] = json!(wrong);
                record["response"]["effective_sha256"] = json!(
                    helm_schema_test_support::source_digest::sha256_hex(wrong.as_bytes())
                );
            }
            "truncated" | "extra-document" => {}
            field => record["response"][field] = json!("mismatch"),
        }
        let mut raw = serde_json::to_vec(&record)?;
        if mutation == "truncated" {
            raw.pop();
        }
        if mutation == "extra-document" {
            raw.extend_from_slice(b"{}");
        }
        fs::write(scratch.path().join(format!("{}.json", case.key)), raw)?;
        let checker = Checker::replay(scratch.path());
        assert!(
            matches!(
                case.installation
                    .decide(&checker, &case.resource, None, &case.identity, &[]),
                Decision::Uncertain(_)
            ),
            "{mutation}"
        );
    }
    let absent = ScratchDir::new("native-missing")?;
    let checker = Checker::live(&absent.path().join("missing-binary"), absent.path());
    assert!(matches!(
        case.installation
            .decide(&checker, &case.resource, None, &case.identity, &[]),
        Decision::Uncertain(_)
    ));
    Ok(())
}

#[test]
fn chart_versions_and_duplicates_abstain_while_catalog_keeps_authority() -> eyre::Result<()> {
    let case = starrocks()?;
    let checker = Checker::replay(&root().join("testdata/crdcheck"));
    for field in ["served", "name"] {
        let mut source = case.source["installation"].clone();
        source["crds"][0]["documents"][0]["spec"]["versions"][0][field] = if field == "served" {
            json!(false)
        } else {
            json!("v2")
        };
        let installation: Installation = serde_json::from_value(source)?;
        assert!(
            matches!(installation.decide(&checker,&case.resource,None,&case.identity,&[]),Decision::Uncertain(reason) if reason.contains("version"))
        );
    }
    let mut source = case.source["installation"].clone();
    source["resources"]
        .as_array_mut()
        .ok_or_eyre("resources missing")?
        .push(case.resource.clone());
    let installation: Installation = serde_json::from_value(source)?;
    assert!(
        matches!(installation.decide(&checker,&case.resource,None,&case.identity,&[]),Decision::Uncertain(reason) if reason.contains("duplicate"))
    );

    // Catalog authority retains the legacy decision even when the chart disagrees.
    let catalog = ScratchDir::new("native-catalog-conflict")?;
    let path = catalog
        .path()
        .join("default/starrocks.com/starrockscluster_v1.json");
    fs::create_dir_all(path.parent().ok_or_eyre("catalog parent missing")?)?;
    let schema = &case.source["installation"]["crds"][0]["documents"][0]["spec"]["versions"][0]["schema"]
        ["openAPIV3Schema"];
    fs::write(&path, serde_json::to_vec(schema)?)?;
    sim_assert_eq!(have:case.installation.decide(&checker,&case.resource,Some(catalog.path()),&case.identity,&[]),want:Decision::Catalog);
    fs::write(path, b"{}")?;
    sim_assert_eq!(have:case.installation.decide(&checker,&case.resource,Some(catalog.path()),&case.identity,&[]),want:Decision::Catalog);
    Ok(())
}

#[test]
fn catalog_authority_survives_installation_vetoes() -> eyre::Result<()> {
    let case = starrocks()?;
    let catalog = ScratchDir::new("catalog-ratchet")?;
    let path = catalog
        .path()
        .join("default/starrocks.com/starrockscluster_v1.json");
    fs::create_dir_all(path.parent().ok_or_eyre("catalog parent missing")?)?;
    fs::write(path, b"{}")?;
    let absent = ScratchDir::new("catalog-no-native")?;
    let checker = Checker::replay(absent.path());
    for problem in ["render changes CRDs or admission", "active install hook"] {
        let mut source = case.source["installation"].clone();
        source["problems"] = json!([problem]);
        source["crds"][0]["documents"][0]["spec"]["versions"][0]["served"] = json!(false);
        let installation: Installation = serde_json::from_value(source)?;
        sim_assert_eq!(
            have: installation.decide(&checker, &case.resource, Some(catalog.path()), &case.identity, &[]),
            want: Decision::Catalog,
        );
        assert!(matches!(
            installation.decide(&checker, &case.resource, None, &case.identity, &[]),
            Decision::Uncertain(_)
        ));
    }
    Ok(())
}
