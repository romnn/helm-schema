//! Producer dumps verified from their own manifests, without trusting the
//! comparator's build.
//!
//! A dump from another revision cannot pass [`crate::manifest::load`]: that
//! check binds a manifest to the build reading it. [`VerifiedDump::open`]
//! instead checks everything the dump says about itself (harness version,
//! unique registered keys, exact artifact bytes, Helm-ready companions derived
//! from those bytes, no foreign files) and records its build provenance.
//! [`DumpPair::new`] then requires both dumps to describe the same registry
//! entries generated from the same recipe inputs, which are also the inputs in
//! the comparator's own checkout, so its charts and defaults are the ones both
//! revisions read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::DiffError;
use crate::acceptance::{ChartInputs, ComparedChart, LoadedInputs};
use crate::manifest::{
    self, BuildProvenance, CompanionFile, HARNESS_VERSION, INTERNAL_DIR, LOCK_FILE, MANIFEST_FILE,
    Manifest, ManifestEntry,
};
use crate::registry::{self, ArtifactId, ArtifactSpec, GenerationRecipe};
use crate::source_digest::sha256_hex;

/// A producer dump whose every artifact matches its own manifest.
#[derive(Debug)]
pub struct VerifiedDump {
    root: PathBuf,
    manifest: Manifest,
    manifest_sha256: String,
    entries: BTreeMap<String, ManifestEntry>,
}

/// One schema artifact of a [`VerifiedDump`], with its exact bytes.
#[derive(Debug, Clone)]
pub struct VerifiedSchema {
    /// The registry key.
    pub key: String,
    /// The artifact's path inside its dump.
    pub path: PathBuf,
    /// The artifact's exact bytes.
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`.
    pub sha256: String,
    /// SHA-256 of the dump's manifest.
    pub dump_manifest_sha256: String,
}

impl VerifiedDump {
    /// Reads and verifies the dump at `root`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the manifest is malformed, from another
    /// harness version, lists an unregistered or duplicate key, or disagrees
    /// with any artifact or companion byte; [`DiffError::Infrastructure`]
    /// when a file cannot be read.
    pub fn open(root: &Path) -> Result<Self, DiffError> {
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest_bytes = read(&manifest_path)?;
        let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
            DiffError::Identity(format!("parse {}: {error}", manifest_path.display()))
        })?;
        if manifest.harness_version != HARNESS_VERSION {
            return Err(DiffError::Identity(format!(
                "{} has harness version {}, expected {HARNESS_VERSION}",
                manifest_path.display(),
                manifest.harness_version
            )));
        }
        let specs = registry::registry()
            .into_iter()
            .map(|spec| (spec.id.key(), spec))
            .collect::<BTreeMap<_, _>>();
        let mut entries = BTreeMap::new();
        let mut listed =
            BTreeSet::from([MANIFEST_FILE, INTERNAL_DIR, LOCK_FILE].map(str::to_string));
        for entry in &manifest.artifacts {
            let spec = specs.get(&entry.key).ok_or_else(|| {
                DiffError::Identity(format!(
                    "{} lists unregistered {}",
                    root.display(),
                    entry.key
                ))
            })?;
            if entries.insert(entry.key.clone(), entry.clone()).is_some() {
                return Err(DiffError::Identity(format!(
                    "{} lists {} twice",
                    root.display(),
                    entry.key
                )));
            }
            let bytes = read_listed(root, &entry.file, entry.size, &entry.sha256)?;
            listed.insert(entry.file.clone());
            if let Some(helm_ready) = &entry.helm_ready {
                verify_companions(root, spec, &bytes, helm_ready)?;
                listed.insert(helm_ready.schema.file.clone());
                listed.insert(helm_ready.defs_map.file.clone());
            }
        }
        check_no_foreign_files(root, &listed)?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
            manifest_sha256: sha256_hex(&manifest_bytes),
            entries,
        })
    }

    /// The dump's directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// SHA-256 of the dump's manifest bytes.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    /// The build the producer was compiled as.
    #[must_use]
    pub fn build(&self) -> &BuildProvenance {
        &self.manifest.build
    }

    /// Every registry key the dump lists.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The artifact `key`, its bytes read again and checked against the manifest.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the dump does not list `key` or the bytes
    /// changed since the dump was opened.
    pub fn schema(&self, key: &str) -> Result<VerifiedSchema, DiffError> {
        let entry = self.entries.get(key).ok_or_else(|| {
            DiffError::Identity(format!("{} does not list {key}", self.root.display()))
        })?;
        let bytes = read_listed(&self.root, &entry.file, entry.size, &entry.sha256)?;
        Ok(VerifiedSchema {
            key: key.to_string(),
            path: self.root.join(&entry.file),
            sha256: sha256_hex(&bytes),
            bytes,
            dump_manifest_sha256: self.manifest_sha256.clone(),
        })
    }
}

/// Two verified dumps of the same registry entries and recipe inputs.
#[derive(Debug)]
pub struct DumpPair {
    /// The baseline revision's dump.
    pub baseline: VerifiedDump,
    /// The candidate revision's dump.
    pub candidate: VerifiedDump,
}

impl DumpPair {
    /// Pairs two dumps whose entries agree on everything but their bytes and
    /// whose chart inputs are the inputs in `testdata`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the key sets, an entry's kind, file,
    /// fixture, recipe or input digest differ, or a recipe's inputs in
    /// `testdata` differ from what both producers read.
    pub fn new(
        baseline: VerifiedDump,
        candidate: VerifiedDump,
        testdata: &Path,
    ) -> Result<Self, DiffError> {
        let baseline_keys = baseline.entries.keys().collect::<BTreeSet<_>>();
        let candidate_keys = candidate.entries.keys().collect::<BTreeSet<_>>();
        if baseline_keys != candidate_keys {
            return Err(DiffError::Identity(format!(
                "dumps list different keys: baseline-only {:?}, candidate-only {:?}",
                baseline_keys
                    .difference(&candidate_keys)
                    .collect::<Vec<_>>(),
                candidate_keys
                    .difference(&baseline_keys)
                    .collect::<Vec<_>>()
            )));
        }
        let specs = registry::registry()
            .into_iter()
            .map(|spec| (spec.id.key(), spec))
            .collect::<BTreeMap<_, _>>();
        let mut digester = manifest::InputDigester::new(testdata.to_path_buf());
        for (key, left) in &baseline.entries {
            let right = candidate
                .entries
                .get(key)
                .ok_or_else(|| DiffError::Identity(format!("candidate does not list {key}")))?;
            let ManifestEntry {
                key: _,
                kind,
                file,
                fixture,
                recipe,
                inputs_sha256,
                size: _,
                sha256: _,
                helm_ready: _,
            } = left;
            for (field, same) in [
                ("kind", *kind == right.kind),
                ("file", *file == right.file),
                ("fixture", *fixture == right.fixture),
                ("recipe", *recipe == right.recipe),
                ("inputs", *inputs_sha256 == right.inputs_sha256),
            ] {
                if !same {
                    return Err(DiffError::Identity(format!(
                        "dumps disagree on the {field} of {key}"
                    )));
                }
            }
            let spec = specs.get(key).ok_or_else(|| {
                DiffError::Identity(format!("{key} is not registered in this comparator"))
            })?;
            let current = digester.digest(&spec.recipe).map_err(|error| {
                DiffError::Infrastructure(format!("digest the inputs of {key}: {error}"))
            })?;
            if current != *inputs_sha256 {
                return Err(DiffError::Identity(format!(
                    "the inputs of {key} in {} differ from the ones both producers read",
                    testdata.display()
                )));
            }
        }
        Ok(Self {
            baseline,
            candidate,
        })
    }

    /// The whole-chart and lean schema pairs the acceptance battery screens,
    /// in registry order, optionally only `selection`'s labels.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] when a selected label is unknown or the
    /// selection is empty.
    pub fn compared_charts(
        &self,
        selection: Option<&BTreeSet<String>>,
    ) -> Result<Vec<ComparedChart>, DiffError> {
        let mut charts = Vec::new();
        let mut known = BTreeSet::new();
        for spec in registry::registry() {
            let Some((label, chart)) = screened_schema(&spec) else {
                continue;
            };
            known.insert(label.clone());
            if selection.is_some_and(|selection| !selection.contains(&label)) {
                continue;
            }
            charts.push(self.compared_chart(&spec.id.key(), label, chart)?);
        }
        if let Some(selection) = selection
            && let Some(unknown) = selection.iter().find(|label| !known.contains(*label))
        {
            return Err(DiffError::Request(format!("unknown chart {unknown}")));
        }
        if charts.is_empty() {
            return Err(DiffError::Request("no chart selected".to_string()));
        }
        Ok(charts)
    }

    fn compared_chart(
        &self,
        key: &str,
        label: String,
        chart: &'static str,
    ) -> Result<ComparedChart, DiffError> {
        let baseline = self.baseline.schema(key)?;
        let candidate = self.candidate.schema(key)?;
        let candidate_path = candidate.path.clone();
        Ok(ComparedChart {
            label,
            chart_relative_path: chart.to_string(),
            inputs: ChartInputs::Artifact {
                load: Box::new(move || {
                    let mut baseline_schema = serde_json::from_slice(&baseline.bytes)?;
                    helm_schema_core::normalize_schema_pattern_dialects(&mut baseline_schema);
                    Ok(LoadedInputs {
                        baseline: baseline_schema,
                        baseline_bytes: baseline.bytes,
                        candidate: serde_json::from_slice(&candidate.bytes)?,
                        candidate_bytes: candidate.bytes,
                        defaults: crate::acceptance::discovery::read_coalesced_defaults(chart)?,
                    })
                }),
                candidate_path,
            },
            // Attribution needs each revision's own annotate regeneration,
            // which a dump does not carry; the comparator never regenerates it.
            annotation: None,
            replay: Vec::new(),
        })
    }
}

/// The battery label and chart directory of a schema artifact the
/// acceptance battery screens: whole-chart corpus schemas and lean controls.
fn screened_schema(spec: &ArtifactSpec) -> Option<(String, &'static str)> {
    let GenerationRecipe::Chart(recipe) = spec.recipe else {
        return None;
    };
    match spec.id {
        ArtifactId::Chart(_) => {
            let stem = spec
                .dump_name
                .strip_prefix("helm-schema.cli.chart-corpus.")?
                .strip_suffix(".schema.json")?;
            Some((stem.to_string(), recipe.chart))
        }
        ArtifactId::Lean(lean) => Some((format!("lean/{}", lean.chart()), recipe.chart)),
        ArtifactId::Template(_)
        | ArtifactId::FinalPolicy(_)
        | ArtifactId::Authoring(_)
        | ArtifactId::Ir(_) => None,
    }
}

fn read(path: &Path) -> Result<Vec<u8>, DiffError> {
    std::fs::read(path)
        .map_err(|error| DiffError::Infrastructure(format!("read {}: {error}", path.display())))
}

/// The bytes of the dump-relative `file`, which must be a safe relative path
/// with exactly `size` bytes hashing to `sha256`.
fn read_listed(root: &Path, file: &str, size: u64, sha256: &str) -> Result<Vec<u8>, DiffError> {
    if !registry::is_safe_relative_path(file) {
        return Err(DiffError::Identity(format!("unsafe artifact path {file}")));
    }
    let bytes = read(&root.join(file))?;
    if u64::try_from(bytes.len()).ok() != Some(size) || sha256_hex(&bytes) != sha256 {
        return Err(DiffError::Identity(format!(
            "{} differs from its manifest entry",
            root.join(file).display()
        )));
    }
    Ok(bytes)
}

/// Checks that the Helm-ready files are the recorded derivation of `bytes`.
fn verify_companions(
    root: &Path,
    spec: &ArtifactSpec,
    bytes: &[u8],
    helm_ready: &manifest::HelmReadyFiles,
) -> Result<(), DiffError> {
    let derived = manifest::helm_ready_files(spec, bytes)
        .map_err(|error| DiffError::Identity(format!("parse {}: {error}", spec.dump_name)))?
        .ok_or_else(|| {
            DiffError::Identity(format!("{} has no Helm-ready spelling", spec.dump_name))
        })?;
    for (recorded, derived) in [&helm_ready.schema, &helm_ready.defs_map]
        .into_iter()
        .zip(derived)
    {
        let have = read(&root.join(&derived.file))?;
        if *recorded != CompanionFile::new(derived.file.clone(), &derived.bytes)
            || have != derived.bytes
        {
            return Err(DiffError::Identity(format!(
                "{} is not the Helm-ready spelling of {}",
                derived.file, spec.dump_name
            )));
        }
    }
    Ok(())
}

/// Refuses files in the dump's directory or its `internal/` directory that
/// the manifest does not list.
fn check_no_foreign_files(root: &Path, listed: &BTreeSet<String>) -> Result<(), DiffError> {
    let mut foreign = Vec::new();
    for directory in [root.to_path_buf(), root.join(INTERNAL_DIR)] {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && directory != root => {
                continue;
            }
            Err(error) => {
                return Err(DiffError::Infrastructure(format!(
                    "list {}: {error}",
                    directory.display()
                )));
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| {
                DiffError::Infrastructure(format!("list {}: {error}", directory.display()))
            })?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
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
        Err(DiffError::Identity(format!(
            "{} holds files its manifest does not list: {foreign:?}",
            root.display()
        )))
    }
}

/// The registry key, recipe input digest and recipe hash of each screened chart label.
///
/// # Errors
///
/// [`DiffError::Identity`] when a screened artifact is missing from `dump`.
pub fn chart_keys(dump: &VerifiedDump) -> Result<BTreeMap<String, ChartKey>, DiffError> {
    let mut keys = BTreeMap::new();
    for spec in registry::registry() {
        let Some((label, chart)) = screened_schema(&spec) else {
            continue;
        };
        let key = spec.id.key();
        let entry = dump.entries.get(&key).ok_or_else(|| {
            DiffError::Identity(format!("{} does not list {key}", dump.root.display()))
        })?;
        keys.insert(
            label,
            ChartKey {
                key,
                chart,
                inputs_sha256: entry.inputs_sha256.clone(),
                recipe_sha256: super::history::canonical_sha256(&entry.recipe)?,
            },
        );
    }
    Ok(keys)
}

/// A screened chart's registry identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartKey {
    /// The registry key.
    pub key: String,
    /// The chart directory relative to the corpus charts.
    pub chart: &'static str,
    /// The producer's input digest of its recipe.
    pub inputs_sha256: String,
    /// SHA-256 of its canonical recipe description.
    pub recipe_sha256: String,
}

/// One artifact's bytes in both dumps.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactPair {
    /// The registry key.
    pub key: String,
    /// The artifact family.
    pub kind: String,
    /// SHA-256 of main's bytes.
    pub baseline_sha256: String,
    /// SHA-256 of the candidate's bytes.
    pub candidate_sha256: String,
}

impl DumpPair {
    /// Every artifact's hash in both dumps, in key order.
    #[must_use]
    pub fn artifacts(&self) -> Vec<ArtifactPair> {
        self.baseline
            .entries
            .iter()
            .filter_map(|(key, baseline)| {
                let candidate = self.candidate.entries.get(key)?;
                Some(ArtifactPair {
                    key: key.clone(),
                    kind: baseline.kind.clone(),
                    baseline_sha256: baseline.sha256.clone(),
                    candidate_sha256: candidate.sha256.clone(),
                })
            })
            .collect()
    }
}
