//! A produced artifact is evidence only for the build, checkout and complete
//! registry it was produced from.
//!
//! Producing all 203 artifacts takes minutes, so these tests forge a complete
//! manifest: one real, cheap IR artifact, and placeholder files for every other
//! registered entry. The structural checks see a complete corpus; each test then
//! breaks one provenance fact the consumer must check.

use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::consume::consume_from;
use helm_schema_test_support::generate;
use helm_schema_test_support::manifest::{
    self, BuildProvenance, HARNESS_VERSION, LOCK_FILE, MANIFEST_FILE, Manifest, ManifestEntry,
    ProvenanceError,
};
use helm_schema_test_support::registry::{self, ArtifactId, ArtifactTarget, IrId};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const ID: ArtifactId = ArtifactId::Ir(IrId::NatsService);

/// Writes a complete manifest whose only real artifact is [`ID`].
fn forge(out: &Path) -> eyre::Result<Manifest> {
    let real = generate::produce_entries(&[ID.spec()], out, 1, false)?
        .pop()
        .ok_or_eyre("no entry produced")?;
    let mut artifacts = Vec::new();
    for spec in registry::registry() {
        if spec.id == ID {
            artifacts.push(real.clone());
            continue;
        }
        let entry = ManifestEntry::new(&spec, "0".repeat(64), b"{}");
        std::fs::write(out.join(&entry.file), b"{}").wrap_err("write placeholder")?;
        artifacts.push(entry);
    }
    let manifest = Manifest {
        harness_version: HARNESS_VERSION,
        build: BuildProvenance::compiled(),
        producer_binary_sha256: "forged".to_string(),
        testdata: manifest::testdata_location(),
        artifacts,
    };
    write_manifest(out, &manifest)?;
    Ok(manifest)
}

fn write_manifest(out: &Path, manifest: &Manifest) -> eyre::Result<()> {
    std::fs::write(
        out.join(MANIFEST_FILE),
        serde_json::to_vec_pretty(manifest)?,
    )
    .wrap_err("write manifest")
}

/// Forges a manifest, applies `tamper`, and returns the consumer's typed refusal.
fn refusal(
    tamper: impl FnOnce(&Path, &mut Manifest) -> eyre::Result<()>,
) -> eyre::Result<ProvenanceError> {
    let out = ScratchDir::new("consume")?;
    let mut manifest = forge(out.path())?;
    tamper(out.path(), &mut manifest)?;
    write_manifest(out.path(), &manifest)?;
    match consume_from(Some(out.path()), &ID.spec()) {
        Ok(_) => eyre::bail!("a tampered corpus was accepted"),
        Err(error) => error
            .downcast::<ProvenanceError>()
            .map_err(|error| eyre::eyre!("untyped refusal: {error:?}")),
    }
}

fn entry_mut<'m>(manifest: &'m mut Manifest, key: &str) -> eyre::Result<&'m mut ManifestEntry> {
    manifest
        .artifacts
        .iter_mut()
        .find(|entry| entry.key == key)
        .ok_or_eyre("forged manifest lacks the entry")
}

/// The registry field a consumer names after `tamper` edits [`ID`]'s entry.
fn mismatched_field(tamper: fn(&mut ManifestEntry)) -> eyre::Result<&'static str> {
    let error = refusal(|_, manifest| {
        tamper(entry_mut(manifest, &ID.key())?);
        Ok(())
    })?;
    match error {
        ProvenanceError::EntryMismatch { field, .. } => Ok(field),
        other => eyre::bail!("expected an entry mismatch, got {other}"),
    }
}

#[test]
fn complete_manifest_serves_the_verified_artifact() -> eyre::Result<()> {
    let out = ScratchDir::new("consume")?;
    forge(out.path())?;
    let produced = consume_from(Some(out.path()), &ID.spec())?;
    let local = consume_from(None, &ID.spec())?;
    sim_assert_eq!(have: produced, want: local);
    Ok(())
}

#[test]
fn one_entry_manifest_is_incomplete() -> eyre::Result<()> {
    let error = refusal(|_, manifest| {
        manifest.artifacts.retain(|entry| entry.key == ID.key());
        Ok(())
    })?;
    let ProvenanceError::Incomplete { missing } = error else {
        eyre::bail!("expected an incomplete manifest, got {error}");
    };
    sim_assert_eq!(have: missing.len(), want: 202);
    Ok(())
}

#[test]
fn duplicate_and_unregistered_entries_are_refused() -> eyre::Result<()> {
    let error = refusal(|_, manifest| {
        let first = manifest.artifacts.first().cloned().ok_or_eyre("empty")?;
        manifest.artifacts.push(first);
        Ok(())
    })?;
    let ProvenanceError::Unexpected { keys } = error else {
        eyre::bail!("expected a duplicate refusal, got {error}");
    };
    sim_assert_eq!(have: keys, want: vec!["chart/airflow".to_string()]);

    let error = refusal(|_, manifest| {
        manifest.artifacts.first_mut().ok_or_eyre("empty")?.key = "chart/unregistered".to_string();
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::Unexpected { .. }),
        "expected an unregistered refusal, got {error}"
    );
    Ok(())
}

#[test]
fn entries_must_agree_with_the_registry() -> eyre::Result<()> {
    sim_assert_eq!(
        have: mismatched_field(|entry| entry.kind = "chart".to_string())?,
        want: "kind"
    );
    sim_assert_eq!(have: mismatched_field(|entry| entry.fixture = None)?, want: "fixture");
    sim_assert_eq!(have: mismatched_field(|entry| entry.recipe = Value::Null)?, want: "recipe");
    Ok(())
}

#[test]
fn stale_producer_build_is_refused() -> eyre::Result<()> {
    let error = refusal(|_, manifest| {
        manifest.build.source_sha256 = "0".repeat(64);
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::StaleProducer { .. }),
        "expected a stale producer, got {error}"
    );
    let error = refusal(|_, manifest| {
        manifest.build.profile = "release".to_string();
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::StaleProducer { .. }),
        "expected a stale producer, got {error}"
    );
    Ok(())
}

#[test]
fn harness_version_and_location_are_bound() -> eyre::Result<()> {
    let error = refusal(|_, manifest| {
        manifest.harness_version = HARNESS_VERSION + 1;
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::HarnessVersion { .. }),
        "expected a harness refusal, got {error}"
    );
    let error = refusal(|_, manifest| {
        manifest.testdata = "/elsewhere/testdata".to_string();
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::Location { .. }),
        "expected a location refusal, got {error}"
    );
    Ok(())
}

#[test]
fn changed_inputs_or_bytes_are_refused() -> eyre::Result<()> {
    let error = refusal(|_, manifest| {
        entry_mut(manifest, &ID.key())?.inputs_sha256 = "0".repeat(64);
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::StaleInputs { .. }),
        "expected stale inputs, got {error}"
    );

    // Same size, other content.
    let error = refusal(|out, manifest| {
        let path = out.join(&entry_mut(manifest, &ID.key())?.file);
        let mut bytes = std::fs::read(&path)?;
        *bytes.last_mut().ok_or_eyre("empty artifact")? = b' ';
        std::fs::write(&path, bytes)?;
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::ArtifactBytes { .. }),
        "expected changed bytes, got {error}"
    );

    // Any other registered artifact missing makes the corpus incomplete.
    let error = refusal(|out, manifest| {
        let entry = manifest.artifacts.first().ok_or_eyre("empty")?;
        std::fs::remove_file(out.join(&entry.file))?;
        Ok(())
    })?;
    eyre::ensure!(
        matches!(error, ProvenanceError::ArtifactBytes { .. }),
        "expected a missing artifact, got {error}"
    );
    Ok(())
}

#[test]
fn full_verification_rechecks_every_artifacts_inputs() -> eyre::Result<()> {
    let out = ScratchDir::new("consume")?;
    let manifest = forge(out.path())?;
    let Err(error) = manifest::verify_all(out.path(), &manifest) else {
        eyre::bail!("placeholder artifacts passed full verification");
    };
    eyre::ensure!(
        matches!(error, ProvenanceError::StaleInputs { .. }),
        "expected the first placeholder to fail its inputs, got {error}"
    );
    Ok(())
}

#[test]
fn stale_compiled_producer_refuses_to_produce() -> eyre::Result<()> {
    let out = ScratchDir::new("consume")?;
    let mut stale = BuildProvenance::compiled();
    stale.source_sha256 = "0".repeat(64);
    let Err(error) = generate::produce(out.path(), 1, &stale, false) else {
        eyre::bail!("a stale producer produced");
    };
    eyre::ensure!(
        matches!(
            error.downcast_ref::<ProvenanceError>(),
            Some(ProvenanceError::StaleBuild { .. })
        ),
        "expected a stale build, got {error:?}"
    );
    sim_assert_eq!(have: std::fs::read_dir(out.path())?.count(), want: 0);
    Ok(())
}

#[test]
fn producer_refuses_an_owned_output_directory() -> eyre::Result<()> {
    let out = ScratchDir::new("consume")?;
    std::fs::write(out.path().join(LOCK_FILE), b"")?;
    let Err(error) = generate::produce(out.path(), 1, &BuildProvenance::compiled(), false) else {
        eyre::bail!("the producer ignored another run's lock");
    };
    eyre::ensure!(
        format!("{error:#}").contains("another producer owns"),
        "{error:#}"
    );
    Ok(())
}

#[test]
#[should_panic(expected = "the locally generated artifact differs from its fixture")]
fn local_generation_compares_with_the_committed_fixture() {
    // Register the NATS service IR under the surveyor HPA fixture.
    let mut spec = ID.spec();
    spec.target = ArtifactTarget::Fixture(
        "crates/helm-schema-ir/tests/fixtures/surveyor_hpa.ir.json".to_string(),
    );
    let _ignored = consume_from(None, &spec);
}

#[test]
fn helm_ready_companions_are_the_shortened_artifact_and_its_name_map() -> eyre::Result<()> {
    let out = ScratchDir::new("consume")?;
    let spec = ArtifactId::FinalPolicy(registry::PolicyId::Full).spec();
    let entry = generate::produce_entries(&[spec.clone(), ID.spec()], out.path(), 1, true)?;
    let [schema_entry, ir_entry] = entry.as_slice() else {
        eyre::bail!("two entries expected");
    };
    sim_assert_eq!(have: ir_entry.helm_ready.as_ref(), want: None);
    let helm_ready = schema_entry
        .helm_ready
        .as_ref()
        .ok_or_eyre("schema artifacts get Helm-ready companions")?;
    sim_assert_eq!(
        have: [&helm_ready.schema.file, &helm_ready.defs_map.file],
        want: [
            "internal/helm-schema.final-output.full.helm.schema.json",
            "internal/helm-schema.final-output.full.defs-map.json",
        ]
    );

    let artifact: Value =
        serde_json::from_slice(&std::fs::read(out.path().join(&schema_entry.file))?)?;
    let shortened: Value =
        serde_json::from_slice(&std::fs::read(out.path().join(&helm_ready.schema.file))?)?;
    let readable_names: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(out.path().join(&helm_ready.defs_map.file))?)?;
    assert!(
        !readable_names.is_empty(),
        "the full fixture has definitions"
    );
    let mut restored = shortened.clone();
    helm_schema_json_schema_minify::rename_definitions(&mut restored, &readable_names);
    sim_assert_eq!(have: restored, want: artifact);
    let defs = shortened
        .get("$defs")
        .and_then(Value::as_object)
        .ok_or_eyre("shortened definitions expected")?;
    sim_assert_eq!(
        have: defs.keys().cloned().collect::<std::collections::BTreeSet<_>>(),
        want: readable_names.keys().cloned().collect()
    );
    Ok(())
}
