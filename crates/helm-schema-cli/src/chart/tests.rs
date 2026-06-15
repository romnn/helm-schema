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
