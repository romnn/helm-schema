//! Running the real Helm on a copy of a chart whose root `values.schema.json`
//! has short `$defs` keys.
//!
//! Helm refuses chart files over 5 MiB, and a readable schema can exceed that.
//! [`run`] copies the chart to a scratch directory, replaces only the root
//! schema by its compact, shortened form, runs Helm on the copy and relays
//! Helm's output with each short key translated back to its readable name.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use helm_schema_json_schema_minify::{
    ShortenedSchema, expand_short_definition_names, shorten_definition_names,
};
use serde_json::Value;

const SCHEMA_FILE: &str = "values.schema.json";

/// The Helm subcommand to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelmCommand {
    /// `helm lint`.
    Lint,
    /// `helm template`.
    Template,
}

/// One Helm invocation on a chart directory.
#[derive(Debug, Clone)]
pub struct HelmRunOptions {
    /// The Helm subcommand.
    pub command: HelmCommand,
    /// The Helm executable: a path, or a name looked up on `PATH`.
    pub helm: PathBuf,
    /// The chart directory, as the caller spelled it.
    pub chart: PathBuf,
    /// Helm arguments after the chart, passed verbatim.
    pub args: Vec<OsString>,
    /// The directory that holds the scratch copy of the chart.
    pub scratch_root: PathBuf,
}

/// Why Helm could not be run on the shortened copy, or ended early.
#[derive(Debug, thiserror::Error)]
pub enum HelmRunError {
    /// The root schema could not be read.
    #[error("failed to read schema {path}")]
    ReadSchema {
        /// The root schema.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: io::Error,
    },

    /// The root schema is not JSON.
    #[error("failed to parse schema {path}")]
    ParseSchema {
        /// The root schema.
        path: PathBuf,
        /// Underlying parse failure.
        #[source]
        source: serde_json::Error,
    },

    /// The scratch directory could not be created.
    #[error("failed to create a scratch directory in {path}")]
    CreateScratch {
        /// The scratch root.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: io::Error,
    },

    /// A chart entry could not be copied; a dangling link fails here.
    #[error("failed to copy {path}")]
    Copy {
        /// The chart entry.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: io::Error,
    },

    /// A directory link points at one of its own parents.
    #[error("{path} links to a directory that contains it")]
    LinkCycle {
        /// The link.
        path: PathBuf,
    },

    /// A chart entry is neither a file nor a directory.
    #[error("{path} is neither a file nor a directory")]
    SpecialFile {
        /// The chart entry.
        path: PathBuf,
    },

    /// The scratch directory lies inside the chart being copied.
    #[error("the scratch directory {path} is inside the chart")]
    ScratchInsideChart {
        /// The scratch directory.
        path: PathBuf,
    },

    /// The shortened schema could not be written.
    #[error("failed to write the shortened schema {path}")]
    WriteSchema {
        /// The copy's root schema.
        path: PathBuf,
        /// Underlying failure.
        #[source]
        source: io::Error,
    },

    /// Helm could not be started.
    #[error("failed to run {helm}")]
    Spawn {
        /// The Helm executable.
        helm: PathBuf,
        /// Underlying failure.
        #[source]
        source: io::Error,
    },

    /// Helm's output could not be read or written.
    #[error("failed to relay Helm's output")]
    Relay(#[source] io::Error),

    /// Helm could not be stopped or reaped.
    #[error("failed to wait for Helm")]
    Wait(#[source] io::Error),

    /// A signal stopped the run; Helm was stopped and the copy removed.
    #[error("interrupted by signal {signal}")]
    Interrupted {
        /// The signal number.
        signal: usize,
    },
}

/// Which of Helm's output streams a line came from.
#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

/// Runs `helm <command> <chart copy> <args…>` and returns Helm's status.
///
/// Without a root `values.schema.json`, Helm runs on `options.chart` itself.
/// Otherwise the whole chart tree is copied under `options.scratch_root`,
/// with links materialized, and only the copy's root schema is replaced by
/// its compact form with short `$defs` keys. Helm keeps the caller's working
/// directory, environment and standard input. Each line Helm writes goes to
/// the same stream here. Diagnostics (both streams of `lint`, standard error
/// of `template`) have complete `#/$defs/<short key>` tokens translated; the
/// manifests `template` writes to standard output stay byte for byte. A short
/// key is looked up in the root schema's map only, so a dependency schema's
/// own `#/$defs/<key>` with the same spelling is translated too.
///
/// `interrupt` holds the number of a signal that should stop the run, or 0;
/// the caller owns signal registration. It is checked while the chart is
/// copied and while Helm runs, however slowly the caller reads Helm's output:
/// once it is set, Helm is stopped and reaped and the copy removed. The
/// relay thread may then stay blocked writing to a stdout or stderr no one
/// reads, holding that stream's lock; a caller that keeps running after an
/// interrupt keeps that thread.
///
/// # Errors
///
/// Returns an error if the schema cannot be read or parsed, the chart cannot
/// be copied, Helm cannot be started or its output relayed, or `interrupt`
/// was set ([`HelmRunError::Interrupted`]).
pub fn run(options: &HelmRunOptions, interrupt: &AtomicUsize) -> Result<ExitStatus, HelmRunError> {
    let schema_path = options.chart.join(SCHEMA_FILE);
    if !schema_path.is_file() {
        return run_helm(options, &options.chart, &BTreeMap::new(), interrupt);
    }
    let bytes = std::fs::read(&schema_path).map_err(|source| HelmRunError::ReadSchema {
        path: schema_path.clone(),
        source,
    })?;
    let schema: Value =
        serde_json::from_slice(&bytes).map_err(|source| HelmRunError::ParseSchema {
            path: schema_path.clone(),
            source,
        })?;
    let shortened = shorten_definition_names(&schema);

    let scratch = tempfile::Builder::new()
        .prefix("helm-schema-")
        .tempdir_in(&options.scratch_root)
        .map_err(|source| HelmRunError::CreateScratch {
            path: options.scratch_root.clone(),
            source,
        })?;
    let outcome = run_on_copy(options, &shortened, scratch.path(), interrupt);
    let scratch_path = scratch.path().to_path_buf();
    if let Err(error) = scratch.close() {
        // Helm's status, or the run's own error, stays the outcome. The
        // report is best effort: stderr may be closed, or blocked and locked
        // by a relay left behind, so it is written on a thread of its own
        // that only an interrupt stops waiting for.
        let report = format!(
            "warning: failed to remove the scratch directory {}: {error}\n",
            scratch_path.display()
        );
        let reporting = thread::spawn(move || {
            let _ = io::stderr().write_all(report.as_bytes());
        });
        while !reporting.is_finished() && interrupt.load(Ordering::SeqCst) == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }
    outcome
}

/// Copies the chart into `scratch`, writes the shortened schema there and
/// runs Helm on the copy.
fn run_on_copy(
    options: &HelmRunOptions,
    shortened: &ShortenedSchema,
    scratch: &Path,
    interrupt: &AtomicUsize,
) -> Result<ExitStatus, HelmRunError> {
    let scratch = std::fs::canonicalize(scratch).map_err(|source| copy_error(scratch, source))?;
    let chart = std::fs::canonicalize(&options.chart)
        .map_err(|source| copy_error(&options.chart, source))?;
    // Keep the directory name, which Helm's messages show.
    let copy = scratch.join(chart.file_name().unwrap_or(OsStr::new("chart")));
    // The root schema is written afresh, whatever the original's permissions.
    let skip = chart.join(SCHEMA_FILE);
    copy_tree(&chart, &copy, &scratch, &skip, interrupt, &mut Vec::new())?;
    let copy_schema = copy.join(SCHEMA_FILE);
    serde_json::to_vec(&shortened.schema)
        .map_err(io::Error::from)
        .and_then(|compact| std::fs::write(&copy_schema, compact))
        .map_err(|source| HelmRunError::WriteSchema {
            path: copy_schema,
            source,
        })?;
    run_helm(options, &copy, &shortened.readable_names, interrupt)
}

/// [`HelmRunError::Interrupted`] once `interrupt` holds a signal.
fn check_interrupt(interrupt: &AtomicUsize) -> Result<(), HelmRunError> {
    match interrupt.load(Ordering::SeqCst) {
        0 => Ok(()),
        signal => Err(HelmRunError::Interrupted { signal }),
    }
}

fn copy_error(path: &Path, source: io::Error) -> HelmRunError {
    HelmRunError::Copy {
        path: path.to_path_buf(),
        source,
    }
}

/// Copies directory `source` to `destination`, following links, except the
/// entry `skip`.
///
/// `ancestors` holds the canonical paths of the directories being copied, so
/// a link back to one of them is refused instead of copied forever.
fn copy_tree(
    source: &Path,
    destination: &Path,
    scratch: &Path,
    skip: &Path,
    interrupt: &AtomicUsize,
    ancestors: &mut Vec<PathBuf>,
) -> Result<(), HelmRunError> {
    let canonical = std::fs::canonicalize(source).map_err(|error| copy_error(source, error))?;
    if canonical.starts_with(scratch) {
        return Err(HelmRunError::ScratchInsideChart {
            path: scratch.to_path_buf(),
        });
    }
    if ancestors.contains(&canonical) {
        return Err(HelmRunError::LinkCycle {
            path: source.to_path_buf(),
        });
    }
    ancestors.push(canonical);
    std::fs::create_dir(destination).map_err(|error| copy_error(destination, error))?;
    for entry in std::fs::read_dir(source).map_err(|error| copy_error(source, error))? {
        check_interrupt(interrupt)?;
        let entry = entry.map_err(|error| copy_error(source, error))?;
        let from = entry.path();
        if from == skip {
            continue;
        }
        let to = destination.join(entry.file_name());
        let metadata = std::fs::metadata(&from).map_err(|error| copy_error(&from, error))?;
        if metadata.is_dir() {
            copy_tree(&from, &to, scratch, skip, interrupt, ancestors)?;
        } else if metadata.is_file() {
            std::fs::copy(&from, &to).map_err(|error| copy_error(&from, error))?;
        } else {
            return Err(HelmRunError::SpecialFile { path: from });
        }
    }
    ancestors.pop();
    Ok(())
}

/// Runs Helm on `chart`, relays its output until both streams close, and
/// reaps it.
fn run_helm(
    options: &HelmRunOptions,
    chart: &Path,
    readable_names: &BTreeMap<String, String>,
    interrupt: &AtomicUsize,
) -> Result<ExitStatus, HelmRunError> {
    let command = match options.command {
        HelmCommand::Lint => "lint",
        HelmCommand::Template => "template",
    };
    let mut child = Command::new(&options.helm)
        .arg(command)
        .arg(chart)
        .args(&options.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| HelmRunError::Spawn {
            helm: options.helm.clone(),
            source,
        })?;
    // `template` writes manifests to stdout, which stay byte for byte.
    let stdout_names = match options.command {
        HelmCommand::Lint => readable_names.clone(),
        HelmCommand::Template => BTreeMap::new(),
    };
    let relay = relay(
        child.stdout.take(),
        child.stderr.take(),
        stdout_names,
        readable_names.clone(),
    );
    // The relay's writes block while the caller does not read, so the
    // interrupt is watched here instead.
    while !relay.is_finished() && interrupt.load(Ordering::SeqCst) == 0 {
        thread::sleep(Duration::from_millis(20));
    }
    let relayed = match check_interrupt(interrupt) {
        Ok(()) => relay
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the relay thread panicked"))),
        // The relay may be blocked for good; it is left behind.
        Err(_) => Ok(()),
    };
    if relayed.is_err() {
        // Kill fails only when Helm has already ended; it is reaped either way.
        let _ = child.kill();
    }
    // Helm may keep running after closing its streams; the interrupt still
    // stops it.
    let status = loop {
        if check_interrupt(interrupt).is_err() {
            let _ = child.kill();
            break child.wait().map_err(HelmRunError::Wait)?;
        }
        if let Some(status) = child.try_wait().map_err(HelmRunError::Wait)? {
            break status;
        }
        thread::sleep(Duration::from_millis(20));
    };
    check_interrupt(interrupt)?;
    relayed.map_err(HelmRunError::Relay)?;
    Ok(status)
}

/// Relays Helm's two streams on a thread, each line to the same stream here
/// with its map's short keys translated, until both close or one fails.
fn relay(
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    stdout_names: BTreeMap<String, String>,
    stderr_names: BTreeMap<String, String>,
) -> JoinHandle<io::Result<()>> {
    let (lines, received) = sync_channel(64);
    read_lines(Stream::Out, stdout, lines.clone());
    read_lines(Stream::Err, stderr, lines);
    thread::spawn(move || {
        for line in received {
            match line? {
                (Stream::Out, line) => write_line(
                    &mut io::stdout().lock(),
                    &expand_short_definition_names(&line, &stdout_names),
                )?,
                (Stream::Err, line) => write_line(
                    &mut io::stderr().lock(),
                    &expand_short_definition_names(&line, &stderr_names),
                )?,
            }
        }
        Ok(())
    })
}

/// Sends each line of `pipe` on a thread of its own, then drops `lines`.
fn read_lines(
    stream: Stream,
    pipe: Option<impl Read + Send + 'static>,
    lines: SyncSender<io::Result<(Stream, Vec<u8>)>>,
) {
    let Some(pipe) = pipe else {
        return;
    };
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => return,
                Ok(_) => {
                    if lines.send(Ok((stream, line))).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    // The relay stops at the error; if it is gone, no one is left to tell.
                    let _ = lines.send(Err(error));
                    return;
                }
            }
        }
    });
}

fn write_line(out: &mut impl Write, line: &[u8]) -> io::Result<()> {
    out.write_all(line)?;
    out.flush()
}
