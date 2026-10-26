//! The persistent witness universe and the hash-chained receipts.
//!
//! Every witness ever discovered is kept as an immutable, version-1 record
//! whose identity excludes everything a candidate can move: its schema
//! digest, a JSON pointer, an ordinal or a checkpoint number. Records arrive
//! in immutable batches. One writer stages and verifies each transaction
//! before atomically publishing its gate and complete batch list. Shared bytes
//! live in a content-addressed blob store and are rehashed on every read.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::DiffError;
use crate::source_digest::sha256_hex;

/// The only witness, plan and receipt format this tool reads.
pub const FORMAT_VERSION: u32 = 1;

/// The observation every schema witness answers.
pub const SCHEMA_ACCEPTANCE: &str = "schema-acceptance";

/// `value` as canonical JSON: object members sorted by key, no whitespace.
#[must_use]
pub fn canonical_json(value: &Value) -> Vec<u8> {
    let mut sorted = value.clone();
    sorted.sort_all_objects();
    // Serializing a `Value` cannot fail.
    serde_json::to_vec(&sorted).unwrap_or_default()
}

/// SHA-256 of `value`'s canonical JSON.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when `value` cannot be serialized.
pub fn canonical_sha256<T: Serialize>(value: &T) -> Result<String, DiffError> {
    let value = serde_json::to_value(value)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    Ok(sha256_hex(&canonical_json(&value)))
}

/// Writes `bytes` to the new file `path`: a temporary sibling renamed into
/// place, refusing to replace anything.
///
/// # Errors
///
/// [`DiffError::Identity`] when `path` exists; [`DiffError::Infrastructure`]
/// when the file cannot be written.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), DiffError> {
    publish_new(path, bytes, true)
}

/// [`write_new`], with the data flushed to disk before the rename when
/// `durable`. Content-addressed blobs skip the flush: every read rehashes
/// them, so a blob lost to a crash fails closed instead of misleading.
fn publish_new(path: &Path, bytes: &[u8], durable: bool) -> Result<(), DiffError> {
    if path.exists() {
        return Err(DiffError::Identity(format!(
            "{} already exists; outputs are never replaced",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| DiffError::Infrastructure(format!("{} has no parent", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|error| io(parent, &error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| io(parent, &error))?;
    temporary
        .write_all(bytes)
        .and_then(|()| {
            if durable {
                temporary.as_file().sync_all()
            } else {
                Ok(())
            }
        })
        .map_err(|error| io(path, &error))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| io(path, &error.error))?;
    Ok(())
}

fn io(path: &Path, error: &std::io::Error) -> DiffError {
    DiffError::Infrastructure(format!("{}: {error}", path.display()))
}

fn read(path: &Path) -> Result<Vec<u8>, DiffError> {
    std::fs::read(path).map_err(|error| io(path, &error))
}

/// Content-addressed immutable bytes.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// The store under `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The store's directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, sha256: &str) -> Result<PathBuf, DiffError> {
        let prefix = sha256
            .get(..2)
            .filter(|_| sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| DiffError::Identity(format!("invalid blob id {sha256}")))?;
        Ok(self.root.join(prefix).join(sha256))
    }

    /// Stores `bytes` and returns their SHA-256; an existing blob must hold
    /// exactly these bytes.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the stored blob differs;
    /// [`DiffError::Infrastructure`] when it cannot be written.
    pub fn put(&self, bytes: &[u8]) -> Result<String, DiffError> {
        let sha256 = sha256_hex(bytes);
        let path = self.path(&sha256)?;
        if path.exists() {
            self.get(&sha256)?;
        } else {
            publish_new(&path, bytes, false)?;
        }
        Ok(sha256)
    }

    /// The bytes of `sha256`, rehashed.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the blob is missing or its bytes changed.
    pub fn get(&self, sha256: &str) -> Result<Vec<u8>, DiffError> {
        let path = self.path(sha256)?;
        let bytes = std::fs::read(&path)
            .map_err(|error| DiffError::Identity(format!("blob {sha256}: {error}")))?;
        if sha256_hex(&bytes) != sha256 {
            return Err(DiffError::Identity(format!("blob {sha256} changed")));
        }
        Ok(bytes)
    }
}

/// Which document a schema witness is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessMode {
    /// A structural probe of the exact-template lane, composed over the
    /// chart's coalesced defaults.
    TemplateProbe,
    /// `helm lint`'s raw values document.
    LintRaw,
    /// `helm lint`'s twice-coalesced document.
    LintCoalescedTwice,
    /// The once-coalesced document `helm template` validates.
    Template,
    /// A version-1 non-schema observation control.
    Observation,
}

/// Everything that makes two witnesses the same experiment, and nothing a
/// candidate can move.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WitnessIdentity {
    /// The chart's label.
    pub chart: String,
    /// The producer's input digest of the chart's recipe.
    pub chart_input_sha256: String,
    /// SHA-256 of the canonical recipe description.
    pub recipe_sha256: String,
    /// SHA-256 of the exact values-file bytes.
    pub values_sha256: String,
    /// Which document is judged.
    pub mode: WitnessMode,
    /// The Kubernetes version Helm renders for.
    pub kubernetes_version: String,
    /// SHA-256 of the canonical, ordered applicable context overlays.
    pub context_sha256: String,
    /// The observation answered.
    pub observation: String,
}

impl WitnessIdentity {
    /// The witness id: SHA-256 of the canonical identity.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when the identity cannot be serialized.
    pub fn id(&self) -> Result<String, DiffError> {
        canonical_sha256(self)
    }
}

/// A document-lane witness's lint facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    /// Whether it is a forced raw-lint audit.
    pub lint_control: bool,
    /// The applicable context-only overlays, defaults included.
    pub control_overlays: Vec<Value>,
    /// Blob of the composed document's canonical JSON.
    pub composed_blob: String,
}

/// A template-lane witness's probe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum InstanceRecord {
    /// The chart's defaults alone.
    Defaults,
    /// A sparse override coalesced over the defaults.
    SparseOverride(Value),
    /// A complete coalesced document.
    Coalesced(Value),
}

/// One persistent witness, version 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WitnessRecord {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// [`WitnessIdentity::id`].
    pub id: String,
    /// The experiment.
    pub identity: WitnessIdentity,
    /// The name screening reported when it was discovered.
    pub name: String,
    /// The overlay Helm receives.
    pub overlay: Value,
    /// Blob of the exact values-file bytes.
    pub values_blob: String,
    /// SHA-256 of the composed document's canonical JSON.
    pub composed_sha256: String,
    /// Blob of the chart's canonical coalesced defaults, for template probes.
    pub defaults_blob: Option<String>,
    /// The template probe, for the exact-template lane.
    pub instance: Option<InstanceRecord>,
    /// The lint facts, for the document lane.
    pub document: Option<DocumentRecord>,
    /// The control case, for an observation witness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<ControlRecord>,
    /// Where it came from: row origins and discovering attempts.
    pub origins: Vec<String>,
}

/// An observation witness's control case and every blob it needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRecord {
    /// Blob of the case's canonical JSON.
    pub case_blob: String,
    /// The case's closure: input, chart files and grounding evidence, sorted.
    pub blobs: Vec<String>,
}

impl ControlRecord {
    /// The case, rehashed and checked against `identity`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the case, its identity or its closure
    /// differ from the record.
    pub fn case(
        &self,
        identity: &WitnessIdentity,
        blobs: &BlobStore,
    ) -> Result<super::controls::ControlCase, DiffError> {
        let case: super::controls::ControlCase =
            serde_json::from_slice(&blobs.get(&self.case_blob)?)
                .map_err(|error| DiffError::Identity(format!("control case: {error}")))?;
        case.validate()?;
        let closure = case.closure(&super::controls::ControlBlobs::store(blobs.root()))?;
        if case.identity()? != *identity || closure.into_iter().collect::<Vec<_>>() != self.blobs {
            return Err(DiffError::Identity(format!(
                "control case {} does not match its witness",
                case.id
            )));
        }
        Ok(case)
    }
}

impl WitnessRecord {
    /// Checks the record's version, its id and its blobs.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] on any mismatch.
    pub fn verify(&self, blobs: &BlobStore) -> Result<(), DiffError> {
        if self.format_version != FORMAT_VERSION {
            return Err(DiffError::Identity(format!(
                "witness {} has format version {}",
                self.id, self.format_version
            )));
        }
        if self.identity.id()? != self.id {
            return Err(DiffError::Identity(format!(
                "witness {} does not hash to its id",
                self.id
            )));
        }
        if sha256_hex(&blobs.get(&self.values_blob)?) != self.identity.values_sha256 {
            return Err(DiffError::Identity(format!(
                "witness {} values blob mismatch",
                self.id
            )));
        }
        match (&self.instance, &self.document, &self.defaults_blob) {
            _ if self.identity.mode == WitnessMode::Observation => {
                let control = self.control.as_ref().ok_or_else(|| {
                    DiffError::Identity(format!("observation witness {} has no case", self.id))
                })?;
                let case = control.case(&self.identity, blobs)?;
                if self.instance.is_some()
                    || self.document.is_some()
                    || self.defaults_blob.is_some()
                    || self.composed_sha256 != case.input.coalesced_document_sha256
                {
                    return Err(DiffError::Identity(format!(
                        "observation witness {} carries schema-lane facts",
                        self.id
                    )));
                }
            }
            _ if self.control.is_some() => {
                return Err(DiffError::Identity(format!(
                    "schema witness {} carries a control case",
                    self.id
                )));
            }
            (Some(_), None, Some(defaults)) => {
                blobs.get(defaults)?;
            }
            (None, Some(document), None) => {
                if sha256_hex(&blobs.get(&document.composed_blob)?) != self.composed_sha256 {
                    return Err(DiffError::Identity(format!(
                        "witness {} composed blob mismatch",
                        self.id
                    )));
                }
            }
            _ => {
                return Err(DiffError::Identity(format!(
                    "witness {} names neither or both lanes",
                    self.id
                )));
            }
        }
        Ok(())
    }
}

/// Immutable witness batches and their content-addressed blobs.
#[derive(Debug)]
pub struct Universe {
    root: PathBuf,
    blobs: BlobStore,
}

/// The complete witness union of a transaction.
#[derive(Debug, Default)]
pub struct UniverseState {
    /// Every witness, by id, with origins merged.
    pub witnesses: BTreeMap<String, WitnessRecord>,
    /// Immutable batch hashes, in publication order.
    pub batches: Vec<String>,
    /// The hash of the ordered batch list, or `None` before genesis.
    pub head: Option<String>,
}

impl UniverseState {
    /// The content identity of the sorted witness ids.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when serialization fails.
    pub fn hash(&self) -> Result<String, DiffError> {
        canonical_sha256(&self.witnesses.keys().collect::<Vec<_>>())
    }
}

/// One atomic publication point for both the gate and witness union.
#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryHead {
    gate: Option<GateHead>,
    batches: Vec<String>,
}

/// Holds the history's single writer lock for the whole transaction.
#[derive(Debug)]
pub struct HistoryLock {
    file: std::fs::File,
}

impl HistoryLock {
    /// Acquires the lock before reading any history state.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] if another writer holds the lock.
    pub fn acquire(root: &Path) -> Result<Self, DiffError> {
        std::fs::create_dir_all(root).map_err(|error| io(root, &error))?;
        let path = root.join(".lock");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| io(&path, &error))?;
        file.try_lock().map_err(|error| {
            DiffError::Identity(format!("history is locked ({}): {error}", path.display()))
        })?;
        Ok(Self { file })
    }
}

impl Drop for HistoryLock {
    fn drop(&mut self) {
        if let Err(err) = self.file.unlock() {
            tracing::error!(?err, "release history lock");
        }
    }
}

impl Universe {
    /// Opens the universe under `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let blobs = BlobStore::new(root.join("blobs"));
        Self { root, blobs }
    }

    /// Its immutable blob store.
    #[must_use]
    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    fn batch_path(&self, sha256: &str) -> PathBuf {
        self.root.join("batches").join(format!("{sha256}.jsonl"))
    }

    fn read_head(&self) -> Result<HistoryHead, DiffError> {
        let path = self.root.join("head.json");
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(HistoryHead::default())
            }
            Err(error) => Err(io(&path, &error)),
        }
    }

    /// The trusted gate published with this universe.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] if genesis has not been published.
    pub fn gate_head(&self) -> Result<GateHead, DiffError> {
        self.read_head()?
            .gate
            .filter(|head| head.format_version == FORMAT_VERSION)
            .ok_or_else(|| DiffError::Identity("history has no supported gate head".into()))
    }

    /// The witness ids of an immutable batch.
    ///
    /// # Errors
    ///
    /// [`DiffError`] if the batch is missing or changed.
    pub fn batch_ids(&self, sha256: &str) -> Result<BTreeSet<String>, DiffError> {
        Ok(self
            .read_batch(sha256)?
            .into_iter()
            .map(|record| record.id)
            .collect())
    }

    fn read_batch(&self, sha256: &str) -> Result<Vec<WitnessRecord>, DiffError> {
        let bytes = read(&self.batch_path(sha256))?;
        if sha256_hex(&bytes) != sha256 {
            return Err(DiffError::Identity(format!("batch {sha256} changed")));
        }
        parse_batch(&bytes)
    }

    /// Loads every published batch, verifying all records and blobs.
    ///
    /// # Errors
    ///
    /// [`DiffError`] for missing, changed or inconsistent inputs.
    pub fn load(&self) -> Result<UniverseState, DiffError> {
        let mut state = UniverseState::default();
        for batch in self.read_head()?.batches {
            for witness in self.read_batch(&batch)? {
                witness.verify(&self.blobs)?;
                merge(&mut state.witnesses, witness)?;
            }
            state.batches.push(batch);
        }
        if !state.batches.is_empty() {
            state.head = Some(canonical_sha256(&state.batches)?);
        }
        Ok(state)
    }

    /// Verifies a receipt's union and batch-list identities against published batches.
    ///
    /// # Errors
    ///
    /// [`DiffError`] for a dropped batch or a substituted union identity.
    pub fn verify_snapshot(&self, receipt: &Receipt) -> Result<(), DiffError> {
        let mut ids = BTreeSet::new();
        for batch in &receipt.witness_batch_hashes {
            ids.extend(self.batch_ids(batch)?);
        }
        if canonical_sha256(&ids)? != receipt.universe_hash
            || Some(canonical_sha256(&receipt.witness_batch_hashes)?)
                != receipt.universe_journal_head_sha256
        {
            return Err(DiffError::Identity(
                "receipt universe identity does not match its batches".into(),
            ));
        }
        Ok(())
    }

    /// Stages one batch in `state`; it remains invisible until [`Self::publish`].
    ///
    /// # Errors
    ///
    /// [`DiffError`] for invalid records or filesystem errors.
    pub fn append(
        &self,
        state: &mut UniverseState,
        mut witnesses: Vec<WitnessRecord>,
    ) -> Result<Option<String>, DiffError> {
        if witnesses.is_empty() {
            return Ok(None);
        }
        witnesses.sort_by(|left, right| left.id.cmp(&right.id));
        let mut bytes = Vec::new();
        for witness in &witnesses {
            witness.verify(&self.blobs)?;
            serde_json::to_writer(&mut bytes, witness)
                .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
            bytes.push(b'\n');
        }
        if witnesses
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if left.id == right.id))
        {
            return Err(DiffError::Identity(
                "duplicate witness in a staged batch".into(),
            ));
        }
        let hash = sha256_hex(&bytes);
        let path = self.batch_path(&hash);
        if path.exists() {
            if read(&path)? != bytes {
                return Err(DiffError::Identity(format!("batch {hash} changed")));
            }
        } else {
            write_new(&path, &bytes)?;
        }
        for witness in witnesses {
            merge(&mut state.witnesses, witness)?;
        }
        state.batches.push(hash.clone());
        state.head = Some(canonical_sha256(&state.batches)?);
        Ok(Some(hash))
    }

    /// Atomically publishes a complete transaction's union and optional new gate.
    ///
    /// # Errors
    ///
    /// [`DiffError`] when the head cannot be written.
    pub fn publish(
        &self,
        _lock: &HistoryLock,
        state: &UniverseState,
        gate: Option<GateHead>,
    ) -> Result<(), DiffError> {
        let head = HistoryHead {
            gate: gate.or(self.read_head()?.gate),
            batches: state.batches.clone(),
        };
        let bytes = serde_json::to_vec(&head)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.root).map_err(|error| io(&self.root, &error))?;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| io(&self.root, &error))?;
        temporary
            .persist(self.root.join("head.json"))
            .map_err(|error| io(&self.root, &error.error))?;
        Ok(())
    }
}

fn parse_batch(batch: &[u8]) -> Result<Vec<WitnessRecord>, DiffError> {
    let mut records = Vec::new();
    let mut previous: Option<String> = None;
    for line in batch.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let record: WitnessRecord = serde_json::from_slice(line)
            .map_err(|error| DiffError::Identity(format!("witness record: {error}")))?;
        if previous
            .as_ref()
            .is_some_and(|previous| *previous >= record.id)
        {
            return Err(DiffError::Identity(
                "batch is not sorted by unique witness id".to_string(),
            ));
        }
        previous = Some(record.id.clone());
        records.push(record);
    }
    Ok(records)
}

/// Adds `witness`, merging origins into an existing record of the same id,
/// which must otherwise be identical.
fn merge(
    witnesses: &mut BTreeMap<String, WitnessRecord>,
    witness: WitnessRecord,
) -> Result<(), DiffError> {
    match witnesses.get_mut(&witness.id) {
        None => {
            witnesses.insert(witness.id.clone(), witness);
        }
        Some(existing) => {
            let mut left = existing.clone();
            let mut right = witness.clone();
            left.origins.clear();
            right.origins.clear();
            left.name.clear();
            right.name.clear();
            if left != right {
                return Err(DiffError::Identity(format!(
                    "witness {} recorded twice with different contents",
                    witness.id
                )));
            }
            let origins = existing
                .origins
                .iter()
                .chain(&witness.origins)
                .cloned()
                .collect::<BTreeSet<_>>();
            existing.origins = origins.into_iter().collect();
        }
    }
    Ok(())
}

/// What a receipt records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    /// The pinned main revision compared with itself, ordinal 0.
    Genesis,
    /// One construction checkpoint compared with main.
    Checkpoint,
    /// History recomputed over an expanded universe.
    Amendment,
    /// History replayed under new identities.
    Migration,
    /// The final landing comparison.
    Landing,
}

/// One checkpoint the reviewed plan declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedCheckpoint {
    /// Its construction ordinal, 1 through 10.
    pub ordinal: u32,
    /// Its subject.
    pub subject: String,
    /// Whether it must shrink the regression set or earn closures
    /// (`semantic`), or may keep it unchanged (`representation`).
    pub kind: CheckpointKind,
}

/// Whether a checkpoint must make semantic progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    /// Must shrink the regression set or add verified closures.
    Semantic,
    /// May keep the regression set unchanged.
    Representation,
}

/// The reviewed history plan, version 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPlan {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// The history's identity.
    pub history_id: String,
    /// The main revision's commit.
    pub main_commit: String,
    /// The planned checkpoints, ordinals 1 through 10 without gaps.
    pub checkpoints: Vec<PlannedCheckpoint>,
    /// The only ordinal that may establish the reviewed seed.
    pub seed_ordinal: u32,
    /// The reviewer whose identity seed and amendment reviews must carry.
    pub reviewer: String,
    /// SHA-256 of the reviewed context roster every capture must retain.
    pub context_roster_sha256: String,
}

impl HistoryPlan {
    /// Reads and checks a plan.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] when it is malformed, of another version, or its
    /// ordinals are not exactly 1 through 10 with the seed at 2.
    pub fn read(path: &Path) -> Result<Self, DiffError> {
        let plan: Self = serde_json::from_slice(&read(path)?)
            .map_err(|error| DiffError::Request(format!("plan {}: {error}", path.display())))?;
        if plan.format_version != FORMAT_VERSION
            || !plan
                .checkpoints
                .iter()
                .map(|checkpoint| checkpoint.ordinal)
                .eq(1..=10)
            || plan.seed_ordinal != 2
            || plan.history_id.is_empty()
            || plan.reviewer.is_empty()
        {
            return Err(DiffError::Request(format!(
                "plan {} is not a version-1 plan of checkpoints 1-10 seeded at 2",
                path.display()
            )));
        }
        Ok(plan)
    }

    /// The planned checkpoint `ordinal`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the plan has none.
    pub fn checkpoint(&self, ordinal: u32) -> Result<&PlannedCheckpoint, DiffError> {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.ordinal == ordinal)
            .ok_or_else(|| DiffError::Identity(format!("the plan has no checkpoint {ordinal}")))
    }
}

/// A sealed receipt, version 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// This tool's version and the SHA-256 of its executable.
    pub tool_version_and_hash: String,
    /// The run that produced the receipt.
    pub run_id: String,
    /// The history's identity.
    pub history_id: String,
    /// What the receipt records.
    pub kind: ReceiptKind,
    /// The last accepted construction ordinal; an amendment keeps its predecessor's.
    pub checkpoint_ordinal: u32,
    /// The compared checkpoint's commit.
    pub checkpoint_commit: Option<String>,
    /// SHA-256 of the compared capture's manifest.
    pub capture_hash: Option<String>,
    /// SHA-256 of main's capture manifest.
    pub baseline_capture_hash: String,
    /// SHA-256 of the screened suite: registry keys and recipe inputs.
    pub suite_hash: String,
    /// SHA-256 of the scored control registry, `None` before controls are captured.
    pub controls_hash: Option<String>,
    /// SHA-256 of the context roster's exact bytes, `None` without one.
    pub context_roster_hash: String,
    /// The provider bundle's input digest.
    pub provider_hash: String,
    /// The pinned Helm engine's identity.
    pub helm_identity: String,
    /// SHA-256 of the predecessor receipt; `None` only for genesis.
    pub predecessor_receipt_sha256: Option<String>,
    /// SHA-256 of the reviewed seed ledger, once seeded.
    pub seed_ledger_sha256: Option<String>,
    /// The universe identity the predecessor recorded.
    pub universe_parent_hash: Option<String>,
    /// [`UniverseState::hash`] the comparison used.
    pub universe_hash: String,
    /// The ordered batch-list hash the comparison used.
    pub universe_journal_head_sha256: Option<String>,
    /// Every published batch, in order.
    pub witness_batch_hashes: Vec<String>,
    /// SHA-256 of each compared dump's manifest: main's, then the candidate's.
    pub artifact_manifest_hashes: Vec<String>,
    /// SHA-256 of the coverage report.
    pub coverage_hash: String,
    /// SHA-256 of the proof index.
    pub proof_index_hash: String,
    /// SHA-256 of the regression set.
    pub regression_set_hash: String,
    /// SHA-256 of the earned closure set.
    pub earned_closure_set_hash: String,
    /// SHA-256 of the full outcome table.
    pub full_outcome_table_hash: String,
    /// SHA-256 of the control runner manifest, `None` before controls are captured.
    pub control_runner_manifest_hash: Option<String>,
    /// SHA-256 of the observation results, `None` before controls are captured.
    pub observation_results_hash: Option<String>,
    /// [`super::build::dir_sha256`] of the admission suite.
    pub admission_suite_hash: String,
    /// SHA-256 of the history's sealed plan.
    pub plan_sha256: String,
    /// SHA-256 of the comparison's `payload.json`, the index of every table
    /// and evidence file it published.
    pub payload_index_sha256: String,
    /// `construction` or `landing`.
    pub gate_mode: String,
    /// Always `NON-LANDABLE` for a construction receipt.
    pub landability: String,
    /// The comparison's exit code.
    pub exit_code: u8,
    /// SHA-256 of this receipt's canonical JSON without this field.
    pub seal_sha256: String,
}

impl Receipt {
    /// The receipt's hash over every other field.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be serialized.
    pub fn compute_sha256(&self) -> Result<String, DiffError> {
        let mut value = serde_json::to_value(self)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        if let Some(members) = value.as_object_mut() {
            members.remove("seal_sha256");
        }
        Ok(sha256_hex(&canonical_json(&value)))
    }

    /// Seals the receipt with its own hash.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be serialized.
    pub fn seal(mut self) -> Result<Self, DiffError> {
        self.seal_sha256 = self.compute_sha256()?;
        Ok(self)
    }

    /// Reads a receipt and checks its version and seal.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when it is malformed, of another version or unsealed.
    pub fn read(path: &Path) -> Result<Self, DiffError> {
        let receipt: Self = serde_json::from_slice(&read(path)?)
            .map_err(|error| DiffError::Identity(format!("receipt {}: {error}", path.display())))?;
        if receipt.format_version != FORMAT_VERSION {
            return Err(DiffError::Identity(format!(
                "receipt {} has format version {}",
                path.display(),
                receipt.format_version
            )));
        }
        if receipt.compute_sha256()? != receipt.seal_sha256 {
            return Err(DiffError::Identity(format!(
                "receipt {} does not match its seal",
                path.display()
            )));
        }
        Ok(receipt)
    }

    /// Writes the sealed receipt to the new file `path`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when `path` exists or the receipt is unsealed.
    pub fn write(&self, path: &Path) -> Result<(), DiffError> {
        if self.compute_sha256()? != self.seal_sha256 {
            return Err(DiffError::Identity("receipt is not sealed".to_string()));
        }
        let value = serde_json::to_value(self)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        let mut bytes = serde_json::to_vec_pretty(&value)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        bytes.push(b'\n');
        write_new(path, &bytes)
    }
}

/// The trusted gate head the orchestrator advances once per accepted successor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateHead {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// The trusted receipt's hash.
    pub seal_sha256: String,
    /// Its path inside the history.
    pub receipt: String,
}
