use super::build_composed_values_yaml;
use crate::chart::ChartContext;
use crate::chart::discover_chart_contexts;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

fn yaml_pointer<'a>(doc: &'a serde_yaml::Value, path: &[&str]) -> Option<&'a serde_yaml::Value> {
    let mut current = doc;
    for segment in path {
        let map = current.as_mapping()?;
        current = map.get(serde_yaml::Value::String((*segment).to_string()))?;
    }
    Some(current)
}

fn discover(chart_dir: &VfsPath) -> color_eyre::eyre::Result<Vec<ChartContext>> {
    Ok(discover_chart_contexts(chart_dir)?.charts)
}

#[test]
fn composed_subchart_globals_preserve_parent_explicit_null_defaults() -> color_eyre::eyre::Result<()>
{
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "global:\n  imageRegistry:\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "global:\n  imageRegistry: docker.io\n",
    )?;

    let composed =
        build_composed_values_yaml(&discover(&chart_dir)?, true)?.expect("composed values yaml");
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    assert!(
        yaml_pointer(&doc, &["global", "imageRegistry"]).is_some_and(serde_yaml::Value::is_null),
        "root explicit null should remain authoritative: {doc:?}"
    );
    assert!(
        yaml_pointer(&doc, &["child", "global", "imageRegistry"])
            .is_some_and(serde_yaml::Value::is_null),
        "subchart global mirror should preserve the parent explicit null: {doc:?}"
    );

    Ok(())
}

#[test]
fn composed_subchart_globals_hoist_when_parent_key_is_absent() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "global:\n  imageRegistry: docker.io\n",
    )?;

    let composed =
        build_composed_values_yaml(&discover(&chart_dir)?, true)?.expect("composed values yaml");
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    sim_assert_eq!(
        have: yaml_pointer(&doc, &["global", "imageRegistry"]),
        want: Some(&serde_yaml::Value::String("docker.io".to_string()))
    );
    sim_assert_eq!(
        have: yaml_pointer(&doc, &["child", "global", "imageRegistry"]),
        want: Some(&serde_yaml::Value::String("docker.io".to_string()))
    );

    Ok(())
}
