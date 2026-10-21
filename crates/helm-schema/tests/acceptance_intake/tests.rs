use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use super::{load, read_baseline, read_entry, select};

#[test]
fn intake_is_exactly_bound_to_the_commit_and_fixture() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let intake = load(&root.join("testdata/acceptance-intake/manifest.json"))?;
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
    let intake = load(&root.join("testdata/acceptance-intake/manifest.json"))?;
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

#[cfg(unix)]
#[test]
fn intake_symlinks_escaping_the_intake_root_are_rejected() -> eyre::Result<()> {
    use super::{INTAKE_DIR, IntakePathError, read_contained};

    let temp = test_util::scratch::ScratchDir::new("acceptance-intake-escape")?;
    let root = temp.path().join("clone");
    let intake = root.join(INTAKE_DIR);
    std::fs::create_dir_all(&intake)?;
    std::fs::write(intake.join("inside.schema.json"), b"{}")?;
    std::fs::write(temp.path().join("outside.schema.json"), b"{}")?;
    std::os::unix::fs::symlink(
        temp.path().join("outside.schema.json"),
        intake.join("escape.schema.json"),
    )?;
    std::os::unix::fs::symlink(
        intake.join("inside.schema.json"),
        intake.join("alias.schema.json"),
    )?;
    let inside = format!("{INTAKE_DIR}/inside.schema.json");
    sim_assert_eq!(have: read_contained(&root, &inside)?, want: b"{}".to_vec());
    let alias = format!("{INTAKE_DIR}/alias.schema.json");
    sim_assert_eq!(have: read_contained(&root, &alias)?, want: b"{}".to_vec());
    let escape = format!("{INTAKE_DIR}/escape.schema.json");
    let error = read_contained(&root, &escape)
        .err()
        .ok_or_eyre("a symlink escaping the intake root was read")?;
    assert!(matches!(
        error.downcast_ref::<IntakePathError>(),
        Some(IntakePathError::Escapes { .. })
    ));

    // A symlinked intake directory leads outside the clone even when the file inside it is real.
    let moved = temp.path().join("moved");
    let moved_intake = moved.join(INTAKE_DIR);
    std::fs::create_dir_all(moved.join("testdata"))?;
    std::os::unix::fs::symlink(&intake, &moved_intake)?;
    let error = read_contained(&moved, &inside)
        .err()
        .ok_or_eyre("a symlinked intake directory was read")?;
    assert!(matches!(
        error.downcast_ref::<IntakePathError>(),
        Some(IntakePathError::Escapes { .. })
    ));
    Ok(())
}
