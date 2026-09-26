//! Executes Helm under a controlled environment and replays exact prior executions.
//!
//! An execution is identified by everything that can change what Helm prints:
//! the Helm binary's bytes and version, the platform, the complete child
//! environment and working directory, every argument, and the content of
//! every chart tree and values file an argument names. Content lives at
//! content-addressed paths under one root, so equal content is always named
//! by equal arguments and a temporary path can never leak into a diagnostic
//! under an otherwise identical key.
//!
//! A completed execution is stored under its key as one directory published
//! by an atomic rename of a synced staging directory. Readers verify the
//! request and output hashes; an incomplete or corrupt entry is a miss.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use wait4::Wait4 as _;

/// The pinned Helm release every adjudication runs.
const PINNED_HELM_VERSION: &str = "v4.2.3";

/// Domain of invocation keys and the stored entry format. Changing what an
/// entry holds or how a request is encoded must change this.
const INVOCATION_FORMAT: &str = "helm-schema/helm-invocation/v2";

/// The preparation policy a prepared tree identity covers. Changing what
/// preparation removes, adds or repacks must change this.
const PREPARATION_POLICY: &str = "helm-schema/prepared-chart/v1";

/// The release name every adjudication renders.
const RELEASE_NAME: &str = "adjudication";

/// A chart tree at its content address.
#[derive(Clone, Debug)]
pub(crate) struct PreparedTree {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
}

/// Whether an execution may be replayed from, and published to, the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Cacheability {
    Cacheable,
    /// Replayable only as a client-only `helm template`, where `lookup`
    /// answers empty: nothing reaches a cluster.
    ClientOnly,
    /// Identical inputs need not produce identical output, for this reason.
    Bypass(String),
}

/// How the outputs of one invocation were obtained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Outcome {
    Executed,
    /// Replayed from the store entry of an earlier identical execution.
    Replayed,
    Bypassed(String),
}

/// One completed Helm execution: Helm ran and exited with its own status.
pub(crate) struct HelmExecution {
    /// Helm's exit code: 0 on success, 1 when Helm reports an error.
    pub(crate) exit_code: i32,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) record: InvocationRecord,
}

impl HelmExecution {
    pub(crate) const fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// What one invocation cost, retained beside its outputs.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct InvocationRecord {
    pub(crate) stage: String,
    pub(crate) key: String,
    pub(crate) outcome: Outcome,
    pub(crate) exit_code: i32,
    /// Duration of the original execution, also for a replay.
    pub(crate) elapsed_ms: u64,
    /// Time spent finding and verifying a stored entry.
    pub(crate) lookup_ms: u64,
    /// Peak resident set size of the original child alone, from its own `wait4`.
    pub(crate) max_rss_bytes: u64,
    pub(crate) input_bytes: u64,
    pub(crate) stdout_bytes: u64,
    pub(crate) stderr_bytes: u64,
}

/// Everything that identifies one Helm execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InvocationRequest {
    pub(crate) format: String,
    pub(crate) platform: String,
    pub(crate) helm_sha256: String,
    pub(crate) helm_version: String,
    pub(crate) working_directory: String,
    /// The child's complete environment, sorted by name.
    pub(crate) environment: Vec<(String, String)>,
    pub(crate) arguments: Vec<String>,
    /// The content identity of each path an argument or the environment
    /// names, in argument order, then Helm's home directory.
    pub(crate) inputs: Vec<(String, String)>,
    /// Whether the invocation is a client-only `helm template`: no server
    /// dry run, no validation against a cluster, and no kubeconfig.
    pub(crate) client_only: bool,
}

impl InvocationRequest {
    /// Whether `arguments` and `environment` describe a client-only template.
    fn is_client_only(arguments: &[String], environment: &[(String, String)]) -> bool {
        arguments
            .first()
            .is_some_and(|command| command == "template")
            && !arguments.iter().any(|argument| {
                argument == "--validate" || argument.starts_with("--dry-run=server")
            })
            && !environment.iter().any(|(name, _)| name == "KUBECONFIG")
    }

    /// The stored encoding of the request: pretty JSON.
    fn encode(&self) -> eyre::Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// SHA-256 of the domain-separated stored encoding of the request.
    pub(crate) fn key(&self) -> eyre::Result<String> {
        Ok(key_of(&self.encode()?))
    }
}

fn key_of(request: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, INVOCATION_FORMAT.as_bytes());
    hash_field(&mut hasher, request);
    hex(&hasher.finalize())
}

/// A `helm template` of a prepared chart with one values file.
pub(crate) struct TemplateRequest<'a> {
    pub(crate) chart: &'a PreparedTree,
    /// The exact bytes Helm reads through `-f`.
    pub(crate) values: &'a [u8],
    pub(crate) kubernetes_version: &'a str,
}

/// The result half of a stored entry.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredResult {
    exit_code: i32,
    stdout_sha256: String,
    stderr_sha256: String,
    elapsed_ms: u64,
    max_rss_bytes: u64,
}

/// The last completed file in a store entry: the SHA-256 of every other
/// file by name, so no damaged file, the result metadata included, replays.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct EntryManifest {
    request: String,
    result: String,
    stdout: String,
    stderr: String,
}

/// What identifies an executable file's bytes without reading them.
#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    modified: std::time::SystemTime,
    inode: u64,
}

impl FileStamp {
    fn of(path: &Path) -> eyre::Result<Self> {
        let metadata = fs::metadata(path)?;
        #[cfg(unix)]
        let inode = std::os::unix::fs::MetadataExt::ino(&metadata);
        #[cfg(not(unix))]
        let inode = 0;
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified()?,
            inode,
        })
    }
}

/// The pinned Helm binary and the root its inputs and entries live under.
pub(crate) struct HelmRunner {
    program: PathBuf,
    helm_sha256: String,
    /// The program's stamp when its bytes were hashed.
    program_stamp: FileStamp,
    root: PathBuf,
    /// Whether entries are replayed and published.
    replay: bool,
}

static SHARED_RUNNER: OnceLock<Result<HelmRunner, String>> = OnceLock::new();

impl HelmRunner {
    /// The runner of this process, with the Helm version checked once.
    /// `SCHEMA_HELM_INVOCATION_CACHE` names a persistent store; without it
    /// a private temporary root is used and nothing is replayed.
    pub(crate) fn shared() -> eyre::Result<&'static Self> {
        SHARED_RUNNER
            .get_or_init(|| {
                let runner = match std::env::var_os("SCHEMA_HELM_INVOCATION_CACHE") {
                    Some(root) => Self::new(&PathBuf::from(root).join("v2"), true),
                    None => tempfile::Builder::new()
                        .prefix("helm-schema-helm-root-")
                        .tempdir()
                        .map_err(eyre::Report::from)
                        .and_then(|root| Self::new(&root.keep(), false)),
                };
                runner.map_err(|error| format!("{error:?}"))
            })
            .as_ref()
            .map_err(|error| eyre::eyre!("{error}"))
    }

    /// A runner rooted at `root`, replaying stored entries when `replay`.
    ///
    /// # Errors
    ///
    /// Returns an error when no Helm is on `PATH`, it is not the pinned
    /// release, or `root` cannot be created.
    pub(crate) fn new(root: &Path, replay: bool) -> eyre::Result<Self> {
        Self::with_program(root, replay, find_helm()?)
    }

    /// A runner executing `program` as Helm.
    ///
    /// # Errors
    ///
    /// Returns an error when `program` is not the pinned Helm release or
    /// `root` cannot be created.
    pub(crate) fn with_program(root: &Path, replay: bool, program: PathBuf) -> eyre::Result<Self> {
        fs::create_dir_all(root.join("home"))?;
        let root = root.canonicalize()?;
        let program_stamp = FileStamp::of(&program)?;
        let helm_sha256 = hex(&Sha256::digest(fs::read(&program)?));
        let runner = Self {
            program,
            helm_sha256,
            program_stamp,
            root,
            replay,
        };
        let version = Command::new(&runner.program)
            .env_clear()
            .envs(runner.environment()?)
            .current_dir(&runner.root)
            .args(["version", "--template", "{{.Version}}"])
            .output()
            .wrap_err("read Helm version")?;
        eyre::ensure!(
            version.status.success() && version.stdout == PINNED_HELM_VERSION.as_bytes(),
            "adjudication requires Helm {PINNED_HELM_VERSION}: {}",
            String::from_utf8_lossy(&version.stderr)
        );
        Ok(runner)
    }

    /// A fresh directory on the root's filesystem for building a tree.
    pub(crate) fn staging_dir(&self) -> eyre::Result<PathBuf> {
        let staging = self.root.join("staging");
        fs::create_dir_all(&staging)?;
        Ok(tempfile::Builder::new()
            .prefix("tree-")
            .tempdir_in(staging)?
            .keep())
    }

    /// Moves the tree built at `staged` to its content address, or discards
    /// it when that address already holds the same content.
    pub(crate) fn publish_tree(&self, staged: &Path) -> eyre::Result<PreparedTree> {
        let sha256 = tree_sha256(staged)?;
        let trees = self.root.join("trees");
        fs::create_dir_all(&trees)?;
        let path = trees.join(&sha256);
        if fs::rename(staged, &path).is_err() {
            fs::remove_dir_all(staged)?;
            eyre::ensure!(
                tree_sha256(&path)? == sha256,
                "prepared tree {} no longer matches its content address",
                path.display()
            );
        }
        Ok(PreparedTree { path, sha256 })
    }

    /// Runs `helm template` for `request`, writing its outputs and record
    /// into `case` under `stage`, or replays an identical stored execution
    /// when `cacheability` allows it.
    ///
    /// A child that cannot be started or waited for, that a signal ends, or
    /// that exits with a status other than Helm's own 0 or 1 is a harness
    /// failure, never a Helm verdict, and is never stored.
    pub(crate) fn template(
        &self,
        request: &TemplateRequest<'_>,
        case: &Path,
        stage: &str,
        cacheability: &Cacheability,
    ) -> eyre::Result<HelmExecution> {
        let (values_path, values_sha256) = self.publish_input(request.values)?;
        let chart = utf8(&request.chart.path)?;
        let values = utf8(&values_path)?;
        let home = self.root.join("home");
        let environment = self.environment()?;
        let arguments = [
            "template",
            RELEASE_NAME,
            chart,
            "--kube-version",
            request.kubernetes_version,
            "--skip-schema-validation",
            "-f",
            values,
        ]
        .map(str::to_string)
        .to_vec();
        let invocation = InvocationRequest {
            format: INVOCATION_FORMAT.to_string(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            helm_sha256: self.helm_sha256.clone(),
            helm_version: PINNED_HELM_VERSION.to_string(),
            working_directory: utf8(&self.root)?.to_string(),
            client_only: InvocationRequest::is_client_only(&arguments, &environment),
            environment,
            arguments,
            // Helm reads its configuration, cache and data under the home.
            inputs: vec![
                (chart.to_string(), request.chart.sha256.clone()),
                (values.to_string(), values_sha256),
                (utf8(&home)?.to_string(), tree_sha256(&home)?),
            ],
        };
        self.run(
            &invocation,
            u64::try_from(request.values.len())?,
            case,
            stage,
            cacheability,
        )
    }

    fn run(
        &self,
        invocation: &InvocationRequest,
        input_bytes: u64,
        case: &Path,
        stage: &str,
        cacheability: &Cacheability,
    ) -> eyre::Result<HelmExecution> {
        self.verify_program()?;
        let request = invocation.encode()?;
        let key = invocation.key()?;
        let entry = self.entry_dir(&key);
        let lookup = Instant::now();
        let cacheability = match cacheability {
            Cacheability::ClientOnly if !invocation.client_only => {
                &Cacheability::Bypass("calls lookup outside a client-only template".to_string())
            }
            other => other,
        };
        let replayable = self.replay && !matches!(cacheability, Cacheability::Bypass(_));
        let stored = if replayable {
            read_entry(&entry, &request)?
        } else {
            None
        };
        let lookup_ms = elapsed_ms(lookup);
        let (result, stdout, stderr, outcome) = if let Some((result, stdout, stderr)) = stored {
            fs::write(case.join(format!("{stage}.yaml")), &stdout)?;
            fs::write(case.join(format!("{stage}.stderr")), &stderr)?;
            (result, stdout, stderr, Outcome::Replayed)
        } else {
            let (result, stdout, stderr) = self.execute(invocation, case, stage)?;
            if replayable {
                publish_entry(&entry, &request, &result, &stdout, &stderr)?;
            }
            let outcome = match cacheability {
                Cacheability::Cacheable | Cacheability::ClientOnly => Outcome::Executed,
                Cacheability::Bypass(reason) => Outcome::Bypassed(reason.clone()),
            };
            (result, stdout, stderr, outcome)
        };
        fs::write(
            case.join(format!("{stage}.status")),
            format!("exit status: {}", result.exit_code),
        )?;
        let record = InvocationRecord {
            stage: stage.to_string(),
            key,
            outcome,
            exit_code: result.exit_code,
            elapsed_ms: result.elapsed_ms,
            lookup_ms,
            max_rss_bytes: result.max_rss_bytes,
            input_bytes,
            stdout_bytes: u64::try_from(stdout.len())?,
            stderr_bytes: u64::try_from(stderr.len())?,
        };
        fs::write(
            case.join(format!("{stage}.invocation.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "record": record,
                "request": invocation,
                "entry": entry,
            }))?,
        )?;
        Ok(HelmExecution {
            exit_code: result.exit_code,
            stdout,
            stderr,
            record,
        })
    }

    fn execute(
        &self,
        invocation: &InvocationRequest,
        case: &Path,
        stage: &str,
    ) -> eyre::Result<(StoredResult, Vec<u8>, Vec<u8>)> {
        let stdout_path = case.join(format!("{stage}.yaml"));
        let stderr_path = case.join(format!("{stage}.stderr"));
        let started = Instant::now();
        let child = Command::new(&self.program)
            .env_clear()
            .envs(
                invocation
                    .environment
                    .iter()
                    .map(|(name, value)| (name, value)),
            )
            .current_dir(&invocation.working_directory)
            .args(&invocation.arguments)
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout_path)?)
            .stderr(fs::File::create(&stderr_path)?)
            .spawn()
            .wrap_err_with(|| format!("start Helm {stage}; evidence={}", case.display()))?;
        let usage = child
            .wait4()
            .wrap_err_with(|| format!("wait for Helm {stage}; evidence={}", case.display()))?;
        let elapsed_ms = elapsed_ms(started);
        let Some(exit_code @ (0 | 1)) = usage.status.code() else {
            eyre::bail!(
                "Helm {stage} ended abnormally ({}); evidence={}",
                usage.status,
                case.display()
            );
        };
        let stdout = fs::read(&stdout_path)?;
        let stderr = fs::read(&stderr_path)?;
        let result = StoredResult {
            exit_code,
            stdout_sha256: hex(&Sha256::digest(&stdout)),
            stderr_sha256: hex(&Sha256::digest(&stderr)),
            elapsed_ms,
            max_rss_bytes: usage.rusage.maxrss,
        };
        Ok((result, stdout, stderr))
    }

    /// Fails when the program's file changed since its bytes were hashed and
    /// its bytes now differ: every key names the hashed bytes.
    fn verify_program(&self) -> eyre::Result<()> {
        let stamp = FileStamp::of(&self.program)
            .wrap_err_with(|| format!("start Helm: {}", self.program.display()))?;
        if stamp == self.program_stamp {
            return Ok(());
        }
        eyre::ensure!(
            hex(&Sha256::digest(fs::read(&self.program)?)) == self.helm_sha256,
            "Helm executable {} changed during the run",
            self.program.display()
        );
        Ok(())
    }

    /// The child environment: nothing inherited, Helm's directories inside the root.
    fn environment(&self) -> eyre::Result<Vec<(String, String)>> {
        let home = self.root.join("home");
        let home = utf8(&home)?;
        let mut environment = vec![
            ("HELM_CACHE_HOME".to_string(), format!("{home}/cache")),
            ("HELM_CONFIG_HOME".to_string(), format!("{home}/config")),
            ("HELM_DATA_HOME".to_string(), format!("{home}/data")),
            ("HOME".to_string(), home.to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("TZ".to_string(), "UTC".to_string()),
        ];
        environment.sort();
        Ok(environment)
    }

    /// Writes `bytes` to its content address and returns that path and hash.
    fn publish_input(&self, bytes: &[u8]) -> eyre::Result<(PathBuf, String)> {
        let sha256 = hex(&Sha256::digest(bytes));
        let inputs = self.root.join("inputs");
        fs::create_dir_all(&inputs)?;
        let path = inputs.join(format!("{sha256}.json"));
        if fs::read(&path).is_ok_and(|stored| stored == bytes) {
            return Ok((path, sha256));
        }
        let mut staged = tempfile::NamedTempFile::new_in(&inputs)?;
        staged.write_all(bytes)?;
        staged.as_file().sync_all()?;
        staged.persist(&path)?;
        Ok((path, sha256))
    }

    fn entry_dir(&self, key: &str) -> PathBuf {
        self.root.join(key.get(..2).unwrap_or("00")).join(key)
    }
}

/// The verified outputs of the entry at `entry` for exactly `request`, or
/// `None` for a missing, incomplete or corrupt entry. A corrupt entry is
/// moved aside so that a fresh execution can take its place; moving aside
/// an entry another writer just completed costs only a replay.
fn read_entry(entry: &Path, request: &[u8]) -> eyre::Result<Option<StoredEntry>> {
    if !entry.exists() {
        return Ok(None);
    }
    match verified_entry(entry, request) {
        Ok(stored) => Ok(Some(stored)),
        Err(error) => {
            let parent = entry.parent().ok_or_eyre("store entry has no parent")?;
            let quarantine = tempfile::Builder::new()
                .prefix(".quarantine-")
                .tempdir_in(parent)?
                .keep();
            eprintln!(
                "Helm invocation entry {} is unusable ({error:#}); moved to {}",
                entry.display(),
                quarantine.display()
            );
            // Another reader may have moved it first.
            let _ = fs::rename(entry, quarantine.join("entry"));
            Ok(None)
        }
    }
}

/// A verified stored result with its stdout and stderr.
type StoredEntry = (StoredResult, Vec<u8>, Vec<u8>);

fn verified_entry(entry: &Path, request: &[u8]) -> eyre::Result<StoredEntry> {
    let manifest: EntryManifest = serde_json::from_slice(&fs::read(entry.join("manifest.json"))?)?;
    let stored_request = fs::read(entry.join("request.json"))?;
    let result = fs::read(entry.join("result.json"))?;
    let stdout = fs::read(entry.join("stdout"))?;
    let stderr = fs::read(entry.join("stderr"))?;
    eyre::ensure!(
        manifest == EntryManifest::of(&stored_request, &result, &stdout, &stderr),
        "stored files do not match the entry manifest"
    );
    eyre::ensure!(stored_request == request, "stored request differs");
    let result: StoredResult = serde_json::from_slice(&result)?;
    eyre::ensure!(
        matches!(result.exit_code, 0 | 1),
        "stored exit code {} is no completed Helm execution",
        result.exit_code
    );
    eyre::ensure!(
        hex(&Sha256::digest(&stdout)) == result.stdout_sha256
            && hex(&Sha256::digest(&stderr)) == result.stderr_sha256,
        "stored outputs do not match their hashes"
    );
    Ok((result, stdout, stderr))
}

impl EntryManifest {
    fn of(request: &[u8], result: &[u8], stdout: &[u8], stderr: &[u8]) -> Self {
        Self {
            request: hex(&Sha256::digest(request)),
            result: hex(&Sha256::digest(result)),
            stdout: hex(&Sha256::digest(stdout)),
            stderr: hex(&Sha256::digest(stderr)),
        }
    }
}

/// Publishes a complete entry at `entry` unless one is already there. The
/// manifest is written last; an entry without a matching one is a miss.
fn publish_entry(
    entry: &Path,
    request: &[u8],
    result: &StoredResult,
    stdout: &[u8],
    stderr: &[u8],
) -> eyre::Result<()> {
    if entry.exists() {
        return Ok(());
    }
    let parent = entry.parent().ok_or_eyre("store entry has no parent")?;
    fs::create_dir_all(parent)?;
    let staged = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(parent)?;
    let result = serde_json::to_vec_pretty(result)?;
    let manifest = serde_json::to_vec_pretty(&EntryManifest::of(request, &result, stdout, stderr))?;
    for (name, bytes) in [
        ("request.json", request),
        ("stdout", stdout),
        ("stderr", stderr),
        ("result.json", &result),
        ("manifest.json", &manifest),
    ] {
        let mut file = fs::File::create(staged.path().join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::File::open(staged.path())?.sync_all()?;
    // A rename never replaces a complete entry: the target is a non-empty directory.
    if fs::rename(staged.path(), entry).is_ok() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn find_helm() -> eyre::Result<PathBuf> {
    let path = std::env::var_os("PATH").ok_or_eyre("PATH is not set")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("helm");
        if candidate.is_file() {
            return Ok(candidate.canonicalize()?);
        }
    }
    eyre::bail!("no helm on PATH")
}

fn utf8(path: &Path) -> eyre::Result<&str> {
    path.to_str()
        .ok_or_else(|| eyre::eyre!("non-UTF-8 path: {}", path.display()))
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The invocations of one stage, summed.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct StageTotals {
    pub(crate) invocations: usize,
    pub(crate) executed: usize,
    pub(crate) replayed: usize,
    pub(crate) bypassed: usize,
    pub(crate) failures: usize,
    /// Duration of the original executions, replayed ones included.
    pub(crate) elapsed_ms: u64,
    /// Duration of the executions this run actually performed.
    pub(crate) executed_ms: u64,
    pub(crate) lookup_ms: u64,
    pub(crate) max_elapsed_ms: u64,
    pub(crate) max_rss_bytes: u64,
    pub(crate) input_bytes: u64,
    pub(crate) stdout_bytes: u64,
}

/// Sums `records` per stage.
pub(crate) fn stage_totals(records: &[InvocationRecord]) -> BTreeMap<String, StageTotals> {
    let mut totals = BTreeMap::<String, StageTotals>::new();
    for record in records {
        let stage = totals.entry(record.stage.clone()).or_default();
        stage.invocations += 1;
        match record.outcome {
            Outcome::Executed => stage.executed += 1,
            Outcome::Replayed => stage.replayed += 1,
            Outcome::Bypassed(_) => stage.bypassed += 1,
        }
        if record.outcome != Outcome::Replayed {
            stage.executed_ms += record.elapsed_ms;
        }
        stage.failures += usize::from(record.exit_code != 0);
        stage.elapsed_ms += record.elapsed_ms;
        stage.lookup_ms += record.lookup_ms;
        stage.max_elapsed_ms = stage.max_elapsed_ms.max(record.elapsed_ms);
        stage.max_rss_bytes = stage.max_rss_bytes.max(record.max_rss_bytes);
        stage.input_bytes += record.input_bytes;
        stage.stdout_bytes += record.stdout_bytes;
    }
    totals
}

/// SHA-256 over the sorted relative paths, entry types, lengths and bytes of
/// the tree at `root`, under the preparation policy. Host metadata such as
/// timestamps, modes and ownership is not content and is not hashed.
///
/// # Errors
///
/// Returns an error for an unreadable tree, a link, or a non-UTF-8 path.
pub(crate) fn tree_sha256(root: &Path) -> eyre::Result<String> {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, PREPARATION_POLICY.as_bytes());
    hash_tree(&mut hasher, root, "")?;
    Ok(hex(&hasher.finalize()))
}

fn hash_tree(hasher: &mut Sha256, directory: &Path, relative: &str) -> eyre::Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| eyre::eyre!("non-UTF-8 chart path: {}", entry.path().display()))?;
        let path = format!("{relative}{name}");
        let kind = entry.file_type()?;
        if kind.is_dir() {
            hash_field(hasher, b"directory");
            hash_field(hasher, path.as_bytes());
            hash_tree(hasher, &entry.path(), &format!("{path}/"))?;
        } else if kind.is_file() {
            hash_field(hasher, b"file");
            hash_field(hasher, path.as_bytes());
            hash_field(hasher, &fs::read(entry.path())?);
        } else {
            eyre::bail!("prepared chart holds a link: {}", entry.path().display());
        }
    }
    hash_field(hasher, b"end");
    Ok(())
}

/// Length-prefixes `bytes` so that no two field sequences share an encoding.
fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}
