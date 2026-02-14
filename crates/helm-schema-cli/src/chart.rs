use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use color_eyre::eyre::{Result, WrapErr, eyre};
use flate2::read::GzDecoder;
use helm_schema_ast::{DefineIndex, TreeSitterParser};
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use vfs::VfsPath;

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

fn vfs<T>(res: std::result::Result<T, vfs::VfsError>) -> Result<T> {
    res.map_err(|e| eyre!(e.to_string()))
}

pub fn discover_chart_contexts(root_chart_dir: &VfsPath) -> Result<ChartDiscovery> {
    let mut out = Vec::new();
    discover_chart_contexts_inner(root_chart_dir, &[], &mut out)?;
    Ok(ChartDiscovery { charts: out })
}

fn discover_chart_contexts_inner(
    chart_dir: &VfsPath,
    parent_prefix: &[String],
    out: &mut Vec<ChartContext>,
) -> Result<()> {
    let chart_yaml = read_chart_yaml(chart_dir)
        .wrap_err_with(|| format!("read Chart.yaml: {}", chart_dir.as_str()))?;

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

    let vendor_charts_dir = vfs(chart_dir.join("charts")).wrap_err("join charts/")?;
    if !vfs(vendor_charts_dir.is_dir()).wrap_err("stat charts/")? {
        return Ok(());
    }

    for ent in vfs(vendor_charts_dir.read_dir()).wrap_err("read charts/ dir")? {
        let sub_dir = if vfs(ent.is_dir()).wrap_err("stat charts/ entry")? {
            let chart_yaml_path = vfs(ent.join("Chart.yaml")).wrap_err("join Chart.yaml")?;
            if !vfs(chart_yaml_path.is_file()).wrap_err("stat Chart.yaml")? {
                continue;
            }
            ent
        } else if vfs(ent.is_file()).wrap_err("stat charts/ entry")? {
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

            extract_chart_archive(&ent)
                .wrap_err_with(|| format!("extract chart archive: {}", ent.as_str()))?
        } else {
            continue;
        };

        let sub_chart_yaml = read_chart_yaml(&sub_dir)
            .wrap_err_with(|| format!("read Chart.yaml: {}", sub_dir.as_str()))?;

        let sub_name = sub_chart_yaml
            .name
            .clone()
            .or_else(|| {
                let name = sub_dir.filename();
                if name.is_empty() { None } else { Some(name) }
            })
            .ok_or_else(|| eyre!("subchart name missing for {}", sub_dir.as_str()))?;

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

fn extract_chart_archive(path: &VfsPath) -> Result<VfsPath> {
    let mut bytes = Vec::new();
    vfs(path.open_file())
        .wrap_err_with(|| format!("open archive: {}", path.as_str()))?
        .read_to_end(&mut bytes)
        .wrap_err_with(|| format!("read archive: {}", path.as_str()))?;

    let gz = GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(gz);

    let root = VfsPath::new(vfs::MemoryFS::new());
    for entry in archive.entries().wrap_err("read tar entries")? {
        let mut e = entry.wrap_err("read tar entry")?;
        let p = e.path().wrap_err("read tar entry path")?;
        let path_str = p.to_string_lossy();
        let out = vfs(root.join(path_str.as_ref())).wrap_err("join unpack path")?;
        if e.header().entry_type().is_dir() {
            vfs(out.create_dir_all()).wrap_err("create dir")?;
        } else {
            vfs(out.parent().create_dir_all()).wrap_err("create parent dirs")?;
            let mut f = vfs(out.create_file()).wrap_err("create file")?;
            std::io::copy(&mut e, &mut f).wrap_err("copy entry")?;
        }
    }

    find_chart_dir(&root)?.ok_or_else(|| eyre!("no Chart.yaml found in {}", path.as_str()))
}

fn find_chart_dir(root: &VfsPath) -> Result<Option<VfsPath>> {
    let direct = vfs(root.join("Chart.yaml")).wrap_err("join Chart.yaml")?;
    if vfs(direct.is_file()).wrap_err("stat Chart.yaml")? {
        return Ok(Some(root.clone()));
    }

    let entries = vfs(root.read_dir()).wrap_err("read extracted root")?;
    for ent in entries {
        if !vfs(ent.is_dir()).wrap_err("stat extracted entry")? {
            continue;
        }
        let chart_yaml = vfs(ent.join("Chart.yaml")).wrap_err("join Chart.yaml")?;
        if vfs(chart_yaml.is_file()).wrap_err("stat Chart.yaml")? {
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

fn read_chart_yaml(chart_dir: &VfsPath) -> Result<ChartYaml> {
    let path = vfs(chart_dir.join("Chart.yaml")).wrap_err("join Chart.yaml")?;
    let mut bytes = Vec::new();
    vfs(path.open_file())
        .wrap_err_with(|| format!("open Chart.yaml: {}", path.as_str()))?
        .read_to_end(&mut bytes)
        .wrap_err_with(|| format!("read Chart.yaml: {}", path.as_str()))?;

    let doc: ChartYaml = serde_yaml::from_slice(&bytes)
        .wrap_err_with(|| format!("parse Chart.yaml: {}", path.as_str()))?;
    Ok(doc)
}

pub fn build_define_index(charts: &[ChartContext], include_tests: bool) -> Result<DefineIndex> {
    let mut idx = DefineIndex::new();

    for c in charts {
        for path in list_template_sources_for_define_index(&c.chart_dir, include_tests)? {
            let mut src = String::new();
            vfs(path.open_file())
                .wrap_err_with(|| format!("open template: {}", path.as_str()))?
                .read_to_string(&mut src)
                .wrap_err_with(|| format!("read template: {}", path.as_str()))?;
            idx.add_source(&TreeSitterParser, &src)
                .wrap_err_with(|| format!("parse define source: {}", path.as_str()))?;
        }
    }

    Ok(idx)
}

pub fn list_manifest_templates(chart_dir: &VfsPath, include_tests: bool) -> Result<Vec<VfsPath>> {
    let templates_dir = vfs(chart_dir.join("templates")).wrap_err("join templates/")?;
    if !vfs(templates_dir.is_dir()).wrap_err("stat templates/")? {
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
) -> Result<Option<String>> {
    let root = charts
        .first()
        .ok_or_else(|| eyre!("no charts discovered"))?;

    let root_values_path = vfs(root.chart_dir.join("values.yaml")).wrap_err("join values.yaml")?;
    let mut doc = if vfs(root_values_path.is_file()).wrap_err("stat values.yaml")? {
        let mut bytes = Vec::new();
        vfs(root_values_path.open_file())
            .wrap_err_with(|| format!("open values.yaml: {}", root_values_path.as_str()))?
            .read_to_end(&mut bytes)
            .wrap_err_with(|| format!("read values.yaml: {}", root_values_path.as_str()))?;
        serde_yaml::from_slice::<YamlValue>(&bytes)
            .wrap_err_with(|| format!("parse values.yaml: {}", root_values_path.as_str()))?
    } else {
        YamlValue::Mapping(serde_yaml::Mapping::default())
    };

    if include_subchart_values {
        for c in charts {
            if c.values_prefix.is_empty() {
                continue;
            }

            let path = vfs(c.chart_dir.join("values.yaml")).wrap_err("join values.yaml")?;
            if !vfs(path.is_file()).wrap_err("stat values.yaml")? {
                continue;
            }

            let mut bytes = Vec::new();
            vfs(path.open_file())
                .wrap_err_with(|| format!("open values.yaml: {}", path.as_str()))?
                .read_to_end(&mut bytes)
                .wrap_err_with(|| format!("read values.yaml: {}", path.as_str()))?;
            let mut sub_doc: YamlValue = serde_yaml::from_slice(&bytes)
                .wrap_err_with(|| format!("parse values.yaml: {}", path.as_str()))?;

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
    chart_dir: &VfsPath,
    include_tests: bool,
) -> Result<Vec<VfsPath>> {
    let templates_dir = vfs(chart_dir.join("templates")).wrap_err("join templates/")?;
    if !vfs(templates_dir.is_dir()).wrap_err("stat templates/")? {
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
) -> Result<()> {
    for ent in
        vfs(dir.read_dir()).wrap_err_with(|| format!("read templates dir: {}", dir.as_str()))?
    {
        if vfs(ent.is_dir()).wrap_err("stat template entry")? {
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
        } else if vfs(ent.is_file()).wrap_err("stat template entry")? {
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
