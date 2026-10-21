use color_eyre::eyre;
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
