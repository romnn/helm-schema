//! The producer manifest, its provenance, and the one validator every reader uses.
//!
//! A manifest is evidence only for the exact tree and build it came from. It
//! records the build provenance compiled into the producer, the testdata
//! location, and per artifact the recipe, a digest of the recipe's inputs, and
//! the artifact's size and SHA-256. [`load`] checks the provenance and the
//! complete registry membership before any artifact is read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{self, ArtifactSpec, ArtifactTarget, GenerationRecipe, PROVIDER_BUNDLE};
use crate::source_digest::{self, collect_files, digest_files, sha256_hex};

/// Bumped whenever the producer's output layout or checks change meaning.
pub const HARNESS_VERSION: u32 = 2;

/// The manifest file name inside the producer's output directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// The subdirectory holding artifacts without committed fixtures.
pub const INTERNAL_DIR: &str = "internal";

/// Marks an output directory a producer currently owns.
pub const LOCK_FILE: &str = ".corpus-generation.lock";

/// Why a manifest or artifact is not evidence for the current checkout.
#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    /// A file could not be read or listed.
    #[error("read {path}: {source}")]
    Io {
        /// The unreadable path.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The manifest is not valid manifest JSON.
    #[error("parse producer manifest {path}: {source}")]
    Parse {
        /// The manifest path.
        path: PathBuf,
        /// The underlying error.
        source: serde_json::Error,
    },
    /// The manifest was written by another harness version.
    #[error("manifest harness version {found}, expected {expected}")]
    HarnessVersion {
        /// Recorded version.
        found: u32,
        /// This build's version.
        expected: u32,
    },
    /// The manifest was written by a producer built from another tree or configuration.
    #[error(
        "stale producer: the manifest was written by a producer built as {recorded:?}, \
         but this build is {expected:?}; rebuild and re-run corpus_generation"
    )]
    StaleProducer {
        /// Provenance recorded in the manifest.
        recorded: Box<BuildProvenance>,
        /// Provenance compiled into this binary.
        expected: Box<BuildProvenance>,
    },
    /// The source tree differs from the one this binary was compiled from.
    #[error(
        "stale build: this binary was compiled from source digest {compiled}, but the tree \
         now hashes to {current}; rebuild the producer and the tests"
    )]
    StaleBuild {
        /// Digest compiled into this binary.
        compiled: String,
        /// Digest of the tree now.
        current: String,
    },
    /// The manifest was produced for testdata at another location.
    #[error("the manifest was produced for testdata at {recorded}, not {current}")]
    Location {
        /// Recorded testdata directory.
        recorded: String,
        /// This checkout's testdata directory.
        current: String,
    },
    /// Registered artifacts are absent from the manifest.
    #[error("incomplete manifest: no entry for {missing:?}")]
    Incomplete {
        /// Keys of the missing artifacts.
        missing: Vec<String>,
    },
    /// The manifest lists an artifact twice or one the registry does not know.
    #[error("manifest lists duplicate or unregistered artifacts {keys:?}")]
    Unexpected {
        /// Offending keys.
        keys: Vec<String>,
    },
    /// An entry disagrees with the registry.
    #[error("manifest entry {key} disagrees with the registry on its {field}")]
    EntryMismatch {
        /// The entry key.
        key: String,
        /// The disagreeing field.
        field: &'static str,
    },
    /// An artifact file is absent or has another size or SHA-256.
    #[error("artifact {path} is missing or differs from its manifest entry")]
    ArtifactBytes {
        /// The artifact path.
        path: PathBuf,
    },
    /// The output directory holds files the manifest does not list.
    #[error("{dir} holds files no registered artifact owns: {files:?}")]
    ForeignFiles {
        /// The output directory.
        dir: PathBuf,
        /// Relative paths of the foreign files.
        files: Vec<String>,
    },
    /// A recipe's input files changed since the producer read them.
    #[error("the inputs of {key} changed since the producer read them")]
    StaleInputs {
        /// The entry key.
        key: String,
    },
}

/// The build a producer or consumer binary was compiled as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildProvenance {
    /// [`source_digest::generation_source_digest`] of the tree at compile time.
    pub source_sha256: String,
    /// Compilation target triple.
    pub target: String,
    /// Cargo profile.
    pub profile: String,
}

impl BuildProvenance {
    /// The provenance embedded into this binary by the build script.
    #[must_use]
    pub fn compiled() -> BuildProvenance {
        BuildProvenance {
            source_sha256: env!("HELM_SCHEMA_GENERATION_SOURCE_SHA256").to_string(),
            target: env!("HELM_SCHEMA_BUILD_TARGET").to_string(),
            profile: env!("HELM_SCHEMA_BUILD_PROFILE").to_string(),
        }
    }

    /// Refuses a build whose compiled source digest differs from the tree now.
    ///
    /// # Errors
    ///
    /// [`ProvenanceError::StaleBuild`] on a mismatch.
    pub fn check_current(&self) -> Result<(), ProvenanceError> {
        let root = test_util::workspace_root();
        let current = source_digest::generation_source_digest(&root)
            .map_err(|source| ProvenanceError::Io { path: root, source })?;
        if current == self.source_sha256 {
            Ok(())
        } else {
            Err(ProvenanceError::StaleBuild {
                compiled: self.source_sha256.clone(),
                current,
            })
        }
    }
}

/// The producer's record of one complete run over the whole registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`HARNESS_VERSION`] of the producer.
    pub harness_version: u32,
    /// [`BuildProvenance::compiled`] of the producer.
    pub build: BuildProvenance,
    /// SHA-256 of the producer executable; informational.
    pub producer_binary_sha256: String,
    /// Absolute testdata directory the recipes read. Provider source
    /// identities embed absolute cache roots, so a manifest is bound to its
    /// checkout location.
    pub testdata: String,
    /// One entry per registered artifact, in registry order.
    pub artifacts: Vec<ManifestEntry>,
}

/// One produced artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// [`crate::registry::ArtifactId::key`].
    pub key: String,
    /// [`crate::registry::ArtifactKind::name`].
    pub kind: String,
    /// Path relative to the output directory.
    pub file: String,
    /// Repository-relative fixture path, absent for internal artifacts.
    pub fixture: Option<String>,
    /// [`GenerationRecipe::describe`].
    pub recipe: Value,
    /// [`InputDigester::digest`] of the recipe.
    pub inputs_sha256: String,
    /// Artifact size in bytes.
    pub size: u64,
    /// Artifact SHA-256.
    pub sha256: String,
    /// The Helm-ready copy of a schema artifact, when the producer ran with
    /// `--helm-ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_ready: Option<HelmReadyFiles>,
}

/// A schema artifact as handed to Helm: `$defs` renamed to short keys,
/// compact JSON plus a newline, and the map from each short key back to its
/// readable name.
///
/// Both are derived from the artifact's bytes alone, so they are evidence
/// exactly when the artifact is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelmReadyFiles {
    /// The shortened schema.
    pub schema: CompanionFile,
    /// The short-to-readable name map, pretty JSON plus a newline.
    pub defs_map: CompanionFile,
}

/// One file derived from an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFile {
    /// Path relative to the output directory.
    pub file: String,
    /// Size in bytes.
    pub size: u64,
    /// SHA-256.
    pub sha256: String,
}

impl CompanionFile {
    /// The record of `bytes` written at `file`.
    #[must_use]
    pub fn new(file: String, bytes: &[u8]) -> CompanionFile {
        CompanionFile {
            file,
            size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(bytes),
        }
    }
}

/// A file derived from an artifact, with its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedFile {
    /// Path relative to the output directory.
    pub file: String,
    /// The file's bytes.
    pub bytes: Vec<u8>,
}

/// The Helm-ready schema and name map of a schema artifact's `bytes`, in that
/// order, or `None` for an artifact Helm never reads.
///
/// # Errors
///
/// Returns an error when `bytes` are not JSON.
pub fn helm_ready_files(
    spec: &ArtifactSpec,
    bytes: &[u8],
) -> Result<Option<[DerivedFile; 2]>, serde_json::Error> {
    let shipped = match &spec.recipe {
        GenerationRecipe::Chart(chart) => chart.minimize,
        GenerationRecipe::FinalPolicy(_) => true,
        GenerationRecipe::Template(_) | GenerationRecipe::Ir(_) => false,
    };
    if !shipped {
        return Ok(None);
    }
    let schema: Value = serde_json::from_slice(bytes)?;
    let shortened = helm_schema_json_schema_minify::shorten_definition_names(&schema);
    let mut schema_bytes = serde_json::to_vec(&shortened.schema)?;
    schema_bytes.push(b'\n');
    let mut map_bytes = serde_json::to_vec_pretty(&shortened.readable_names)?;
    map_bytes.push(b'\n');
    let base = spec
        .dump_name
        .strip_suffix(".schema.json")
        .unwrap_or(&spec.dump_name);
    Ok(Some([
        DerivedFile {
            file: format!("{INTERNAL_DIR}/{base}.helm.schema.json"),
            bytes: schema_bytes,
        },
        DerivedFile {
            file: format!("{INTERNAL_DIR}/{base}.defs-map.json"),
            bytes: map_bytes,
        },
    ]))
}

impl ManifestEntry {
    /// The entry for `spec`'s artifact `bytes`, generated from inputs with digest `inputs_sha256`.
    #[must_use]
    pub fn new(spec: &ArtifactSpec, inputs_sha256: String, bytes: &[u8]) -> ManifestEntry {
        ManifestEntry {
            key: spec.id.key(),
            kind: spec.id.kind().name().to_string(),
            file: artifact_file(spec),
            fixture: match &spec.target {
                ArtifactTarget::Fixture(path) => Some(path.clone()),
                ArtifactTarget::Internal => None,
            },
            recipe: spec.recipe.describe(),
            inputs_sha256,
            size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(bytes),
            helm_ready: None,
        }
    }

    /// The first registry-derived field on which this entry disagrees with `spec`.
    fn mismatch(&self, spec: &ArtifactSpec) -> Option<&'static str> {
        let expected = ManifestEntry::new(spec, String::new(), &[]);
        if self.kind != expected.kind {
            Some("kind")
        } else if self.file != expected.file {
            Some("file")
        } else if self.fixture != expected.fixture {
            Some("fixture")
        } else if self.recipe != expected.recipe {
            Some("recipe")
        } else {
            None
        }
    }
}

/// Where the producer writes `spec`, relative to its output directory.
#[must_use]
pub fn artifact_file(spec: &ArtifactSpec) -> String {
    match spec.target {
        ArtifactTarget::Fixture(_) => spec.dump_name.clone(),
        ArtifactTarget::Internal => format!("{INTERNAL_DIR}/{}", spec.dump_name),
    }
}

/// The absolute testdata directory of this checkout, as recorded in manifests.
#[must_use]
pub fn testdata_location() -> String {
    test_util::workspace_testdata()
        .to_string_lossy()
        .into_owned()
}

/// Reads the manifest in `dir` and checks everything short of artifact bytes.
///
/// The harness version, the producer's build provenance, the source tree, the
/// testdata location, exact registry membership without duplicates, every
/// entry's kind, file, fixture and recipe, and every artifact's presence and
/// size must all match.
///
/// # Errors
///
/// Returns the first [`ProvenanceError`] found.
pub fn load(dir: &Path) -> Result<Manifest, ProvenanceError> {
    let path = dir.join(MANIFEST_FILE);
    let bytes = std::fs::read(&path).map_err(|source| ProvenanceError::Io {
        path: path.clone(),
        source,
    })?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|source| ProvenanceError::Parse { path, source })?;
    check_structure(dir, &manifest)?;
    Ok(manifest)
}

/// Checks a manifest's provenance and complete registry membership.
fn check_structure(dir: &Path, manifest: &Manifest) -> Result<(), ProvenanceError> {
    if manifest.harness_version != HARNESS_VERSION {
        return Err(ProvenanceError::HarnessVersion {
            found: manifest.harness_version,
            expected: HARNESS_VERSION,
        });
    }
    let expected = BuildProvenance::compiled();
    if manifest.build != expected {
        return Err(ProvenanceError::StaleProducer {
            recorded: Box::new(manifest.build.clone()),
            expected: Box::new(expected),
        });
    }
    expected.check_current()?;
    let current = testdata_location();
    if manifest.testdata != current {
        return Err(ProvenanceError::Location {
            recorded: manifest.testdata.clone(),
            current,
        });
    }

    let specs = registry::registry();
    let registered = specs
        .iter()
        .map(|spec| spec.id.key())
        .collect::<BTreeSet<_>>();
    let mut entries = BTreeMap::new();
    let mut unexpected = Vec::new();
    for entry in &manifest.artifacts {
        if !registered.contains(&entry.key) || entries.insert(entry.key.clone(), entry).is_some() {
            unexpected.push(entry.key.clone());
        }
    }
    if !unexpected.is_empty() {
        return Err(ProvenanceError::Unexpected { keys: unexpected });
    }
    let missing = registered
        .iter()
        .filter(|key| !entries.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(ProvenanceError::Incomplete { missing });
    }

    for spec in &specs {
        let key = spec.id.key();
        let Some(entry) = entries.get(&key) else {
            return Err(ProvenanceError::Incomplete { missing: vec![key] });
        };
        if let Some(field) = entry.mismatch(spec) {
            return Err(ProvenanceError::EntryMismatch { key, field });
        }
        let path = dir.join(&entry.file);
        let size = std::fs::metadata(&path).map(|metadata| metadata.len()).ok();
        if size != Some(entry.size) {
            return Err(ProvenanceError::ArtifactBytes { path });
        }
    }
    Ok(())
}

/// Reads one artifact of a [`load`]ed manifest after rechecking its recipe
/// inputs and its bytes.
///
/// # Errors
///
/// [`ProvenanceError::StaleInputs`] when the recipe's inputs changed, and
/// [`ProvenanceError::ArtifactBytes`] when the file differs from its entry.
pub fn read_artifact(
    dir: &Path,
    manifest: &Manifest,
    spec: &ArtifactSpec,
    inputs: &mut InputDigester,
) -> Result<Vec<u8>, ProvenanceError> {
    let key = spec.id.key();
    let entry = manifest
        .artifacts
        .iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| ProvenanceError::Incomplete {
            missing: vec![key.clone()],
        })?;
    if inputs.digest(&spec.recipe)? != entry.inputs_sha256 {
        return Err(ProvenanceError::StaleInputs { key });
    }
    let path = dir.join(&entry.file);
    let bytes = std::fs::read(&path).map_err(|source| ProvenanceError::Io {
        path: path.clone(),
        source,
    })?;
    if u64::try_from(bytes.len()).ok() != Some(entry.size) || sha256_hex(&bytes) != entry.sha256 {
        return Err(ProvenanceError::ArtifactBytes { path });
    }
    Ok(bytes)
}

/// Checks `manifest` completely against `dir`: structure and provenance as in
/// [`load`], every recipe's inputs, every artifact's bytes, and no file the
/// manifest does not list.
///
/// # Errors
///
/// Returns the first [`ProvenanceError`] found.
pub fn verify_all(dir: &Path, manifest: &Manifest) -> Result<(), ProvenanceError> {
    check_structure(dir, manifest)?;
    let mut inputs = InputDigester::new(test_util::workspace_testdata());
    let mut listed = BTreeSet::new();
    for spec in registry::registry() {
        let bytes = read_artifact(dir, manifest, &spec, &mut inputs)?;
        let key = spec.id.key();
        let Some(entry) = manifest.artifacts.iter().find(|entry| entry.key == key) else {
            continue;
        };
        listed.insert(entry.file.clone());
        if let Some(helm_ready) = &entry.helm_ready {
            verify_helm_ready(dir, &spec, &bytes, helm_ready)?;
            listed.insert(helm_ready.schema.file.clone());
            listed.insert(helm_ready.defs_map.file.clone());
        }
    }
    listed.extend([MANIFEST_FILE, INTERNAL_DIR, LOCK_FILE].map(str::to_string));
    let mut foreign = Vec::new();
    for listing in [dir.to_path_buf(), dir.join(INTERNAL_DIR)] {
        let entries = std::fs::read_dir(&listing).map_err(|source| ProvenanceError::Io {
            path: listing.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| ProvenanceError::Io {
                path: listing.clone(),
                source,
            })?;
            let path = entry.path();
            let relative = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if !listed.contains(&relative) {
                foreign.push(relative);
            }
        }
    }
    if foreign.is_empty() {
        Ok(())
    } else {
        Err(ProvenanceError::ForeignFiles {
            dir: dir.to_path_buf(),
            files: foreign,
        })
    }
}

/// Checks that the Helm-ready files of an artifact are its recorded
/// derivation from `bytes`.
fn verify_helm_ready(
    dir: &Path,
    spec: &ArtifactSpec,
    bytes: &[u8],
    helm_ready: &HelmReadyFiles,
) -> Result<(), ProvenanceError> {
    let artifact = dir.join(artifact_file(spec));
    let derived = helm_ready_files(spec, bytes)
        .map_err(|source| ProvenanceError::Parse {
            path: artifact.clone(),
            source,
        })?
        .ok_or(ProvenanceError::ArtifactBytes { path: artifact })?;
    for (recorded, derived) in [&helm_ready.schema, &helm_ready.defs_map]
        .into_iter()
        .zip(derived)
    {
        let path = dir.join(&derived.file);
        let have = std::fs::read(&path).map_err(|source| ProvenanceError::Io {
            path: path.clone(),
            source,
        })?;
        if *recorded != CompanionFile::new(derived.file, &derived.bytes) || have != derived.bytes {
            return Err(ProvenanceError::ArtifactBytes { path });
        }
    }
    Ok(())
}

/// Digests the files each recipe reads besides the source tree.
///
/// Whole-chart recipes read their full chart directory, values files and the
/// provider bundle; template recipes read their template, their values file
/// unless inline values replace it, their helpers and the bundle; IR recipes
/// read their template and helpers. Helper directories contribute only their
/// immediate files with the loaded extension, exactly what the loader reads.
/// The bundle digest is computed at most once.
#[derive(Debug)]
pub struct InputDigester {
    testdata: PathBuf,
    provider_bundle: Option<String>,
}

impl InputDigester {
    /// A digester over the testdata directory `testdata`.
    #[must_use]
    pub fn new(testdata: PathBuf) -> InputDigester {
        InputDigester {
            testdata,
            provider_bundle: None,
        }
    }

    /// The input digest of `recipe`.
    ///
    /// # Errors
    ///
    /// [`ProvenanceError::Io`] when an input cannot be read.
    pub fn digest(&mut self, recipe: &GenerationRecipe) -> Result<String, ProvenanceError> {
        let testdata = self.testdata.clone();
        let mut files = Vec::new();
        let uses_provider_bundle = match recipe {
            GenerationRecipe::Chart(chart) => {
                collect_all(&testdata.join("charts").join(chart.chart), &mut files)?;
                files.extend(chart.values_files.iter().map(|path| testdata.join(path)));
                true
            }
            GenerationRecipe::FinalPolicy(policy) => {
                collect_all(
                    &testdata.join("charts").join(policy.chart.chart),
                    &mut files,
                )?;
                files.extend(
                    policy
                        .chart
                        .values_files
                        .iter()
                        .map(|path| testdata.join(path)),
                );
                true
            }
            GenerationRecipe::Template(template) => {
                files.push(testdata.join(template.template_path));
                if template.inline_values.is_none() {
                    files.push(testdata.join(template.values_path));
                }
                self.define_sources(template.define_sources, &mut files)?;
                true
            }
            GenerationRecipe::Ir(ir) => {
                files.push(testdata.join(ir.template_path));
                self.define_sources(ir.define_sources, &mut files)?;
                false
            }
        };
        let digest = digest_files(&testdata, &files).map_err(|source| ProvenanceError::Io {
            path: testdata.clone(),
            source,
        })?;
        if !uses_provider_bundle {
            return Ok(digest);
        }
        let bundle = self.provider_bundle()?;
        Ok(sha256_hex(
            format!("{digest}\n{PROVIDER_BUNDLE}\t{bundle}\n").as_bytes(),
        ))
    }

    fn provider_bundle(&mut self) -> Result<String, ProvenanceError> {
        if let Some(digest) = &self.provider_bundle {
            return Ok(digest.clone());
        }
        let bundle = self.testdata.join(PROVIDER_BUNDLE);
        let mut files = Vec::new();
        collect_all(&bundle, &mut files)?;
        let digest = digest_files(&bundle, &files).map_err(|source| ProvenanceError::Io {
            path: bundle.clone(),
            source,
        })?;
        self.provider_bundle = Some(digest.clone());
        Ok(digest)
    }

    fn define_sources(
        &self,
        sources: test_util::DefineSourceSpec<'_>,
        files: &mut Vec<PathBuf>,
    ) -> Result<(), ProvenanceError> {
        files.extend(
            sources
                .helper_templates
                .iter()
                .map(|path| self.testdata.join(path)),
        );
        for (dir, extension) in sources.helper_template_dirs {
            let dir = self.testdata.join(dir);
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                // The loader reads a missing helper directory as empty.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => return Err(ProvenanceError::Io { path: dir, source }),
            };
            for entry in entries {
                let path = entry
                    .map_err(|source| ProvenanceError::Io {
                        path: dir.clone(),
                        source,
                    })?
                    .path();
                if path.extension().is_some_and(|found| found == *extension) {
                    files.push(path);
                }
            }
        }
        files.extend(
            sources
                .file_sources
                .iter()
                .map(|(_, path)| self.testdata.join(path)),
        );
        Ok(())
    }
}

fn collect_all(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), ProvenanceError> {
    collect_files(dir, files, &|_| true).map_err(|source| ProvenanceError::Io {
        path: dir.to_path_buf(),
        source,
    })
}
