//! Per-process scratch directories under the cargo target directory.
//!
//! Test scratch never goes to the system temporary directory. It lives in a
//! root that belongs to this module alone:
//!
//! ```text
//! <root>/.sweep.lock                         serializes registration and sweeps
//! <root>/.owners/<pid>-<nonce>.lock          held by its process while it lives
//! <root>/<crate>/<label>-<pid>-<nonce>-<n>/  one scratch directory
//! <root>/<crate>/<label>-<pid>-<nonce>-<n>.helm-schema-scratch  its marker
//! ```
//!
//! - `<root>` is `HELM_SCHEMA_SCRATCH_ROOT` when set, else `<target>/scratch`,
//!   where `<target>` is `CARGO_TARGET_DIR` (relative to the workspace root)
//!   or the workspace `target/`.
//! - `<crate>` is the running package's `CARGO_PKG_NAME`, which cargo and
//!   nextest set for test and `cargo run` processes, else `unknown`.
//! - `<pid>-<nonce>` is the owner: the process id and a random nonce drawn
//!   when the process registers, so a reused pid is a different owner.
//!
//! A marker's name carries the full `<label>-<owner>-<n>` grammar and its
//! contents repeat the owner, so a marker can only ever name the one
//! directory it was written beside; malformed markers are ignored.
//!
//! A [`ScratchDir`] removes its directory when dropped. A killed process drops
//! nothing, and [`ScratchDir::keep`] opts out, so ownership is also a lock:
//! before its first scratch directory a process registers by creating and
//! locking its owner file, and the operating system releases that lock when
//! the process dies. Registration then sweeps: it removes each directory whose
//! marker names an owner whose lock is free or gone, holding that owner's lock
//! until the directory and the lock file are removed. Registration and sweep
//! both hold `.sweep.lock`, so no sweeper runs between an owner file's
//! creation and its lock. The sweep starts from the markers this module
//! writes beside its directories, deletes only the directory a marker names,
//! and never follows a symbolic link. The marker sits beside the directory,
//! not inside it, so a scratch directory holds exactly what its user wrote
//! (a chart, an archive root, a content-hashed tree).
//!
//! [`preserve`] copies a failure's evidence to
//! `<target>/evidence/<crate>/<label>-<owner>-<n>/`, which no sweep touches,
//! and marks it the same way. Registration prunes marked evidence whose
//! marker is older than seven days; that is an age limit, not a byte bound,
//! and unmarked entries there are never touched.
//!
//! Trust boundary: the roots belong to this module. A marker whose directory
//! was replaced by a symbolic link or a file is dropped without touching
//! what replaced it, but a real directory substituted at a marked path
//! inside the private root is out of scope and is deleted with its marker.
//!
//! The guarantee covers scratch made through this module. A test that passes
//! a system temporary path to `tempdir_in` or `new_in`, and production code
//! such as the Kubernetes cache's default root, are outside it.

use std::collections::BTreeMap;
use std::fs;
use std::hash::{BuildHasher as _, RandomState};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use color_eyre::eyre::{self, WrapErr, eyre};

/// Overrides the scratch root, for tests that need a root of their own.
pub const ROOT_VAR: &str = "HELM_SCHEMA_SCRATCH_ROOT";
/// Appended to a scratch directory's path, names the file beside it that
/// marks it as scratch and holds its owner.
pub(crate) const MARKER_SUFFIX: &str = ".helm-schema-scratch";
pub(crate) const OWNERS: &str = ".owners";
const SWEEP_LOCK: &str = ".sweep.lock";
/// Evidence older than this is pruned when a process registers.
const EVIDENCE_RETENTION: Duration = Duration::from_hours(7 * 24);

/// This process's registration, or why it failed.
static OWNER: OnceLock<std::result::Result<Owner, String>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A registered owner: its identity and its held lock.
#[derive(Debug)]
pub(crate) struct Owner {
    pub(crate) id: String,
    _lock: fs::File,
}

/// A fresh directory under [`root`] that is removed when dropped.
#[derive(Debug)]
pub struct ScratchDir {
    /// Empty once [`ScratchDir::keep`] took it.
    path: PathBuf,
}

impl ScratchDir {
    /// Creates `<root>/<crate>/<label>-<owner>-<n>` and its marker,
    /// registering this process and sweeping first on its first call.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty, `.`, `..` or multi-component label, or
    /// when registration fails or the directory or its marker cannot be
    /// created.
    pub fn new(label: &str) -> eyre::Result<Self> {
        eyre::ensure!(
            valid_label(label),
            "scratch label {label:?} must be a non-empty single path component"
        );
        let root = root();
        let owner =
            match OWNER.get_or_init(|| register(&root).map_err(|error| format!("{error:?}"))) {
                Ok(owner) => owner,
                Err(error) => return Err(eyre!("{error}")).wrap_err("register scratch owner"),
            };
        let package = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "unknown".to_string());
        let parent = root.join(package);
        fs::create_dir_all(&parent)
            .wrap_err_with(|| format!("create scratch parent {}", parent.display()))?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("{label}-{}-{n}", owner.id));
        fs::create_dir(&path).wrap_err_with(|| format!("create scratch {}", path.display()))?;
        let dir = Self { path };
        fs::write(marker_path(&dir.path), &owner.id)
            .wrap_err_with(|| format!("mark scratch {}", dir.path.display()))?;
        Ok(dir)
    }

    /// The directory's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keeps the directory past this guard; a sweep removes it once this
    /// process has exited.
    #[must_use]
    pub fn keep(mut self) -> PathBuf {
        std::mem::take(&mut self.path)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let removed = match fs::remove_dir_all(&self.path) {
            Ok(()) => true,
            Err(error) => error.kind() == ErrorKind::NotFound,
        };
        // A directory that could not be removed keeps its marker, so a later
        // sweep reclaims it.
        if removed {
            let _ = fs::remove_file(marker_path(&self.path));
        }
    }
}

/// `TMPDIR`, `TMP` and `TEMP` naming one scratch directory kept for this
/// process, for a child process that would otherwise write to the system
/// temporary directory (Go reads `TMPDIR` on Unix, `TMP` and `TEMP` on
/// Windows).
///
/// # Errors
///
/// Returns an error when the directory cannot be created.
pub fn temp_env() -> eyre::Result<[(&'static str, PathBuf); 3]> {
    static DIR: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        ScratchDir::new("child-tmp")
            .map(ScratchDir::keep)
            .map_err(|error| format!("{error:?}"))
    });
    match dir {
        Ok(dir) => Ok([
            ("TMPDIR", dir.clone()),
            ("TMP", dir.clone()),
            ("TEMP", dir.clone()),
        ]),
        Err(error) => Err(eyre!("{error}")).wrap_err("create the child temporary directory"),
    }
}

/// The cargo target directory: `CARGO_TARGET_DIR`, else the workspace
/// `target/`.
#[must_use]
pub fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        // Joining an absolute path replaces the workspace root.
        Some(dir) if !dir.is_empty() => crate::workspace_root().join(dir),
        _ => crate::workspace_root().join("target"),
    }
}

/// The scratch root: [`ROOT_VAR`] when set, else `<target>/scratch`.
#[must_use]
pub fn root() -> PathBuf {
    match std::env::var_os(ROOT_VAR) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => target_dir().join("scratch"),
    }
}

/// `<target>/evidence`, where preserved failure evidence lives.
#[must_use]
pub fn evidence_root() -> PathBuf {
    target_dir().join("evidence")
}

/// Copies `dir`, a scratch directory or a directory inside one, to the same
/// relative path under [`evidence_root`], which no sweep touches, marks the
/// copied scratch directory like scratch, and returns the copy's path. A
/// copy, not a move: the caller may still be using `dir`.
///
/// # Errors
///
/// Returns an error when `dir` is not inside a scratch directory under
/// [`root`], or cannot be copied or marked.
pub fn preserve(dir: &Path) -> eyre::Result<PathBuf> {
    let root = root().canonicalize().wrap_err("resolve the scratch root")?;
    let dir = dir
        .canonicalize()
        .wrap_err_with(|| format!("resolve {}", dir.display()))?;
    let relative = dir
        .strip_prefix(&root)
        .wrap_err_with(|| format!("{} is not scratch", dir.display()))?;
    let mut components = relative.components();
    let (Some(package), Some(name)) = (components.next(), components.next()) else {
        eyre::bail!("{} is not inside a scratch directory", dir.display());
    };
    let name = name.as_os_str().to_str().unwrap_or_default();
    let owner = name_owner(name)
        .ok_or_else(|| eyre!("{} is not inside a scratch directory", dir.display()))?;
    let entry = evidence_root().join(package).join(name);
    let destination = evidence_root().join(relative);
    // Marked before any payload is copied, so an interrupted or failed copy
    // is still pruned. Rewritten on every copy, so the retention clock
    // restarts.
    fs::create_dir_all(&entry).wrap_err_with(|| format!("create {}", entry.display()))?;
    fs::write(marker_path(&entry), owner)
        .wrap_err_with(|| format!("mark evidence {}", entry.display()))?;
    copy_tree(&dir, &destination)
        .wrap_err_with(|| format!("copy {} to {}", dir.display(), destination.display()))?;
    Ok(destination)
}

/// Copies the directories and regular files under `from` into `to`, which
/// may already exist; symbolic links are skipped.
///
/// # Errors
///
/// Returns an error when a directory cannot be read or created, or a file
/// cannot be copied.
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Registers this process as a scratch owner under `root` and sweeps.
pub(crate) fn register(root: &Path) -> eyre::Result<Owner> {
    prune_evidence(&evidence_root());
    let owners = root.join(OWNERS);
    fs::create_dir_all(&owners).wrap_err_with(|| format!("create {}", owners.display()))?;
    let _sweep = lock_root(root)?;
    let nonce = RandomState::new().hash_one((std::process::id(), SystemTime::now()));
    let id = format!("{}-{nonce:016x}", std::process::id());
    let path = owner_lock(root, &id);
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .wrap_err_with(|| format!("create {}", path.display()))?;
    lock.try_lock()
        .wrap_err_with(|| format!("lock {}", path.display()))?;
    sweep(root, &id);
    Ok(Owner { id, _lock: lock })
}

/// Blocks until this process holds `<root>/.sweep.lock`; dropping the file
/// releases it.
pub(crate) fn lock_root(root: &Path) -> eyre::Result<fs::File> {
    let path = root.join(SWEEP_LOCK);
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .wrap_err_with(|| format!("open {}", path.display()))?;
    lock.lock()
        .wrap_err_with(|| format!("lock {}", path.display()))?;
    Ok(lock)
}

/// What a sweep learned about one owner.
enum OwnerState {
    /// Alive, or in doubt: its scratch stays.
    Live,
    /// Dead. Its lock, when the lock file still exists, stays held until its
    /// scratch and lock file are removed.
    Dead(Option<fs::File>),
}

/// Removes every marker under `root` whose owner is dead, with the scratch
/// directory it names, and every free owner lock file except `own`'s. The
/// caller holds [`lock_root`].
pub(crate) fn sweep(root: &Path, own: &str) {
    let mut owners: BTreeMap<String, OwnerState> = BTreeMap::new();
    for package in real_dirs(root) {
        for marked in marked_entries(&package) {
            if marked.owner == own {
                continue;
            }
            let state = owners
                .entry(marked.owner.clone())
                .or_insert_with(|| owner_state(root, &marked.owner));
            if matches!(state, OwnerState::Dead(_)) {
                remove_marked(&marked);
            }
        }
    }
    // A process that made no scratch leaves only its lock file.
    if let Ok(entries) = fs::read_dir(root.join(OWNERS)) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(owner) = name.to_str().and_then(|name| name.strip_suffix(".lock")) else {
                continue;
            };
            if owner != own && !owners.contains_key(owner) {
                owners.insert(owner.to_string(), owner_state(root, owner));
            }
        }
    }
    for (owner, state) in owners {
        if let OwnerState::Dead(Some(lock)) = state {
            let _ = fs::remove_file(owner_lock(root, &owner));
            drop(lock);
        }
    }
}

fn owner_lock(root: &Path, owner: &str) -> PathBuf {
    root.join(OWNERS).join(format!("{owner}.lock"))
}

/// Whether `owner` is dead, taking its lock when it is. A missing lock file
/// is a dead owner: registration creates the lock file before any scratch,
/// and only a sweep holding the dead owner's lock removes it.
fn owner_state(root: &Path, owner: &str) -> OwnerState {
    match fs::OpenOptions::new()
        .write(true)
        .open(owner_lock(root, owner))
    {
        Ok(lock) => match lock.try_lock() {
            Ok(()) => OwnerState::Dead(Some(lock)),
            Err(_) => OwnerState::Live,
        },
        Err(error) if error.kind() == ErrorKind::NotFound => OwnerState::Dead(None),
        Err(_) => OwnerState::Live,
    }
}

/// A directory this module created, found through its marker.
pub(crate) struct Marked {
    pub(crate) marker: PathBuf,
    pub(crate) dir: PathBuf,
    pub(crate) owner: String,
}

/// The well-formed markers directly inside `parent`: regular files named
/// `<label>-<owner>-<n>` plus [`MARKER_SUFFIX`] whose contents are that same
/// owner. Each names `<parent>/<label>-<owner>-<n>`, and nothing else.
pub(crate) fn marked_entries(parent: &Path) -> Vec<Marked> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let marker = entry.path();
        let name = entry.file_name();
        let Some(stem) = name
            .to_str()
            .and_then(|name| name.strip_suffix(MARKER_SUFFIX))
        else {
            continue;
        };
        let Some(owner) = name_owner(stem) else {
            continue;
        };
        let is_file = fs::symlink_metadata(&marker).is_ok_and(|metadata| metadata.is_file());
        if is_file && fs::read_to_string(&marker).is_ok_and(|contents| contents == owner) {
            found.push(Marked {
                dir: parent.join(stem),
                marker,
                owner,
            });
        }
    }
    found
}

/// Removes a marked directory and then its marker. A marker whose path no
/// longer holds a real directory (it is gone, or a symbolic link or a file
/// replaced it) is dropped without touching that path. When the removal
/// fails or the path's state cannot be read, the marker stays for a retry.
pub(crate) fn remove_marked(marked: &Marked) {
    let removed = match fs::symlink_metadata(&marked.dir) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&marked.dir).is_ok(),
        Ok(_) => true,
        Err(error) => error.kind() == ErrorKind::NotFound,
    };
    if removed {
        let _ = fs::remove_file(&marked.marker);
    }
}

/// The owner of a `<label>-<pid>-<nonce>-<n>` name: `<pid>-<nonce>` with a
/// decimal pid, a 16-digit lowercase hex nonce, a decimal `<n>` and a valid
/// label.
fn name_owner(name: &str) -> Option<String> {
    let mut parts = name.rsplitn(4, '-');
    let n = parts.next()?;
    let nonce = parts.next()?;
    let pid = parts.next()?;
    let label = parts.next()?;
    let owner = format!("{pid}-{nonce}");
    let well_formed = !n.is_empty()
        && n.bytes().all(|byte| byte.is_ascii_digit())
        && valid_owner(&owner)
        && valid_label(label);
    well_formed.then_some(owner)
}

/// Whether `owner` is `<decimal pid>-<16 lowercase hex digits>` exactly as
/// registration spells it.
fn valid_owner(owner: &str) -> bool {
    let Some((pid, nonce)) = owner.split_once('-') else {
        return false;
    };
    match (pid.parse::<u32>(), u64::from_str_radix(nonce, 16)) {
        (Ok(pid), Ok(nonce)) => format!("{pid}-{nonce:016x}") == owner,
        _ => false,
    }
}

/// Whether `label` is one non-empty path component other than `.` and `..`.
fn valid_label(label: &str) -> bool {
    !label.is_empty() && label != "." && label != ".." && !label.contains(['/', '\\'])
}

/// The marker beside the scratch directory `dir`.
fn marker_path(dir: &Path) -> PathBuf {
    let mut marker = dir.as_os_str().to_owned();
    marker.push(MARKER_SUFFIX);
    PathBuf::from(marker)
}

/// The directories directly inside `dir`, excluding symbolic links and
/// dot-named entries.
fn real_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if !hidden && fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
            dirs.push(path);
        }
    }
    dirs
}

/// Removes the marked entries of `<evidence>/<crate>/` whose marker was last
/// written more than [`EVIDENCE_RETENTION`] ago; unmarked entries stay.
pub(crate) fn prune_evidence(evidence: &Path) {
    let Some(cutoff) = SystemTime::now().checked_sub(EVIDENCE_RETENTION) else {
        return;
    };
    for package in real_dirs(evidence) {
        for marked in marked_entries(&package) {
            let modified =
                fs::symlink_metadata(&marked.marker).and_then(|metadata| metadata.modified());
            if modified.is_ok_and(|modified| modified < cutoff) {
                remove_marked(&marked);
            }
        }
    }
}
