//! The artifact registry's shape and its agreement with the committed fixtures.

use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::registry::{
    self, ArtifactId, ArtifactKind, ArtifactSpec, ArtifactTarget, ChartId,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn registry_holds_every_fixture_family_once() -> eyre::Result<()> {
    let specs = registry::registry();
    registry::validate(&specs)?;
    let internal = specs
        .iter()
        .filter(|spec| spec.target == ArtifactTarget::Internal)
        .map(|spec| spec.id)
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: internal,
        want: vec![ArtifactId::Chart(ChartId::SignozPostgresql)]
    );
    sim_assert_eq!(have: specs.len(), want: 203);
    Ok(())
}

#[test]
fn validate_names_every_broken_rule() -> eyre::Result<()> {
    let mut specs = registry::registry();
    let first = specs.first().cloned().ok_or_eyre("empty registry")?;
    // A second artifact under an existing file name and an escaping fixture
    // path, while one template fixture goes missing from its family.
    specs.push(ArtifactSpec {
        id: ArtifactId::Chart(ChartId::SignozPostgresql),
        dump_name: first.dump_name.clone(),
        target: ArtifactTarget::Fixture("../outside.schema.json".to_string()),
        recipe: first.recipe,
    });
    specs
        .retain(|spec| spec.id.kind() != ArtifactKind::Template || spec.dump_name.contains("nats"));

    let Err(error) = registry::validate(&specs) else {
        eyre::bail!("a broken registry validated");
    };
    let message = error.to_string();
    for expected in [
        "duplicate id Chart(SignozPostgresql)",
        "duplicate file name helm-schema.cli.chart-corpus.airflow.schema.json",
        "unsafe fixture path \"../outside.schema.json\"",
        "template fixtures: 3, expected 20",
        "chart fixtures: 157, expected 156",
    ] {
        eyre::ensure!(
            message.contains(expected),
            "missing {expected:?} in:\n{message}"
        );
    }
    Ok(())
}

#[test]
fn safe_paths_are_plain_relative_components() {
    let verdicts = [
        "testdata/a.json",
        "/abs/a.json",
        "../a.json",
        "a/./b.json",
        "",
        "a\\b.json",
    ]
    .map(registry::is_safe_relative_path);
    sim_assert_eq!(have: verdicts, want: [true, false, false, false, false, false]);
    let names = ["a.json", "a/b.json", "..", ""].map(registry::is_safe_file_name);
    sim_assert_eq!(have: names, want: [true, false, false, false]);
}

/// Every fixture directory holds exactly the registered fixtures, so a fixture
/// can neither lose its producer nor exist without one.
#[test]
fn fixture_directories_hold_exactly_the_registered_fixtures() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let registered = registry::registry()
        .into_iter()
        .filter_map(|spec| match spec.target {
            ArtifactTarget::Fixture(path) => Some(path),
            ArtifactTarget::Internal => None,
        })
        .collect::<BTreeSet<_>>();
    let mut on_disk = BTreeSet::new();
    for dir in [
        "testdata/chart-corpus-schemas",
        "testdata/emission-profile-schemas/lean",
        "testdata/final-output-schemas",
        "crates/helm-schema-gen/tests/fixtures",
        "crates/helm-schema-ir/tests/fixtures",
    ] {
        for entry in std::fs::read_dir(root.join(dir)).wrap_err_with(|| format!("list {dir}"))? {
            let name = entry?.file_name().to_string_lossy().to_string();
            if name != ".DS_Store" {
                on_disk.insert(format!("{dir}/{name}"));
            }
        }
    }
    sim_assert_eq!(have: on_disk, want: registered);
    Ok(())
}

#[test]
fn chart_ids_round_trip_through_their_directories() {
    for chart in ChartId::CORPUS.iter().chain([&ChartId::SignozPostgresql]) {
        sim_assert_eq!(
            have: ChartId::from_relative_path(chart.relative_path()),
            want: Some(*chart)
        );
        assert!(
            helm_schema_test_support::generate::chart_dir(chart.relative_path()).is_dir(),
            "{chart:?} has no chart directory"
        );
    }
    sim_assert_eq!(have: ChartId::from_relative_path("no-such-chart"), want: None);
}
