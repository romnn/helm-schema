//! The persistent witness universe and the hash-chained receipts.
//!
//! Every witness ever discovered is kept as an immutable, version-1 record
//! whose identity excludes everything a candidate can move: its schema
//! digest, a JSON pointer, an ordinal or a checkpoint number. Records arrive
//! in append-only batches. Each batch is journaled under an exclusive lock,
//! and the journal's head hash chains every batch to its predecessor, so a
//! dropped batch, a rolled-back head or a substituted record fails
//! verification. Large shared bytes live in a content-addressed blob store
//! and are rehashed on every read.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::DiffError;
use crate::source_digest::sha256_hex;

/// The only witness, journal, plan and receipt format this tool reads.
pub const FORMAT_VERSION: u32 = 1;

/// The observation every schema witness answers.
pub const SCHEMA_ACCEPTANCE: &str = "schema-acceptance";

/// `value` as canonical JSON: object members sorted by key, no whitespace.
#[must_use]
pub fn canonical_json(value: &Value) -> Vec<u8> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(members) => {
                let mut keys = members.keys().collect::<Vec<_>>();
                keys.sort();
                let mut sorted_members = Map::new();
                for key in keys {
                    if let Some(member) = members.get(key) {
                        sorted_members.insert(key.clone(), sorted(member));
                    }
                }
                Value::Object(sorted_members)
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
        }
    }
    // Serializing a `Value` cannot fail.
    serde_json::to_vec(&sorted(value)).unwrap_or_default()
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

/// Who added a batch to the universe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// The run that discovered or added the batch.
    pub run_id: String,
    /// The capture whose comparison discovered it, if any.
    pub capture_hash: Option<String>,
    /// Why: `genesis`, `discovery`, `amendment` or `migration`.
    pub reason: String,
}

/// One journal record: a batch chained to its predecessor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRecord {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// Zero-based position in the journal.
    pub sequence: u64,
    /// SHA-256 of the previous record, `None` for the first.
    pub previous_sha256: Option<String>,
    /// SHA-256 of the batch file.
    pub batch_sha256: String,
    /// Witnesses in the batch.
    pub witnesses: usize,
    /// Who added it.
    pub attempt: Attempt,
}

/// The append-only witness universe of one history.
#[derive(Debug)]
pub struct Universe {
    root: PathBuf,
    blobs: BlobStore,
}

/// A loaded universe: every witness, by id, with its journal head.
#[derive(Debug, Default)]
pub struct UniverseState {
    /// Every witness ever journaled, by id, origins merged.
    pub witnesses: BTreeMap<String, WitnessRecord>,
    /// Every journal record, in order.
    pub journal: Vec<JournalRecord>,
    /// SHA-256 of the last journal record, `None` when empty.
    pub head: Option<String>,
}

impl UniverseState {
    /// SHA-256 of the sorted witness ids: the universe's content identity.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when the ids cannot be serialized.
    pub fn hash(&self) -> Result<String, DiffError> {
        canonical_sha256(&self.witnesses.keys().collect::<Vec<_>>())
    }

    /// The batch hashes, in journal order.
    #[must_use]
    pub fn batch_hashes(&self) -> Vec<String> {
        self.journal
            .iter()
            .map(|record| record.batch_sha256.clone())
            .collect()
    }
}

/// Holds the history's exclusive lock until dropped.
#[derive(Debug)]
pub struct HistoryLock {
    path: PathBuf,
}

impl Drop for HistoryLock {
    fn drop(&mut self) {
        // A stale lock blocks the next writer, which is the safe failure.
        let _removed = std::fs::remove_file(&self.path);
    }
}

impl Universe {
    /// The universe under `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let blobs = BlobStore::new(root.join("blobs"));
        Self { root, blobs }
    }

    /// The universe's blob store.
    #[must_use]
    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    /// Takes the exclusive lock.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when another writer holds it.
    pub fn lock(&self) -> Result<HistoryLock, DiffError> {
        std::fs::create_dir_all(&self.root).map_err(|error| io(&self.root, &error))?;
        let path = self.root.join(".lock");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                DiffError::Identity(format!("history is locked ({}): {error}", path.display()))
            })?;
        Ok(HistoryLock { path })
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join("journal.jsonl")
    }

    fn head_path(&self) -> PathBuf {
        self.root.join("head")
    }

    fn batch_path(&self, sha256: &str) -> PathBuf {
        self.root.join("batches").join(format!("{sha256}.jsonl"))
    }

    /// The witness ids of the journaled batch `sha256`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the batch is missing or changed.
    pub fn batch_ids(&self, sha256: &str) -> Result<BTreeSet<String>, DiffError> {
        let batch = read(&self.batch_path(sha256))?;
        if sha256_hex(&batch) != sha256 {
            return Err(DiffError::Identity(format!("batch {sha256} changed")));
        }
        Ok(parse_batch(&batch)?
            .into_iter()
            .map(|record| record.id)
            .collect())
    }

    /// Loads and verifies the whole universe: the chain, every batch, every
    /// record and blob.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] on any broken link, hash or record.
    pub fn load(&self) -> Result<UniverseState, DiffError> {
        let mut state = UniverseState::default();
        let journal = match std::fs::read(self.journal_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if self.head_path().exists() {
                    return Err(DiffError::Identity("universe head without journal".into()));
                }
                return Ok(state);
            }
            Err(error) => return Err(io(&self.journal_path(), &error)),
        };
        for (sequence, line) in journal.split(|byte| *byte == b'\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let record: JournalRecord = serde_json::from_slice(line).map_err(|error| {
                DiffError::Identity(format!("journal record {sequence}: {error}"))
            })?;
            if record.format_version != FORMAT_VERSION
                || usize::try_from(record.sequence).ok() != Some(state.journal.len())
                || record.previous_sha256 != state.head
            {
                return Err(DiffError::Identity(format!(
                    "journal record {sequence} breaks the chain"
                )));
            }
            let batch = read(&self.batch_path(&record.batch_sha256))?;
            if sha256_hex(&batch) != record.batch_sha256 {
                return Err(DiffError::Identity(format!(
                    "batch {} changed",
                    record.batch_sha256
                )));
            }
            let records = parse_batch(&batch)?;
            if records.len() != record.witnesses {
                return Err(DiffError::Identity(format!(
                    "batch {} holds {} witnesses, journaled {}",
                    record.batch_sha256,
                    records.len(),
                    record.witnesses
                )));
            }
            for witness in records {
                witness.verify(&self.blobs)?;
                merge(&mut state.witnesses, witness)?;
            }
            state.head = Some(canonical_sha256(&record)?);
            state.journal.push(record);
        }
        let head = std::fs::read_to_string(self.head_path())
            .map_err(|error| io(&self.head_path(), &error))?;
        if Some(head.trim()) != state.head.as_deref() {
            return Err(DiffError::Identity(
                "universe head does not match its journal".to_string(),
            ));
        }
        Ok(state)
    }

    /// Journals `witnesses` as one batch after the current head and advances
    /// the head; the caller holds the lock. An empty batch journals nothing.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the universe's head moved since `state`
    /// was loaded; [`DiffError::Infrastructure`] when a file cannot be written.
    pub fn append(
        &self,
        _lock: &HistoryLock,
        state: &mut UniverseState,
        witnesses: Vec<WitnessRecord>,
        attempt: Attempt,
    ) -> Result<Option<String>, DiffError> {
        let current = match std::fs::read_to_string(self.head_path()) {
            Ok(head) => Some(head.trim().to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io(&self.head_path(), &error)),
        };
        if current != state.head {
            return Err(DiffError::Identity(
                "the universe head moved since it was loaded".to_string(),
            ));
        }
        if witnesses.is_empty() {
            return Ok(None);
        }
        let mut sorted = witnesses;
        sorted.sort_by(|left, right| left.id.cmp(&right.id));
        let mut batch = Vec::new();
        for witness in &sorted {
            let value = serde_json::to_value(witness)
                .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
            batch.extend(canonical_json(&value));
            batch.push(b'\n');
        }
        let batch_sha256 = sha256_hex(&batch);
        let path = self.batch_path(&batch_sha256);
        if path.exists() {
            if read(&path)? != batch {
                return Err(DiffError::Identity(format!("batch {batch_sha256} differs")));
            }
        } else {
            write_new(&path, &batch)?;
        }
        let record = JournalRecord {
            format_version: FORMAT_VERSION,
            sequence: u64::try_from(state.journal.len()).unwrap_or(u64::MAX),
            previous_sha256: state.head.clone(),
            batch_sha256: batch_sha256.clone(),
            witnesses: sorted.len(),
            attempt,
        };
        let record_value = serde_json::to_value(&record)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        let mut line = canonical_json(&record_value);
        line.push(b'\n');
        let mut journal = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path())
            .map_err(|error| io(&self.journal_path(), &error))?;
        journal
            .write_all(&line)
            .and_then(|()| journal.sync_all())
            .map_err(|error| io(&self.journal_path(), &error))?;
        let head = canonical_sha256(&record)?;
        let next_head = self.root.join(format!("head.{}", record.sequence));
        write_new(&next_head, head.as_bytes())?;
        std::fs::rename(&next_head, self.head_path())
            .map_err(|error| io(&self.head_path(), &error))?;
        for witness in sorted {
            merge(&mut state.witnesses, witness)?;
        }
        state.head = Some(head);
        state.journal.push(record);
        Ok(Some(batch_sha256))
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
        let ordinals = plan
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.ordinal)
            .collect::<Vec<_>>();
        if plan.format_version != FORMAT_VERSION
            || ordinals != (1..=10).collect::<Vec<_>>()
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
    pub context_roster_hash: Option<String>,
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
    /// The universe journal head the comparison used.
    pub universe_journal_head_sha256: Option<String>,
    /// Every journaled batch, in order.
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
