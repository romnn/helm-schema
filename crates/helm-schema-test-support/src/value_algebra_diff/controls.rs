//! Version-1 non-schema observation controls: the data registry, the
//! same-revision runner protocol, response validation and scoring.
//!
//! A control is data, never a script: a frozen chart tree, values and
//! coalesced document in a hash-verified blob store, a closed query
//! (`output_v1`, `termination_v1` or `alias_v1`) and a pinned Helm grounding.
//! The runner is the revision's own IR library test executable; its single
//! ignored entry point [`RUNNER_ENTRY`] reads a request from
//! [`REQUEST_ENV`] and writes one response to [`RESPONSE_ENV`]. The runner
//! never sees the grounding, so it cannot answer from the oracle. Scoring is
//! separate from transport: [`score`] compares an exact answer with a
//! complete grounding, and [`classify_observation`] turns a main and a
//! candidate answer into a comparison [`Class`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::DiffError;
use super::compare::Class;
use super::history::{WitnessIdentity, WitnessMode, canonical_json, canonical_sha256, write_new};
use crate::manifest::BuildProvenance;
use crate::source_digest::sha256_hex;

/// The protocol, registry and response format version.
pub const PROTOCOL_VERSION: u32 = 1;
/// The runner's libtest entry point.
pub const RUNNER_ENTRY: &str = "tests::value_algebra_controls::protocol_v1";
/// The environment variable naming the absolute request path.
pub const REQUEST_ENV: &str = "HELM_SCHEMA_CONTROL_REQUEST";
/// The environment variable naming the absolute, not yet existing response path.
pub const RESPONSE_ENV: &str = "HELM_SCHEMA_CONTROL_RESPONSE";
/// How long one runner invocation may take.
pub const RUNNER_TIMEOUT: Duration = Duration::from_secs(120);
/// The query discriminants version 1 defines.
pub const QUERY_KINDS: &[&str] = &["alias_v1", "output_v1", "termination_v1"];

/// Whether a case is scored or only retains Helm evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// A candidate observation is evaluated and compared with the grounding.
    Scored,
    /// Helm evidence only; never a candidate query, coverage or progress.
    HelmGroundingOnly,
}

/// The engine mode a control executes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    /// `helm template`.
    Render,
    /// `helm lint`.
    Lint,
    /// Rendering with Helm's strict engine.
    StrictRender,
}

/// The analysis cache mode of a control's session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    /// The default cache behavior.
    Default,
    /// Caches disabled.
    Disabled,
}

/// The release a control renders as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// The release name.
    pub name: String,
    /// The release namespace.
    pub namespace: String,
}

/// A control's frozen logical input; every digest names a registry blob.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlInput {
    /// The [`ChartTree`] blob.
    pub chart_tree_sha256: String,
    /// The original values-file bytes.
    pub original_values_sha256: String,
    /// The coalesced document's JSON, the same composition Helm sees.
    pub coalesced_document_sha256: String,
    /// The context (capabilities, API versions) description.
    pub context_sha256: String,
    /// The release identity.
    pub release: Release,
    /// The Kubernetes version rendered for.
    pub kubernetes_version: String,
    /// The engine mode.
    pub mode: EngineMode,
    /// The generation recipe description.
    pub recipe_sha256: String,
    /// The bound policy description.
    pub bound_policy_sha256: String,
    /// The cache mode.
    pub cache_mode: CacheMode,
}

impl ControlInput {
    /// The blob digests the input references directly.
    #[must_use]
    pub fn blobs(&self) -> [&str; 6] {
        [
            &self.chart_tree_sha256,
            &self.original_values_sha256,
            &self.coalesced_document_sha256,
            &self.context_sha256,
            &self.recipe_sha256,
            &self.bound_policy_sha256,
        ]
    }

    /// SHA-256 of the canonical logical input.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be serialized.
    pub fn sha256(&self) -> Result<String, DiffError> {
        canonical_sha256(self)
    }
}

/// What a termination query covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationScope {
    /// The whole render.
    Render,
}

/// The property an alias experiment tests; a label, never an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasRelation {
    /// A write through one reference is visible through another.
    WriteVisible,
    /// A snapshot is unaffected by a later write.
    SnapshotStable,
}

/// A version-1 query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// The YAML string at `pointer` of `document` in the executed `file`.
    OutputV1 {
        /// The chart-relative template file.
        file: String,
        /// The zero-based YAML document in that file's output.
        document: usize,
        /// The RFC 6901 pointer into the document.
        pointer: String,
    },
    /// The render's final execution exit.
    TerminationV1 {
        /// What the query covers.
        scope: TerminationScope,
    },
    /// The Boolean the program emits after its own alias experiment.
    AliasV1 {
        /// The chart-relative template file.
        file: String,
        /// The zero-based YAML document in that file's output.
        document: usize,
        /// The RFC 6901 pointer into the document.
        pointer: String,
        /// The property tested.
        relation: AliasRelation,
    },
}

impl Query {
    /// The query's discriminant.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::OutputV1 { .. } => "output_v1",
            Self::TerminationV1 { .. } => "termination_v1",
            Self::AliasV1 { .. } => "alias_v1",
        }
    }

    /// SHA-256 of the canonical query.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be serialized.
    pub fn sha256(&self) -> Result<String, DiffError> {
        canonical_sha256(self)
    }
}

/// Where a render aborted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortPhase {
    /// Template parsing.
    TemplateParse,
    /// Template evaluation.
    TemplateEval,
    /// Parsing the rendered YAML.
    RenderYaml,
}

/// The value selected by an output or alias query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selection<T> {
    /// The selected value: base64 bytes for output, a Boolean for alias.
    Value {
        /// The value.
        value: T,
    },
    /// The file, document or key is absent on normal completion.
    Missing,
    /// The selected YAML value has another type.
    WrongType {
        /// Its canonical JSON value.
        value: Value,
    },
    /// The selected document is not YAML.
    InvalidYaml,
    /// The render aborted before emitting it.
    NotEmitted {
        /// Where it aborted.
        phase: AbortPhase,
    },
}

/// A render's final exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Termination {
    /// The render completed with output.
    Normal,
    /// The render aborted.
    Abort {
        /// Where.
        phase: AbortPhase,
    },
}

/// One feasible observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observation {
    /// An `output_v1` answer; the value is the string's UTF-8 bytes in base64.
    Output {
        /// The selection.
        selection: Selection<String>,
    },
    /// A `termination_v1` answer.
    Termination {
        /// The exit.
        #[serde(flatten)]
        outcome: Termination,
    },
    /// An `alias_v1` answer.
    Alias {
        /// The selection.
        selection: Selection<bool>,
    },
}

impl Observation {
    /// The query discriminant this observation answers.
    #[must_use]
    pub const fn query_kind(&self) -> &'static str {
        match self {
            Self::Output { .. } => "output_v1",
            Self::Termination { .. } => "termination_v1",
            Self::Alias { .. } => "alias_v1",
        }
    }

    /// The canonical JSON bytes that order and identify observations.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be serialized.
    pub fn canonical(&self) -> Result<Vec<u8>, DiffError> {
        let value = serde_json::to_value(self)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        Ok(canonical_json(&value))
    }
}

/// `alternatives` sorted and deduplicated by canonical JSON.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when one cannot be serialized.
pub fn normalize_alternatives(
    alternatives: Vec<Observation>,
) -> Result<Vec<Observation>, DiffError> {
    let mut keyed = BTreeMap::new();
    for alternative in alternatives {
        keyed.insert(alternative.canonical()?, alternative);
    }
    Ok(keyed.into_values().collect())
}

/// How completely a grounding covers the feasible outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundCoverage {
    /// Every feasible outcome is known; it can score correctness.
    Complete,
    /// Observed samples only; never an exact oracle.
    Sampled,
}

/// A pinned Helm grounding of a control.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grounding {
    /// The Helm invocation and evidence record.
    pub evidence_sha256: String,
    /// The coverage.
    pub coverage: GroundCoverage,
    /// The normalized observations.
    pub alternatives: Vec<Observation>,
}

/// One registry case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlCase {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// The unique case id.
    pub id: String,
    /// Scored or grounding-only.
    pub role: Role,
    /// The frozen input.
    pub input: ControlInput,
    /// The query; `None` for grounding-only cases.
    pub query: Option<Query>,
    /// The grounding-only case's descriptive claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    /// The Helm grounding.
    pub grounding: Grounding,
}

impl ControlCase {
    /// The scored query, if this case is scored.
    #[must_use]
    pub const fn scored_query(&self) -> Option<&Query> {
        match (self.role, &self.query) {
            (Role::Scored, Some(query)) => Some(query),
            (Role::Scored | Role::HelmGroundingOnly, _) => None,
        }
    }

    /// The observation witness's identity: the input and the query,
    /// nothing a candidate can move.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] for a grounding-only case.
    pub fn identity(&self) -> Result<WitnessIdentity, DiffError> {
        let query = self.scored_query().ok_or_else(|| {
            DiffError::Request(format!("{}: a grounding-only case is no witness", self.id))
        })?;
        Ok(WitnessIdentity {
            chart: OBSERVATION_CHART.to_string(),
            chart_input_sha256: self.input.chart_tree_sha256.clone(),
            recipe_sha256: self.input.recipe_sha256.clone(),
            values_sha256: self.input.original_values_sha256.clone(),
            mode: WitnessMode::Observation,
            kubernetes_version: self.input.kubernetes_version.clone(),
            context_sha256: self.input.context_sha256.clone(),
            observation: format!(
                "observation_v1 input={} query={}",
                self.input.sha256()?,
                query.sha256()?
            ),
        })
    }

    /// The observation witness id: [`ControlCase::identity`]'s id.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] for a grounding-only case.
    pub fn witness_id(&self) -> Result<String, DiffError> {
        self.identity()?.id()
    }

    /// The pinned Helm evidence the grounding names, bound to this case's
    /// input and, for a scored case, reproducing its grounding.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when the evidence is missing, belongs to
    /// another input or invocation, or derives another answer;
    /// [`DiffError::Request`] when its diagnostic is unrecognized.
    pub fn check_grounding(&self, blobs: &ControlBlobs) -> Result<GroundingEvidence, DiffError> {
        let problem = |message: String| DiffError::Identity(format!("{}: {message}", self.id));
        let evidence: GroundingEvidence =
            serde_json::from_slice(&blobs.read(&self.grounding.evidence_sha256)?)
                .map_err(|error| problem(format!("grounding evidence: {error}")))?;
        let show_only = match self.scored_query() {
            Some(Query::OutputV1 { file, .. } | Query::AliasV1 { file, .. }) => Some(file.clone()),
            Some(Query::TerminationV1 { .. }) => None,
            None => evidence.show_only.clone(),
        };
        let has_values = !blobs.read(&self.input.original_values_sha256)?.is_empty();
        let expected = grounding_argv(&self.input, show_only.as_deref(), has_values);
        if evidence.format_version != PROTOCOL_VERSION
            || evidence.chart_tree_sha256 != self.input.chart_tree_sha256
            || evidence.original_values_sha256 != self.input.original_values_sha256
            || evidence.release != self.input.release
            || evidence.kubernetes_version != self.input.kubernetes_version
            || evidence.show_only != show_only
            || evidence.argv != expected
            || self.input.mode != EngineMode::Render
        {
            return Err(problem(
                "the grounding evidence is not the pinned render of this input".into(),
            ));
        }
        let stdout = blobs.read(&evidence.stdout_sha256)?;
        let stderr = blobs.read(&evidence.stderr_sha256)?;
        if let Some(query) = self.scored_query() {
            let derived = derive_observation(query, evidence.exit_code, &stdout, &stderr)
                .map_err(|message| DiffError::Request(format!("{}: {message}", self.id)))?;
            let reproduced = match self.grounding.coverage {
                GroundCoverage::Complete => self.grounding.alternatives == vec![derived],
                GroundCoverage::Sampled => self.grounding.alternatives.contains(&derived),
            };
            if !reproduced {
                return Err(problem(
                    "the grounding evidence derives another observation".into(),
                ));
            }
        }
        Ok(evidence)
    }

    /// Every blob the case needs: its input closure and its evidence.
    ///
    /// # Errors
    ///
    /// As [`ControlBlobs::input_closure`] and [`ControlCase::check_grounding`].
    pub fn closure(&self, blobs: &ControlBlobs) -> Result<BTreeSet<String>, DiffError> {
        let mut closure = blobs.input_closure(&self.input)?;
        let evidence = self.check_grounding(blobs)?;
        closure.extend([
            self.grounding.evidence_sha256.clone(),
            evidence.stdout_sha256,
            evidence.stderr_sha256,
        ]);
        Ok(closure)
    }

    /// Checks the role shape and T0's supported scored domain.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] naming the first problem.
    pub fn validate(&self) -> Result<(), DiffError> {
        let problem = |message: &str| Err(DiffError::Request(format!("{}: {message}", self.id)));
        if self.format_version != PROTOCOL_VERSION {
            return problem("unknown case format_version");
        }
        let alternatives = normalize_alternatives(self.grounding.alternatives.clone())?;
        if alternatives != self.grounding.alternatives || alternatives.is_empty() {
            return problem("grounding alternatives must be nonempty, sorted and unique");
        }
        match (self.role, &self.query, &self.claim) {
            (Role::HelmGroundingOnly, None, Some(claim)) if !claim.trim().is_empty() => Ok(()),
            (Role::HelmGroundingOnly, _, _) => {
                problem("a grounding-only case has `query: null` and a descriptive claim")
            }
            (Role::Scored, None, _) | (Role::Scored, Some(_), Some(_)) => {
                problem("a scored case has a query and no claim")
            }
            (Role::Scored, Some(query), None) => {
                if self.input.mode != EngineMode::Render {
                    return problem("scored cases support only mode render in version 1");
                }
                let document = match query {
                    Query::OutputV1 { document, .. } | Query::AliasV1 { document, .. } => *document,
                    Query::TerminationV1 { .. } => 0,
                };
                if document != 0 {
                    return problem("scored output and alias queries select document 0");
                }
                if alternatives
                    .iter()
                    .any(|alternative| alternative.query_kind() != query.kind())
                {
                    return problem("grounding observations must answer the query");
                }
                Ok(())
            }
        }
    }
}

/// The universe chart label of observation witnesses; no chart label holds
/// a colon.
pub const OBSERVATION_CHART: &str = "control:observation_v1";

/// One pinned Helm invocation that grounds a control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundingEvidence {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// `helm version` of the binary that ran.
    pub helm_version: String,
    /// SHA-256 of that binary.
    pub helm_sha256: String,
    /// The arguments, `{chart}` and `{values}` standing for the staged chart
    /// and values file: exactly [`grounding_argv`].
    pub argv: Vec<String>,
    /// The chart tree rendered.
    pub chart_tree_sha256: String,
    /// The values file rendered.
    pub original_values_sha256: String,
    /// The release.
    pub release: Release,
    /// The Kubernetes version.
    pub kubernetes_version: String,
    /// The `--show-only` file, if any.
    pub show_only: Option<String>,
    /// The exit code.
    pub exit_code: i32,
    /// Blob of stdout.
    pub stdout_sha256: String,
    /// Blob of stderr.
    pub stderr_sha256: String,
}

/// The pinned render's arguments of `input`.
#[must_use]
pub fn grounding_argv(
    input: &ControlInput,
    show_only: Option<&str>,
    has_values: bool,
) -> Vec<String> {
    let mut argv = [
        "template",
        &input.release.name,
        "{chart}",
        "--namespace",
        &input.release.namespace,
        "--kube-version",
        &input.kubernetes_version,
    ]
    .map(str::to_string)
    .to_vec();
    if has_values {
        argv.extend(["--values".to_string(), "{values}".to_string()]);
    }
    if let Some(file) = show_only {
        argv.extend(["--show-only".to_string(), file.to_string()]);
    }
    argv
}

/// The abort phase pinned Helm's `stderr` names, or `None` for an
/// unrecognized diagnostic.
#[must_use]
pub fn abort_phase(stderr: &str) -> Option<AbortPhase> {
    if stderr.contains("execution error at (") {
        Some(AbortPhase::TemplateEval)
    } else if stderr.contains("parse error at (") {
        Some(AbortPhase::TemplateParse)
    } else if stderr.contains("YAML parse error on") {
        Some(AbortPhase::RenderYaml)
    } else {
        None
    }
}

/// The normalized observation of `query` in a pinned render's outcome.
///
/// # Errors
///
/// A message when the diagnostic is unrecognized or the output is not one
/// YAML document.
pub fn derive_observation(
    query: &Query,
    exit_code: i32,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Observation, String> {
    let phase = if exit_code == 0 {
        None
    } else {
        Some(
            abort_phase(&String::from_utf8_lossy(stderr))
                .ok_or_else(|| "unrecognized Helm diagnostic".to_string())?,
        )
    };
    let selected = |pointer: &str| -> Result<Option<Value>, String> {
        let text = std::str::from_utf8(stdout).map_err(|error| error.to_string())?;
        let mut documents = Vec::new();
        for document in serde_yaml::Deserializer::from_str(text) {
            let value =
                serde_yaml::Value::deserialize(document).map_err(|error| error.to_string())?;
            if !value.is_null() {
                documents.push(value);
            }
        }
        let [document] = documents.as_slice() else {
            return Err(format!("{} rendered documents, not one", documents.len()));
        };
        let json = serde_json::to_value(document).map_err(|error| error.to_string())?;
        Ok(json.pointer(pointer).cloned())
    };
    Ok(match (query, phase) {
        (Query::TerminationV1 { .. }, None) => Observation::Termination {
            outcome: Termination::Normal,
        },
        (Query::TerminationV1 { .. }, Some(phase)) => Observation::Termination {
            outcome: Termination::Abort { phase },
        },
        (Query::OutputV1 { .. }, Some(phase)) => Observation::Output {
            selection: Selection::NotEmitted { phase },
        },
        (Query::AliasV1 { .. }, Some(phase)) => Observation::Alias {
            selection: Selection::NotEmitted { phase },
        },
        (Query::OutputV1 { pointer, .. }, None) => Observation::Output {
            selection: match selected(pointer)? {
                None => Selection::Missing,
                Some(Value::String(text)) => output_value(text.as_bytes()),
                Some(other) => Selection::WrongType { value: other },
            },
        },
        (Query::AliasV1 { pointer, .. }, None) => Observation::Alias {
            selection: match selected(pointer)? {
                None => Selection::Missing,
                Some(Value::Bool(value)) => Selection::Value { value },
                Some(other) => Selection::WrongType { value: other },
            },
        },
    })
}

/// A chart tree blob: chart-relative file names to blob digests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartTree {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// Normalized relative names to blob digests.
    pub files: BTreeMap<String, String>,
}

impl ChartTree {
    /// Parses and checks a chart tree blob.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] for an unknown version or a name that is
    /// absolute, empty, non-normal or contains `..`.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiffError> {
        let tree: Self = serde_json::from_slice(bytes)
            .map_err(|error| DiffError::Request(format!("chart tree: {error}")))?;
        if tree.format_version != PROTOCOL_VERSION {
            return Err(DiffError::Request(
                "chart tree: unknown format_version".into(),
            ));
        }
        for name in tree.files.keys() {
            if !is_normal_relative(name) {
                return Err(DiffError::Request(format!(
                    "chart tree: {name:?} is not a normalized relative name"
                )));
            }
        }
        Ok(tree)
    }
}

/// Whether `name` is a normalized relative `/`-separated path: no empty,
/// `.` or `..` segment, no backslash and no leading or trailing slash.
#[must_use]
pub fn is_normal_relative(name: &str) -> bool {
    !name.contains('\\')
        && name
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// A content-addressed blob directory: `<root>/<sha256>`, or a history
/// blob store's `<root>/<first two>/<sha256>`.
#[derive(Clone, Debug)]
pub struct ControlBlobs {
    root: PathBuf,
    nested: bool,
}

impl ControlBlobs {
    /// The flat blob directory `root`.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self {
            root,
            nested: false,
        }
    }

    /// A history blob store's blobs under `root`.
    #[must_use]
    pub fn store(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            nested: true,
        }
    }

    fn path(&self, sha256: &str) -> PathBuf {
        match (self.nested, sha256.get(..2)) {
            (true, Some(prefix)) => self.root.join(prefix).join(sha256),
            (true, None) | (false, _) => self.root.join(sha256),
        }
    }

    /// The directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The bytes of blob `sha256`, rehashed.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when it is missing or its bytes do not hash
    /// to its name.
    pub fn read(&self, sha256: &str) -> Result<Vec<u8>, DiffError> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(DiffError::Identity(format!(
                "blob {sha256:?} is not a SHA-256"
            )));
        }
        let bytes = std::fs::read(self.path(sha256))
            .map_err(|error| DiffError::Identity(format!("blob {sha256}: {error}")))?;
        if sha256_hex(&bytes) != sha256 {
            return Err(DiffError::Identity(format!(
                "blob {sha256}: content mismatch"
            )));
        }
        Ok(bytes)
    }

    /// Every blob an input needs, its chart tree's files included.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] or [`DiffError::Request`] when a blob is
    /// missing, mismatched or the tree is malformed.
    pub fn input_closure(&self, input: &ControlInput) -> Result<BTreeSet<String>, DiffError> {
        let mut blobs = input
            .blobs()
            .iter()
            .map(|sha| (*sha).to_string())
            .collect::<BTreeSet<_>>();
        let tree = ChartTree::parse(&self.read(&input.chart_tree_sha256)?)?;
        blobs.extend(tree.files.into_values());
        for blob in &blobs {
            self.read(blob)?;
        }
        Ok(blobs)
    }

    /// The input's chart files and coalesced document.
    ///
    /// # Errors
    ///
    /// As [`ControlBlobs::input_closure`], and [`DiffError::Request`] when
    /// the coalesced document is not JSON.
    pub fn chart(&self, input: &ControlInput) -> Result<LoadedChart, DiffError> {
        let tree = ChartTree::parse(&self.read(&input.chart_tree_sha256)?)?;
        let mut files = BTreeMap::new();
        for (name, sha) in tree.files {
            files.insert(name, self.read(&sha)?);
        }
        let coalesced = serde_json::from_slice(&self.read(&input.coalesced_document_sha256)?)
            .map_err(|error| DiffError::Request(format!("coalesced document: {error}")))?;
        Ok(LoadedChart { files, coalesced })
    }
}

/// A control's chart files and coalesced document.
#[derive(Clone, Debug)]
pub struct LoadedChart {
    /// Chart-relative names to bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The coalesced values document.
    pub coalesced: Value,
}

/// One registry index row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseEntry {
    /// The case id.
    pub id: String,
    /// The case file, relative to the registry.
    pub file: String,
    /// SHA-256 of the case file.
    pub sha256: String,
}

/// A registry's `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlsManifest {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// The cases, sorted by id.
    pub cases: Vec<CaseEntry>,
    /// Every blob under `blobs/`, sorted.
    pub blobs: Vec<String>,
}

/// A verified control registry.
#[derive(Clone, Debug)]
pub struct ControlRegistry {
    /// The registry directory.
    pub root: PathBuf,
    /// SHA-256 of `manifest.json`, which binds every case and blob.
    pub sha256: String,
    /// The cases by id.
    pub cases: BTreeMap<String, ControlCase>,
}

impl ControlRegistry {
    /// Opens and verifies the registry at `root`: sorted unique cases whose
    /// bytes match the index, a complete and exact blob index, and every
    /// case's inputs present.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] for a malformed registry or case;
    /// [`DiffError::Identity`] for a hash mismatch or missing blob.
    pub fn open(root: &Path) -> Result<Self, DiffError> {
        let manifest_bytes = std::fs::read(root.join("manifest.json"))
            .map_err(|error| DiffError::Request(format!("{}: {error}", root.display())))?;
        let manifest: ControlsManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| DiffError::Request(format!("controls manifest: {error}")))?;
        if manifest.format_version != PROTOCOL_VERSION {
            return Err(DiffError::Request(
                "controls manifest: unknown format_version".into(),
            ));
        }
        let ids = manifest.cases.iter().map(|entry| entry.id.as_str());
        if !ids
            .clone()
            .zip(ids.skip(1))
            .all(|(left, right)| left < right)
        {
            return Err(DiffError::Request(
                "controls manifest: cases must be sorted and unique".into(),
            ));
        }
        let blob_names = manifest.blobs.iter().collect::<BTreeSet<_>>();
        if blob_names.len() != manifest.blobs.len()
            || !blob_names.iter().copied().eq(manifest.blobs.iter())
        {
            return Err(DiffError::Request(
                "controls manifest: blobs must be sorted and unique".into(),
            ));
        }
        let blobs = ControlBlobs::new(root.join("blobs"));
        let mut present = BTreeSet::new();
        for entry in std::fs::read_dir(blobs.root())
            .map_err(|error| DiffError::Request(format!("controls blobs: {error}")))?
        {
            let entry = entry.map_err(|error| DiffError::Infrastructure(error.to_string()))?;
            present.insert(entry.file_name().to_string_lossy().into_owned());
        }
        if !present.iter().eq(manifest.blobs.iter()) {
            return Err(DiffError::Identity(
                "controls blobs differ from the manifest's blob index".into(),
            ));
        }
        for blob in &manifest.blobs {
            blobs.read(blob)?;
        }
        let mut cases = BTreeMap::new();
        for entry in &manifest.cases {
            if !is_normal_relative(&entry.file) {
                return Err(DiffError::Request(format!(
                    "{}: bad case file name",
                    entry.id
                )));
            }
            let file = Path::new(&entry.file);
            let bytes = std::fs::read(root.join(file))
                .map_err(|error| DiffError::Request(format!("{}: {error}", entry.id)))?;
            if sha256_hex(&bytes) != entry.sha256 {
                return Err(DiffError::Identity(format!(
                    "{}: case bytes mismatch",
                    entry.id
                )));
            }
            let case: ControlCase = serde_json::from_slice(&bytes)
                .map_err(|error| DiffError::Request(format!("{}: {error}", entry.id)))?;
            if case.id != entry.id {
                return Err(DiffError::Request(format!(
                    "{}: id differs from the index",
                    entry.id
                )));
            }
            case.validate()?;
            let needed = case.closure(&blobs)?;
            if let Some(missing) = needed.iter().find(|blob| !blob_names.contains(blob)) {
                return Err(DiffError::Identity(format!(
                    "{}: blob {missing} is not indexed",
                    case.id
                )));
            }
            cases.insert(case.id.clone(), case);
        }
        Ok(Self {
            root: root.to_path_buf(),
            sha256: sha256_hex(&manifest_bytes),
            cases,
        })
    }

    /// The registry's blob store.
    #[must_use]
    pub fn blobs(&self) -> ControlBlobs {
        ControlBlobs::new(self.root.join("blobs"))
    }
}

/// A protocol request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// The operation.
    #[serde(flatten)]
    pub operation: Operation,
}

/// A request's operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    /// Evaluate one query on one input.
    Observe {
        /// The absolute private blob root.
        blob_root: PathBuf,
        /// The logical input.
        input: Box<ControlInput>,
        /// The query, untyped so that an unknown variant reports unsupported.
        query: Value,
    },
    /// Report the runner's identity and capabilities.
    Describe,
}

/// Why a runner could not decide exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    /// A value the answer depends on is unknown.
    ValueUnknown,
    /// Not every path is covered.
    PathCoverage,
    /// A bound truncated the execution.
    Truncated,
    /// The executed legacy result lacks the requested fact.
    RepresentationGap,
}

/// Whether the projection only read the executed result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionStatus {
    /// Read-only guard and term projection.
    ReadOnly,
}

/// What an abstract-unknown answer is based on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidence {
    /// The evaluator and its version.
    pub evaluator: String,
    /// SHA-256 of the executed result's stable dump.
    pub result_sha256: String,
    /// The gaps or bounds hit, sorted.
    pub gaps: Vec<String>,
    /// The projection status.
    pub projection: ProjectionStatus,
}

/// Why a runner cannot answer at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    /// The query variant is not implemented.
    QueryNotImplemented,
    /// The mode is not implemented.
    ModeNotImplemented,
    /// A private hook is unavailable.
    PrivateHookUnavailable,
}

/// A runner-reported infrastructure failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureCode {
    /// A malformed request or query.
    BadRequest,
    /// The input blobs do not match their digests.
    InputMismatch,
    /// The runner is not the expected build.
    RunnerMismatch,
    /// A protocol violation.
    ProtocolError,
    /// The runner panicked.
    Panic,
    /// The runner timed out.
    Timeout,
}

/// Exact answers are always complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactCoverage {
    /// Every feasible observation.
    Complete,
}

/// A runner's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlResult {
    /// The complete set of feasible observations.
    Exact {
        /// Nonempty, sorted and unique by canonical JSON.
        alternatives: Vec<Observation>,
        /// Always complete.
        coverage: ExactCoverage,
    },
    /// A measured precision gap.
    AbstractUnknown {
        /// Why.
        reason: UnknownReason,
        /// What it is based on.
        evidence: ExecutionEvidence,
    },
    /// An evaluator capability gap.
    Unsupported {
        /// The query discriminant asked.
        query_kind: String,
        /// The mode asked.
        mode: EngineMode,
        /// Why.
        reason: UnsupportedReason,
    },
    /// The runner could not execute the request.
    InfrastructureFailure {
        /// What failed.
        code: InfrastructureCode,
        /// A diagnostic.
        diagnostic: String,
    },
}

/// A version-1 observe response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResponseV1 {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// SHA-256 of the request bytes answered.
    pub request_sha256: String,
    /// SHA-256 of the runner executable.
    pub runner_build_id: String,
    /// [`ControlInput::sha256`].
    pub input_sha256: String,
    /// SHA-256 of the canonical query.
    pub query_sha256: String,
    /// The answer.
    pub result: ControlResult,
}

/// A version-1 describe response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescribeResponse {
    /// [`PROTOCOL_VERSION`].
    pub format_version: u32,
    /// SHA-256 of the request bytes answered.
    pub request_sha256: String,
    /// SHA-256 of the runner executable.
    pub runner_build_id: String,
    /// The supported query discriminants.
    pub queries: Vec<String>,
    /// The supported modes.
    pub modes: Vec<EngineMode>,
    /// The supported cache modes.
    pub cache_modes: Vec<CacheMode>,
    /// The generation source the runner was compiled from.
    pub build: BuildProvenance,
}

/// What a runner's describe answer advertises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// The supported query discriminants.
    pub queries: Vec<String>,
    /// The supported modes.
    pub modes: Vec<EngineMode>,
    /// The supported cache modes.
    pub cache_modes: Vec<CacheMode>,
}

/// The runner side: answers the request named by [`REQUEST_ENV`] into
/// [`RESPONSE_ENV`] as build `runner_build_id` (normally
/// [`executable_sha256`]) with `observe`, which sees only the verified
/// blobs, the input and the typed query.
///
/// An unknown query variant reports unsupported, a malformed query is a bad
/// request and mismatched blobs are an input mismatch; `observe` never runs
/// for them.
///
/// # Errors
///
/// [`DiffError::Request`] for a missing environment or an unknown protocol
/// version; [`DiffError::Infrastructure`] when the response cannot be
/// written. The caller turns an error into a failing test, which the tool
/// reads as an infrastructure failure.
pub fn serve(
    runner_build_id: String,
    capabilities: &Capabilities,
    observe: impl FnOnce(&ControlBlobs, &ControlInput, &Query) -> ControlResult,
) -> Result<(), DiffError> {
    let path = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| DiffError::Request(format!("{name} must name an absolute path")))
    };
    let (request_path, response_path) = (path(REQUEST_ENV)?, path(RESPONSE_ENV)?);
    let bytes = std::fs::read(&request_path)
        .map_err(|error| DiffError::Request(format!("{}: {error}", request_path.display())))?;
    let request_sha256 = sha256_hex(&bytes);
    let request: ControlRequest = serde_json::from_slice(&bytes)
        .map_err(|error| DiffError::Request(format!("request: {error}")))?;
    if request.format_version != PROTOCOL_VERSION {
        return Err(DiffError::Request(format!(
            "unknown protocol version {}",
            request.format_version
        )));
    }
    let response = match request.operation {
        Operation::Describe => serde_json::to_value(DescribeResponse {
            format_version: PROTOCOL_VERSION,
            request_sha256,
            runner_build_id,
            queries: capabilities.queries.clone(),
            modes: capabilities.modes.clone(),
            cache_modes: capabilities.cache_modes.clone(),
            build: BuildProvenance::compiled(),
        }),
        Operation::Observe {
            blob_root,
            input,
            query,
        } => {
            let result = answer(&blob_root, &input, &query, capabilities, observe);
            serde_json::to_value(ControlResponseV1 {
                format_version: PROTOCOL_VERSION,
                request_sha256,
                runner_build_id,
                input_sha256: input.sha256()?,
                query_sha256: sha256_hex(&canonical_json(&query)),
                result,
            })
        }
    }
    .map_err(|error| DiffError::Infrastructure(format!("response: {error}")))?;
    write_new(&response_path, &canonical_json(&response))
}

/// The runner's answer to one observe request.
fn answer(
    blob_root: &Path,
    input: &ControlInput,
    query: &Value,
    capabilities: &Capabilities,
    observe: impl FnOnce(&ControlBlobs, &ControlInput, &Query) -> ControlResult,
) -> ControlResult {
    let infrastructure =
        |code, diagnostic: String| ControlResult::InfrastructureFailure { code, diagnostic };
    let Some(kind) = query.get("kind").and_then(Value::as_str) else {
        return infrastructure(InfrastructureCode::BadRequest, "query has no kind".into());
    };
    let unsupported = |reason| ControlResult::Unsupported {
        query_kind: kind.to_string(),
        mode: input.mode,
        reason,
    };
    if !QUERY_KINDS.contains(&kind) || !capabilities.queries.iter().any(|known| known == kind) {
        return unsupported(UnsupportedReason::QueryNotImplemented);
    }
    if !capabilities.modes.contains(&input.mode)
        || !capabilities.cache_modes.contains(&input.cache_mode)
    {
        return unsupported(UnsupportedReason::ModeNotImplemented);
    }
    let query = match serde_json::from_value::<Query>(query.clone()) {
        Ok(query) => query,
        Err(error) => return infrastructure(InfrastructureCode::BadRequest, error.to_string()),
    };
    if !blob_root.is_absolute() {
        return infrastructure(InfrastructureCode::BadRequest, "relative blob root".into());
    }
    let blobs = ControlBlobs::new(blob_root.to_path_buf());
    if let Err(error) = blobs.input_closure(input) {
        return infrastructure(InfrastructureCode::InputMismatch, error.to_string());
    }
    observe(&blobs, input, &query)
}

/// SHA-256 of the running executable: the runner build id.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when it cannot be read.
pub fn executable_sha256() -> Result<String, DiffError> {
    let path = std::env::current_exe()
        .map_err(|error| DiffError::Infrastructure(format!("current executable: {error}")))?;
    file_sha256(&path)
}

fn file_sha256(path: &Path) -> Result<String, DiffError> {
    std::fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", path.display())))
}

/// The tool side: a runner executable and its build id.
#[derive(Clone, Debug)]
pub struct Runner {
    /// The executable.
    pub path: PathBuf,
    /// SHA-256 of its bytes.
    pub build_id: String,
}

/// One runner invocation's raw record.
#[derive(Clone, Debug)]
pub struct Invocation {
    /// SHA-256 of the request bytes.
    pub request_sha256: String,
    /// The response bytes.
    pub response: Vec<u8>,
}

impl Runner {
    /// The runner at `path`, hashed.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] when it is missing.
    pub fn new(path: &Path) -> Result<Self, DiffError> {
        let path = std::path::absolute(path)
            .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))?;
        let build_id = std::fs::read(&path)
            .map(|bytes| sha256_hex(&bytes))
            .map_err(|error| DiffError::Request(format!("runner {}: {error}", path.display())))?;
        Ok(Self { path, build_id })
    }

    /// Runs `request` in the new private directory `run_dir`, keeping the
    /// request, response, stdout, stderr and status there.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] for a nonzero exit, a timeout or a
    /// missing response; libtest's stdout is never read as a result.
    pub fn invoke(
        &self,
        run_dir: &Path,
        request: &ControlRequest,
        timeout: Duration,
    ) -> Result<Invocation, DiffError> {
        let infrastructure = |message: String| {
            DiffError::Infrastructure(format!("{}: {message}", run_dir.display()))
        };
        if run_dir.exists() {
            return Err(DiffError::Identity(format!(
                "{} already exists",
                run_dir.display()
            )));
        }
        std::fs::create_dir_all(run_dir).map_err(|error| infrastructure(error.to_string()))?;
        let run_dir =
            std::path::absolute(run_dir).map_err(|error| infrastructure(error.to_string()))?;
        let request_value = serde_json::to_value(request)
            .map_err(|error| infrastructure(format!("request: {error}")))?;
        let request_bytes = canonical_json(&request_value);
        let (request_path, response_path) =
            (run_dir.join("request.json"), run_dir.join("response.json"));
        write_new(&request_path, &request_bytes)?;
        let output = |name: &str| {
            std::fs::File::create(run_dir.join(name))
                .map_err(|error| infrastructure(error.to_string()))
        };
        let mut child = Command::new(&self.path)
            .args([
                "--exact",
                RUNNER_ENTRY,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(REQUEST_ENV, &request_path)
            .env(RESPONSE_ENV, &response_path)
            .current_dir(&run_dir)
            .stdin(Stdio::null())
            .stdout(output("stdout.log")?)
            .stderr(output("stderr.log")?)
            .spawn()
            .map_err(|error| infrastructure(format!("spawn {}: {error}", self.path.display())))?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let reaped = child.kill().and_then(|()| child.wait());
                    return Err(infrastructure(match reaped {
                        Ok(_) => format!("runner timed out after {timeout:?}"),
                        Err(error) => {
                            format!(
                                "runner timed out after {timeout:?} and was not reaped: {error}"
                            )
                        }
                    }));
                }
                Err(error) => return Err(infrastructure(error.to_string())),
            }
        };
        std::fs::write(run_dir.join("status"), format!("{status}\n"))
            .map_err(|error| infrastructure(error.to_string()))?;
        if !status.success() {
            return Err(infrastructure(format!("runner exited with {status}")));
        }
        let response = std::fs::read(&response_path)
            .map_err(|error| infrastructure(format!("no response: {error}")))?;
        Ok(Invocation {
            request_sha256: sha256_hex(&request_bytes),
            response,
        })
    }

    /// Asks the runner to describe itself.
    ///
    /// # Errors
    ///
    /// As [`Runner::invoke`]; [`DiffError::Identity`] when the response
    /// answers another request or build.
    pub fn describe(&self, run_dir: &Path) -> Result<DescribeResponse, DiffError> {
        let request = ControlRequest {
            format_version: PROTOCOL_VERSION,
            operation: Operation::Describe,
        };
        let invocation = self.invoke(run_dir, &request, RUNNER_TIMEOUT)?;
        let response: DescribeResponse = serde_json::from_slice(&invocation.response)
            .map_err(|error| DiffError::Infrastructure(format!("describe response: {error}")))?;
        self.check_envelope(
            response.format_version,
            &response.request_sha256,
            &response.runner_build_id,
            &invocation.request_sha256,
        )?;
        Ok(response)
    }

    /// Evaluates `query` on `input` with the blobs at `blob_root`, checking
    /// the whole response envelope.
    ///
    /// # Errors
    ///
    /// As [`Runner::invoke`]; [`DiffError::Identity`] when the response
    /// answers another request, input, query or build;
    /// [`DiffError::Infrastructure`] for a malformed response or a reported
    /// infrastructure failure.
    pub fn observe(
        &self,
        run_dir: &Path,
        blob_root: &Path,
        input: &ControlInput,
        query: &Value,
        timeout: Duration,
    ) -> Result<ControlResponseV1, DiffError> {
        let request = ControlRequest {
            format_version: PROTOCOL_VERSION,
            operation: Operation::Observe {
                blob_root: blob_root.to_path_buf(),
                input: Box::new(input.clone()),
                query: query.clone(),
            },
        };
        let invocation = self.invoke(run_dir, &request, timeout)?;
        let response: ControlResponseV1 = serde_json::from_slice(&invocation.response)
            .map_err(|error| DiffError::Infrastructure(format!("observe response: {error}")))?;
        self.check_envelope(
            response.format_version,
            &response.request_sha256,
            &response.runner_build_id,
            &invocation.request_sha256,
        )?;
        if response.input_sha256 != input.sha256()?
            || response.query_sha256 != sha256_hex(&canonical_json(query))
        {
            return Err(DiffError::Identity(
                "response answers another input or query".into(),
            ));
        }
        check_result(&response.result, query)?;
        Ok(response)
    }

    fn check_envelope(
        &self,
        format_version: u32,
        answered: &str,
        build_id: &str,
        sent: &str,
    ) -> Result<(), DiffError> {
        if format_version != PROTOCOL_VERSION {
            return Err(DiffError::Infrastructure(
                "response has another protocol version".into(),
            ));
        }
        if answered != sent {
            return Err(DiffError::Identity(
                "response answers another request".into(),
            ));
        }
        if build_id != self.build_id {
            return Err(DiffError::Identity(format!(
                "runner_mismatch: response from build {build_id}, invoked {}",
                self.build_id
            )));
        }
        Ok(())
    }
}

/// Checks a result's shape against its query.
fn check_result(result: &ControlResult, query: &Value) -> Result<(), DiffError> {
    let kind = query
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match result {
        ControlResult::Exact { alternatives, .. } => {
            let normalized = normalize_alternatives(alternatives.clone())?;
            if alternatives.is_empty() || normalized != *alternatives {
                return Err(DiffError::Infrastructure(
                    "protocol_error: exact alternatives must be nonempty, sorted and unique".into(),
                ));
            }
            if alternatives
                .iter()
                .any(|alternative| alternative.query_kind() != kind)
            {
                return Err(DiffError::Infrastructure(
                    "protocol_error: an exact observation answers another query".into(),
                ));
            }
            Ok(())
        }
        ControlResult::AbstractUnknown { .. } | ControlResult::Unsupported { .. } => Ok(()),
        ControlResult::InfrastructureFailure { code, diagnostic } => Err(
            DiffError::Infrastructure(format!("runner reported {code:?}: {diagnostic}")),
        ),
    }
}

/// How an answer scores against a grounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Score {
    /// Exact and equal to the complete grounding.
    Correct,
    /// Exact and different from the complete grounding.
    Wrong,
    /// Exact, but the grounding is sampled: no correctness either way.
    Ungrounded,
    /// A measured precision gap.
    Unknown,
    /// An evaluator capability gap.
    Unsupported,
}

/// Scores `result` against `grounding`.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] for an infrastructure result, which is
/// never scored.
pub fn score(result: &ControlResult, grounding: &Grounding) -> Result<Score, DiffError> {
    match result {
        ControlResult::Exact { alternatives, .. } => Ok(match grounding.coverage {
            GroundCoverage::Sampled => Score::Ungrounded,
            GroundCoverage::Complete
                if normalize_alternatives(alternatives.clone())?
                    == normalize_alternatives(grounding.alternatives.clone())? =>
            {
                Score::Correct
            }
            GroundCoverage::Complete => Score::Wrong,
        }),
        ControlResult::AbstractUnknown { .. } => Ok(Score::Unknown),
        ControlResult::Unsupported { .. } => Ok(Score::Unsupported),
        ControlResult::InfrastructureFailure { code, diagnostic } => Err(
            DiffError::Infrastructure(format!("runner reported {code:?}: {diagnostic}")),
        ),
    }
}

/// One scored observation's outcome in one comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationOutcome {
    /// The observation witness id.
    pub witness: String,
    /// The case id.
    pub case: String,
    /// Main's answer.
    pub main: ControlResult,
    /// The candidate's answer.
    pub candidate: ControlResult,
    /// Main's score.
    pub main_score: Score,
    /// The candidate's score.
    pub candidate_score: Score,
    /// The class.
    pub class: Class,
    /// The error direction of a regression.
    pub direction: Option<String>,
    /// SHA-256 of the candidate's semantic answer.
    pub answer_sha256: String,
}

/// What an answer claims: an exact set, or a gap whose representation does
/// not matter.
fn semantic_answer(result: &ControlResult) -> Value {
    match result {
        ControlResult::Exact { alternatives, .. } => json!({"exact": alternatives}),
        ControlResult::AbstractUnknown { .. } => json!("abstract_unknown"),
        ControlResult::Unsupported { .. } => json!("unsupported"),
        ControlResult::InfrastructureFailure { .. } => json!("infrastructure_failure"),
    }
}

/// Classifies one scored observation from both answers and the grounding.
///
/// Unknown to correct closes; correct to unknown is a precision regression;
/// unknown to wrong is a new unsound claim; wrong to unknown withdraws the
/// claim but stays open; unsupported on either side is unadjudicated, and
/// two different exact answers without a complete grounding are too.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] for an infrastructure result.
pub fn classify_observation(
    case: &ControlCase,
    main: &ControlResult,
    candidate: &ControlResult,
) -> Result<ObservationOutcome, DiffError> {
    let (main_score, candidate_score) = (
        score(main, &case.grounding)?,
        score(candidate, &case.grounding)?,
    );
    let answer = canonical_json(&semantic_answer(candidate));
    let same = canonical_json(&semantic_answer(main)) == answer;
    let regression = |direction: &str| (Class::Regression, Some(direction.to_string()));
    let (class, direction) = match (main_score, candidate_score) {
        (Score::Correct, Score::Correct) => (Class::Identical, None),
        (Score::Correct, Score::Wrong) => regression("observation:correct-to-wrong"),
        (Score::Correct, Score::Unknown) => regression("observation:correct-to-unknown"),
        (Score::Unknown, Score::Wrong) => regression("observation:unknown-to-wrong"),
        (Score::Wrong, Score::Wrong) if !same => regression("observation:changed-wrong"),
        (Score::Wrong, Score::Wrong) | (Score::Unknown | Score::Wrong, Score::Unknown) => {
            (Class::Inherited, None)
        }
        (Score::Unknown | Score::Wrong, Score::Correct) => (Class::Closure, None),
        (Score::Ungrounded, Score::Ungrounded) if same => (Class::Identical, None),
        (Score::Unsupported | Score::Ungrounded, _)
        | (_, Score::Unsupported | Score::Ungrounded) => (Class::Unadjudicated, None),
    };
    Ok(ObservationOutcome {
        witness: case.witness_id()?,
        case: case.id.clone(),
        main: main.clone(),
        candidate: candidate.clone(),
        main_score,
        candidate_score,
        class,
        direction,
        answer_sha256: sha256_hex(&answer),
    })
}

/// Coverage counts over scored answers and the grounding-only cases.
#[must_use]
pub fn coverage(outcomes: &[ObservationOutcome], grounding_only: usize) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for outcome in outcomes {
        let label = match outcome.candidate_score {
            Score::Correct => "exact_correct",
            Score::Wrong => "exact_wrong",
            Score::Ungrounded => "exact_ungrounded",
            Score::Unknown => "unknown",
            Score::Unsupported => "unsupported",
        };
        *counts.entry(label.to_string()).or_default() += 1;
    }
    json!({
        "scored": outcomes.len(),
        "grounding_only": grounding_only,
        "candidate_scores": counts,
    })
}

/// An output selection's value: `bytes` in standard base64.
#[must_use]
pub fn output_value(bytes: &[u8]) -> Selection<String> {
    use base64::Engine as _;
    Selection::Value {
        value: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

/// Writes a registry: blobs, cases and the manifest that binds them.
#[derive(Debug)]
pub struct RegistryBuilder {
    root: PathBuf,
    blobs: BTreeSet<String>,
    cases: Vec<CaseEntry>,
}

impl RegistryBuilder {
    /// The builder's blobs.
    #[must_use]
    pub fn blobs(&self) -> ControlBlobs {
        ControlBlobs::new(self.root.join("blobs"))
    }

    /// Runs the pinned Helm `helm` on `input` as [`grounding_argv`] says,
    /// in the new directory `scratch`, stores the evidence and returns the
    /// grounding of `query` it derives, complete.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when Helm cannot run; [`DiffError::Request`]
    /// when its outcome derives no observation.
    pub fn ground(
        &mut self,
        helm: &Path,
        input: &ControlInput,
        query: &Query,
        scratch: &Path,
    ) -> Result<Grounding, DiffError> {
        let show_only = match query {
            Query::OutputV1 { file, .. } | Query::AliasV1 { file, .. } => Some(file.as_str()),
            Query::TerminationV1 { .. } => None,
        };
        let has_values = !self.blobs().read(&input.original_values_sha256)?.is_empty();
        let argv = grounding_argv(input, show_only, has_values);
        let staged = StagedChart::new(&self.blobs(), input, scratch)?;
        let output = staged.run(helm, &argv)?;
        let exit_code = output.status.code().ok_or_else(|| {
            DiffError::Infrastructure("the pinned Helm was killed by a signal".into())
        })?;
        let version = staged.run(helm, &["version".to_string()])?;
        let evidence = GroundingEvidence {
            format_version: PROTOCOL_VERSION,
            helm_version: String::from_utf8_lossy(&version.stdout).trim().to_string(),
            helm_sha256: std::fs::read(helm)
                .map(|bytes| sha256_hex(&bytes))
                .map_err(|error| {
                    DiffError::Infrastructure(format!("{}: {error}", helm.display()))
                })?,
            argv,
            chart_tree_sha256: input.chart_tree_sha256.clone(),
            original_values_sha256: input.original_values_sha256.clone(),
            release: input.release.clone(),
            kubernetes_version: input.kubernetes_version.clone(),
            show_only: show_only.map(str::to_string),
            exit_code,
            stdout_sha256: self.blob(&output.stdout)?,
            stderr_sha256: self.blob(&output.stderr)?,
        };
        let value = serde_json::to_value(&evidence)
            .map_err(|error| DiffError::Infrastructure(format!("evidence: {error}")))?;
        let observation = derive_observation(query, exit_code, &output.stdout, &output.stderr)
            .map_err(DiffError::Request)?;
        Ok(Grounding {
            evidence_sha256: self.blob(&canonical_json(&value))?,
            coverage: GroundCoverage::Complete,
            alternatives: vec![observation],
        })
    }

    /// A builder for the new registry directory `root`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Identity`] when `root` exists;
    /// [`DiffError::Infrastructure`] when it cannot be created.
    pub fn new(root: &Path) -> Result<Self, DiffError> {
        if root.exists() {
            return Err(DiffError::Identity(format!(
                "{} already exists",
                root.display()
            )));
        }
        for dir in ["blobs", "cases"] {
            std::fs::create_dir_all(root.join(dir)).map_err(|error| {
                DiffError::Infrastructure(format!("{}: {error}", root.display()))
            })?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            blobs: BTreeSet::new(),
            cases: Vec::new(),
        })
    }

    /// Stores `bytes` and returns their digest.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when the blob cannot be written.
    pub fn blob(&mut self, bytes: &[u8]) -> Result<String, DiffError> {
        let sha = sha256_hex(bytes);
        if self.blobs.insert(sha.clone()) {
            write_new(&self.root.join("blobs").join(&sha), bytes)?;
        }
        Ok(sha)
    }

    /// Stores a chart and its documents and returns a render-mode input with
    /// the default release, context, recipe and bounds.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when a blob cannot be written.
    pub fn render_input(
        &mut self,
        files: &[(&str, &str)],
        values: &str,
        coalesced: &Value,
    ) -> Result<ControlInput, DiffError> {
        let mut tree = BTreeMap::new();
        for (name, contents) in files {
            tree.insert((*name).to_string(), self.blob(contents.as_bytes())?);
        }
        let tree = serde_json::to_value(ChartTree {
            format_version: PROTOCOL_VERSION,
            files: tree,
        })
        .map_err(|error| DiffError::Infrastructure(format!("chart tree: {error}")))?;
        Ok(ControlInput {
            chart_tree_sha256: self.blob(&canonical_json(&tree))?,
            original_values_sha256: self.blob(values.as_bytes())?,
            coalesced_document_sha256: self.blob(&canonical_json(coalesced))?,
            context_sha256: self.blob(&canonical_json(&json!({"capabilities": "default"})))?,
            release: Release {
                name: "p".into(),
                namespace: "default".into(),
            },
            kubernetes_version: "v1.29.0".into(),
            mode: EngineMode::Render,
            recipe_sha256: self.blob(&canonical_json(&json!({"recipe": "default"})))?,
            bound_policy_sha256: self.blob(&canonical_json(&json!({"bounds": "default"})))?,
            cache_mode: CacheMode::Default,
        })
    }

    /// Adds `case` as `cases/<id>.json`.
    ///
    /// # Errors
    ///
    /// [`DiffError::Request`] for an invalid case;
    /// [`DiffError::Infrastructure`] when it cannot be written.
    pub fn case(&mut self, case: &ControlCase) -> Result<(), DiffError> {
        case.validate()?;
        let value = serde_json::to_value(case)
            .map_err(|error| DiffError::Infrastructure(format!("case: {error}")))?;
        let bytes = canonical_json(&value);
        let file = format!("cases/{}.json", case.id);
        write_new(&self.root.join(&file), &bytes)?;
        self.cases.push(CaseEntry {
            id: case.id.clone(),
            file,
            sha256: sha256_hex(&bytes),
        });
        Ok(())
    }

    /// Writes the manifest and opens the verified registry.
    ///
    /// # Errors
    ///
    /// As [`ControlRegistry::open`].
    pub fn finish(mut self) -> Result<ControlRegistry, DiffError> {
        self.cases.sort_by(|left, right| left.id.cmp(&right.id));
        let manifest = serde_json::to_value(ControlsManifest {
            format_version: PROTOCOL_VERSION,
            cases: self.cases,
            blobs: self.blobs.into_iter().collect(),
        })
        .map_err(|error| DiffError::Infrastructure(format!("manifest: {error}")))?;
        write_new(&self.root.join("manifest.json"), &canonical_json(&manifest))?;
        ControlRegistry::open(&self.root)
    }
}

/// One case of a registry specification.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseSpec {
    /// The case id.
    pub id: String,
    /// The chart directory, relative to the specification.
    pub chart: PathBuf,
    /// The query.
    pub query: Query,
}

/// Builds the new registry `out` from the specification `spec`, a JSON list
/// of [`CaseSpec`]s: each chart rendered with empty values and grounded by
/// the pinned Helm `helm`.
///
/// # Errors
///
/// [`DiffError::Request`] for an unreadable specification or chart, or a
/// case Helm's outcome does not ground; otherwise as [`RegistryBuilder`].
pub fn ground_registry(spec: &Path, helm: &Path, out: &Path) -> Result<ControlRegistry, DiffError> {
    let request =
        |error: &dyn std::fmt::Display| DiffError::Request(format!("{}: {error}", spec.display()));
    let specs: Vec<CaseSpec> = std::fs::read(spec)
        .map_err(|error| request(&error))
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|error| request(&error)))?;
    let base = spec.parent().unwrap_or_else(|| Path::new("."));
    let scratch = out.with_extension("ground");
    let mut builder = RegistryBuilder::new(out)?;
    for CaseSpec { id, chart, query } in specs {
        let files = chart_files(&base.join(&chart))?;
        let files = files
            .iter()
            .map(|(name, contents)| (name.as_str(), contents.as_str()))
            .collect::<Vec<_>>();
        let input = builder.render_input(&files, "", &json!({}))?;
        let grounding = builder.ground(helm, &input, &query, &scratch.join(&id))?;
        builder.case(&ControlCase {
            format_version: PROTOCOL_VERSION,
            id,
            role: Role::Scored,
            input,
            query: Some(query),
            claim: None,
            grounding,
        })?;
    }
    builder.finish()
}

/// Every file under `root` by `/`-separated relative name, as UTF-8.
fn chart_files(root: &Path) -> Result<BTreeMap<String, String>, DiffError> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| DiffError::Request(format!("{}: {error}", dir.display())))?;
        for entry in entries {
            let path = entry
                .map_err(|error| DiffError::Request(format!("{}: {error}", dir.display())))?
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path
                .strip_prefix(root)
                .map_err(|error| DiffError::Request(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))?;
            files.insert(name, contents);
        }
    }
    Ok(files)
}

/// One captured observation: a scored case's validated response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRecord {
    /// The case id.
    pub case: String,
    /// The observation witness id.
    pub witness: String,
    /// The validated response.
    pub response: ControlResponseV1,
}

/// Evaluates scored `case` with `runner` in the new directory `run_dir`,
/// reading blobs from `blob_root`.
///
/// # Errors
///
/// [`DiffError::Request`] for a grounding-only case; otherwise as
/// [`Runner::observe`].
pub fn observe_case(
    runner: &Runner,
    run_dir: &Path,
    blob_root: &Path,
    case: &ControlCase,
) -> Result<ObservationRecord, DiffError> {
    let query = case
        .scored_query()
        .ok_or_else(|| DiffError::Request(format!("{} is not scored", case.id)))?;
    let query = serde_json::to_value(query)
        .map_err(|error| DiffError::Infrastructure(format!("query: {error}")))?;
    let blob_root = std::path::absolute(blob_root)
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", blob_root.display())))?;
    let response = runner.observe(run_dir, &blob_root, &case.input, &query, RUNNER_TIMEOUT)?;
    Ok(ObservationRecord {
        case: case.id.clone(),
        witness: case.witness_id()?,
        response,
    })
}

/// Copies exactly the indexed files of `registry` into the new directory
/// `target` and opens the copy, which must hash identically.
///
/// # Errors
///
/// [`DiffError::Identity`] when the copy differs;
/// [`DiffError::Infrastructure`] when a file cannot be copied.
pub fn copy_registry(
    registry: &ControlRegistry,
    target: &Path,
) -> Result<ControlRegistry, DiffError> {
    let manifest_bytes = std::fs::read(registry.root.join("manifest.json"))
        .map_err(|error| DiffError::Infrastructure(format!("controls manifest: {error}")))?;
    let manifest: ControlsManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| DiffError::Request(format!("controls manifest: {error}")))?;
    let mut files = vec![PathBuf::from("manifest.json")];
    files.extend(
        manifest
            .cases
            .iter()
            .map(|entry| PathBuf::from(&entry.file)),
    );
    files.extend(
        manifest
            .blobs
            .iter()
            .map(|blob| Path::new("blobs").join(blob)),
    );
    std::fs::create_dir_all(target.join("blobs"))
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", target.display())))?;
    for file in files {
        let destination = target.join(&file);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                DiffError::Infrastructure(format!("{}: {error}", parent.display()))
            })?;
        }
        let bytes = std::fs::read(registry.root.join(&file))
            .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", file.display())))?;
        write_new(&destination, &bytes)?;
    }
    let copy = ControlRegistry::open(target)?;
    if copy.sha256 != registry.sha256 {
        return Err(DiffError::Identity("the registry copy differs".into()));
    }
    Ok(copy)
}

/// Copies the blobs `case` needs from `registry` into the new directory
/// `blob_root`, so a replay sees nothing else.
///
/// # Errors
///
/// As [`ControlBlobs::input_closure`]; [`DiffError::Infrastructure`] when
/// a blob cannot be written.
pub fn stage_case_blobs(
    registry: &ControlRegistry,
    case: &ControlCase,
    blob_root: &Path,
) -> Result<(), DiffError> {
    stage_blobs(&registry.blobs(), case, blob_root)
}

/// Copies the blobs `case` needs from `source` into the new flat directory
/// `blob_root`, so a replay sees nothing else.
///
/// # Errors
///
/// As [`ControlCase::closure`]; [`DiffError::Infrastructure`] when a blob
/// cannot be written.
pub fn stage_blobs(
    source: &ControlBlobs,
    case: &ControlCase,
    blob_root: &Path,
) -> Result<(), DiffError> {
    std::fs::create_dir_all(blob_root)
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", blob_root.display())))?;
    for blob in case.closure(source)? {
        write_new(&blob_root.join(&blob), &source.read(&blob)?)?;
    }
    Ok(())
}

/// A control's chart and values staged into a scratch directory for the
/// pinned Helm, with a private home.
#[derive(Debug)]
pub struct StagedChart {
    chart: PathBuf,
    values: PathBuf,
    home: PathBuf,
}

impl StagedChart {
    /// Stages `input`'s chart files and values from `blobs` into the new
    /// directory `scratch`.
    ///
    /// # Errors
    ///
    /// As [`ControlBlobs::read`]; [`DiffError::Infrastructure`] when a file
    /// cannot be written.
    pub fn new(
        blobs: &ControlBlobs,
        input: &ControlInput,
        scratch: &Path,
    ) -> Result<Self, DiffError> {
        let infrastructure = |path: &Path, error: std::io::Error| {
            DiffError::Infrastructure(format!("{}: {error}", path.display()))
        };
        let chart = scratch.join("chart");
        let tree = ChartTree::parse(&blobs.read(&input.chart_tree_sha256)?)?;
        for (name, sha) in &tree.files {
            let path = chart.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| infrastructure(parent, error))?;
            }
            write_new(&path, &blobs.read(sha)?)?;
        }
        let values = scratch.join("values.yaml");
        write_new(&values, &blobs.read(&input.original_values_sha256)?)?;
        let home = scratch.join("home");
        std::fs::create_dir_all(&home).map_err(|error| infrastructure(&home, error))?;
        Ok(Self {
            chart: std::path::absolute(&chart).map_err(|error| infrastructure(&chart, error))?,
            values: std::path::absolute(&values).map_err(|error| infrastructure(&values, error))?,
            home,
        })
    }

    /// Runs `helm` with `argv`, `{chart}` and `{values}` substituted, in a
    /// cleared environment.
    ///
    /// # Errors
    ///
    /// [`DiffError::Infrastructure`] when it cannot be spawned.
    pub fn run(&self, helm: &Path, argv: &[String]) -> Result<std::process::Output, DiffError> {
        let arguments = argv
            .iter()
            .map(|argument| {
                argument
                    .replace("{chart}", &self.chart.to_string_lossy())
                    .replace("{values}", &self.values.to_string_lossy())
            })
            .collect::<Vec<_>>();
        Command::new(helm)
            .env_clear()
            .env("HOME", &self.home)
            .env("HELM_CACHE_HOME", self.home.join("cache"))
            .env("HELM_CONFIG_HOME", self.home.join("config"))
            .env("HELM_DATA_HOME", self.home.join("data"))
            .args(&arguments)
            .output()
            .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", helm.display())))
    }
}
