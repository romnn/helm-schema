use crate::chart::discovery;
use crate::chart::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn dependency_activation_paths_are_scoped_from_chart_yaml() -> color_eyre::eyre::Result<()> {
    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        r#"
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
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        r#"
apiVersion: v2
name: child
version: 0.1.0
dependencies:
  - name: leaf
    version: 0.1.0
    condition: leaf.enabled
    tags:
      - nested
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/charts/leaf/Chart.yaml")?,
        "apiVersion: v2\nname: leaf\nversion: 0.1.0\n",
    )?;

    let charts = discover_chart_contexts(&chart_dir)?;
    let child = charts
        .iter()
        .find(|chart| chart.values_prefix == ["kid".to_string()])
        .ok_or_else(|| color_eyre::eyre::eyre!("discover child chart"))?;
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
        .ok_or_else(|| color_eyre::eyre::eyre!("discover nested leaf chart"))?;
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
fn vendored_chart_archive_respects_load_budget() -> color_eyre::eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
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
fn vendored_chart_archive_respects_expanded_budget() -> color_eyre::eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
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
fn vendored_chart_archive_respects_entry_budget() -> color_eyre::eyre::Result<()> {
    use std::io::Write;

    let chart_dir = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
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
fn vendored_chart_archive_rejects_unsafe_entry_paths() -> color_eyre::eyre::Result<()> {
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

    Ok(())
}
