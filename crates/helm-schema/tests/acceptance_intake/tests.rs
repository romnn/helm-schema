use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use super::{INTAKE_DIR, load, read_baseline, read_entry, select};

#[test]
fn intake_is_exactly_bound_to_the_commit_and_fixture() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let intake = load(&root.join(INTAKE_DIR).join("manifest.json"))?;
    for entry in &intake.entries {
        let expected = std::fs::read(root.join(&entry.file))?;
        // The ordinary suite runs in shallow checkouts; maintenance checks resolve historical refs.
        let selected = select(&intake.entries, &entry.reference, &entry.fixture)?;
        sim_assert_eq!(have: read_entry(selected, &root)?, want: expected);
    }
    let fixture = "testdata/chart-corpus-schemas/daskhub.schema.json";
    assert!(read_baseline("not-an-intake-commit", fixture).is_err());
    assert!(select(&intake.entries, "unregistered", fixture).is_err());
    let mut duplicated = intake.entries.clone();
    duplicated.extend(intake.entries.clone());
    for entry in &intake.entries {
        assert!(select(&duplicated, &entry.reference, &entry.fixture).is_err());
    }
    Ok(())
}

#[test]
fn changed_intake_inputs_or_bytes_are_rejected() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let intake = load(&root.join(INTAKE_DIR).join("manifest.json"))?;
    for entry in intake.entries {
        let mut changed = entry.clone();
        changed.inputs_sha256 = "0".repeat(64);
        assert!(read_entry(&changed, &root).is_err());
        changed = entry.clone();
        changed.sha256 = "0".repeat(64);
        assert!(read_entry(&changed, &root).is_err());
        changed = entry.clone();
        changed.recipe = serde_json::Value::Null;
        assert!(read_entry(&changed, &root).is_err());
        changed = entry;
        changed.file = "../outside.schema.json".into();
        assert!(read_entry(&changed, &root).is_err());
    }
    Ok(())
}

/// Builds a clone whose testdata links to the real charts and provider bundle,
/// so `read_entry` checks its recipe and inputs against real data while the
/// intake directory itself stays a scratch fixture.
#[cfg(unix)]
fn scratch_clone(
    root: &std::path::Path,
    intake: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
) -> eyre::Result<()> {
    let testdata = root.join("testdata");
    std::fs::create_dir_all(&testdata)?;
    for child in ["charts", "provider-bundle"] {
        std::os::unix::fs::symlink(
            test_util::workspace_testdata().join(child),
            testdata.join(child),
        )?;
    }
    intake(&root.join(INTAKE_DIR))?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn intake_symlinks_escaping_the_intake_root_are_rejected() -> eyre::Result<()> {
    use std::os::unix::fs::symlink;

    use super::IntakePathError;

    let workspace = test_util::workspace_root();
    let intake = load(&workspace.join(INTAKE_DIR).join("manifest.json"))?;
    let entry = intake.entries.first().ok_or_eyre("no intake baselines")?;
    let expected = std::fs::read(workspace.join(&entry.file))?;
    let scratch = test_util::scratch::ScratchDir::new("acceptance-intake-escape")?;
    let outside = scratch.path().join("outside.schema.json");
    std::fs::write(&outside, &expected)?;
    let escapes = |root: &std::path::Path| -> eyre::Result<bool> {
        let error = read_entry(entry, root)
            .err()
            .ok_or_eyre("an intake baseline outside the intake root was read")?;
        Ok(matches!(
            error.downcast_ref::<IntakePathError>(),
            Some(IntakePathError::Escapes { .. })
        ))
    };

    // Control: the same clone layout with real bytes inside the intake root is read.
    let copied = scratch.path().join("copied");
    scratch_clone(&copied, |intake| {
        std::fs::create_dir_all(intake)?;
        std::fs::write(copied.join(&entry.file), &expected)
    })?;
    sim_assert_eq!(have: read_entry(entry, &copied)?, want: expected.clone());

    // A symlinked file whose target lies outside the intake root.
    let linked = scratch.path().join("linked");
    scratch_clone(&linked, |intake| {
        std::fs::create_dir_all(intake)?;
        symlink(&outside, linked.join(&entry.file))
    })?;
    assert!(escapes(&linked)?);

    // A symlinked intake directory leads outside the clone even when its files are real.
    let moved = scratch.path().join("moved");
    scratch_clone(&moved, |intake| symlink(copied.join(INTAKE_DIR), intake))?;
    assert!(escapes(&moved)?);
    Ok(())
}
