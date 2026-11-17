//! Oracle discovery follows the task's target root independently of Cargo's executable layout.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::invocation::find_helmsweep;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const EXPECTED: &str = "HELM_SCHEMA_LOOKUP_EXPECTED";

struct LookupCase {
    target: Option<PathBuf>,
    override_path: Option<PathBuf>,
    expected: Option<PathBuf>,
}

fn check_lookup(expected: OsString) -> eyre::Result<()> {
    if expected.is_empty() {
        assert!(find_helmsweep().is_err());
        return Ok(());
    }
    let expected = PathBuf::from(expected);
    if expected.is_file() {
        sim_assert_eq!(have: find_helmsweep()?, want: expected.canonicalize()?);
    } else {
        let error = find_helmsweep()
            .err()
            .ok_or_eyre("missing oracle was found")?;
        assert!(
            error.to_string().contains(&expected.display().to_string()),
            "{error:?}"
        );
    }
    Ok(())
}

fn run_lookup_case(root: &Path, case: LookupCase) -> eyre::Result<()> {
    let LookupCase {
        target,
        override_path,
        expected,
    } = case;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "helmsweep_lookup_uses_target_root_and_explicit_override",
            "--nocapture",
        ])
        .current_dir(root.join("crates/helm-schema-test-support"))
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("HELM_SCHEMA_HELMSWEEP")
        .env(EXPECTED, expected.unwrap_or_default());
    if let Some(target) = target {
        child.env("CARGO_TARGET_DIR", target);
    }
    if let Some(override_path) = override_path {
        child.env("HELM_SCHEMA_HELMSWEEP", override_path);
    }
    let output = child.output()?;
    eyre::ensure!(
        output.status.success(),
        "oracle lookup child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn helmsweep_lookup_uses_target_root_and_explicit_override() -> eyre::Result<()> {
    // Each child owns its environment; no test mutates process-global configuration.
    if let Some(expected) = std::env::var_os(EXPECTED) {
        return check_lookup(expected);
    }

    let root = test_util::workspace_root();
    let scratch = ScratchDir::new("oracle-lookup")?;
    let name = format!("helmsweep{}", std::env::consts::EXE_SUFFIX);
    let program = scratch.path().join(&name);
    let explicit = scratch.path().join("explicit-oracle");
    fs::write(&program, b"target oracle")?;
    fs::write(&explicit, b"explicit oracle")?;
    let relative = PathBuf::from("target/lookup-relative");
    let default = root.join("target").join(&name);
    for case in [
        LookupCase {
            target: Some(scratch.path().to_path_buf()),
            override_path: None,
            expected: Some(program.clone()),
        },
        LookupCase {
            target: Some(relative.clone()),
            override_path: None,
            expected: Some(root.join(relative).join(&name)),
        },
        LookupCase {
            target: None,
            override_path: None,
            expected: Some(default.clone()),
        },
        LookupCase {
            target: Some(PathBuf::new()),
            override_path: None,
            expected: Some(default),
        },
        LookupCase {
            target: Some(scratch.path().join("missing-target")),
            override_path: Some(explicit.clone()),
            expected: Some(explicit),
        },
        LookupCase {
            target: Some(scratch.path().to_path_buf()),
            override_path: Some(scratch.path().join("missing-override")),
            expected: None,
        },
    ] {
        run_lookup_case(&root, case)?;
    }
    Ok(())
}
