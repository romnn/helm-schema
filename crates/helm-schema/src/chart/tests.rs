use super::*;

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

    let discovery = discover_chart_contexts(&chart_dir)?;
    let child = discovery
        .charts
        .iter()
        .find(|chart| chart.values_prefix == ["kid".to_string()])
        .ok_or_else(|| color_eyre::eyre::eyre!("discover child chart"))?;
    assert_eq!(
        child.dependency_activation.condition_paths,
        vec!["global.kidEnabled".to_string(), "kid.enabled".to_string()]
    );
    assert_eq!(
        child.dependency_activation.tag_paths,
        vec!["tags.observability".to_string()]
    );

    let leaf = discovery
        .charts
        .iter()
        .find(|chart| chart.values_prefix == ["kid".to_string(), "leaf".to_string()])
        .ok_or_else(|| color_eyre::eyre::eyre!("discover nested leaf chart"))?;
    assert_eq!(
        leaf.dependency_activation.condition_paths,
        vec!["kid.leaf.enabled".to_string()]
    );
    assert_eq!(
        leaf.dependency_activation.tag_paths,
        vec!["tags.nested".to_string()]
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

    let err = super::discovery::discover_chart_contexts_with_budget(
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
            assert_eq!(limit_bytes, 128);
        }
        other => panic!("expected load budget error, got {other:?}"),
    }

    Ok(())
}
