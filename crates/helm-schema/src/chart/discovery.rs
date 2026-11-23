use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use tracing::instrument;

use super::LoadedChart;
use super::paths::scope_values_path;
use super::types::{ChartContext, ChartDependencyActivation};
use crate::chart_source::RootChartSource;
use crate::error::{CliError, EngineResult};

#[derive(Debug, Deserialize)]
struct ChartYaml {
    name: Option<String>,

    version: Option<String>,

    #[serde(rename = "apiVersion")]
    api_version: Option<String>,

    #[serde(rename = "appVersion")]
    app_version: Option<String>,

    description: Option<String>,

    home: Option<String>,

    icon: Option<String>,

    #[serde(rename = "type")]
    chart_type: Option<String>,

    annotations: Option<BTreeMap<String, String>>,

    dependencies: Option<Vec<ChartDependency>>,
}

#[derive(Debug, Deserialize)]
struct ChartDependency {
    name: String,
    alias: Option<String>,
    condition: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RequirementsYaml {
    dependencies: Option<Vec<ChartDependency>>,
}

#[derive(Debug, Clone)]
struct DependencyMetadata {
    values_key: String,
    activation: ChartDependencyActivation,
}

struct InstalledChart {
    files: LoadedChart,
    name: String,
    metadata: ChartYaml,
    children: Vec<InstalledChart>,
}

impl InstalledChart {
    fn read(mut files: LoadedChart, is_root: bool) -> EngineResult<Self> {
        let metadata = read_chart_yaml(&files)?;
        files.set_api_version(metadata.api_version.as_deref());
        let name = metadata
            .name
            .clone()
            .unwrap_or_else(|| files.filename().to_string());
        if name.is_empty() && metadata.name.is_none() && !is_root {
            return Err(CliError::SubchartNameMissing {
                path: files.path.clone(),
            });
        }
        Ok(Self {
            files,
            name,
            metadata,
            children: Vec::new(),
        })
    }

    fn load_children(&mut self) -> EngineResult<()> {
        self.files.visit_vendored_charts(|child| {
            self.children.push(Self::read(child, false)?);
            Ok(())
        })?;
        reject_duplicate_installed_dependency_names(
            &self.children,
            self.files.file_path("charts"),
        )?;
        for child in &mut self.children {
            child.load_children()?;
        }
        Ok(())
    }
}

#[instrument(skip_all)]
pub(crate) fn discover_chart_contexts(
    source: &RootChartSource,
    include_tests: bool,
) -> EngineResult<Vec<ChartContext>> {
    let mut root = InstalledChart::read(source.load(include_tests)?, true)?;
    root.load_children()?;
    let mut out = Vec::new();
    discover_chart_contexts_inner(&root, &root.name, &[], &[], false, &mut out);
    Ok(out)
}

fn discover_chart_contexts_inner(
    chart: &InstalledChart,
    template_namespace: &str,
    parent_prefix: &[String],
    dependency_activation_chain: &[ChartDependencyActivation],
    listed_dependency: bool,
    out: &mut Vec<ChartContext>,
) {
    let chart_yaml = &chart.metadata;
    let is_library = chart_yaml
        .chart_type
        .as_deref()
        .is_some_and(|chart_type| chart_type.eq_ignore_ascii_case("library"));
    let mut static_root_strings = chart_static_root_strings(chart_yaml);
    // `.Template.BasePath` is per chart, not per file: Helm reports the
    // executing chart's own `templates` directory for every file it owns,
    // whatever subdirectory that file lives in. Binding it here lets the
    // ordinary constant folding resolve `include (print $.Template.BasePath
    // "/x.yaml")` to that chart's own template.
    static_root_strings.insert(
        vec!["Template".to_string(), "BasePath".to_string()],
        format!("{template_namespace}/templates"),
    );

    out.push(ChartContext {
        files: chart.files.clone(),
        values_prefix: parent_prefix.to_vec(),
        template_namespace: template_namespace.to_string(),
        is_library,
        static_root_strings,
        dependency_activation_chain: dependency_activation_chain.to_vec(),
        listed_dependency,
    });

    let dependency_metadata_by_name = dependency_metadata_map(chart_yaml, parent_prefix);

    for child in &chart.children {
        let listed = dependency_metadata_by_name.get(&child.name);
        let dependency_metadata = listed.cloned().unwrap_or_else(|| {
            vec![DependencyMetadata {
                values_key: child.name.clone(),
                activation: ChartDependencyActivation::default(),
            }]
        });

        for dependency_metadata in dependency_metadata {
            let child_namespace = format!(
                "{template_namespace}/charts/{}",
                dependency_metadata.values_key
            );
            let mut prefix = parent_prefix.to_vec();
            prefix.push(dependency_metadata.values_key);

            // Only condition/tag-carrying edges add an activation level; an
            // unconditional dependency keeps its parent's chain, so a child's
            // chain always extends its parent's as a prefix.
            let mut chain = dependency_activation_chain.to_vec();
            let activation = dependency_metadata.activation;
            if !activation.condition_paths.is_empty() || !activation.tag_paths.is_empty() {
                chain.push(activation);
            }

            discover_chart_contexts_inner(
                child,
                &child_namespace,
                &prefix,
                &chain,
                listed.is_some(),
                out,
            );
        }
    }
}

fn chart_static_root_strings(chart: &ChartYaml) -> BTreeMap<Vec<String>, String> {
    let mut strings = BTreeMap::new();
    for (field, value) in [
        ("Name", chart.name.as_ref()),
        ("Version", chart.version.as_ref()),
        ("APIVersion", chart.api_version.as_ref()),
        ("AppVersion", chart.app_version.as_ref()),
        ("Description", chart.description.as_ref()),
        ("Home", chart.home.as_ref()),
        ("Icon", chart.icon.as_ref()),
        ("Type", chart.chart_type.as_ref()),
    ] {
        // Helm reads these through `chart.Metadata`'s Go `string` fields, so
        // an absent key renders as the zero value `""` and still satisfies
        // `typeIs "string"`. Dropping it would make the read unresolved.
        strings.insert(
            vec!["Chart".to_string(), field.to_string()],
            value.cloned().unwrap_or_default(),
        );
    }
    for (key, value) in chart.annotations.as_ref().into_iter().flatten() {
        strings.insert(
            vec!["Chart".to_string(), "Annotations".to_string(), key.clone()],
            value.clone(),
        );
    }
    strings
}

fn dependency_metadata_map(
    chart_yaml: &ChartYaml,
    parent_prefix: &[String],
) -> BTreeMap<String, Vec<DependencyMetadata>> {
    let mut out = BTreeMap::new();
    let deps = chart_yaml.dependencies.as_deref().unwrap_or_default();

    for dependency in deps {
        let values_key = dependency
            .alias
            .clone()
            .unwrap_or_else(|| dependency.name.clone());
        out.entry(dependency.name.clone())
            .or_insert_with(Vec::new)
            .push(DependencyMetadata {
                values_key,
                activation: dependency_activation(dependency, parent_prefix),
            });
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
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for path in condition
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| scope_chart_yaml_value_path(path, parent_prefix))
    {
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    paths
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

fn read_chart_yaml(chart: &LoadedChart) -> EngineResult<ChartYaml> {
    let manifest = chart.manifest()?;
    let mut metadata: ChartYaml = serde_yaml::from_str(manifest.yaml_text()?)?;
    let mut dependency_source = manifest.path.clone();
    if metadata.dependencies.is_none()
        && let Some(requirements) = chart.file("requirements.yaml")
    {
        let yaml: RequirementsYaml = serde_yaml::from_str(requirements.yaml_text()?)?;
        metadata.dependencies = yaml.dependencies;
        dependency_source = requirements.path.clone();
    }
    reject_duplicate_dependency_values_keys(
        metadata.dependencies.as_deref().unwrap_or_default(),
        dependency_source,
    )?;
    Ok(metadata)
}

fn reject_duplicate_dependency_values_keys(
    dependencies: &[ChartDependency],
    path: String,
) -> EngineResult<()> {
    let mut declarations_by_values_key = BTreeMap::new();
    for dependency in dependencies {
        let values_key = dependency.alias.as_deref().unwrap_or(&dependency.name);
        declarations_by_values_key
            .entry(values_key)
            .or_insert_with(Vec::new)
            .push(dependency.name.as_str());
    }
    let details = declarations_by_values_key
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(values_key, names)| {
            let declarations = names
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "  `{values_key}`: {} declarations ({declarations})",
                names.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if details.is_empty() {
        return Ok(());
    }
    Err(CliError::DuplicateDependencyValuesKeys { path, details })
}

fn reject_duplicate_installed_dependency_names(
    installed_charts: &[InstalledChart],
    path: String,
) -> EngineResult<()> {
    let mut entries_by_name = BTreeMap::new();
    for chart in installed_charts {
        entries_by_name
            .entry(&chart.name)
            .or_insert_with(Vec::new)
            .push(chart.files.path.as_str());
    }
    let details = entries_by_name
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(name, paths)| format!("  `{name}`: {}", paths.join(", ")))
        .collect::<Vec<_>>()
        .join("\n");
    if details.is_empty() {
        return Ok(());
    }
    Err(CliError::DuplicateInstalledDependencyNames { path, details })
}
