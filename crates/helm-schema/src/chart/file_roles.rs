use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use vfs::VfsPath;

use crate::error::EngineResult;

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

impl ChartFile {
    pub(crate) fn has_role(&self, role: FileRole) -> bool {
        self.roles.contains(&role)
    }
}

type ChartFileMap = BTreeMap<String, ChartFile>;

/// List the chart files that play `role`, in stable path order.
pub(crate) fn files_with_role(
    chart_dir: &VfsPath,
    include_tests: bool,
    role: FileRole,
) -> EngineResult<Vec<VfsPath>> {
    Ok(list_chart_files(chart_dir, include_tests)?
        .into_iter()
        .filter(|file| file.has_role(role))
        .map(|file| file.path)
        .collect())
}

#[tracing::instrument(skip_all)]
pub(crate) fn list_chart_files(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> EngineResult<Vec<ChartFile>> {
    let mut files = ChartFileMap::new();

    collect_template_roles(chart_dir, include_tests, &mut files)?;
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
    chart_dir: &VfsPath,
    include_tests: bool,
    files: &mut ChartFileMap,
) -> EngineResult<()> {
    let templates_dir = chart_dir.join("templates")?;
    if !templates_dir.is_dir()? {
        return Ok(());
    }

    let mut paths = Vec::new();
    list_files_recursive(&templates_dir, include_tests, &mut paths)?;

    for path in paths {
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
