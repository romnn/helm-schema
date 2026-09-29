//! Executes Helm under a controlled environment and replays exact prior executions.
//!
//! Helm runs either as the pinned CLI, one child per invocation, or inside
//! resident `helmsweep serve` processes (`tools/helmsweep`), one per
//! concurrent render, which answer each `helm template` with exactly the
//! CLI's stdout, stderr and exit code from Helm's own code. `SCHEMA_HELM_ENGINE=cli` selects the CLI; the
//! resident engine is the default.
//!
//! An execution is identified by everything that can change what Helm prints:
//! the engine, its program's bytes and version, the platform, the complete child
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
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use test_util::scratch::ScratchDir;
use wait4::Wait4 as _;

/// Names a persistent Helm invocation store.
const INVOCATION_CACHE_VAR: &str = "SCHEMA_HELM_INVOCATION_CACHE";
/// Selects the Helm engine: `helmsweep` (the default) or `cli`.
const ENGINE_VAR: &str = "SCHEMA_HELM_ENGINE";
/// Names the helmsweep program instead of the one in the target directory.
const HELMSWEEP_VAR: &str = "HELM_SCHEMA_HELMSWEEP";

/// The pinned Helm release every adjudication runs.
const PINNED_HELM_VERSION: &str = "v4.2.3";

/// Domain of invocation keys and the stored entry format. Changing what an
/// entry holds or how a request is encoded must change this.
const INVOCATION_FORMAT: &str = "helm-schema/helm-invocation/v3";

/// The preparation policy a prepared tree identity covers. Changing what
/// preparation removes, adds or repacks must change this.
const PREPARATION_POLICY: &str = "helm-schema/prepared-chart/v1";

/// The release name every adjudication renders.
const RELEASE_NAME: &str = "adjudication";

/// The engine names a key binds.
const CLI_ENGINE: &str = "helm-cli";
const RESIDENT_ENGINE: &str = "helmsweep";

/// The lines of `helmsweep version` naming the Helm it is built from and the
/// release build information templates see (`helm version` of the pinned
/// v4.2.3 release binary).
const HELMSWEEP_HELM_LINES: [&str; 2] = [
    "helm.sh/helm/v4 v4.2.3 => ./third_party/helm-v4.2.3",
    "helm-build v4.2.3 43e8b7feece8beb0fcba47059ec9b522fd929a64 clean go1.26.5",
];

/// How long a resident server may take to answer one render before it is
/// a harness failure.
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(600);

/// A resident server holding more than this after a render is retired, not
/// kept idle: idle servers stay small beside the pool's memory budget.
const RETIRE_ABOVE_BYTES: u64 = 128 << 20;

/// A chart tree at its content address, made only by [`HelmRunner::publish_tree`].
#[derive(Clone, Debug)]
pub struct PreparedTree {
    path: PathBuf,
    sha256: String,
}

impl PreparedTree {
    /// Where the tree lives, under the runner's root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The tree's content identity ([`tree_sha256`]).
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Whether an execution may be replayed from, and published to, the store.
///
/// Callers cannot declare a render replayable: a render's policy comes from
/// its chart's content ([`render_cacheability`](crate::helm::cache_policy::render_cacheability)),
/// and only this module tree marks its own fixed charts replayable.
/// [`Cacheability::bypass`] is always safe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cacheability(Replay);

/// The replay policy a [`Cacheability`] carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Replay {
    /// Identical inputs produce identical output.
    Cacheable,
    /// Replayable only as a client-only `helm template`, where `lookup`
    /// answers empty: nothing reaches a cluster.
    ClientOnly,
    /// Identical inputs need not produce identical output, for this reason.
    Bypass(String),
}

impl Cacheability {
    /// Never replayed, for `reason`.
    #[must_use]
    pub const fn bypass(reason: String) -> Self {
        Self(Replay::Bypass(reason))
    }

    /// The policy, for inspection.
    #[must_use]
    pub const fn replay(&self) -> &Replay {
        &self.0
    }

    /// A policy this crate decided from a chart's content or for a chart it
    /// wrote itself.
    pub(crate) const fn trusted(replay: Replay) -> Self {
        Self(replay)
    }
}

/// How the outputs of one invocation were obtained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Outcome {
    /// Run by Helm now.
    Executed,
    /// Replayed from the store entry of an earlier identical execution.
    Replayed,
    /// Run by Helm now, never stored, for this reason.
    Bypassed(String),
}

/// One completed Helm execution: Helm ran and exited with its own status.
pub struct HelmExecution {
    /// Helm's exit code: 0 on success, 1 when Helm reports an error.
    pub exit_code: i32,
    /// Helm's stdout.
    pub stdout: Vec<u8>,
    /// Helm's stderr.
    pub stderr: Vec<u8>,
    /// What the execution cost and how it was obtained.
    pub record: InvocationRecord,
}

impl HelmExecution {
    /// Whether Helm exited 0.
    #[must_use]
    pub const fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// What one invocation cost, retained beside its outputs.
#[derive(Clone, Debug, Serialize)]
pub struct InvocationRecord {
    /// The caller's label for the execution, such as `render` or `coalesce`.
    pub stage: String,
    /// The invocation key ([`InvocationRequest::key`]).
    pub key: String,
    /// How the outputs were obtained.
    pub outcome: Outcome,
    /// Helm's exit code.
    pub exit_code: i32,
    /// Duration of the original execution, also for a replay.
    pub elapsed_ms: u64,
    /// Time spent finding and verifying a stored entry.
    pub lookup_ms: u64,
    /// Peak memory of the original execution alone: a CLI child's own
    /// `wait4` peak, or the memory a resident server held while it rendered.
    pub max_rss_bytes: u64,
    /// Lifetime peak of the resident server that rendered, 0 for the CLI
    /// and replays: telemetry of the server, not of this render.
    pub server_max_rss_bytes: u64,
    /// Size of the values file.
    pub input_bytes: u64,
    /// Size of Helm's stdout.
    pub stdout_bytes: u64,
    /// Size of Helm's stderr.
    pub stderr_bytes: u64,
}

/// Everything that identifies one Helm execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationRequest {
    /// The key and entry format, such as `helm-schema/helm-invocation/v3`.
    pub format: String,
    /// `<os>-<arch>`.
    pub platform: String,
    /// `helm-cli` or `helmsweep` (the resident server).
    pub engine: String,
    /// The engine program's bytes and its reported version: Helm's for the
    /// CLI, the complete `helmsweep version` (build id, Go, Helm, jsonschema
    /// and patch) for the resident server.
    pub program_sha256: String,
    /// The engine program's reported version.
    pub program_version: String,
    /// The child's working directory, the runner's root.
    pub working_directory: String,
    /// The child's complete environment, sorted by name.
    pub environment: Vec<(String, String)>,
    /// The child's arguments.
    pub arguments: Vec<String>,
    /// The content identity of each path an argument or the environment
    /// names, in argument order, then Helm's home directory.
    pub inputs: Vec<(String, String)>,
    /// Whether the invocation is a client-only `helm template`: no server
    /// dry run, no validation against a cluster, and no kubeconfig.
    pub client_only: bool,
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
    ///
    /// # Errors
    ///
    /// Returns an error when the request cannot be encoded.
    pub fn key(&self) -> eyre::Result<String> {
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
pub struct TemplateRequest<'a> {
    /// The chart Helm renders.
    pub chart: &'a PreparedTree,
    /// The exact bytes Helm reads through `-f`.
    pub values: &'a [u8],
    /// Helm's `--kube-version`.
    pub kubernetes_version: &'a str,
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

/// The pinned Helm engine and the root its inputs and entries live under.
pub struct HelmRunner {
    program: PathBuf,
    program_sha256: String,
    program_version: String,
    /// The program's stamp when its bytes were hashed.
    program_stamp: FileStamp,
    root: PathBuf,
    /// Whether entries are replayed and published.
    replay: bool,
    /// The resident servers, or `None` for the CLI engine.
    resident: Option<ResidentPool>,
}

static SHARED_RUNNER: OnceLock<Result<HelmRunner, String>> = OnceLock::new();

impl HelmRunner {
    /// The runner of this process, with the Helm version checked once.
    /// `SCHEMA_HELM_INVOCATION_CACHE` names a persistent store; without it
    /// a private temporary root is used and nothing is replayed.
    ///
    /// # Errors
    ///
    /// Returns the error [`HelmRunner::new`] returned for this process.
    pub fn shared() -> eyre::Result<&'static Self> {
        SHARED_RUNNER
            .get_or_init(|| {
                let runner = match std::env::var_os(INVOCATION_CACHE_VAR) {
                    Some(root) => Self::new(&PathBuf::from(root).join("v2"), true),
                    None => {
                        ScratchDir::new("helm-root").and_then(|root| Self::new(&root.keep(), false))
                    }
                };
                runner.map_err(|error| format!("{error:?}"))
            })
            .as_ref()
            .map_err(|error| eyre::eyre!("{error}"))
    }

    /// A runner rooted at `root`, replaying stored entries when `replay`,
    /// on the engine `SCHEMA_HELM_ENGINE` names: `helmsweep` (the default)
    /// or `cli`.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown engine, a missing or unpinned engine
    /// program, or a `root` that cannot be created.
    pub fn new(root: &Path, replay: bool) -> eyre::Result<Self> {
        match std::env::var(ENGINE_VAR).as_deref() {
            Ok("cli") => Self::with_program(root, replay, find_helm()?),
            Ok("helmsweep") | Err(std::env::VarError::NotPresent) => {
                Self::with_helmsweep(root, replay, find_helmsweep()?, RENDER_TIMEOUT)
            }
            other => eyre::bail!("{ENGINE_VAR} must be cli or helmsweep, not {other:?}"),
        }
    }

    /// A runner executing `program` as Helm.
    ///
    /// # Errors
    ///
    /// Returns an error when `program` is not the pinned Helm release or
    /// `root` cannot be created.
    pub fn with_program(root: &Path, replay: bool, program: PathBuf) -> eyre::Result<Self> {
        let runner = Self::unstarted(root, replay, program)?;
        let version = runner.program_output(&["version", "--template", "{{.Version}}"])?;
        eyre::ensure!(
            version == PINNED_HELM_VERSION,
            "adjudication requires Helm {PINNED_HELM_VERSION}, not {version:?}"
        );
        Ok(Self {
            program_version: version,
            ..runner
        })
    }

    /// A runner rendering in resident `helmsweep serve` processes started
    /// from `program`, under the environment a CLI child gets, each render
    /// answered within `timeout`.
    ///
    /// # Errors
    ///
    /// Returns an error when `program` is not built from the pinned Helm or
    /// the server cannot be started.
    pub fn with_helmsweep(
        root: &Path,
        replay: bool,
        program: PathBuf,
        timeout: Duration,
    ) -> eyre::Result<Self> {
        let runner = Self::unstarted(root, replay, program)?;
        let version = runner.program_output(&["version"])?;
        eyre::ensure!(
            HELMSWEEP_HELM_LINES
                .iter()
                .all(|wanted| version.lines().any(|line| line == *wanted)),
            "helmsweep {} is not built from Helm {PINNED_HELM_VERSION}: {version:?}",
            runner.program.display()
        );
        Ok(Self {
            program_version: version,
            resident: Some(ResidentPool {
                idle: Mutex::new(Vec::new()),
                timeout,
            }),
            ..runner
        })
    }

    fn unstarted(root: &Path, replay: bool, program: PathBuf) -> eyre::Result<Self> {
        fs::create_dir_all(root.join("home"))?;
        fs::create_dir_all(root.join("tmp"))?;
        let root = root.canonicalize()?;
        let program_stamp = FileStamp::of(&program)?;
        let program_sha256 = hex(&Sha256::digest(fs::read(&program)?));
        Ok(Self {
            program,
            program_sha256,
            program_version: String::new(),
            program_stamp,
            root,
            replay,
            resident: None,
        })
    }

    /// The engine program's stdout for `arguments` under the child environment.
    fn program_output(&self, arguments: &[&str]) -> eyre::Result<String> {
        let output = Command::new(&self.program)
            .env_clear()
            .envs(self.environment()?)
            .current_dir(&self.root)
            .args(arguments)
            .output()
            .wrap_err_with(|| format!("run {} {arguments:?}", self.program.display()))?;
        eyre::ensure!(
            output.status.success(),
            "{} {arguments:?} failed: {}",
            self.program.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    }

    /// A fresh directory on the root's filesystem for building a tree.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created.
    pub fn staging_dir(&self) -> eyre::Result<PathBuf> {
        let staging = self.root.join("staging");
        fs::create_dir_all(&staging)?;
        Ok(tempfile::Builder::new()
            .prefix("tree-")
            .tempdir_in(staging)?
            .keep())
    }

    /// Moves the tree built at `staged` to its content address, or discards
    /// it when that address already holds the same content.
    ///
    /// # Errors
    ///
    /// Returns an error when the tree cannot be hashed or moved.
    pub fn publish_tree(&self, staged: &Path) -> eyre::Result<PreparedTree> {
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
    ///
    /// # Errors
    ///
    /// Returns an error for such a harness failure or when the store or
    /// case cannot be written.
    pub fn template(
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
        let call = TemplateCall {
            chart: chart.to_string(),
            kubernetes_version: request.kubernetes_version.to_string(),
            values: values.to_string(),
        };
        let arguments = call.arguments();
        let engine = if self.resident.is_some() {
            RESIDENT_ENGINE
        } else {
            CLI_ENGINE
        };
        let invocation = InvocationRequest {
            format: INVOCATION_FORMAT.to_string(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            engine: engine.to_string(),
            program_sha256: self.program_sha256.clone(),
            program_version: self.program_version.clone(),
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
            &call,
            u64::try_from(request.values.len())?,
            case,
            stage,
            cacheability,
        )
    }

    fn run(
        &self,
        invocation: &InvocationRequest,
        call: &TemplateCall,
        input_bytes: u64,
        case: &Path,
        stage: &str,
        cacheability: &Cacheability,
    ) -> eyre::Result<HelmExecution> {
        // The stage's inputs are recorded before anything runs, so a failure's
        // preserved bundle always carries its chart and values; a completed
        // stage adds its measurements.
        let record_path = case.join(format!("{stage}{INVOCATION_RECORD_SUFFIX}"));
        let mut record_json = serde_json::json!({
            "chart": call.chart,
            "values": call.values,
            "kubernetes_version": call.kubernetes_version,
            "request": invocation,
        });
        fs::write(&record_path, serde_json::to_vec_pretty(&record_json)?)?;
        self.verify_program()?;
        let request = invocation.encode()?;
        let key = invocation.key()?;
        let entry = self.entry_dir(&key);
        let lookup = Instant::now();
        let lookup_outside =
            Replay::Bypass("calls lookup outside a client-only template".to_string());
        let cacheability = match cacheability.replay() {
            Replay::ClientOnly if !invocation.client_only => &lookup_outside,
            other => other,
        };
        let replayable = self.replay && !matches!(cacheability, Replay::Bypass(_));
        let stored = if replayable {
            read_entry(&entry, &request)?
        } else {
            None
        };
        let lookup_ms = elapsed_ms(lookup);
        let (result, stdout, stderr, outcome, server_max_rss_bytes) =
            if let Some((result, stdout, stderr)) = stored {
                fs::write(case.join(format!("{stage}.yaml")), &stdout)?;
                fs::write(case.join(format!("{stage}.stderr")), &stderr)?;
                (result, stdout, stderr, Outcome::Replayed, 0)
            } else {
                let (result, stdout, stderr, server_max_rss_bytes) =
                    if let Some(resident) = &self.resident {
                        resident.execute(self, call, case, stage)?
                    } else {
                        let (result, stdout, stderr) = self.execute(invocation, case, stage)?;
                        (result, stdout, stderr, 0)
                    };
                if replayable {
                    publish_entry(&entry, &request, &result, &stdout, &stderr)?;
                }
                let outcome = match cacheability {
                    Replay::Cacheable | Replay::ClientOnly => Outcome::Executed,
                    Replay::Bypass(reason) => Outcome::Bypassed(reason.clone()),
                };
                (result, stdout, stderr, outcome, server_max_rss_bytes)
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
            server_max_rss_bytes,
            input_bytes,
            stdout_bytes: u64::try_from(stdout.len())?,
            stderr_bytes: u64::try_from(stderr.len())?,
        };
        if let Some(fields) = record_json.as_object_mut() {
            fields.insert("record".to_string(), serde_json::to_value(&record)?);
            fields.insert("entry".to_string(), serde_json::to_value(&entry)?);
        }
        fs::write(&record_path, serde_json::to_vec_pretty(&record_json)?)?;
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
            .wrap_err_with(|| {
                format!(
                    "start Helm {stage}; evidence={}",
                    preserve_failure(case).display()
                )
            })?;
        let usage = child.wait4().wrap_err_with(|| {
            format!(
                "wait for Helm {stage}; evidence={}",
                preserve_failure(case).display()
            )
        })?;
        let elapsed_ms = elapsed_ms(started);
        let Some(exit_code @ (0 | 1)) = usage.status.code() else {
            eyre::bail!(
                "Helm {stage} ended abnormally ({}); evidence={}",
                usage.status,
                preserve_failure(case).display()
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
            hex(&Sha256::digest(fs::read(&self.program)?)) == self.program_sha256,
            "Helm executable {} changed during the run",
            self.program.display()
        );
        Ok(())
    }

    /// The child environment: nothing inherited, Helm's directories and the
    /// Go temporary directory (`TMPDIR` on Unix, `TMP`/`TEMP` on Windows)
    /// inside the root.
    fn environment(&self) -> eyre::Result<Vec<(String, String)>> {
        let home = self.root.join("home");
        let home = utf8(&home)?;
        let tmp = self.root.join("tmp");
        let mut environment = vec![
            ("HELM_CACHE_HOME".to_string(), format!("{home}/cache")),
            ("HELM_CONFIG_HOME".to_string(), format!("{home}/config")),
            ("HELM_DATA_HOME".to_string(), format!("{home}/data")),
            ("HOME".to_string(), home.to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("TEMP".to_string(), utf8(&tmp)?.to_string()),
            ("TMP".to_string(), utf8(&tmp)?.to_string()),
            ("TMPDIR".to_string(), utf8(&tmp)?.to_string()),
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

/// One `helm template` as both engines run it.
struct TemplateCall {
    chart: String,
    kubernetes_version: String,
    values: String,
}

impl TemplateCall {
    /// The CLI's arguments, which the key binds for either engine.
    fn arguments(&self) -> Vec<String> {
        [
            "template",
            RELEASE_NAME,
            &self.chart,
            "--kube-version",
            &self.kubernetes_version,
            "--skip-schema-validation",
            "-f",
            &self.values,
        ]
        .map(str::to_string)
        .to_vec()
    }
}

/// One line of `helmsweep serve`'s answers (tools/helmsweep/serve.go).
#[derive(Debug, Deserialize)]
struct ServeAnswer {
    id: Option<u64>,
    exit_code: Option<i32>,
    peak_bytes: u64,
    held_bytes: u64,
    max_rss_bytes: u64,
    error: Option<String>,
}

/// The resident `helmsweep serve` processes. Each serves one render at a
/// time, so every log record of a render is that render's own; a caller
/// takes an idle server or starts one, so there are no more servers than
/// concurrent renders. A server that fails, answers late or holds more than
/// `RETIRE_ABOVE_BYTES` after a render is terminated and reaped, never kept.
struct ResidentPool {
    idle: Mutex<Vec<Resident>>,
    timeout: Duration,
}

/// One `helmsweep serve` process: a request line out, its answer line back
/// through a reader thread. Dropping it kills and reaps the process.
struct Resident {
    child: Child,
    stdin: ChildStdin,
    answers: mpsc::Receiver<std::io::Result<String>>,
    next_id: u64,
}

impl ResidentPool {
    /// Renders `call` in an idle or new server, which writes stdout and
    /// stderr where the CLI engine's child would; also returns the server's
    /// lifetime peak. Anything but the answer to this request within the
    /// timeout (the server ending, a malformed line, another id, an error, a
    /// status Helm never uses) is a harness failure.
    fn execute(
        &self,
        runner: &HelmRunner,
        call: &TemplateCall,
        case: &Path,
        stage: &str,
    ) -> eyre::Result<(StoredResult, Vec<u8>, Vec<u8>, u64)> {
        let idle = lock(&self.idle).pop();
        let mut resident = match idle {
            Some(resident) => resident,
            None => Resident::start(runner)?,
        };
        let stdout_path = case.join(format!("{stage}.yaml"));
        let stderr_path = case.join(format!("{stage}.stderr"));
        let started = Instant::now();
        let answer = resident
            .render(call, &stdout_path, &stderr_path, self.timeout)
            .wrap_err_with(|| {
                format!(
                    "helmsweep serve failed; evidence={}",
                    preserve_failure(case).display()
                )
            })?;
        let elapsed_ms = elapsed_ms(started);
        if let Some(error) = answer.error {
            eyre::bail!(
                "Helm {stage} ended abnormally in helmsweep ({error}); evidence={}",
                preserve_failure(case).display()
            );
        }
        let Some(exit_code @ (0 | 1)) = answer.exit_code else {
            eyre::bail!(
                "helmsweep answered exit code {:?}; evidence={}",
                answer.exit_code,
                preserve_failure(case).display()
            );
        };
        if answer.held_bytes <= RETIRE_ABOVE_BYTES {
            lock(&self.idle).push(resident);
        }
        let stdout = fs::read(&stdout_path)?;
        let stderr = fs::read(&stderr_path)?;
        let result = StoredResult {
            exit_code,
            stdout_sha256: hex(&Sha256::digest(&stdout)),
            stderr_sha256: hex(&Sha256::digest(&stderr)),
            elapsed_ms,
            max_rss_bytes: answer.peak_bytes,
        };
        Ok((result, stdout, stderr, answer.max_rss_bytes))
    }
}

impl Resident {
    fn start(runner: &HelmRunner) -> eyre::Result<Self> {
        let mut child = Command::new(&runner.program)
            .env_clear()
            .envs(runner.environment()?)
            .current_dir(&runner.root)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .wrap_err_with(|| format!("start {} serve", runner.program.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_eyre("helmsweep serve has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_eyre("helmsweep serve has no stdout")?;
        let (sender, answers) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            answers,
            next_id: 0,
        })
    }

    fn render(
        &mut self,
        call: &TemplateCall,
        stdout: &Path,
        stderr: &Path,
        timeout: Duration,
    ) -> eyre::Result<ServeAnswer> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({
            "id": id,
            "op": "template",
            "release": RELEASE_NAME,
            "chart": call.chart,
            "kube_version": call.kubernetes_version,
            "values": [call.values],
            "skip_schema_validation": true,
            "stdout_path": utf8(stdout)?,
            "stderr_path": utf8(stderr)?,
        });
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        self.stdin.write_all(&line)?;
        self.stdin.flush()?;
        let answer = match self.answers.recv_timeout(timeout) {
            Ok(answer) => answer?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eyre::bail!("no answer within {timeout:?}")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                eyre::bail!("the server ended without answering")
            }
        };
        let answer: ServeAnswer = serde_json::from_str(&answer)
            .wrap_err_with(|| format!("malformed answer {answer:?}"))?;
        eyre::ensure!(
            answer.id == Some(id),
            "answer {answer:?} is not to request {id}"
        );
        Ok(answer)
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        // Bounded for a server in any state: an idle one would also end at
        // EOF, a failed one may never.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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

/// `HELM_SCHEMA_HELMSWEEP`, else `helmsweep` in the target directory this
/// test binary was built into (`task build:helmsweep` puts it there).
///
/// # Errors
///
/// Returns an error when neither names a helmsweep program.
pub fn find_helmsweep() -> eyre::Result<PathBuf> {
    if let Some(path) = std::env::var_os(HELMSWEEP_VAR) {
        return Ok(PathBuf::from(path).canonicalize()?);
    }
    let executable = std::env::current_exe()?;
    // <target>/<profile>/deps/<test binary>
    let target = executable
        .ancestors()
        .nth(3)
        .ok_or_eyre("test binary is not inside a target directory")?;
    let program = target.join(format!("helmsweep{}", std::env::consts::EXE_SUFFIX));
    eyre::ensure!(
        program.is_file(),
        "no helmsweep at {}: run `task build:helmsweep`, set {HELMSWEEP_VAR}, \
         or {ENGINE_VAR}=cli",
        program.display()
    );
    Ok(program.canonicalize()?)
}

/// The first `helm` on `PATH`, canonicalized.
///
/// # Errors
///
/// Returns an error when `PATH` names no `helm`.
pub fn find_helm() -> eyre::Result<PathBuf> {
    let path = std::env::var_os("PATH").ok_or_eyre("PATH is not set")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(format!("helm{}", std::env::consts::EXE_SUFFIX));
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
pub struct StageTotals {
    /// Invocations of the stage.
    pub invocations: usize,
    /// Of those, executed now.
    pub executed: usize,
    /// Of those, replayed from the store.
    pub replayed: usize,
    /// Of those, executed without replay.
    pub bypassed: usize,
    /// Of those, with a nonzero exit code.
    pub failures: usize,
    /// Duration of the original executions, replayed ones included.
    pub elapsed_ms: u64,
    /// Duration of the executions this run actually performed.
    pub executed_ms: u64,
    /// Time spent finding and verifying stored entries.
    pub lookup_ms: u64,
    /// The longest original execution.
    pub max_elapsed_ms: u64,
    /// The largest peak memory of an execution.
    pub max_rss_bytes: u64,
    /// Total size of the values files.
    pub input_bytes: u64,
    /// Total size of Helm's stdout.
    pub stdout_bytes: u64,
}

/// Sums `records` per stage.
#[must_use]
pub fn stage_totals(records: &[InvocationRecord]) -> BTreeMap<String, StageTotals> {
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
pub fn tree_sha256(root: &Path) -> eyre::Result<String> {
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

/// Names the record [`HelmRunner::template`] writes into a case before it
/// runs a stage: the stage's chart tree, values file and Kubernetes version.
const INVOCATION_RECORD_SUFFIX: &str = ".invocation.json";

/// Copies a failure's evidence directory out of the sweep's reach into
/// `<target>/evidence/` and returns the path to report. Every stage the case
/// ran brings its chart tree (`charts/<stage>`) and values
/// (`inputs/<stage>.values.json`), and the copied invocation records and the
/// chart's `prepared.json` name those copies by bundle-relative paths, so
/// the bundle reproduces the case alone. A directory that cannot be copied
/// is reported where it is.
#[must_use]
pub fn preserve_failure(case: &Path) -> PathBuf {
    match preserve_bundle(case) {
        Ok(preserved) => preserved,
        Err(error) => {
            eprintln!("evidence {} stays in scratch: {error:#}", case.display());
            case.to_path_buf()
        }
    }
}

fn preserve_bundle(case: &Path) -> eyre::Result<PathBuf> {
    let preserved = test_util::scratch::preserve(case)?;
    // Original path -> bundle-relative copy.
    let mut copies: BTreeMap<String, String> = BTreeMap::new();
    for entry in fs::read_dir(&preserved)? {
        let record_path = entry?.path();
        let Some(stage) = record_path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(INVOCATION_RECORD_SUFFIX))
            .map(str::to_string)
        else {
            continue;
        };
        let mut record: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&record_path)?)?;
        for (field, relative) in [
            ("chart", format!("charts/{stage}")),
            ("values", format!("inputs/{stage}.values.json")),
        ] {
            let original = record
                .get(field)
                .and_then(serde_json::Value::as_str)
                .ok_or_eyre("invocation record lacks a path")?
                .to_string();
            let copy = preserved.join(&relative);
            if field == "chart" {
                test_util::scratch::copy_tree(Path::new(&original), &copy)?;
            } else {
                fs::create_dir_all(preserved.join("inputs"))?;
                fs::copy(&original, &copy)?;
            }
            record.insert(
                field.to_string(),
                serde_json::Value::String(relative.clone()),
            );
            copies.insert(original, relative);
        }
        fs::write(&record_path, serde_json::to_vec_pretty(&record)?)?;
    }
    // A probe's chart evidence names the same trees; point it at the copies.
    if let Some(prepared) = case
        .parent()
        .map(|parent| parent.join("prepared.json"))
        .filter(|prepared| prepared.is_file())
    {
        let mut fields: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&prepared)?)?;
        for value in fields.values_mut() {
            if let Some(copy) = value.as_str().and_then(|original| copies.get(original)) {
                *value = serde_json::Value::String(copy.clone());
            }
        }
        fs::write(
            preserved.join("prepared.json"),
            serde_json::to_vec_pretty(&fields)?,
        )?;
    }
    Ok(preserved)
}
