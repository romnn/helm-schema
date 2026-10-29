//! Shared fixtures of the differential-gate tests.

use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::manifest::{
    self, BuildProvenance, HARNESS_VERSION, HelmReadyFiles, MANIFEST_FILE, Manifest, ManifestEntry,
};
use helm_schema_test_support::registry::{self, ArtifactId, ArtifactSpec};

/// The corpus chart the dump tests use.
pub const CHART: &str = "dict-config";

fn chart_spec() -> eyre::Result<ArtifactSpec> {
    registry::registry()
        .into_iter()
        .find(|spec| {
            matches!(spec.id, ArtifactId::Chart(_)) && spec.id.key() == format!("chart/{CHART}")
        })
        .ok_or_eyre("dict-config is not registered")
}

/// Writes a dump at `root` holding only `CHART`'s artifact with `bytes`.
///
/// # Errors
///
/// When the chart is not registered or a file cannot be written.
pub fn write_dump(root: &Path, bytes: &[u8]) -> eyre::Result<()> {
    let spec = chart_spec()?;
    std::fs::create_dir_all(root.join(manifest::INTERNAL_DIR))?;
    let mut digester = manifest::InputDigester::new(test_util::workspace_testdata());
    let mut entry = ManifestEntry::new(&spec, digester.digest(&spec.recipe)?, bytes);
    std::fs::write(root.join(&entry.file), bytes)?;
    let [schema, defs_map] =
        manifest::helm_ready_files(&spec, bytes)?.ok_or_eyre("no Helm-ready spelling")?;
    std::fs::write(root.join(&schema.file), &schema.bytes)?;
    std::fs::write(root.join(&defs_map.file), &defs_map.bytes)?;
    entry.helm_ready = Some(HelmReadyFiles {
        schema: manifest::CompanionFile::new(schema.file, &schema.bytes),
        defs_map: manifest::CompanionFile::new(defs_map.file, &defs_map.bytes),
    });
    let manifest = Manifest {
        harness_version: HARNESS_VERSION,
        build: BuildProvenance::compiled(),
        producer_binary_sha256: "synthetic".into(),
        testdata: manifest::testdata_location(),
        artifacts: vec![entry],
    };
    std::fs::write(
        root.join(MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
