//! The scratch sweep removes a killed process's directories and keeps a live
//! process's.
//!
//! Every actor runs as a child under a scratch root of its own
//! ([`ROOT_VAR`]), so no other test process can sweep in between.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::{ROOT_VAR, ScratchDir};

/// Selects what [`scratch_child`] does when this binary re-invokes itself.
const ROLE_VAR: &str = "TEST_UTIL_SCRATCH_ROLE";
/// Prefixes the line on which a holding child reports its directory.
const MARKER: &str = "scratch-dir=";

/// The child half of the sweep test; a no-op unless [`ROLE_VAR`] is set.
#[test]
fn scratch_child() -> eyre::Result<()> {
    match std::env::var(ROLE_VAR).as_deref() {
        Ok("hold") => {
            let dir = ScratchDir::new("sweep-held")?;
            println!("{MARKER}{}", dir.path().display());
            std::io::stdout().flush()?;
            // The parent kills this process long before the sleep ends.
            std::thread::sleep(Duration::from_secs(600));
            Ok(())
        }
        Ok("sweep") => {
            let _dir = ScratchDir::new("sweep-trigger")?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn child(root: &Path, role: &str) -> eyre::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "scratch_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROOT_VAR, root)
        .env(ROLE_VAR, role);
    Ok(command)
}

/// Starts a holding child and returns it with the directory it reported.
fn holder(root: &Path) -> eyre::Result<(Child, PathBuf)> {
    let mut held = child(root, "hold")?.stdout(Stdio::piped()).spawn()?;
    let stdout = held
        .stdout
        .take()
        .ok_or_eyre("holding child has no stdout")?;
    for line in BufReader::new(stdout).lines() {
        // libtest prints the test's name before its output on the same line.
        if let Some((_, path)) = line?.split_once(MARKER) {
            return Ok((held, PathBuf::from(path)));
        }
    }
    held.kill()?;
    held.wait()?;
    eyre::bail!("holding child reported no scratch directory")
}

#[test]
fn killed_process_scratch_is_swept_and_live_scratch_is_kept() -> eyre::Result<()> {
    let root = ScratchDir::new("sweep-scenario")?;
    let (mut killed, dead) = holder(root.path())?;
    let (mut alive, live) = holder(root.path())?;
    killed.kill()?;
    killed.wait()?;
    let left_behind = dead.is_dir();

    let status = child(root.path(), "sweep")?.status()?;
    let swept = (dead.is_dir(), live.is_dir());
    alive.kill()?;
    alive.wait()?;
    eyre::ensure!(status.success(), "sweeping child failed: {status}");
    sim_assert_eq!(
        have: (left_behind, swept),
        want: (true, (false, true)),
        "a killed process leaves its scratch behind; the next process sweeps it and keeps the live one"
    );
    Ok(())
}
