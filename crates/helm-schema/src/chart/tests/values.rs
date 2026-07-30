use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;

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

fn discover(chart_dir: &VfsPath) -> eyre::Result<Vec<ChartContext>> {
    Ok(discover_chart_contexts(chart_dir)?)
}

#[test]
fn composed_subchart_globals_apply_parent_null_deletion() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
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
        indoc! {"
            global:
              imageRegistry:
        "},
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
        indoc! {"
            global:
              imageRegistry: docker.io
        "},
    )?;

    let composed = build_composed_values_yaml(&discover(&chart_dir)?, true)?
        .ok_or_eyre("composed values yaml")?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    assert!(
        yaml_pointer(&doc, &["global", "imageRegistry"]).is_some_and(serde_yaml::Value::is_null),
        "root explicit null should remain authoritative: {doc:?}"
    );
    assert!(
        yaml_pointer(&doc, &["child", "global"])
            .and_then(serde_yaml::Value::as_mapping)
            .is_some_and(serde_yaml::Mapping::is_empty),
        "the injected null should delete the child default at its coalesce stage: {doc:?}"
    );

    Ok(())
}

#[test]
fn composed_subchart_globals_stay_in_the_child_when_parent_key_is_absent() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
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
        indoc! {"
            global:
              imageRegistry: docker.io
        "},
    )?;

    let composed = build_composed_values_yaml(&discover(&chart_dir)?, true)?
        .ok_or_eyre("composed values yaml")?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    sim_assert_eq!(
        have: yaml_pointer(&doc, &["global", "imageRegistry"]),
        want: None
    );
    sim_assert_eq!(
        have: yaml_pointer(&doc, &["child", "global", "imageRegistry"]),
        want: Some(&serde_yaml::Value::String("docker.io".to_string()))
    );

    Ok(())
}

#[test]
fn scalar_parent_global_skips_injection_and_keeps_child_defaults() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "global: disabled\n")?;
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
        indoc! {"
            global:
              imageRegistry: docker.io
        "},
    )?;

    let composed = build_composed_values_yaml(&discover(&chart_dir)?, true)?
        .ok_or_eyre("composed values yaml")?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    sim_assert_eq!(
        have: yaml_pointer(&doc, &["global"]),
        want: Some(&serde_yaml::Value::String("disabled".to_string()))
    );
    sim_assert_eq!(
        have: yaml_pointer(&doc, &["child", "global", "imageRegistry"]),
        want: Some(&serde_yaml::Value::String("docker.io".to_string()))
    );

    Ok(())
}

#[test]
fn scalar_child_global_skips_parent_injection_and_child_defaults() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
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
        indoc! {"
            global:
              imageRegistry: parent.example
            child:
              global: disabled
        "},
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
        indoc! {"
            global:
              imageRegistry: docker.io
        "},
    )?;

    let composed = build_composed_values_yaml(&discover(&chart_dir)?, true)?
        .ok_or_eyre("composed values yaml")?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&composed)?;

    sim_assert_eq!(
        have: yaml_pointer(&doc, &["child", "global"]),
        want: Some(&serde_yaml::Value::String("disabled".to_string()))
    );

    Ok(())
}
