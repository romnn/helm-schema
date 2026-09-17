use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::chart::discovery;
use crate::chart::*;
use crate::error::CliError;
use test_util::prelude::sim_assert_eq;

#[test]
fn legacy_boolean_alias_keys_are_aggregated_across_values_declarations() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {r#"
            on: root
            canonical: {true: allowed, false: allowed}
            quoted: {"no": allowed, 'YES': allowed}
            tagged: {!!str off: allowed}
            value: on
            flow: {Yes: root-flow, "OFF": allowed}
            sequence:
              - {n: nested-sequence}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "nested: {Off: child}\n",
    )?;

    let override_dir = tempfile::tempdir()?;
    let override_path = override_dir.path().join("override.yaml");
    std::fs::write(&override_path, "Y: override\n")?;
    let charts = discover_chart_contexts(&chart_dir)?;
    let Err(error) =
        reject_legacy_boolean_alias_keys(&charts, std::slice::from_ref(&override_path))
    else {
        return Err(eyre::eyre!("plain legacy aliases were accepted"));
    };
    let CliError::YamlBooleanAliasKeys { details } = error else {
        return Err(eyre::eyre!("unexpected rejection: {error}"));
    };

    let mut want = vec![
        "/charts/child/values.yaml:1:10: unquoted key `Off`".to_string(),
        "/values.yaml:1:1: unquoted key `on`".to_string(),
        "/values.yaml:6:8: unquoted key `Yes`".to_string(),
        "/values.yaml:8:6: unquoted key `n`".to_string(),
        format!("{}:1:1: unquoted key `Y`", override_path.display()),
    ];
    want.sort();
    let want = want
        .into_iter()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    sim_assert_eq!(have: details, want: want);
    Ok(())
}

#[test]
fn every_measured_legacy_boolean_alias_spelling_is_rejected() -> eyre::Result<()> {
    let aliases = [
        "y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "on", "On", "ON", "off", "Off",
        "OFF",
    ];
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    let mut source = String::new();
    for alias in aliases {
        writeln!(source, "{alias}: value")?;
    }
    test_util::write(&chart_dir.join("values.yaml")?, source)?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let Err(CliError::YamlBooleanAliasKeys { details }) =
        reject_legacy_boolean_alias_keys(&charts, &[])
    else {
        return Err(eyre::eyre!("a measured legacy alias spelling was accepted"));
    };
    let want = aliases
        .iter()
        .enumerate()
        .map(|(index, alias)| format!("  /values.yaml:{}:1: unquoted key `{alias}`", index + 1))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: details, want: want.join("\n"));
    Ok(())
}

#[test]
fn boolean_alias_rejection_precedes_template_analysis() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "yes: rejected\n")?;
    test_util::write(
        &chart_dir.join("templates/broken.yaml")?,
        "{{ if definitely not a valid action }}\n",
    )?;

    let result = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..Default::default()
        },
    })
    .analysis();
    let Err(error) = result else {
        return Err(eyre::eyre!("legacy alias did not stop preparation"));
    };
    let CliError::YamlBooleanAliasKeys { details } = error else {
        return Err(eyre::eyre!(
            "analysis ran before declaration validation: {error}"
        ));
    };
    sim_assert_eq!(
        have: details,
        want: "  /values.yaml:1:1: unquoted key `yes`"
    );
    Ok(())
}

#[test]
fn chart_metadata_becomes_segmented_static_root_strings() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            appVersion: "7"
            name: root
            version: 0.1.0
            annotations:
              fips: "true"
              traefik.io/proxy-min-version: "3.0.0"
        "#},
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let root = charts.first().ok_or_eyre("discover root chart")?;
    sim_assert_eq!(
        have: &root.static_root_strings,
        want: &BTreeMap::from([
            (
                vec!["Chart".to_string(), "APIVersion".to_string()],
                "v2".to_string(),
            ),
            (
                vec!["Chart".to_string(), "Annotations".to_string(), "fips".to_string()],
                "true".to_string(),
            ),
            (
                vec![
                    "Chart".to_string(),
                    "Annotations".to_string(),
                    "traefik.io/proxy-min-version".to_string(),
                ],
                "3.0.0".to_string(),
            ),
            (
                vec!["Chart".to_string(), "AppVersion".to_string()],
                "7".to_string(),
            ),
            (
                vec!["Chart".to_string(), "Description".to_string()],
                String::new(),
            ),
            (
                vec!["Chart".to_string(), "Home".to_string()],
                String::new(),
            ),
            (
                vec!["Chart".to_string(), "Icon".to_string()],
                String::new(),
            ),
            (
                vec!["Chart".to_string(), "Name".to_string()],
                "root".to_string(),
            ),
            (
                vec!["Chart".to_string(), "Type".to_string()],
                String::new(),
            ),
            (
                vec!["Chart".to_string(), "Version".to_string()],
                "0.1.0".to_string(),
            ),
        ])
    );

    Ok(())
}

/// `chart.Metadata`'s Go `string` fields have no absent state: Helm v4.2.3
/// renders every unset key as `""` and answers `typeIs "string"` with `true`
/// for each of them (`helm-chart-fields.log`).
#[test]
fn absent_chart_string_fields_are_the_go_zero_value() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {r"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let root = charts.first().ok_or_eyre("discover root chart")?;
    let absent = ["AppVersion", "Description", "Home", "Icon", "Type"]
        .into_iter()
        .map(|field| {
            let key = vec!["Chart".to_string(), field.to_string()];
            let value = root.static_root_strings.get(&key).cloned();
            (field, value)
        })
        .collect::<BTreeMap<_, _>>();
    sim_assert_eq!(
        have: absent,
        want: BTreeMap::from([
            ("AppVersion", Some(String::new())),
            ("Description", Some(String::new())),
            ("Home", Some(String::new())),
            ("Icon", Some(String::new())),
            ("Type", Some(String::new())),
        ])
    );

    Ok(())
}

#[test]
fn dependency_activation_paths_are_scoped_from_chart_yaml() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {r"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: child
                alias: kid
                version: 0.1.0
                condition: kid.enabled, global.kidEnabled
                tags:
                  - observability
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        indoc! {r"
            apiVersion: v2
            name: child
            version: 0.1.0
            dependencies:
              - name: leaf
                version: 0.1.0
                condition: leaf.enabled
                tags:
                  - nested
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/charts/leaf/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: leaf
            version: 0.1.0
        "},
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let child = charts
        .iter()
        .find(|chart| chart.values_prefix == ["kid".to_string()])
        .ok_or_eyre("discover child chart")?;
    sim_assert_eq!(have: child.dependency_activation_chain.len(), want: 1);
    sim_assert_eq!(
        have: child.dependency_activation_chain[0].condition_paths,
        want: vec!["kid.enabled".to_string(), "global.kidEnabled".to_string()]
    );
    sim_assert_eq!(
        have: child.dependency_activation_chain[0].tag_paths,
        want: vec!["tags.observability".to_string()]
    );

    // The nested leaf inherits the kid edge's level AND carries its own:
    // helm only renders it while every ancestor condition activates too.
    let leaf = charts
        .iter()
        .find(|chart| chart.values_prefix == ["kid".to_string(), "leaf".to_string()])
        .ok_or_eyre("discover nested leaf chart")?;
    sim_assert_eq!(have: leaf.dependency_activation_chain.len(), want: 2);
    sim_assert_eq!(
        have: leaf.dependency_activation_chain[0].condition_paths,
        want: vec!["kid.enabled".to_string(), "global.kidEnabled".to_string()]
    );
    sim_assert_eq!(
        have: leaf.dependency_activation_chain[1].condition_paths,
        want: vec!["kid.leaf.enabled".to_string()]
    );
    sim_assert_eq!(
        have: leaf.dependency_activation_chain[1].tag_paths,
        want: vec!["tags.nested".to_string()]
    );

    Ok(())
}

#[test]
fn dependency_aliases_and_activation_are_read_from_helm_v2_requirements() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v1
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("requirements.yaml")?,
        indoc! {"
            dependencies:
              - name: child
                alias: operator
                version: 0.1.0
                condition: feature.operator.enabled
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        indoc! {"
            apiVersion: v1
            name: child
            version: 0.1.0
        "},
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let child = charts
        .iter()
        .find(|chart| chart.values_prefix == ["operator".to_string()])
        .ok_or_eyre("discover aliased Helm v2 dependency")?;
    sim_assert_eq!(have: child.dependency_activation_chain.len(), want: 1);
    sim_assert_eq!(
        have: &child.dependency_activation_chain[0].condition_paths,
        want: &vec!["feature.operator.enabled".to_string()]
    );

    Ok(())
}

#[test]
fn duplicate_dependency_values_keys_are_aggregated_before_discovery() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: first
                alias: shared-root
                version: 0.1.0
              - name: second
                alias: shared-root
                version: 0.1.0
              - name: third
                alias: other-root
                version: 0.1.0
              - name: fourth
                alias: other-root
                version: 0.1.0
        "},
    )?;

    let Err(error) = discover_chart_contexts(&chart_dir) else {
        return Err(eyre::eyre!(
            "duplicate dependency values keys were accepted"
        ));
    };
    let diagnostic = error.to_string();
    let CliError::DuplicateDependencyValuesKeys { path, details } = error else {
        return Err(eyre::eyre!(
            "unexpected duplicate-values-key result: {error}"
        ));
    };
    sim_assert_eq!(have: path, want: "/Chart.yaml");
    sim_assert_eq!(
        have: details,
        want: "  `other-root`: 2 declarations (`third`, `fourth`)\n  `shared-root`: 2 declarations (`first`, `second`)"
    );
    sim_assert_eq!(
        have: diagnostic,
        want: indoc! {r"
            duplicate dependency values keys in /Chart.yaml:
              `other-root`: 2 declarations (`third`, `fourth`)
              `shared-root`: 2 declarations (`first`, `second`)
            each dependency must own a unique .Values root
        "}
        .trim_end()
    );
    Ok(())
}

#[test]
fn one_vendored_chart_expands_to_each_unique_alias() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: shared
                alias: alpha
                condition: alpha.enabled
                version: 0.1.0
              - name: shared
                alias: beta
                condition: beta.enabled
                version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/shared/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: shared
            version: 0.1.0
        "},
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let activations = charts
        .iter()
        .filter(|chart| !chart.values_prefix.is_empty())
        .map(|chart| {
            (
                chart.values_prefix.clone(),
                chart.dependency_activation_chain[0].condition_paths.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    sim_assert_eq!(
        have: activations,
        want: BTreeMap::from([
            (
                vec!["alpha".to_string()],
                vec!["alpha.enabled".to_string()]
            ),
            (
                vec!["beta".to_string()],
                vec!["beta.enabled".to_string()]
            ),
        ])
    );
    Ok(())
}

#[test]
fn legacy_requirements_support_one_chart_under_multiple_aliases() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v1
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("requirements.yaml")?,
        indoc! {"
            dependencies:
              - name: shared
                alias: alpha
                version: 0.1.0
              - name: shared
                alias: beta
                version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/shared/Chart.yaml")?,
        indoc! {"
            apiVersion: v1
            name: shared
            version: 0.1.0
        "},
    )?;

    let prefixes = discover_chart_contexts(&chart_dir)?
        .into_iter()
        .map(|chart| chart.values_prefix)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: prefixes,
        want: BTreeSet::from([
            Vec::new(),
            vec!["alpha".to_string()],
            vec!["beta".to_string()],
        ])
    );
    Ok(())
}

#[test]
fn nested_chart_supports_one_dependency_under_multiple_aliases() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: child
                version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
            dependencies:
              - name: shared
                alias: alpha
                version: 0.1.0
              - name: shared
                alias: beta
                version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/charts/shared/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: shared
            version: 0.1.0
        "},
    )?;

    let prefixes = discover_chart_contexts(&chart_dir)?
        .into_iter()
        .map(|chart| chart.values_prefix)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: prefixes,
        want: BTreeSet::from([
            Vec::new(),
            vec!["child".to_string()],
            vec!["child".to_string(), "alpha".to_string()],
            vec!["child".to_string(), "beta".to_string()],
        ])
    );
    Ok(())
}

#[test]
fn distinct_dependency_names_cannot_share_one_values_key() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: first
                alias: shared-root
                version: 0.1.0
              - name: second
                alias: shared-root
                version: 0.1.0
        "},
    )?;

    let Err(CliError::DuplicateDependencyValuesKeys { path, details }) =
        discover_chart_contexts(&chart_dir)
    else {
        return Err(eyre::eyre!("shared dependency values key was accepted"));
    };
    sim_assert_eq!(have: path, want: "/Chart.yaml");
    sim_assert_eq!(
        have: details,
        want: "  `shared-root`: 2 declarations (`first`, `second`)"
    );
    Ok(())
}

#[test]
fn duplicate_installed_chart_names_are_aggregated() -> eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: shared
                alias: alpha
                version: 0.1.0
              - name: shared
                alias: beta
                version: 0.1.0
        "},
    )?;
    for directory in ["first", "second"] {
        test_util::write(
            &chart_dir.join(format!("charts/{directory}/Chart.yaml"))?,
            indoc! {"
                apiVersion: v2
                name: shared
                version: 0.1.0
            "},
        )?;
    }

    let Err(error) = discover_chart_contexts(&chart_dir) else {
        return Err(eyre::eyre!("duplicate installed chart names were accepted"));
    };
    let diagnostic = error.to_string();
    let CliError::DuplicateInstalledDependencyNames { path, details } = error else {
        return Err(eyre::eyre!("unexpected installed-name result: {error}"));
    };
    sim_assert_eq!(have: path, want: "/charts");
    sim_assert_eq!(
        have: details,
        want: "  `shared`: /charts/first, /charts/second"
    );
    sim_assert_eq!(
        have: diagnostic,
        want: indoc! {r"
            duplicate installed dependency names in /charts:
              `shared`: /charts/first, /charts/second
            Helm's installed-entry association for duplicate internal names is nondeterministic
        "}
        .trim_end()
    );
    Ok(())
}

#[test]
fn vendored_chart_archive_respects_load_budget() -> eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;

    let mut tar_bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut tar_bytes, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let chart_yaml = b"apiVersion: v2\nname: child\nversion: 0.1.0\n";
        let mut chart_header = tar::Header::new_gnu();
        chart_header.set_size(chart_yaml.len() as u64);
        chart_header.set_mode(0o644);
        chart_header.set_cksum();
        builder.append_data(
            &mut chart_header,
            "child/Chart.yaml",
            std::io::Cursor::new(chart_yaml.as_slice()),
        )?;

        let filler = vec![b'x'; 2048];
        let mut values_header = tar::Header::new_gnu();
        values_header.set_size(filler.len() as u64);
        values_header.set_mode(0o644);
        values_header.set_cksum();
        builder.append_data(
            &mut values_header,
            "child/values.yaml",
            std::io::Cursor::new(filler),
        )?;

        builder.finish()?;
    }

    {
        let archive_path = chart_dir.join("charts/child.tgz")?;
        archive_path.parent().create_dir_all()?;
        let mut file = archive_path.create_file()?;
        file.write_all(&tar_bytes)?;
    }

    let err = discovery::discover_chart_contexts_with_budget(
        &chart_dir,
        crate::load_budget::LoadBudget::new(128, 1024),
    )
    .expect_err("archive should exceed load budget");

    match err {
        crate::CliError::LoadBudgetExceeded {
            subject,
            limit_bytes,
        } => {
            assert!(
                subject.contains("child.tgz"),
                "unexpected subject: {subject}"
            );
            sim_assert_eq!(have: limit_bytes, want: 128);
        }
        other => panic!("expected load budget error, got {other:?}"),
    }

    Ok(())
}

#[test]
fn vendored_chart_archive_respects_expanded_budget() -> eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;

    let mut tar_bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut tar_bytes, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let chart_yaml = b"apiVersion: v2\nname: child\nversion: 0.1.0\n";
        let mut chart_header = tar::Header::new_gnu();
        chart_header.set_size(chart_yaml.len() as u64);
        chart_header.set_mode(0o644);
        chart_header.set_cksum();
        builder.append_data(
            &mut chart_header,
            "child/Chart.yaml",
            std::io::Cursor::new(chart_yaml.as_slice()),
        )?;

        let filler = vec![b'x'; 4096];
        let mut values_header = tar::Header::new_gnu();
        values_header.set_size(filler.len() as u64);
        values_header.set_mode(0o644);
        values_header.set_cksum();
        builder.append_data(
            &mut values_header,
            "child/values.yaml",
            std::io::Cursor::new(filler),
        )?;

        builder.finish()?;
    }

    let archive_path = chart_dir.join("charts/child.tgz")?;
    archive_path.parent().create_dir_all()?;
    archive_path.create_file()?.write_all(&tar_bytes)?;

    let err = discovery::discover_chart_contexts_with_budget(
        &chart_dir,
        crate::load_budget::LoadBudget {
            max_chart_archive_unpacked_bytes: 1024,
            ..crate::load_budget::LoadBudget::new(16 * 1024, 1024)
        },
    )
    .expect_err("archive should exceed expanded budget");

    match err {
        crate::CliError::LoadBudgetExceeded {
            subject,
            limit_bytes,
        } => {
            assert!(subject.contains("expanded"));
            sim_assert_eq!(have: limit_bytes, want: 1024);
        }
        other => panic!("expected expanded load budget error, got {other:?}"),
    }

    Ok(())
}

#[test]
fn vendored_chart_archive_respects_entry_budget() -> eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;

    let mut tar_bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut tar_bytes, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        for (path, bytes) in [
            (
                "child/Chart.yaml",
                b"apiVersion: v2\nname: child\nversion: 0.1.0\n".as_slice(),
            ),
            ("child/values.yaml", b"enabled: true\n".as_slice()),
            (
                "child/templates/configmap.yaml",
                b"apiVersion: v1\nkind: ConfigMap\n".as_slice(),
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, std::io::Cursor::new(bytes))?;
        }

        builder.finish()?;
    }

    let archive_path = chart_dir.join("charts/child.tgz")?;
    archive_path.parent().create_dir_all()?;
    archive_path.create_file()?.write_all(&tar_bytes)?;

    let err = discovery::discover_chart_contexts_with_budget(
        &chart_dir,
        crate::load_budget::LoadBudget {
            max_chart_archive_entries: 2,
            ..crate::load_budget::LoadBudget::new(16 * 1024, 1024)
        },
    )
    .expect_err("archive should exceed entry budget");

    match err {
        crate::CliError::LoadEntryBudgetExceeded {
            subject,
            limit_entries,
        } => {
            assert!(subject.contains("child.tgz"));
            sim_assert_eq!(have: limit_entries, want: 2);
        }
        other => panic!("expected entry budget error, got {other:?}"),
    }

    Ok(())
}

#[test]
fn vendored_chart_archive_rejects_unsafe_entry_paths() {
    let err = discovery::validate_archive_entry_path(
        "charts/child.tgz",
        std::path::Path::new("../child/Chart.yaml"),
    )
    .expect_err("archive should reject unsafe entry path");

    match err {
        crate::CliError::UnsafeArchiveEntryPath {
            archive,
            entry_path,
        } => {
            assert!(archive.contains("child.tgz"));
            sim_assert_eq!(have: entry_path, want: "../child/Chart.yaml");
        }
        other => panic!("expected unsafe archive entry error, got {other:?}"),
    }
}

/// Entry-path policy must not depend on the host's path syntax: tar member
/// names are `/`-separated on every platform, and a backslash entry that
/// Windows parses as traversal must not slip through as one opaque Unix
/// filename. Same verdicts on every OS.
#[test]
fn archive_entry_path_verdicts_are_platform_independent() {
    for accepted in ["chart/Chart.yaml", "./chart/values.yaml"] {
        assert!(
            discovery::validate_archive_entry_path(
                "charts/child.tgz",
                std::path::Path::new(accepted)
            )
            .is_ok(),
            "{accepted} must be accepted"
        );
    }
    for rejected in [
        "../child/Chart.yaml",
        "..\\child\\Chart.yaml",
        "chart/..\\..\\evil",
        "/etc/passwd",
    ] {
        assert!(
            discovery::validate_archive_entry_path(
                "charts/child.tgz",
                std::path::Path::new(rejected)
            )
            .is_err(),
            "{rejected} must be rejected"
        );
    }
}
