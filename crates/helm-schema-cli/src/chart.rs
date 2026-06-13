use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use helm_schema_ast::{DefineIndex, TreeSitterParser, extract_values_yaml_descriptions};
use helm_schema_k8s::LocalSchemaUniverse;
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use tracing::instrument;
use vfs::VfsPath;

use crate::chart_files::{self, FileRole};
use crate::error::{CliError, CliResult};

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
    pub values_prefix: Vec<String>,
    pub is_library: bool,
    pub dependency_activation: ChartDependencyActivation,
}

#[derive(Debug, Clone, Default)]
pub struct ChartDependencyActivation {
    pub condition_paths: Vec<String>,
    pub tag_paths: Vec<String>,
}

pub(crate) fn scope_values_path(path: &str, prefix: &[String]) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }

    if path == "global" || path.starts_with("global.") {
        return path.to_string();
    }

    if prefix.is_empty() {
        return path.to_string();
    }

    let prefix = prefix.join(".");
    format!("{prefix}.{path}")
}

fn take_global_key(doc: &mut YamlValue) -> Option<YamlValue> {
    let YamlValue::Mapping(m) = doc else {
        return None;
    };

    let key = YamlValue::String("global".to_string());
    m.remove(&key)
}

fn merge_global_values(root: &mut YamlValue, global_doc: YamlValue) {
    let target = ensure_mapping_path(root, &["global".to_string()]);

    if matches!(target, YamlValue::Null) {
        *target = YamlValue::Mapping(serde_yaml::Mapping::default());
    }

    let YamlValue::Mapping(m) = target else {
        return;
    };

    let YamlValue::Mapping(sub_m) = global_doc else {
        return;
    };

    for (k, v) in sub_m {
        let entry = m.entry(k).or_insert(YamlValue::Null);
        *entry = merge_yaml_prefer_left(entry.clone(), v);
    }
}

#[derive(Debug)]
pub struct ChartDiscovery {
    pub charts: Vec<ChartContext>,
}

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
        .is_some_and(|t| t.eq_ignore_ascii_case("library"));

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

    // `read_dir` order is unspecified across VFS backends — `MemoryFS`
    // sorts alphabetically but `PhysicalFS` returns ext4/btrfs hash
    // order. Sort entries explicitly here so that downstream consumers
    // see a deterministic chart sequence. This matters for
    // last-write-wins resolution in both `DefineIndex` and
    // `HelperCallGraph`: identical inputs must produce identical
    // outputs regardless of the underlying filesystem.
    //
    // Sorting by filename matches the alphabetical convention already
    // used by `list_template_sources_for_define_index` for per-chart
    // template files.
    let mut vendor_entries: Vec<VfsPath> = vendor_charts_dir.read_dir()?.collect();
    vendor_entries.sort_by_key(VfsPath::filename);

    for ent in vendor_entries {
        let sub_dir = if ent.is_dir()? {
            let chart_yaml_path = ent.join("Chart.yaml")?;
            let chart_template_yaml_path = ent.join("Chart.template.yaml")?;
            if !chart_yaml_path.is_file()? && !chart_template_yaml_path.is_file()? {
                continue;
            }
            ent
        } else if ent.is_file()? {
            let file_name = ent.filename();
            let p = Path::new(&file_name);
            let is_tgz = p
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tgz"));
            let is_tar_gz = p
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
                && p.file_stem()
                    .and_then(|stem| Path::new(stem).extension())
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"));

            if !(is_tgz || is_tar_gz) {
                continue;
            }

            extract_chart_archive(&ent)?
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

fn extract_chart_archive(path: &VfsPath) -> CliResult<VfsPath> {
    let mut bytes = Vec::new();
    path.open_file()?.read_to_end(&mut bytes)?;

    let gz = GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(gz);

    let root = VfsPath::new(vfs::MemoryFS::new());
    for entry in archive.entries()? {
        let mut e = entry?;
        // Skip non-file entries: directory entries have paths ending in `/`
        // which `VfsPath::join` rejects, and file entries below create their
        // own parents via `out.parent().create_dir_all()`.
        if !e.header().entry_type().is_file() {
            continue;
        }
        let p = e.path()?;
        let path_str = p.to_string_lossy();
        let out = root.join(path_str.as_ref())?;
        out.parent().create_dir_all()?;
        let mut f = out.create_file()?;
        std::io::copy(&mut e, &mut f)?;
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

    // Sort by filename so multi-chart archives (rare but possible)
    // pick a stable winner across filesystems.
    let mut entries: Vec<VfsPath> = root.read_dir()?.collect();
    entries.sort_by_key(VfsPath::filename);
    for ent in entries {
        if !ent.is_dir()? {
            continue;
        }
        let chart_yaml = ent.join("Chart.yaml")?;
        let chart_template_yaml = ent.join("Chart.template.yaml")?;
        if chart_yaml.is_file()? || chart_template_yaml.is_file()? {
            return Ok(Some(ent));
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

    for d in deps {
        let values_key = d.alias.clone().unwrap_or_else(|| d.name.clone());
        out.insert(
            d.name.clone(),
            DependencyMetadata {
                values_key,
                activation: dependency_activation(d, parent_prefix),
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

    let doc: ChartYaml = serde_yaml::from_slice(&bytes)?;
    Ok(doc)
}

#[instrument(skip_all)]
pub fn build_define_index(charts: &[ChartContext], include_tests: bool) -> CliResult<DefineIndex> {
    let mut idx = DefineIndex::new();

    for c in charts {
        let chart_files = chart_files::list_chart_files(&c.chart_dir, include_tests)?;

        for path in chart_files
            .iter()
            .filter(|file| file.has_role(FileRole::DefineIndexTemplate))
            .map(|file| &file.path)
        {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;
            idx.add_source(&TreeSitterParser, &src)?;
            let abs = path.as_str();
            let root = c.chart_dir.as_str().trim_end_matches('/');
            let rel = abs
                .strip_prefix(root)
                .and_then(|s| s.strip_prefix('/'))
                .unwrap_or(abs);
            idx.add_file_source(rel, &src);
        }

        // Some charts render manifests by loading YAML fragments from `files/` via `.Files.Get`.
        // Collect those sources so downstream IR generation can statically inline them when the
        // file path is a literal.
        for path in chart_files
            .iter()
            .filter(|file| file.has_role(FileRole::FilesGetSource))
            .map(|file| &file.path)
        {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;
            let abs = path.as_str();
            let root = c.chart_dir.as_str().trim_end_matches('/');
            let rel = abs
                .strip_prefix(root)
                .and_then(|s| s.strip_prefix('/'))
                .or_else(|| abs.find("/files/").map(|i| &abs[(i + 1)..]))
                .or_else(|| abs.find("files/").map(|i| &abs[i..]))
                .unwrap_or(abs);
            idx.add_file_source(rel, &src);
        }
    }

    Ok(idx)
}

#[instrument(skip_all)]
pub fn collect_static_crd_universe(charts: &[ChartContext]) -> CliResult<LocalSchemaUniverse> {
    let mut universe = LocalSchemaUniverse::default();

    for chart in charts {
        for path in files_with_role(&chart.chart_dir, false, FileRole::StaticCrd)? {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;

            for document in serde_yaml::Deserializer::from_str(&src) {
                let document = serde_json::Value::deserialize(document)?;
                if document.is_null() {
                    continue;
                }

                universe.insert_crd_document(document);
            }
        }
    }

    Ok(universe)
}

#[instrument(skip_all)]
pub fn list_manifest_templates(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    files_with_role(chart_dir, include_tests, FileRole::ManifestTemplate)
}

#[instrument(skip_all)]
pub fn build_composed_values_yaml(
    charts: &[ChartContext],
    include_subchart_values: bool,
) -> CliResult<Option<String>> {
    let root = charts.first().ok_or(CliError::NoChartsDiscovered)?;

    let root_values_path = root.chart_dir.join("values.yaml")?;
    let mut doc = if root_values_path.is_file()? {
        let mut bytes = Vec::new();
        root_values_path.open_file()?.read_to_end(&mut bytes)?;
        serde_yaml::from_slice::<YamlValue>(&bytes)?
    } else {
        YamlValue::Mapping(serde_yaml::Mapping::default())
    };

    if include_subchart_values {
        // Two passes so every subchart's slot ends up with the FULL union of
        // globals (its own + every sibling's + the root chart's), not just
        // whatever had been hoisted by the time we got around to writing
        // that subchart's slot.
        //
        // Pass 1: hoist `global` from each subchart's values.yaml into
        // the root doc's `.global`. Subchart-local sub_docs are kept
        // aside until we know the full root-global picture.
        let mut sub_docs: Vec<(&ChartContext, YamlValue)> = Vec::new();
        for c in charts {
            if c.values_prefix.is_empty() {
                continue;
            }

            let path = c.chart_dir.join("values.yaml")?;
            if !path.is_file()? {
                continue;
            }

            let mut bytes = Vec::new();
            path.open_file()?.read_to_end(&mut bytes)?;
            let mut sub_doc: YamlValue = serde_yaml::from_slice(&bytes)?;

            // Helm merges subchart defaults under the subchart key, except `global`,
            // which is merged into the top-level `.Values.global` for all charts.
            if let Some(global_doc) = take_global_key(&mut sub_doc) {
                merge_global_values(&mut doc, global_doc);
            }

            sub_docs.push((c, sub_doc));
        }

        // Pass 2: at Helm render time the root's effective `global` is
        // mirrored back into every subchart's `.Values` (so
        // `.Values.global.X` resolves identically in every chart in
        // the tree, and so `helm.sh/helm/v4 chartutil.MergeValues`
        // unconditionally writes `global: {}` into every subchart
        // even when no chart in the tree declares its own `global`).
        // Mirror it into each subchart's slot in the composed values
        // doc so the inferred schema also lets `<subchart>.global`
        // carry the same shape — without this,
        // `additionalProperties: false` on the inferred subchart node
        // rejects `<subchart>.global` and `helm lint --strict` fails
        // against the otherwise-correct merged values.
        let empty_global = YamlValue::Mapping(serde_yaml::Mapping::default());
        let root_global = doc
            .as_mapping()
            .and_then(|m| m.get(YamlValue::String("global".to_string())))
            .cloned()
            .unwrap_or(empty_global);

        for (c, mut sub_doc) in sub_docs {
            if !matches!(sub_doc, YamlValue::Mapping(_)) {
                sub_doc = YamlValue::Mapping(serde_yaml::Mapping::default());
            }
            if let YamlValue::Mapping(m) = &mut sub_doc {
                m.insert(YamlValue::String("global".to_string()), root_global.clone());
            }
            merge_values_at_prefix(&mut doc, &c.values_prefix, sub_doc);
        }
    }

    let serialized = serde_yaml::to_string(&doc)?;
    if serialized.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(serialized))
    }
}

#[instrument(skip_all)]
pub fn build_composed_values_descriptions(
    charts: &[ChartContext],
    include_subchart_values: bool,
    values_files: &[PathBuf],
) -> CliResult<BTreeMap<String, String>> {
    let root = charts.first().ok_or(CliError::NoChartsDiscovered)?;
    let mut descriptions = BTreeMap::new();

    add_values_file_descriptions(&root.chart_dir, &[], &mut descriptions)?;

    if include_subchart_values {
        for chart in charts {
            if chart.values_prefix.is_empty() {
                continue;
            }
            add_values_file_descriptions(
                &chart.chart_dir,
                &chart.values_prefix,
                &mut descriptions,
            )?;
        }
    }

    for path in values_files {
        add_layered_values_file_descriptions(path, &mut descriptions)?;
    }

    Ok(descriptions)
}

fn add_values_file_descriptions(
    chart_dir: &VfsPath,
    prefix: &[String],
    out: &mut BTreeMap<String, String>,
) -> CliResult<()> {
    let values_path = chart_dir.join("values.yaml")?;
    if !values_path.is_file()? {
        return Ok(());
    }

    let mut src = String::new();
    values_path.open_file()?.read_to_string(&mut src)?;
    let descriptions = extract_values_yaml_descriptions(&src)?;

    for (path, description) in descriptions {
        let scoped_path = scope_values_path(&path, prefix);
        out.entry(scoped_path).or_insert(description);
    }

    Ok(())
}

fn add_layered_values_file_descriptions(
    values_path: &Path,
    out: &mut BTreeMap<String, String>,
) -> CliResult<()> {
    let src = std::fs::read_to_string(values_path)?;
    let descriptions = extract_values_yaml_descriptions(&src)?;

    for (path, description) in descriptions {
        out.insert(path, description);
    }

    Ok(())
}

#[instrument(skip_all)]
pub fn list_template_sources_for_define_index(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    files_with_role(chart_dir, include_tests, FileRole::DefineIndexTemplate)
}

fn files_with_role(
    chart_dir: &VfsPath,
    include_tests: bool,
    role: FileRole,
) -> CliResult<Vec<VfsPath>> {
    Ok(chart_files::list_chart_files(chart_dir, include_tests)?
        .into_iter()
        .filter(|file| file.has_role(role))
        .map(|file| file.path)
        .collect())
}

fn merge_values_at_prefix(root: &mut YamlValue, prefix: &[String], sub: YamlValue) {
    let target = ensure_mapping_path(root, prefix);

    if let YamlValue::Mapping(m) = target
        && let YamlValue::Mapping(sub_map) = sub
    {
        for (k, v) in sub_map {
            let entry = m.entry(k).or_insert(YamlValue::Null);
            *entry = merge_yaml_prefer_left(entry.clone(), v);
        }
    }
}

fn ensure_mapping_path<'a>(root: &'a mut YamlValue, path: &[String]) -> &'a mut YamlValue {
    let mut cur = root;

    for seg in path {
        if !matches!(cur, YamlValue::Mapping(_)) {
            *cur = YamlValue::Mapping(serde_yaml::Mapping::default());
        }

        let YamlValue::Mapping(m) = cur else {
            return cur;
        };

        let key = YamlValue::String(seg.clone());
        cur = m
            .entry(key)
            .or_insert_with(|| YamlValue::Mapping(serde_yaml::Mapping::default()));
    }

    cur
}

fn merge_yaml_prefer_left(a: YamlValue, b: YamlValue) -> YamlValue {
    match (a, b) {
        (YamlValue::Null, vb) => vb,
        (YamlValue::Mapping(mut ma), YamlValue::Mapping(mb)) => {
            for (k, vb) in mb {
                let entry = ma.entry(k).or_insert(YamlValue::Null);
                *entry = merge_yaml_prefer_left(entry.clone(), vb);
            }
            YamlValue::Mapping(ma)
        }
        (va, _) => va,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_activation_paths_are_scoped_from_chart_yaml() -> color_eyre::eyre::Result<()> {
        let chart_dir = VfsPath::new(vfs::MemoryFS::new());
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
}
