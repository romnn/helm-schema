use std::collections::{BTreeMap, BTreeSet};

use helm_schema_goyaml::is_authored;
use serde_yaml::Value;
use tracing::instrument;

use super::LoadedChart;
use super::paths::scope_values_path;
use super::types::{ChartContext, ChartDependencyActivation};
use crate::chart_source::RootChartSource;
use crate::error::{CliError, EngineResult};

#[derive(Debug, Clone)]
struct DependencyMetadata {
    values_key: String,
    activation: ChartDependencyActivation,
}

struct InstalledChart {
    files: LoadedChart,
    children: Vec<InstalledChart>,
}

impl InstalledChart {
    fn read(files: LoadedChart) -> EngineResult<Self> {
        reject_duplicate_dependency_values_keys(files.metadata(), files.manifest()?.path.clone())?;
        Ok(Self {
            files,
            children: Vec::new(),
        })
    }

    fn name(&self) -> &str {
        text(self.files.metadata(), "name")
    }

    fn load_children(&mut self) -> EngineResult<()> {
        self.files.visit_vendored_charts(|child| {
            self.children.push(Self::read(child)?);
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
    let mut root = InstalledChart::read(source.load(include_tests)?)?;
    root.load_children()?;
    let mut out = Vec::new();
    discover_chart_contexts_inner(&root, root.name(), &[], &[], false, &mut out);
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
    let metadata = chart.files.metadata();
    let is_library = text(metadata, "type") == "library";
    let mut static_root_strings = chart_static_root_strings(metadata);
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

    let dependency_metadata_by_name = dependency_metadata_map(metadata, parent_prefix);

    for child in &chart.children {
        let name = child.name();
        let listed = dependency_metadata_by_name.get(name);
        let dependency_metadata = listed.cloned().unwrap_or_else(|| {
            vec![DependencyMetadata {
                values_key: name.to_owned(),
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

/// A Go string field of the decoded metadata, with the zero value for an absent or null one.
fn text<'a>(metadata: &'a Value, name: &str) -> &'a str {
    metadata
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn chart_static_root_strings(metadata: &Value) -> BTreeMap<Vec<String>, String> {
    let mut strings = BTreeMap::new();
    for (field, key) in [
        ("Name", "name"),
        ("Version", "version"),
        ("APIVersion", "apiVersion"),
        ("AppVersion", "appVersion"),
        ("Description", "description"),
        ("Home", "home"),
        ("Icon", "icon"),
        ("Type", "type"),
    ] {
        // Helm reads these through `chart.Metadata`'s Go `string` fields, so
        // an absent key renders as the zero value `""` and still satisfies
        // `typeIs "string"`. Dropping it would make the read unresolved.
        strings.insert(
            vec!["Chart".into(), field.into()],
            text(metadata, key).into(),
        );
    }
    if let Some(annotations) = metadata.get("annotations").and_then(Value::as_mapping) {
        for (key, value) in annotations {
            if let (Some(key), Some(value)) = (key.as_str(), value.as_str()) {
                strings.insert(
                    vec!["Chart".into(), "Annotations".into(), key.into()],
                    value.into(),
                );
            }
        }
    }
    strings
}

fn dependency_key(dependency: &Value) -> (&str, &str) {
    let name = text(dependency, "name");
    let alias = text(dependency, "alias");
    (name, if alias.is_empty() { name } else { alias })
}

fn declarations(metadata: &Value) -> impl Iterator<Item = &Value> {
    metadata
        .get("dependencies")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
}

fn dependency_metadata_map(
    metadata: &Value,
    parent_prefix: &[String],
) -> BTreeMap<String, Vec<DependencyMetadata>> {
    let mut out = BTreeMap::<_, Vec<_>>::new();
    for dependency in declarations(metadata) {
        let (name, values_key) = dependency_key(dependency);
        let tags = dependency
            .get("tags")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten();
        // The legacy reader recognized only exact, directly spelled identities and controls.
        let legacy = dependency.get("name").is_some_and(is_authored)
            && ["alias", "condition"]
                .into_iter()
                .all(|field| dependency.get(field).is_none_or(authored_or_empty))
            && tags.clone().all(authored_or_empty);
        out.entry(name.to_owned())
            .or_default()
            .push(DependencyMetadata {
                values_key: values_key.to_owned(),
                activation: ChartDependencyActivation {
                    condition_paths: dependency_condition_paths(
                        text(dependency, "condition"),
                        parent_prefix,
                    ),
                    tag_paths: dependency_tag_paths(tags.filter_map(Value::as_str)),
                    legacy,
                },
            });
    }
    out
}

fn authored_or_empty(value: &Value) -> bool {
    value.is_null() || value.as_str() == Some("") || is_authored(value)
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

fn dependency_tag_paths<'a>(tags: impl Iterator<Item = &'a str>) -> Vec<String> {
    tags.map(str::trim)
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

fn reject_duplicate_dependency_values_keys(metadata: &Value, path: String) -> EngineResult<()> {
    let mut declarations_by_values_key = BTreeMap::<_, Vec<_>>::new();
    for (name, key) in declarations(metadata).map(dependency_key) {
        declarations_by_values_key
            .entry(key)
            .or_default()
            .push(name);
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
            .entry(chart.name())
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
