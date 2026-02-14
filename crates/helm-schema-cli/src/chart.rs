use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr, eyre};
use flate2::read::GzDecoder;
use helm_schema_ast::{DefineIndex, TreeSitterParser};
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: PathBuf,
    pub values_prefix: Vec<String>,
    pub is_library: bool,
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
    _temp_dirs: Vec<TempDir>,
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
}

pub fn discover_chart_contexts(root_chart_dir: &Path) -> Result<ChartDiscovery> {
    let mut out = Vec::new();
    let mut temp_dirs = Vec::new();
    discover_chart_contexts_inner(root_chart_dir, &[], &mut out, &mut temp_dirs)?;
    Ok(ChartDiscovery {
        charts: out,
        _temp_dirs: temp_dirs,
    })
}

fn discover_chart_contexts_inner(
    chart_dir: &Path,
    parent_prefix: &[String],
    out: &mut Vec<ChartContext>,
    temp_dirs: &mut Vec<TempDir>,
) -> Result<()> {
    let chart_yaml = read_chart_yaml(chart_dir)
        .wrap_err_with(|| format!("read Chart.yaml: {}", chart_dir.display()))?;

    let is_library = chart_yaml
        .chart_type
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("library"));

    out.push(ChartContext {
        chart_dir: chart_dir.to_path_buf(),
        values_prefix: parent_prefix.to_vec(),
        is_library,
    });

    let dep_key_by_name = dependency_key_map(&chart_yaml);

    let vendor_charts_dir = chart_dir.join("charts");
    if !vendor_charts_dir.is_dir() {
        return Ok(());
    }

    for ent in std::fs::read_dir(&vendor_charts_dir).wrap_err("read charts/ dir")? {
        let ent = ent.wrap_err("read charts/ entry")?;
        let path = ent.path();
        let ty = ent.file_type().wrap_err("read file type")?;

        let sub_dir = if ty.is_dir() {
            if !path.join("Chart.yaml").is_file() {
                continue;
            }
            path
        } else if ty.is_file() {
            let file_name = path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            let is_tgz = std::path::Path::new(file_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tgz"));
            let is_tar_gz = std::path::Path::new(file_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
                && std::path::Path::new(file_name)
                    .file_stem()
                    .and_then(|stem| std::path::Path::new(stem).extension())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"));

            if !(is_tgz || is_tar_gz) {
                continue;
            }

            let extracted = extract_chart_archive(&path)
                .wrap_err_with(|| format!("extract chart archive: {}", path.display()))?;
            let extracted_dir = extracted
                .chart_dir
                .clone();
            temp_dirs.push(extracted.temp_dir);
            extracted_dir
        } else {
            continue;
        };

        let sub_chart_yaml = read_chart_yaml(&sub_dir)
            .wrap_err_with(|| format!("read Chart.yaml: {}", sub_dir.display()))?;

        let sub_name = sub_chart_yaml
            .name
            .clone()
            .or_else(|| {
                sub_dir
                    .file_name()
                    .and_then(OsStr::to_str)
                    .map(std::string::ToString::to_string)
            })
            .ok_or_else(|| eyre!("subchart name missing for {}", sub_dir.display()))?;

        let key = dep_key_by_name
            .get(&sub_name)
            .cloned()
            .unwrap_or_else(|| sub_name.clone());

        let mut prefix = parent_prefix.to_vec();
        prefix.push(key);

        discover_chart_contexts_inner(&sub_dir, &prefix, out, temp_dirs)?;
    }

    Ok(())
}

struct ExtractedChart {
    chart_dir: PathBuf,
    temp_dir: TempDir,
}

fn extract_chart_archive(path: &Path) -> Result<ExtractedChart> {
    let file = std::fs::File::open(path)
        .wrap_err_with(|| format!("open archive: {}", path.display()))?;
    let gz = GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    let tmp = tempfile::tempdir().wrap_err("create temp dir")?;
    for entry in archive.entries().wrap_err("read tar entries")? {
        let mut entry = entry.wrap_err("read tar entry")?;
        entry.unpack_in(tmp.path()).wrap_err("unpack tar entry")?;
    }

    let chart_dir = find_chart_dir(tmp.path())
        .ok_or_else(|| eyre!("no Chart.yaml found in {}", path.display()))?;

    Ok(ExtractedChart {
        chart_dir,
        temp_dir: tmp,
    })
}

fn find_chart_dir(root: &Path) -> Option<PathBuf> {
    let direct = root.join("Chart.yaml");
    if direct.is_file() {
        return Some(root.to_path_buf());
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return None;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("Chart.yaml").is_file() {
            return Some(path);
        }
    }

    None
}

fn dependency_key_map(chart_yaml: &ChartYaml) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let deps = chart_yaml.dependencies.as_deref().unwrap_or_default();

    for d in deps {
        let key = d.alias.clone().unwrap_or_else(|| d.name.clone());
        out.insert(d.name.clone(), key);
    }

    out
}

fn read_chart_yaml(chart_dir: &Path) -> Result<ChartYaml> {
    let path = chart_dir.join("Chart.yaml");
    let bytes = std::fs::read(&path)
        .wrap_err_with(|| format!("read Chart.yaml: {}", path.display()))?;
    let doc: ChartYaml = serde_yaml::from_slice(&bytes)
        .wrap_err_with(|| format!("parse Chart.yaml: {}", path.display()))?;
    Ok(doc)
}

pub fn build_define_index(charts: &[ChartContext], include_tests: bool) -> Result<DefineIndex> {
    let mut idx = DefineIndex::new();

    for c in charts {
        for path in list_template_sources_for_define_index(&c.chart_dir, include_tests)? {
            let src = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("read template: {}", path.display()))?;
            idx.add_source(&TreeSitterParser, &src)
                .wrap_err_with(|| format!("parse define source: {}", path.display()))?;
        }
    }

    Ok(idx)
}

pub fn list_manifest_templates(chart_dir: &Path, include_tests: bool) -> Result<Vec<PathBuf>> {
    let templates_dir = chart_dir.join("templates");
    if !templates_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    list_templates_recursive(&templates_dir, include_tests, &mut out)?;

    out.retain(|p| {
        let file_name = p
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        if file_name.starts_with('_') {
            return false;
        }

        matches!(p.extension().and_then(OsStr::to_str), Some("yaml" | "yml"))
    });

    out.sort();
    out.dedup();
    Ok(out)
}

pub fn build_composed_values_yaml(
    charts: &[ChartContext],
    include_subchart_values: bool,
) -> Result<Option<String>> {
    let root = charts
        .first()
        .ok_or_else(|| eyre!("no charts discovered"))?;

    let root_values_path = root.chart_dir.join("values.yaml");
    let mut doc = if root_values_path.is_file() {
        let bytes = std::fs::read(&root_values_path)
            .wrap_err_with(|| format!("read values.yaml: {}", root_values_path.display()))?;
        serde_yaml::from_slice::<YamlValue>(&bytes)
            .wrap_err_with(|| format!("parse values.yaml: {}", root_values_path.display()))?
    } else {
        YamlValue::Mapping(serde_yaml::Mapping::default())
    };

    if include_subchart_values {
        for c in charts {
            if c.values_prefix.is_empty() {
                continue;
            }

            let path = c.chart_dir.join("values.yaml");
            if !path.is_file() {
                continue;
            }

            let bytes = std::fs::read(&path)
                .wrap_err_with(|| format!("read values.yaml: {}", path.display()))?;
            let mut sub_doc: YamlValue = serde_yaml::from_slice(&bytes)
                .wrap_err_with(|| format!("parse values.yaml: {}", path.display()))?;

            // Helm merges subchart defaults under the subchart key, except `global`,
            // which is merged into the top-level `.Values.global` for all charts.
            if let Some(global_doc) = take_global_key(&mut sub_doc) {
                merge_global_values(&mut doc, global_doc);
            }

            merge_values_at_prefix(&mut doc, &c.values_prefix, sub_doc);
        }
    }

    let serialized = serde_yaml::to_string(&doc).wrap_err("serialize values yaml")?;
    if serialized.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(serialized))
    }
}

fn list_template_sources_for_define_index(
    chart_dir: &Path,
    include_tests: bool,
) -> Result<Vec<PathBuf>> {
    let templates_dir = chart_dir.join("templates");
    if !templates_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    list_templates_recursive(&templates_dir, include_tests, &mut out)?;

    out.retain(|p| {
        matches!(
            p.extension().and_then(OsStr::to_str),
            Some("tpl" | "yaml" | "yml")
        )
    });

    out.sort();
    out.dedup();
    Ok(out)
}

fn list_templates_recursive(dir: &Path, include_tests: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    for ent in std::fs::read_dir(dir)
        .wrap_err_with(|| format!("read templates dir: {}", dir.display()))?
    {
        let ent = ent.wrap_err("read dir entry")?;
        let path = ent.path();
        let ty = ent.file_type().wrap_err("read file type")?;

        if ty.is_dir() {
            if !include_tests
                && let Some(name) = path.file_name().and_then(OsStr::to_str)
                && name.eq_ignore_ascii_case("tests")
                && path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(OsStr::to_str)
                    .is_some_and(|p| p.eq_ignore_ascii_case("templates"))
            {
                continue;
            }

            list_templates_recursive(&path, include_tests, out)?;
        } else if ty.is_file() {
            out.push(path);
        }
    }

    Ok(())
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
