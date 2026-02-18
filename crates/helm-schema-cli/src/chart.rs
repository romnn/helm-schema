use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use helm_schema_ast::{DefineIndex, TreeSitterParser};
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use vfs::VfsPath;

use crate::error::{CliError, CliResult};

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
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

pub fn discover_chart_contexts(root_chart_dir: &VfsPath) -> CliResult<ChartDiscovery> {
    let mut out = Vec::new();
    discover_chart_contexts_inner(root_chart_dir, &[], &mut out)?;
    Ok(ChartDiscovery { charts: out })
}

fn discover_chart_contexts_inner(
    chart_dir: &VfsPath,
    parent_prefix: &[String],
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
    });

    let dep_key_by_name = dependency_key_map(&chart_yaml);

    let vendor_charts_dir = chart_dir.join("charts")?;
    if !vendor_charts_dir.is_dir()? {
        return Ok(());
    }

    for ent in vendor_charts_dir.read_dir()? {
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

        let key = dep_key_by_name
            .get(&sub_name)
            .cloned()
            .unwrap_or_else(|| sub_name.clone());

        let mut prefix = parent_prefix.to_vec();
        prefix.push(key);

        discover_chart_contexts_inner(&sub_dir, &prefix, out)?;
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
        let p = e.path()?;
        let path_str = p.to_string_lossy();
        let out = root.join(path_str.as_ref())?;
        if e.header().entry_type().is_dir() {
            out.create_dir_all()?;
        } else {
            out.parent().create_dir_all()?;
            let mut f = out.create_file()?;
            std::io::copy(&mut e, &mut f)?;
        }
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

    let entries = root.read_dir()?;
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

fn dependency_key_map(chart_yaml: &ChartYaml) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let deps = chart_yaml.dependencies.as_deref().unwrap_or_default();

    for d in deps {
        let key = d.alias.clone().unwrap_or_else(|| d.name.clone());
        out.insert(d.name.clone(), key);
    }

    out
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

pub fn build_define_index(charts: &[ChartContext], include_tests: bool) -> CliResult<DefineIndex> {
    let mut idx = DefineIndex::new();

    for c in charts {
        for path in list_template_sources_for_define_index(&c.chart_dir, include_tests)? {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;
            idx.add_source(&TreeSitterParser, &src)?;
        }

        // Some charts render manifests by loading YAML fragments from `files/` via `.Files.Get`.
        // Collect those sources so downstream IR generation can statically inline them when the
        // file path is a literal.
        for path in list_chart_files_sources(&c.chart_dir)? {
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

fn list_chart_files_sources(chart_dir: &VfsPath) -> CliResult<Vec<VfsPath>> {
    let files_dir = chart_dir.join("files")?;
    if !files_dir.is_dir()? {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    list_files_recursive(&files_dir, &mut out)?;

    out.retain(|p| {
        let file_name = p.filename();
        let ext = Path::new(&file_name).extension().and_then(|e| e.to_str());
        ext.is_some_and(|e| {
            e.eq_ignore_ascii_case("yaml")
                || e.eq_ignore_ascii_case("yml")
                || e.eq_ignore_ascii_case("tpl")
        })
    });

    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out.dedup_by(|a, b| a.as_str() == b.as_str());
    Ok(out)
}

fn list_files_recursive(dir: &VfsPath, out: &mut Vec<VfsPath>) -> CliResult<()> {
    for ent in dir.read_dir()? {
        if ent.is_dir()? {
            list_files_recursive(&ent, out)?;
        } else if ent.is_file()? {
            out.push(ent);
        }
    }

    Ok(())
}

pub fn list_manifest_templates(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    let templates_dir = chart_dir.join("templates")?;
    if !templates_dir.is_dir()? {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    list_templates_recursive(&templates_dir, include_tests, &mut out)?;

    out.retain(|p| {
        let file_name = p.filename();
        let lower = file_name.to_ascii_lowercase();
        if lower.starts_with('_') {
            return false;
        }

        let ext = Path::new(&file_name).extension().and_then(|e| e.to_str());
        ext.is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
    });

    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out.dedup_by(|a, b| a.as_str() == b.as_str());
    Ok(out)
}

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

fn list_template_sources_for_define_index(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    let templates_dir = chart_dir.join("templates")?;
    if !templates_dir.is_dir()? {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    list_templates_recursive(&templates_dir, include_tests, &mut out)?;

    out.retain(|p| {
        let file_name = p.filename();
        let ext = Path::new(&file_name).extension().and_then(|e| e.to_str());
        ext.is_some_and(|e| {
            e.eq_ignore_ascii_case("tpl")
                || e.eq_ignore_ascii_case("yaml")
                || e.eq_ignore_ascii_case("yml")
        })
    });

    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out.dedup_by(|a, b| a.as_str() == b.as_str());
    Ok(out)
}

fn list_templates_recursive(
    dir: &VfsPath,
    include_tests: bool,
    out: &mut Vec<VfsPath>,
) -> CliResult<()> {
    for ent in dir.read_dir()? {
        if ent.is_dir()? {
            if !include_tests {
                let name = ent.filename();
                let parent_name = ent.parent().filename();
                if name.eq_ignore_ascii_case("tests")
                    && parent_name.eq_ignore_ascii_case("templates")
                {
                    continue;
                }
            }

            list_templates_recursive(&ent, include_tests, out)?;
        } else if ent.is_file()? {
            out.push(ent);
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
