//! Interleavings of registration and sweep, and what a sweep may delete.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use color_eyre::eyre;

use crate::scratch::{MARKER_SUFFIX, OWNERS, ScratchDir, lock_root, prune_evidence, sweep};
use crate::sim_assert_eq;

/// An owner id no test directory names.
const SWEEPER: &str = "1-0000000000000000";

fn owner_lock(root: &Path, owner: &str) -> eyre::Result<(PathBuf, fs::File)> {
    fs::create_dir_all(root.join(OWNERS))?;
    let path = root.join(OWNERS).join(format!("{owner}.lock"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok((path, file))
}

fn marker(parent: &Path, name: &str) -> PathBuf {
    parent.join(format!("{name}{MARKER_SUFFIX}"))
}

fn marked_dir(parent: &Path, name: &str, owner: &str) -> eyre::Result<PathBuf> {
    let dir = parent.join(name);
    fs::create_dir_all(&dir)?;
    fs::write(marker(parent, name), owner)?;
    Ok(dir)
}

/// A sweep that starts while an owner is registering (its lock file created,
/// not yet locked) waits for the registration and then sees the owner alive.
#[test]
fn a_sweep_during_registration_waits_and_keeps_the_new_owner() -> eyre::Result<()> {
    let root = ScratchDir::new("interleave-registration")?;
    let root = root.path().to_path_buf();
    let owner = "7-00000000000000aa";
    let registering = lock_root(&root)?;
    let (lock_path, lock) = owner_lock(&root, owner)?;
    let dir = marked_dir(&root.join("pkg"), &format!("x-{owner}-0"), owner)?;

    let (done, finished) = std::sync::mpsc::channel();
    let sweeper = {
        let root = root.clone();
        std::thread::spawn(move || -> eyre::Result<()> {
            let _sweep = lock_root(&root)?;
            sweep(&root, SWEEPER);
            let _ = done.send(());
            Ok(())
        })
    };
    // Give an unserialized sweeper time to run in the gap; a serialized one
    // stays blocked, so the outcome never depends on this wait.
    let swept_in_the_gap = finished.recv_timeout(Duration::from_millis(200)).is_ok();
    lock.try_lock()?;
    drop(registering);
    sweeper
        .join()
        .map_err(|_| eyre::eyre!("the sweeper thread panicked"))??;

    sim_assert_eq!(
        have: (swept_in_the_gap, dir.is_dir(), lock_path.is_file()),
        want: (false, true, true)
    );
    Ok(())
}

/// A reused pid is a different owner: the dead owner's scratch and lock file
/// go, the live owner's with the same pid stay.
#[test]
fn a_reused_pid_is_a_different_owner() -> eyre::Result<()> {
    let root = ScratchDir::new("interleave-reused-pid")?;
    let root = root.path().to_path_buf();
    let dead = "7-00000000000000aa";
    let live = "7-00000000000000bb";
    let (dead_lock, _unlocked) = owner_lock(&root, dead)?;
    let (live_lock, held) = owner_lock(&root, live)?;
    held.try_lock()?;
    let dead_dir = marked_dir(&root.join("pkg"), &format!("a-{dead}-0"), dead)?;
    let live_dir = marked_dir(&root.join("pkg"), &format!("b-{live}-0"), live)?;

    let guard = lock_root(&root)?;
    sweep(&root, SWEEPER);
    drop(guard);

    sim_assert_eq!(
        have: (dead_dir.is_dir(), dead_lock.is_file(), live_dir.is_dir(), live_lock.is_file()),
        want: (false, false, true, true)
    );
    Ok(())
}

/// The sweep deletes only real directories named by a well-formed marker; it
/// ignores unmarked directories, symbolic-link markers, a marker naming a
/// symbolic link, and packages reached through a symbolic link.
#[cfg(unix)]
#[test]
fn the_sweep_deletes_only_marked_real_directories() -> eyre::Result<()> {
    let root = ScratchDir::new("sweep-ownership")?;
    let outside = ScratchDir::new("sweep-outside")?;
    let root = root.path().to_path_buf();
    let dead = "7-00000000000000aa";
    let pkg = root.join("pkg");
    let marked = marked_dir(&pkg, &format!("marked-{dead}-0"), dead)?;
    let unmarked = pkg.join(format!("unmarked-{dead}-0"));
    fs::create_dir_all(&unmarked)?;
    let linked_marker = pkg.join(format!("linked-marker-{dead}-0"));
    fs::create_dir_all(&linked_marker)?;
    fs::write(outside.path().join("owner"), dead)?;
    std::os::unix::fs::symlink(
        outside.path().join("owner"),
        marker(&pkg, &format!("linked-marker-{dead}-0")),
    )?;
    let linked_dir = pkg.join(format!("linked-dir-{dead}-0"));
    let target = marked_dir(outside.path(), "target", "0-0000000000000000")?;
    std::os::unix::fs::symlink(&target, &linked_dir)?;
    fs::write(marker(&pkg, &format!("linked-dir-{dead}-0")), dead)?;
    let beyond = marked_dir(outside.path(), &format!("beyond-{dead}-0"), dead)?;
    std::os::unix::fs::symlink(outside.path(), root.join("linked-pkg"))?;
    let malformed = marked_dir(&pkg, "malformed", "7-aa")?;

    let guard = lock_root(&root)?;
    sweep(&root, SWEEPER);
    drop(guard);

    sim_assert_eq!(
        have: [
            marked.is_dir(),
            unmarked.is_dir(),
            linked_marker.is_dir(),
            target.is_dir(),
            beyond.is_dir(),
            malformed.is_dir(),
        ],
        want: [false, true, true, true, true, true]
    );
    Ok(())
}

/// A forged or mismatched marker authorizes nothing: an empty stem (the
/// package itself), `..` (the root), an unrelated name, or a name whose owner
/// differs from the marker's contents.
#[test]
fn malformed_markers_authorize_no_deletion() -> eyre::Result<()> {
    let root = ScratchDir::new("sweep-forged")?;
    let root = root.path().to_path_buf();
    let dead = "7-00000000000000aa";
    let other = "7-00000000000000cc";
    let pkg = root.join("pkg");
    fs::create_dir_all(&pkg)?;
    fs::write(pkg.join(MARKER_SUFFIX), dead)?;
    fs::write(marker(&pkg, ".."), dead)?;
    let important = pkg.join("important");
    fs::create_dir_all(&important)?;
    fs::write(marker(&pkg, "important"), dead)?;
    let mismatched = marked_dir(&pkg, &format!("mismatch-{dead}-0"), other)?;

    let guard = lock_root(&root)?;
    sweep(&root, SWEEPER);
    drop(guard);

    sim_assert_eq!(
        have: [pkg.is_dir(), root.is_dir(), important.is_dir(), mismatched.is_dir()],
        want: [true, true, true, true]
    );
    Ok(())
}

/// A stale marker whose directory was replaced by a file is dropped and the
/// file is left alone.
#[test]
fn a_marker_whose_directory_became_a_file_touches_only_the_marker() -> eyre::Result<()> {
    let root = ScratchDir::new("sweep-replaced")?;
    let root = root.path().to_path_buf();
    let dead = "7-00000000000000aa";
    let pkg = root.join("pkg");
    let name = format!("replaced-{dead}-0");
    let replaced = marked_dir(&pkg, &name, dead)?;
    fs::remove_dir(&replaced)?;
    fs::write(&replaced, "not scratch")?;

    let guard = lock_root(&root)?;
    sweep(&root, SWEEPER);
    drop(guard);

    sim_assert_eq!(
        have: (replaced.is_file(), marker(&pkg, &name).exists()),
        want: (true, false)
    );
    Ok(())
}

/// A removal that fails keeps the marker, on drop and in a sweep, so a
/// later sweep reclaims the directory once removal can succeed.
#[cfg(unix)]
#[test]
fn a_failed_removal_keeps_its_marker_and_is_reclaimed_later() -> eyre::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let root = ScratchDir::new("sweep-retry")?;
    let root = root.path().to_path_buf();
    let dead = "7-00000000000000aa";
    let pkg = root.join("pkg");
    let name = format!("stuck-{dead}-0");
    let stuck = marked_dir(&pkg, &name, dead)?;
    let locked = stuck.join("locked");
    fs::create_dir_all(&locked)?;
    fs::write(locked.join("file"), "")?;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555))?;

    let guard = lock_root(&root)?;
    sweep(&root, SWEEPER);
    let kept = (stuck.is_dir(), marker(&pkg, &name).is_file());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755))?;
    sweep(&root, SWEEPER);
    drop(guard);
    let reclaimed = (stuck.exists(), marker(&pkg, &name).exists());

    let dropped = ScratchDir::new("drop-retry")?;
    let dropped_locked = dropped.path().join("locked");
    fs::create_dir_all(&dropped_locked)?;
    fs::write(dropped_locked.join("file"), "")?;
    fs::set_permissions(&dropped_locked, fs::Permissions::from_mode(0o555))?;
    let dropped_path = dropped.path().to_path_buf();
    let dropped_marker = PathBuf::from(format!("{}{MARKER_SUFFIX}", dropped_path.display()));
    drop(dropped);
    let drop_kept = (dropped_path.is_dir(), dropped_marker.is_file());
    fs::set_permissions(&dropped_locked, fs::Permissions::from_mode(0o755))?;
    fs::remove_dir_all(&dropped_path)?;
    fs::remove_file(&dropped_marker)?;

    sim_assert_eq!(
        have: (kept, reclaimed, drop_kept),
        want: ((true, true), (false, false), (true, true))
    );
    Ok(())
}

/// Pruning removes only marked evidence whose marker is older than the
/// retention; fresh marked evidence and unmarked directories stay.
#[test]
fn evidence_pruning_removes_only_old_marked_entries() -> eyre::Result<()> {
    let evidence = ScratchDir::new("evidence-prune")?;
    let pkg = evidence.path().join("pkg");
    let owner = "7-00000000000000aa";
    let old = marked_dir(&pkg, &format!("old-{owner}-0"), owner)?;
    let eight_days_ago = SystemTime::now() - Duration::from_hours(8 * 24);
    fs::File::options()
        .write(true)
        .open(marker(&pkg, &format!("old-{owner}-0")))?
        .set_modified(eight_days_ago)?;
    let fresh = marked_dir(&pkg, &format!("fresh-{owner}-1"), owner)?;
    let unmarked = pkg.join(format!("unmarked-{owner}-2"));
    fs::create_dir_all(&unmarked)?;

    prune_evidence(evidence.path());

    sim_assert_eq!(
        have: [old.exists(), fresh.is_dir(), unmarked.is_dir()],
        want: [false, true, true]
    );
    Ok(())
}

/// A copy into the evidence root that fails part-way leaves a marked entry,
/// so the retention prune still reclaims the partial bundle.
#[cfg(unix)]
#[test]
fn a_partial_evidence_copy_is_marked_and_pruned_later() -> eyre::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    use crate::scratch::{evidence_root, preserve, root};

    let case = ScratchDir::new("evidence-partial")?;
    fs::write(case.path().join("a-readable"), "copied")?;
    let unreadable = case.path().join("z-unreadable");
    fs::write(&unreadable, "never copied")?;
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;
    let copied = preserve(case.path());
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644))?;

    let relative = case.path().canonicalize()?;
    let relative = relative.strip_prefix(root().canonicalize()?)?;
    let entry = evidence_root().join(relative);
    let entry_marker = PathBuf::from(format!("{}{MARKER_SUFFIX}", entry.display()));
    let left = (copied.is_err(), entry.is_dir(), entry_marker.is_file());
    fs::File::options()
        .write(true)
        .open(&entry_marker)?
        .set_modified(SystemTime::now() - Duration::from_hours(8 * 24))?;
    prune_evidence(&evidence_root());

    sim_assert_eq!(
        have: (left, entry.exists(), entry_marker.exists()),
        want: ((true, true, true), false, false)
    );
    Ok(())
}
