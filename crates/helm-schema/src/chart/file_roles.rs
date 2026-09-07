use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::Path;

use vfs::VfsPath;

use crate::error::CliError;
use crate::error::EngineResult;

use super::ChartContext;

/// Structural role a file plays in chart analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum FileRole {
    /// A rendered manifest template under `templates/`.
    ManifestTemplate,
    /// A template source that can define or call helpers.
    DefineIndexTemplate,
    /// A static CRD document under `crds/`.
    StaticCrd,
    /// A static YAML/template fragment reachable through `.Files.Get`.
    FilesGetSource,
    /// `templates/NOTES.txt`: Helm executes it at install/upgrade time, so
    /// its consumers and terminal effects are schema evidence, while its
    /// prose stays out of YAML resource detection.
    NotesTemplate,
}

#[derive(Debug, Clone)]
pub(crate) struct ChartFile {
    pub(crate) path: VfsPath,
    roles: BTreeSet<FileRole>,
}

#[derive(Debug)]
pub(crate) struct LoadedChartFile {
    pub(crate) path: VfsPath,
    roles: BTreeSet<FileRole>,
    source: Option<String>,
}

impl LoadedChartFile {
    pub(crate) fn source(&self) -> EngineResult<&str> {
        self.source
            .as_deref()
            .ok_or_else(|| CliError::NonUtf8ChartSource {
                path: self.path.as_str().to_string(),
            })
    }

    pub(crate) fn utf8_source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub(crate) fn has_role(&self, role: FileRole) -> bool {
        self.roles.contains(&role)
    }
}

#[derive(Debug)]
pub(crate) struct LoadedChart {
    files: Vec<LoadedChartFile>,
}

impl LoadedChart {
    pub(crate) fn files_with_role(&self, role: FileRole) -> impl Iterator<Item = &LoadedChartFile> {
        self.files.iter().filter(move |file| file.has_role(role))
    }
}

#[derive(Debug)]
pub(crate) struct LoadedChartCorpus {
    charts: BTreeMap<String, LoadedChart>,
}

impl LoadedChartCorpus {
    #[tracing::instrument(skip_all)]
    pub(crate) fn load(charts: &[ChartContext], include_tests: bool) -> EngineResult<Self> {
        let mut loaded = BTreeMap::new();
        for chart in charts {
            let mut files = Vec::new();
            for file in list_chart_files(chart, include_tests)? {
                let mut bytes = Vec::new();
                file.path.open_file()?.read_to_end(&mut bytes)?;
                files.push(LoadedChartFile {
                    path: file.path,
                    roles: file.roles,
                    source: String::from_utf8(bytes).ok(),
                });
            }
            loaded.insert(chart.chart_dir.as_str().to_string(), LoadedChart { files });
        }
        Ok(Self { charts: loaded })
    }

    pub(crate) fn chart(&self, chart: &ChartContext) -> EngineResult<&LoadedChart> {
        self.charts
            .get(chart.chart_dir.as_str())
            .ok_or_else(|| CliError::LoadedChartMissing {
                path: chart.chart_dir.as_str().to_string(),
            })
    }
}

type ChartFileMap = BTreeMap<String, ChartFile>;

#[tracing::instrument(skip_all)]
pub(crate) fn list_chart_files(
    chart: &ChartContext,
    include_tests: bool,
) -> EngineResult<Vec<ChartFile>> {
    let mut files = ChartFileMap::new();
    let chart_dir = &chart.chart_dir;

    collect_template_roles(chart, include_tests, &mut files)?;
    collect_directory_roles(
        chart_dir,
        "crds",
        FileRole::StaticCrd,
        is_static_crd_source,
        &mut files,
    )?;
    collect_files_get_roles(chart_dir, &mut files)?;

    Ok(files.into_values().collect())
}

fn collect_template_roles(
    chart: &ChartContext,
    include_tests: bool,
    files: &mut ChartFileMap,
) -> EngineResult<()> {
    let templates_dir = chart.chart_dir.join("templates")?;
    if !templates_dir.is_dir()? {
        return Ok(());
    }

    let mut paths = Vec::new();
    list_files_recursive(&templates_dir, include_tests, &mut paths)?;

    for path in paths {
        // Helm does not register non-partial library templates, including their define blocks.
        if chart.is_library && !path.filename().starts_with('_') {
            continue;
        }
        insert_role(files, path.clone(), FileRole::DefineIndexTemplate);
        if is_notes_template(&path) {
            insert_role(files, path.clone(), FileRole::NotesTemplate);
        }
        if is_manifest_template(&path) {
            insert_role(files, path, FileRole::ManifestTemplate);
        }
    }

    Ok(())
}

fn collect_directory_roles(
    chart_dir: &VfsPath,
    dir_name: &str,
    role: FileRole,
    accept: fn(&VfsPath) -> bool,
    files: &mut ChartFileMap,
) -> EngineResult<()> {
    let dir = chart_dir.join(dir_name)?;
    if !dir.is_dir()? {
        return Ok(());
    }

    let mut paths = Vec::new();
    list_files_recursive(&dir, true, &mut paths)?;

    for path in paths.into_iter().filter(accept) {
        insert_role(files, path, role);
    }

    Ok(())
}

fn insert_role(files: &mut ChartFileMap, path: VfsPath, role: FileRole) {
    let key = path.as_str().to_string();
    files
        .entry(key)
        .and_modify(|file| {
            file.roles.insert(role);
        })
        .or_insert_with(|| ChartFile {
            path,
            roles: BTreeSet::from([role]),
        });
}

fn is_manifest_template(path: &VfsPath) -> bool {
    let file_name = path.filename();
    if file_name.to_ascii_lowercase().starts_with('_') {
        return false;
    }

    extension_is_one_of(path, &["yaml", "yml"])
}

fn is_notes_template(path: &VfsPath) -> bool {
    path.filename() == "NOTES.txt"
}

fn is_static_crd_source(path: &VfsPath) -> bool {
    extension_is_one_of(path, &["json", "yaml", "yml"])
}

fn collect_files_get_roles(chart_dir: &VfsPath, files: &mut ChartFileMap) -> EngineResult<()> {
    let mut paths = Vec::new();
    list_files_recursive_excluding(chart_dir, &["charts", "templates"], &mut paths)?;

    for path in paths.into_iter().filter(is_files_get_source) {
        insert_role(files, path, FileRole::FilesGetSource);
    }

    Ok(())
}

fn is_files_get_source(path: &VfsPath) -> bool {
    !matches!(
        path.filename().as_str(),
        ".helmignore" | "Chart.lock" | "Chart.yaml" | "values.schema.json" | "values.yaml"
    )
}

fn extension_is_one_of(path: &VfsPath, extensions: &[&str]) -> bool {
    let file_name = path.filename();
    let ext = Path::new(&file_name)
        .extension()
        .and_then(|ext| ext.to_str());
    ext.is_some_and(|ext| {
        extensions
            .iter()
            .any(|candidate| ext.eq_ignore_ascii_case(candidate))
    })
}

fn list_files_recursive(
    dir: &VfsPath,
    include_tests: bool,
    out: &mut Vec<VfsPath>,
) -> EngineResult<()> {
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

            list_files_recursive(&ent, include_tests, out)?;
        } else if ent.is_file()? {
            out.push(ent);
        }
    }

    Ok(())
}

fn list_files_recursive_excluding(
    dir: &VfsPath,
    excluded_dir_names: &[&str],
    out: &mut Vec<VfsPath>,
) -> EngineResult<()> {
    for entry in dir.read_dir()? {
        if entry.is_dir()? {
            if excluded_dir_names
                .iter()
                .any(|name| entry.filename().eq_ignore_ascii_case(name))
            {
                continue;
            }
            list_files_recursive_excluding(&entry, excluded_dir_names, out)?;
        } else if entry.is_file()? {
            out.push(entry);
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/file_roles.rs"]
mod tests;
