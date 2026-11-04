use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use super::{ReplayError, json, read};
use crate::manifest::{self, CompanionFile, Manifest, ManifestEntry};

/// Rechecks immutable historical dumps without requiring their source build to
/// equal this replay tool's build. Recipe inputs, registry membership, artifact
/// bytes and Helm-ready derivations remain checked; recorded build provenance
/// stays bound by the manifest digest in the output.
pub(super) fn verify(directory: &Path) -> Result<Manifest, ReplayError> {
    let manifest: Manifest = json(&directory.join(manifest::MANIFEST_FILE))?;
    if directory.join(manifest::LOCK_FILE).exists() {
        return Err(ReplayError::Input(format!(
            "producer still owns {}",
            directory.display()
        )));
    }
    if manifest.harness_version != manifest::HARNESS_VERSION {
        return Err(ReplayError::Input(
            "unsupported producer manifest version".into(),
        ));
    }
    let specs = crate::registry::registry();
    let entries: BTreeMap<_, _> = manifest
        .artifacts
        .iter()
        .map(|entry| (entry.key.as_str(), entry))
        .collect();
    if entries.len() != manifest.artifacts.len() || entries.len() != specs.len() {
        return Err(ReplayError::Input(
            "incomplete or duplicate producer registry".into(),
        ));
    }
    let mut inputs = manifest::InputDigester::new(test_util::workspace_testdata());
    let mut listed = BTreeSet::from([manifest::MANIFEST_FILE.to_owned()]);
    for spec in specs {
        let key = spec.id.key();
        let entry = entries
            .get(key.as_str())
            .ok_or_else(|| ReplayError::Input(format!("missing artifact {key}")))?;
        safe_path(&entry.file)?;
        listed.insert(entry.file.clone());
        let bytes = read(&directory.join(&entry.file))?;
        let mut expected = ManifestEntry::new(&spec, inputs.digest(&spec.recipe)?, &bytes);
        expected.helm_ready.clone_from(&entry.helm_ready);
        if &expected != *entry {
            return Err(ReplayError::Input(format!(
                "producer recipe, inputs or bytes differ for {key}"
            )));
        }
        if let Some(companion) = &entry.helm_ready {
            let derived = manifest::helm_ready_files(&spec, &bytes)
                .map_err(|source| ReplayError::Json {
                    path: directory.join(&entry.file),
                    source,
                })?
                .ok_or_else(|| ReplayError::Input(format!("unexpected companion for {key}")))?;
            for (recorded, expected) in [&companion.schema, &companion.defs_map]
                .into_iter()
                .zip(derived)
            {
                safe_path(&recorded.file)?;
                listed.insert(recorded.file.clone());
                let bytes = read(&directory.join(&recorded.file))?;
                if recorded != &CompanionFile::new(expected.file, &expected.bytes)
                    || bytes != expected.bytes
                {
                    return Err(ReplayError::Input(format!(
                        "producer companion differs for {key}"
                    )));
                }
            }
        }
    }
    let mut files = Vec::new();
    crate::source_digest::collect_files(directory, &mut files, &|_| true).map_err(|source| {
        ReplayError::Io {
            path: directory.into(),
            source,
        }
    })?;
    for path in files {
        let relative = path
            .strip_prefix(directory)
            .map_err(|err| ReplayError::Input(err.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !listed.contains(&relative) {
            return Err(ReplayError::Input(format!(
                "unregistered producer file {relative}"
            )));
        }
    }
    Ok(manifest)
}

fn safe_path(path: &str) -> Result<(), ReplayError> {
    if Path::new(path)
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
        && !path.is_empty()
    {
        Ok(())
    } else {
        Err(ReplayError::Input(format!("unsafe producer path {path}")))
    }
}

/// Verifies the exact artifact read by a worker, even if another process races the initial check.
pub(super) fn artifact(directory: &Path, entry: &ManifestEntry) -> Result<Vec<u8>, ReplayError> {
    let path = directory.join(&entry.file);
    let bytes = read(&path)?;
    if u64::try_from(bytes.len()).ok() != Some(entry.size)
        || crate::source_digest::sha256_hex(&bytes) != entry.sha256
    {
        return Err(manifest::ProvenanceError::ArtifactBytes { path }.into());
    }
    Ok(bytes)
}

pub(super) fn unchanged(directory: &Path, digest: &str) -> Result<(), ReplayError> {
    if directory.join(manifest::LOCK_FILE).exists()
        || crate::source_digest::sha256_hex(&read(&directory.join(manifest::MANIFEST_FILE))?)
            != digest
    {
        return Err(ReplayError::Input(format!(
            "producer changed during replay: {}",
            directory.display()
        )));
    }
    Ok(())
}
