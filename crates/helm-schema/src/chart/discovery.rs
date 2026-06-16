use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;
use tracing::instrument;
use vfs::VfsPath;

use super::paths::scope_values_path;
use super::types::{ChartContext, ChartDependencyActivation, ChartDiscovery};
use crate::error::{CliError, CliResult};

#[derive(Debug, Deserialize)]
struct ChartYaml {
    name: Option<String>,

    #[serde(rename = "type")]
    chart_type: Option<String>,

    dependencies: Option<Vec<ChartDependency>>,
}

#[derive(Debug, Deserialize)]
struct ChartDependency {
    name: String,
    alias: Option<String>,
    condition: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct DependencyMetadata {
    values_key: String,
    activation: ChartDependencyActivation,
}

#[instrument(skip_all)]
pub fn discover_chart_contexts(root_chart_dir: &VfsPath) -> CliResult<ChartDiscovery> {
    let mut out = Vec::new();
    discover_chart_contexts_inner(
        root_chart_dir,
        &[],
        ChartDependencyActivation::default(),
        &mut out,
    )?;
    Ok(ChartDiscovery { charts: out })
}

fn discover_chart_contexts_inner(
    chart_dir: &VfsPath,
    parent_prefix: &[String],
    dependency_activation: ChartDependencyActivation,
    out: &mut Vec<ChartContext>,
) -> CliResult<()> {
    let chart_yaml = read_chart_yaml(chart_dir)?;

    let is_library = chart_yaml
        .chart_type
        .as_deref()
        .is_some_and(|chart_type| chart_type.eq_ignore_ascii_case("library"));

    out.push(ChartContext {
        chart_dir: chart_dir.clone(),
        values_prefix: parent_prefix.to_vec(),
        is_library,
        dependency_activation,
    });

    let dependency_metadata_by_name = dependency_metadata_map(&chart_yaml, parent_prefix);

    let vendor_charts_dir = chart_dir.join("charts")?;
    if !vendor_charts_dir.is_dir()? {
        return Ok(());
    }

    // VFS backends do not promise read_dir ordering. Keep dependency traversal
    // stable because DefineIndex and helper graph construction use
    // last-write-wins semantics for duplicate helper definitions.
    let mut vendor_entries: Vec<VfsPath> = vendor_charts_dir.read_dir()?.collect();
    vendor_entries.sort_by_key(VfsPath::filename);

    for entry in vendor_entries {
        let sub_dir = if entry.is_dir()? {
            let chart_yaml_path = entry.join("Chart.yaml")?;
            let chart_template_yaml_path = entry.join("Chart.template.yaml")?;
            if !chart_yaml_path.is_file()? && !chart_template_yaml_path.is_file()? {
                continue;
            }
            entry
        } else if entry.is_file()? {
            if !is_chart_archive(&entry.filename()) {
                continue;
            }

            extract_chart_archive(&entry)?
        } else {
            continue;
        };

        let sub_chart_yaml = read_chart_yaml(&sub_dir)?;
        let sub_name = sub_chart_yaml
            .name
            .clone()
            .or_else(|| {
                let name = sub_dir.filename();
                if name.is_empty() { None } else { Some(name) }
            })
            .ok_or_else(|| CliError::SubchartNameMissing {
                path: sub_dir.as_str().to_string(),
            })?;

        let dependency_metadata = dependency_metadata_by_name
            .get(&sub_name)
            .cloned()
            .unwrap_or_else(|| DependencyMetadata {
                values_key: sub_name.clone(),
                activation: ChartDependencyActivation::default(),
            });

        let mut prefix = parent_prefix.to_vec();
        prefix.push(dependency_metadata.values_key);

        discover_chart_contexts_inner(&sub_dir, &prefix, dependency_metadata.activation, out)?;
    }

    Ok(())
}

fn is_chart_archive(file_name: &str) -> bool {
    let path = Path::new(file_name);
    let is_tgz = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tgz"));
    let is_tar_gz = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
        && path
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tar"));

    is_tgz || is_tar_gz
}

fn extract_chart_archive(path: &VfsPath) -> CliResult<VfsPath> {
    let mut bytes = Vec::new();
    path.open_file()?.read_to_end(&mut bytes)?;

    let gz = GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(gz);

    let root = VfsPath::new(vfs::MemoryFS::new());
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?;
        let path_str = path.to_string_lossy();
        let out = root.join(path_str.as_ref())?;
        out.parent().create_dir_all()?;
        let mut file = out.create_file()?;
        std::io::copy(&mut entry, &mut file)?;
    }

    find_chart_dir(&root)?.ok_or_else(|| CliError::NoChartYamlInArchive {
        archive: path.as_str().to_string(),
    })
}

fn find_chart_dir(root: &VfsPath) -> CliResult<Option<VfsPath>> {
    let direct = root.join("Chart.yaml")?;
    let direct_template = root.join("Chart.template.yaml")?;
    if direct.is_file()? || direct_template.is_file()? {
        return Ok(Some(root.clone()));
    }

    // Multi-chart archives are rare, but choosing alphabetically keeps the
    // fallback deterministic across filesystems.
    let mut entries: Vec<VfsPath> = root.read_dir()?.collect();
    entries.sort_by_key(VfsPath::filename);
    for entry in entries {
        if !entry.is_dir()? {
            continue;
        }
        let chart_yaml = entry.join("Chart.yaml")?;
        let chart_template_yaml = entry.join("Chart.template.yaml")?;
        if chart_yaml.is_file()? || chart_template_yaml.is_file()? {
            return Ok(Some(entry));
        }
    }

    Ok(None)
}

fn dependency_metadata_map(
    chart_yaml: &ChartYaml,
    parent_prefix: &[String],
) -> BTreeMap<String, DependencyMetadata> {
    let mut out = BTreeMap::new();
    let deps = chart_yaml.dependencies.as_deref().unwrap_or_default();

    for dependency in deps {
        let values_key = dependency
            .alias
            .clone()
            .unwrap_or_else(|| dependency.name.clone());
        out.insert(
            dependency.name.clone(),
            DependencyMetadata {
                values_key,
                activation: dependency_activation(dependency, parent_prefix),
            },
        );
    }

    out
}

fn dependency_activation(
    dependency: &ChartDependency,
    parent_prefix: &[String],
) -> ChartDependencyActivation {
    let condition_paths = dependency
        .condition
        .as_deref()
        .map(|condition| dependency_condition_paths(condition, parent_prefix))
        .unwrap_or_default();

    let tag_paths = dependency
        .tags
        .as_deref()
        .map(dependency_tag_paths)
        .unwrap_or_default();

    ChartDependencyActivation {
        condition_paths,
        tag_paths,
    }
}

fn dependency_condition_paths(condition: &str, parent_prefix: &[String]) -> Vec<String> {
    condition
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| scope_chart_yaml_value_path(path, parent_prefix))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn dependency_tag_paths(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(|tag| format!("tags.{tag}"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn scope_chart_yaml_value_path(path: &str, prefix: &[String]) -> String {
    let path = path.trim();
    if path == "tags" || path.starts_with("tags.") {
        return path.to_string();
    }
    scope_values_path(path, prefix)
}

fn read_chart_yaml(chart_dir: &VfsPath) -> CliResult<ChartYaml> {
    let chart_yaml = chart_dir.join("Chart.yaml")?;
    let chart_template_yaml = chart_dir.join("Chart.template.yaml")?;

    let path = if chart_yaml.is_file()? {
        chart_yaml
    } else if chart_template_yaml.is_file()? {
        chart_template_yaml
    } else {
        chart_yaml
    };

    let mut bytes = Vec::new();
    path.open_file()?.read_to_end(&mut bytes)?;

    Ok(serde_yaml::from_slice(&bytes)?)
}
