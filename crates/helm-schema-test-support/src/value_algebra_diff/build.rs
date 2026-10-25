//! The capture's own build of a revision's binaries, with its receipt.
//!
//! A capture never trusts a binary handed to it: it builds the producer, the
//! contract exporter, the CLI and the IR control runner from the checkout with
//! Cargo, keeps Cargo's artifact messages, and binds the full tracked-tree
//! digest (tests included) before and after the build, the compiler, the build
//! configuration and every binary's hash and dynamic libraries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::DiffError;
use super::history::write_new;
use crate::manifest::BuildProvenance;
use crate::source_digest::sha256_hex;

/// The binaries a capture builds, by their name in the capture's `bin/`.
pub const BINARIES: [&str; 4] = [
    "corpus_generation",
    "contract_export",
    "helm-schema",
    "protocol-runner",
];

/// Build configuration variables recorded when set.
const BUILD_ENVIRONMENT: [&str; 6] = [
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_TARGET",
    "CARGO_BUILD_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "CARGO_PROFILE_DEV_OPT_LEVEL",
];

/// One built binary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryRecord {
    /// SHA-256 of the copied executable.
    pub sha256: String,
    /// Cargo's executable path when built.
    pub cargo_path: String,
    /// Every dynamic library it links, all system libraries.
    pub linked: Vec<String>,
}

/// What a capture's build was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    /// [`super::history::FORMAT_VERSION`].
    pub format_version: u32,
    /// The Cargo commands, in order.
    pub commands: Vec<Vec<String>>,
    /// `rustc -vV`.
    pub rustc: String,
    /// `cargo -V`.
    pub cargo: String,
    /// The recorded build configuration variables.
    pub environment: BTreeMap<String, String>,
    /// [`tree_sha256`] of the checkout before the build.
    pub source_before_sha256: String,
    /// [`tree_sha256`] of the checkout after the build; must equal before.
    pub source_after_sha256: String,
    /// The generation provenance the binaries embed.
    pub provenance: BuildProvenance,
    /// SHA-256 of `cargo-artifacts.jsonl`, Cargo's artifact messages.
    pub artifacts_sha256: String,
    /// The built binaries by capture name.
    pub binaries: BTreeMap<String, BinaryRecord>,
}

fn io(path: &Path, error: &std::io::Error) -> DiffError {
    DiffError::Infrastructure(format!("{}: {error}", path.display()))
}

/// The digest of every tracked or untracked, not ignored, file of the
/// checkout: path and content, sorted, tests and fixtures included.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when git or a file read fails.
pub fn tree_sha256(checkout: &Path) -> Result<String, DiffError> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(checkout)
        .output()
        .map_err(|error| io(checkout, &error))?;
    if !output.status.success() {
        return Err(DiffError::Infrastructure(format!(
            "git ls-files in {} failed",
            checkout.display()
        )));
    }
    let names = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<BTreeSet<_>>();
    let mut index = Vec::new();
    for name in names {
        let path = checkout.join(&name);
        // A tracked file deleted in the work tree is part of the state too.
        let digest = match std::fs::read(&path) {
            Ok(bytes) => sha256_hex(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "deleted".to_string(),
            Err(error) => return Err(io(&path, &error)),
        };
        index.push(format!("{digest} {name}\n"));
    }
    Ok(sha256_hex(index.concat().as_bytes()))
}

/// The digest of every file under `dir`: relative path and content, sorted.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when a file cannot be read.
pub fn dir_sha256(dir: &Path) -> Result<String, DiffError> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).map_err(|error| io(&current, &error))? {
            let path = entry.map_err(|error| io(&current, &error))?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut index = Vec::new();
    for path in files {
        let bytes = std::fs::read(&path).map_err(|error| io(&path, &error))?;
        let relative = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy();
        index.push(format!("{} {relative}\n", sha256_hex(&bytes)));
    }
    Ok(sha256_hex(index.concat().as_bytes()))
}

fn run_text(program: &str, args: &[&str]) -> Result<String, DiffError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| DiffError::Infrastructure(format!("{program}: {error}")))?;
    if !output.status.success() {
        return Err(DiffError::Infrastructure(format!(
            "{program} {args:?} failed with {}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The dynamic libraries `binary` links; any outside the system locations
/// is refused, since the capture could not replay it.
fn linked_libraries(binary: &Path) -> Result<Vec<String>, DiffError> {
    let path = binary.to_string_lossy();
    let (listing, system): (String, &[&str]) = if cfg!(target_os = "macos") {
        (
            run_text("otool", &["-L", &path])?,
            &["/usr/lib/", "/System/Library/"],
        )
    } else {
        (
            run_text("ldd", &[&path])?,
            &["/lib", "/usr/lib", "linux-vdso", "/lib64"],
        )
    };
    let mut linked = Vec::new();
    for line in listing.lines().skip(usize::from(cfg!(target_os = "macos"))) {
        let Some(library) = line.split_whitespace().find(|token| token.starts_with('/')) else {
            continue;
        };
        if !system.iter().any(|prefix| library.starts_with(prefix)) {
            return Err(DiffError::Identity(format!(
                "{} links the non-system library {library}",
                binary.display()
            )));
        }
        linked.push(library.to_string());
    }
    linked.sort();
    Ok(linked)
}

/// Builds every capture binary from `checkout` into `target_dir`, copies
/// them into `bin_dir`, writes `build.json` and `cargo-artifacts.jsonl` there
/// and returns the receipt.
///
/// # Errors
///
/// [`DiffError::Identity`] when the tree changed during the build, a binary
/// links a non-system library or embeds another provenance;
/// [`DiffError::Infrastructure`] when Cargo fails or an artifact is missing.
pub fn build(
    checkout: &Path,
    target_dir: &Path,
    bin_dir: &Path,
    log: &Path,
) -> Result<BuildReceipt, DiffError> {
    let source_before_sha256 = tree_sha256(checkout)?;
    let commands = vec![
        [
            "cargo",
            "build",
            "--locked",
            "--message-format=json",
            "-p",
            "helm-schema-test-support",
            "-p",
            "helm-schema-cli",
            "--bin",
            "corpus_generation",
            "--bin",
            "contract_export",
            "--bin",
            "helm-schema",
        ]
        .map(str::to_string)
        .to_vec(),
        [
            "cargo",
            "test",
            "--locked",
            "--message-format=json",
            "-p",
            "helm-schema-ir",
            "--lib",
            "--no-run",
        ]
        .map(str::to_string)
        .to_vec(),
    ];
    let messages = run_cargo(&commands, checkout, target_dir, log)?;
    let source_after_sha256 = tree_sha256(checkout)?;
    if source_after_sha256 != source_before_sha256 {
        return Err(DiffError::Identity(format!(
            "{} changed during the build",
            checkout.display()
        )));
    }
    let binaries = copy_binaries(&executables(&messages)?, bin_dir)?;
    let mut artifacts = Vec::new();
    for message in &messages {
        artifacts.extend_from_slice(message);
        artifacts.push(b'\n');
    }
    write_new(&bin_dir.join("cargo-artifacts.jsonl"), &artifacts)?;
    let environment = BUILD_ENVIRONMENT
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_string(), value))
        })
        .collect();
    let receipt = BuildReceipt {
        format_version: super::history::FORMAT_VERSION,
        commands,
        rustc: run_text("rustc", &["-vV"])?,
        cargo: run_text("cargo", &["-V"])?,
        environment,
        source_before_sha256,
        source_after_sha256,
        provenance: BuildProvenance {
            source_sha256: crate::source_digest::generation_source_digest(checkout)
                .map_err(|error| io(checkout, &error))?,
            target: env!("HELM_SCHEMA_BUILD_TARGET").to_string(),
            profile: "debug".to_string(),
        },
        artifacts_sha256: sha256_hex(&artifacts),
        binaries,
    };
    let bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&bin_dir.join("build.json"), &bytes)?;
    Ok(receipt)
}

/// Runs each Cargo command in `checkout` with `target_dir`, logging its
/// stderr; returns every JSON message line.
fn run_cargo(
    commands: &[Vec<String>],
    checkout: &Path,
    target_dir: &Path,
    log: &Path,
) -> Result<Vec<Vec<u8>>, DiffError> {
    let mut messages = Vec::new();
    for command in commands {
        let output = Command::new("cargo")
            .args(command.get(1..).unwrap_or_default())
            .env("CARGO_TARGET_DIR", target_dir)
            .current_dir(checkout)
            .output()
            .map_err(|error| io(checkout, &error))?;
        let mut record = format!(
            "$ {}\nstatus: {}\n--- stderr\n",
            command.join(" "),
            output.status
        )
        .into_bytes();
        record.extend_from_slice(&output.stderr);
        append(log, &record)?;
        if !output.status.success() {
            return Err(DiffError::Infrastructure(format!(
                "{} failed; see {}",
                command.join(" "),
                log.display()
            )));
        }
        messages.extend(
            output
                .stdout
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(<[u8]>::to_vec),
        );
    }
    Ok(messages)
}

/// Copies each built binary into `bin_dir` and records it.
fn copy_binaries(
    executables: &BTreeMap<String, PathBuf>,
    bin_dir: &Path,
) -> Result<BTreeMap<String, BinaryRecord>, DiffError> {
    std::fs::create_dir_all(bin_dir).map_err(|error| io(bin_dir, &error))?;
    let mut binaries = BTreeMap::new();
    for name in BINARIES {
        let built = executables
            .get(name)
            .ok_or_else(|| DiffError::Infrastructure(format!("Cargo built no {name}")))?;
        let copy = bin_dir.join(name);
        std::fs::copy(built, &copy).map_err(|error| io(built, &error))?;
        let bytes = std::fs::read(&copy).map_err(|error| io(&copy, &error))?;
        binaries.insert(
            name.to_string(),
            BinaryRecord {
                sha256: sha256_hex(&bytes),
                cargo_path: built.to_string_lossy().into_owned(),
                linked: linked_libraries(&copy)?,
            },
        );
    }
    Ok(binaries)
}

/// The fields of one Cargo JSON message an artifact needs.
#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    target: CargoTarget,
    #[serde(default)]
    profile: CargoProfile,
}

#[derive(Default, Deserialize)]
struct CargoTarget {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: Vec<String>,
}

#[derive(Default, Deserialize)]
struct CargoProfile {
    #[serde(default)]
    test: bool,
}

/// The executables Cargo reported: the named binaries, and the IR library's
/// test executable as `protocol-runner`.
fn executables(messages: &[Vec<u8>]) -> Result<BTreeMap<String, PathBuf>, DiffError> {
    let mut executables = BTreeMap::new();
    for message in messages {
        let Ok(CargoMessage {
            reason,
            executable,
            target,
            profile,
        }) = serde_json::from_slice::<CargoMessage>(message)
        else {
            continue;
        };
        let Some(executable) = executable.filter(|_| reason == "compiler-artifact") else {
            continue;
        };
        let name = target.name.as_str();
        let library = target.kind.iter().any(|kind| kind == "lib");
        let key = match (name, profile.test, library) {
            ("helm_schema_ir", true, true) => "protocol-runner",
            ("corpus_generation" | "contract_export" | "helm-schema", false, false) => name,
            _ => continue,
        };
        if executables
            .insert(key.to_string(), PathBuf::from(executable))
            .is_some()
        {
            return Err(DiffError::Infrastructure(format!(
                "Cargo reported {key} twice"
            )));
        }
    }
    Ok(executables)
}

fn append(log: &Path, bytes: &[u8]) -> Result<(), DiffError> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|error| io(log, &error))?;
    file.write_all(bytes).map_err(|error| io(log, &error))
}

/// Rehashes a capture's binaries against its build receipt.
///
/// # Errors
///
/// [`DiffError::Identity`] when a binary or the artifact log changed.
pub fn verify(bin_dir: &Path, receipt: &BuildReceipt) -> Result<(), DiffError> {
    let artifacts = std::fs::read(bin_dir.join("cargo-artifacts.jsonl"))
        .map_err(|error| DiffError::Identity(format!("{}: {error}", bin_dir.display())))?;
    if sha256_hex(&artifacts) != receipt.artifacts_sha256
        || receipt.source_before_sha256 != receipt.source_after_sha256
    {
        return Err(DiffError::Identity(
            "the build receipt does not hold".into(),
        ));
    }
    for (name, record) in &receipt.binaries {
        let bytes = std::fs::read(bin_dir.join(name))
            .map_err(|error| DiffError::Identity(format!("{name}: {error}")))?;
        if sha256_hex(&bytes) != record.sha256 {
            return Err(DiffError::Identity(format!("{name} changed")));
        }
    }
    if receipt.binaries.len() != BINARIES.len() {
        return Err(DiffError::Identity(
            "the build receipt misses a binary".into(),
        ));
    }
    Ok(())
}
