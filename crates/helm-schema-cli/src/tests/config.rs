use color_eyre::eyre;
use helm_schema::chart_source::RootChartSource;
use helm_schema::generation::{EmissionSelection, SchemaProfile};
use helm_schema::output::LoadBudget;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::resolve;
use crate::cli::{EmissionArgs, PolicyToggle, SchemaProfile as CliSchemaProfile};

fn chart_with_config(config: &str) -> eyre::Result<tempfile::TempDir> {
    let chart = tempfile::tempdir()?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: config-test
            version: 0.1.0
        "},
    )?;
    std::fs::write(chart.path().join("helm-schema.yaml"), config)?;
    Ok(chart)
}

#[test]
fn temporal_combination_keeps_profile_provenance_and_cli_profile_resets_file_delta()
-> eyre::Result<()> {
    let chart = chart_with_config(indoc! {"
        version: 1
        profile: lean
        emission:
          local-conditionals: off
    "})?;
    let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
    let effective = resolve(
        &root,
        chart.path(),
        None,
        false,
        None,
        EmissionArgs::default(),
    )?;
    let resolved = effective.selection.resolve()?;
    sim_assert_eq!(have: resolved.requested_profile(), want: Some(SchemaProfile::Lean));
    sim_assert_eq!(have: resolved.policy().local_conditionals(), want: false);
    sim_assert_eq!(have: effective.file_weakening.len(), want: 4);
    let EmissionSelection::Preset { delta, .. } = effective.selection else {
        return Err(eyre::eyre!("config selection lost preset provenance"));
    };
    sim_assert_eq!(have: delta.root_anchored_conditionals(), want: None);
    sim_assert_eq!(have: delta.local_conditionals(), want: Some(false));
    sim_assert_eq!(have: delta.terminal_clauses(), want: None);
    sim_assert_eq!(have: delta.kind_partitions(), want: None);

    let reset = resolve(
        &root,
        chart.path(),
        None,
        false,
        Some(CliSchemaProfile::Lean),
        EmissionArgs::default(),
    )?;
    sim_assert_eq!(have: reset.selection.resolve()?.policy().local_conditionals(), want: true);
    sim_assert_eq!(have: reset.file_weakening, want: Vec::<&'static str>::new());
    Ok(())
}

#[test]
fn cli_knob_override_wins_over_file_and_profile_values() -> eyre::Result<()> {
    let chart = chart_with_config(indoc! {"
        version: 1
        profile: lean
        emission:
          local-conditionals: off
    "})?;
    let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
    let effective = resolve(
        &root,
        chart.path(),
        None,
        false,
        None,
        EmissionArgs {
            local_conditionals: Some(PolicyToggle::On),
            ..EmissionArgs::default()
        },
    )?;
    sim_assert_eq!(
        have: effective.selection.resolve()?.policy().local_conditionals(),
        want: true
    );
    sim_assert_eq!(have: effective.file_weakening.len(), want: 3);
    assert!(effective.to_yaml()?.contains("source: CLI"));
    Ok(())
}

#[test]
fn malformed_unknown_and_invalid_policy_configs_are_hard_errors() -> eyre::Result<()> {
    for source in [
        indoc! {"
            version: 1
            unknown: true
        "},
        "version: [\n",
        indoc! {"
            version: 1
            profile: lean
            emission:
              local-conditionals: off
              kind-partitions: on
        "},
    ] {
        let chart = chart_with_config(source)?;
        let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
        let result = resolve(
            &root,
            chart.path(),
            None,
            false,
            None,
            EmissionArgs::default(),
        );
        assert!(result.is_err(), "config unexpectedly accepted: {source}");
    }
    Ok(())
}

#[test]
fn config_cannot_activate_narrowing_or_transport_policy() -> eyre::Result<()> {
    for source in [
        indoc! {"
            version: 1
            infer-required: true
        "},
        indoc! {"
            version: 1
            reference-policy: preserve
        "},
        indoc! {"
            version: 1
            override-schema: caller.json
        "},
    ] {
        let chart = chart_with_config(source)?;
        let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
        let result = resolve(
            &root,
            chart.path(),
            None,
            false,
            None,
            EmissionArgs::default(),
        );
        assert!(
            result.is_err(),
            "X-class config unexpectedly accepted: {source}"
        );
    }
    Ok(())
}

#[test]
fn no_config_ignores_malformed_discovered_config() -> eyre::Result<()> {
    let chart = chart_with_config("version: [\n")?;
    let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
    let effective = resolve(
        &root,
        chart.path(),
        None,
        true,
        None,
        EmissionArgs::default(),
    )?;
    sim_assert_eq!(
        have: effective.selection.resolve()?.requested_profile(),
        want: Some(SchemaProfile::Full)
    );
    assert!(effective.to_yaml()?.contains("source: built-in"));
    Ok(())
}

#[test]
fn unsupported_versions_name_the_supported_range_and_remediation() -> eyre::Result<()> {
    for (version, source) in [
        (0, "version: 0\n"),
        (
            2,
            indoc! {"
                version: 2
                future-policy: enabled
            "},
        ),
    ] {
        let chart = chart_with_config(source)?;
        let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
        let error = resolve(
            &root,
            chart.path(),
            None,
            false,
            None,
            EmissionArgs::default(),
        )
        .err()
        .ok_or_else(|| eyre::eyre!("version {version} unexpectedly accepted"))?;
        let message = error.to_string();
        assert!(message.contains("supported range is 1..=1"));
        assert!(message.contains("update the config or the helm-schema binary"));
    }
    Ok(())
}

#[test]
fn dependency_config_is_not_discovered() -> eyre::Result<()> {
    let chart = chart_with_config(indoc! {"
        version: 1
        profile: full
    "})?;
    std::fs::remove_file(chart.path().join("helm-schema.yaml"))?;
    let dependency = chart.path().join("charts/child");
    std::fs::create_dir_all(&dependency)?;
    std::fs::write(
        dependency.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        dependency.join("helm-schema.yaml"),
        indoc! {"
            version: 1
            profile: lean
        "},
    )?;
    let root = RootChartSource::open(chart.path(), LoadBudget::default())?;
    let effective = resolve(
        &root,
        chart.path(),
        None,
        false,
        None,
        EmissionArgs::default(),
    )?;
    sim_assert_eq!(
        have: effective.selection.resolve()?.requested_profile(),
        want: Some(SchemaProfile::Full)
    );
    Ok(())
}
