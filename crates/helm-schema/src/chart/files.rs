use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::read::GzDecoder;
use vfs::VfsPath;

use super::directory::Directory;
use super::discovery::{is_chart_archive, validate_archive_entry_path};
use crate::error::{CliError, EngineResult};
use crate::load_budget::LoadBudget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FileRole {
    ManifestTemplate,
    DefineIndexTemplate,
    StaticCrd,
    FilesGetSource,
    NotesTemplate,
}

/// A chart's view into a directory or archive captured at the input boundary.
/// Vendored directories share their containing source's entries.
#[derive(Debug, Clone)]
pub(crate) struct LoadedChart {
    entries: Arc<LoadedEntries>,
    prefix: String,
    path: String,
    include_tests: bool,
}

#[derive(Debug)]
enum LoadedEntries {
    Directory(Directory),
    Archive {
        source_path: SourcePath,
        entries: Vec<Entry>,
        visible: Vec<usize>,
    },
}

#[derive(Debug)]
struct Entry {
    name: String,
    original_archive_name: Box<[u8]>,
    bytes: Arc<[u8]>,
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedFile {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) bytes: Arc<[u8]>,
}

impl LoadedFile {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn source(&self) -> EngineResult<&str> {
        std::str::from_utf8(&self.bytes).map_err(|_| CliError::NonUtf8ChartSource {
            path: self.path.clone(),
        })
    }

    pub(crate) fn yaml_text(&self) -> EngineResult<&str> {
        std::str::from_utf8(&self.bytes).map_err(|_| {
            let error = std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            );
            vfs::VfsError::from(error)
                .with_context(|| format!("Could not read path '{}'", self.path))
                .into()
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SourcePath {
    Native(PathBuf),
    Virtual { root: VfsPath, path: PathBuf },
}

impl SourcePath {
    fn join_member(&self, member: &str) -> Self {
        match self {
            Self::Native(path) => Self::Native(path.join(member)),
            Self::Virtual { root, path } => Self::Virtual {
                root: root.clone(),
                path: path.join(member),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedRootFile {
    pub(crate) name: String,
    pub(crate) source: String,
}

impl LoadedChart {
    #[tracing::instrument(skip_all)]
    pub(crate) fn read_directory(
        root: &VfsPath,
        native_path: Option<&Path>,
        prepared_file: Option<&PreparedRootFile>,
        include_tests: bool,
    ) -> EngineResult<Self> {
        let directory = Directory::new(root, native_path, prepared_file)?;
        Ok(Self {
            entries: Arc::new(LoadedEntries::Directory(directory)),
            prefix: String::new(),
            path: root.as_str().to_string(),
            include_tests,
        })
    }

    pub(crate) fn read_archive(
        bytes: &[u8],
        archive_path: &str,
        source_path: SourcePath,
        budget: LoadBudget,
        include_tests: bool,
    ) -> EngineResult<Self> {
        if bytes.len() > budget.max_chart_archive_bytes {
            return Err(CliError::LoadBudgetExceeded {
                subject: archive_path.to_owned(),
                limit_bytes: budget.max_chart_archive_bytes,
            });
        }
        let mut archive = tar::Archive::new(GzDecoder::new(bytes));
        let mut entries = Vec::new();
        let mut paths = BTreeMap::new();
        let mut remaining = budget.max_chart_archive_unpacked_bytes;
        for entry in archive.entries()? {
            let entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            if entries.len() >= budget.max_chart_archive_entries {
                return Err(CliError::LoadEntryBudgetExceeded {
                    subject: archive_path.to_string(),
                    limit_entries: budget.max_chart_archive_entries,
                });
            }

            let original_archive_name = entry.path_bytes().into_owned().into_boxed_slice();
            let path = entry.path()?;
            validate_archive_entry_path(archive_path, &path)?;
            let raw_name = path.to_string_lossy();
            let name = raw_name
                .split('/')
                .filter(|part| !part.is_empty() && *part != ".")
                .collect::<Vec<_>>()
                .join("/");
            if name.is_empty() || raw_name.ends_with('/') {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid file name {raw_name} in archive {archive_path}"),
                )
                .into());
            }
            let mut parent = name.as_str();
            while let Some((dir, _)) = parent.rsplit_once('/') {
                if paths.entry(dir.to_string()).or_insert(None).is_some() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!("{dir} is already a file in archive {archive_path}"),
                    )
                    .into());
                }
                parent = dir;
            }
            paths.insert(name.clone(), Some(entries.len()));

            let mut bytes = Vec::new();
            let limit = u64::try_from(remaining)
                .unwrap_or(u64::MAX)
                .saturating_add(1);
            entry.take(limit).read_to_end(&mut bytes)?;
            remaining =
                remaining
                    .checked_sub(bytes.len())
                    .ok_or_else(|| CliError::LoadBudgetExceeded {
                        subject: format!("expanded {archive_path}"),
                        limit_bytes: budget.max_chart_archive_unpacked_bytes,
                    })?;
            entries.push(Entry {
                name,
                original_archive_name,
                bytes: Arc::from(bytes),
            });
        }

        // Extraction exposed the last file at each name in lexical order.
        // Keep that view for analysis while retaining every original member.
        let visible = paths
            .iter()
            .filter_map(|(name, index)| {
                let mut parent = name.as_str();
                while let Some((dir, _)) = parent.rsplit_once('/') {
                    if paths.get(dir).is_some_and(Option::is_some) {
                        return None;
                    }
                    parent = dir;
                }
                *index
            })
            .collect();
        let root = Self {
            entries: Arc::new(LoadedEntries::Archive {
                source_path,
                entries,
                visible,
            }),
            prefix: String::new(),
            path: String::new(),
            include_tests,
        };
        if root.has_archive_manifest() {
            return Ok(root);
        }

        // Match the directory view's first top-level chart in lexical order.
        root.child_directories()
            .into_iter()
            .map(|name| root.archive_subdirectory(name))
            .find(Self::has_archive_manifest)
            .ok_or_else(|| CliError::NoChartYamlInArchive {
                archive: archive_path.to_string(),
            })
    }

    pub(crate) fn analysis_view(&self, include_tests: bool) -> Self {
        Self {
            include_tests,
            ..self.clone()
        }
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn file_path(&self, name: &str) -> String {
        format!("{}/{name}", self.path)
    }

    pub(crate) fn filename(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or_default()
    }

    pub(crate) fn manifest(&self) -> EngineResult<LoadedFile> {
        if let Some(file) = self.file("Chart.yaml")? {
            return Ok(file);
        }
        if let Some(file) = self.file("Chart.template.yaml")? {
            return Ok(file);
        }
        match self.entries.as_ref() {
            LoadedEntries::Directory(dir) => dir.required_manifest(),
            LoadedEntries::Archive {
                source_path,
                entries: _,
                visible: _,
            } => Err(CliError::NoChartYamlInArchive {
                archive: format!("{source_path:?}"),
            }),
        }
    }

    pub(crate) fn file(&self, name: &str) -> EngineResult<Option<LoadedFile>> {
        match self.entries.as_ref() {
            LoadedEntries::Directory(dir) => dir.file(name),
            LoadedEntries::Archive {
                source_path: _,
                entries: _,
                visible: _,
            } => Ok(self
                .archive_entries_under(name)
                .next()
                .filter(|(actual, _)| *actual == name)
                .map(|(name, entry)| self.archive_file(name, entry))),
        }
    }

    pub(crate) fn load_local_file(&self, path: &Path) -> EngineResult<Option<Arc<[u8]>>> {
        match self.entries.as_ref() {
            LoadedEntries::Directory(dir) => dir.load_local_file(path),
            LoadedEntries::Archive {
                source_path: _,
                entries: _,
                visible: _,
            } => Ok(None),
        }
    }

    pub(crate) fn prepare_roles(&self) -> EngineResult<()> {
        if let LoadedEntries::Directory(dir) = self.entries.as_ref() {
            dir.prepare_roles(self.include_tests)?;
        }
        Ok(())
    }

    pub(crate) fn files_with_role(&self, role: FileRole) -> EngineResult<Vec<LoadedFile>> {
        if let LoadedEntries::Directory(dir) = self.entries.as_ref() {
            return dir.files_with_role(role);
        }
        let prefix = match role {
            FileRole::DefineIndexTemplate
            | FileRole::ManifestTemplate
            | FileRole::NotesTemplate => "templates/",
            FileRole::StaticCrd => "crds/",
            FileRole::FilesGetSource => "",
        };
        Ok(self
            .archive_entries_under(prefix)
            .filter(|(name, _)| has_role(name, role, self.include_tests))
            .map(|(name, entry)| self.archive_file(name, entry))
            .collect())
    }

    pub(crate) fn visit_vendored_charts(
        &self,
        budget: LoadBudget,
        mut visit: impl FnMut(Self) -> EngineResult<()>,
    ) -> EngineResult<()> {
        match self.entries.as_ref() {
            LoadedEntries::Directory(dir) => {
                let vendor = dir.path.join("charts")?;
                if !vendor.is_dir()? {
                    return Ok(());
                }
                let mut entries = dir.children(&vendor)?.to_vec();
                entries.sort_by_key(VfsPath::filename);
                for entry in entries {
                    if entry.is_dir()? {
                        if entry.join("Chart.yaml")?.is_file()?
                            || entry.join("Chart.template.yaml")?.is_file()?
                        {
                            let path = entry.as_str().to_string();
                            visit(Self {
                                entries: Arc::new(LoadedEntries::Directory(dir.subchart(entry))),
                                prefix: String::new(),
                                path,
                                include_tests: self.include_tests,
                            })?;
                        }
                    } else if entry.is_file()? && is_chart_archive(&entry.filename()) {
                        let bytes = dir.read(&entry, Some(budget.max_chart_archive_bytes))?;
                        visit(Self::read_archive(
                            &bytes,
                            entry.as_str(),
                            dir.source_path(&entry),
                            budget,
                            self.include_tests,
                        )?)?;
                    }
                }
            }
            LoadedEntries::Archive {
                source_path,
                entries: _,
                visible: _,
            } => {
                let mut children = BTreeMap::new();
                for (relative, entry) in self.archive_entries_under("charts/") {
                    let Some(name) = relative.strip_prefix("charts/") else {
                        continue;
                    };
                    if let Some((dir, _)) = name.split_once('/') {
                        children.entry(dir).or_insert(None);
                    } else if is_chart_archive(name) {
                        children.insert(name, Some(entry));
                    }
                }
                for (name, package) in children {
                    let name = format!("charts/{name}");
                    if let Some(package) = package {
                        let original = String::from_utf8_lossy(&package.original_archive_name);
                        let source = source_path.join_member(&original);
                        visit(Self::read_archive(
                            &package.bytes,
                            &self.file_path(&name),
                            source,
                            budget,
                            self.include_tests,
                        )?)?;
                    } else {
                        let child = self.archive_subdirectory(&name);
                        if child.has_archive_manifest() {
                            visit(child)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn has_archive_manifest(&self) -> bool {
        ["Chart.yaml", "Chart.template.yaml"]
            .into_iter()
            .any(|manifest| {
                self.archive_entries_under(manifest)
                    .any(|(name, _)| name == manifest)
            })
    }

    fn archive_subdirectory(&self, name: &str) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
            prefix: format!("{}{name}/", self.prefix),
            path: self.file_path(name),
            include_tests: self.include_tests,
        }
    }

    fn child_directories(&self) -> BTreeSet<&str> {
        self.archive_entries_under("")
            .filter_map(|(name, _)| name.split_once('/').map(|(dir, _)| dir))
            .collect()
    }

    fn archive_file(&self, name: &str, entry: &Entry) -> LoadedFile {
        LoadedFile {
            name: name.to_string(),
            path: self.file_path(name),
            bytes: Arc::clone(&entry.bytes),
        }
    }

    fn archive_entries_under<'a>(
        &'a self,
        relative_prefix: &str,
    ) -> impl Iterator<Item = (&'a str, &'a Entry)> + use<'a> {
        let (entries, visible): (&[Entry], &[usize]) = match self.entries.as_ref() {
            LoadedEntries::Directory(_) => (&[], &[]),
            LoadedEntries::Archive {
                source_path: _,
                entries,
                visible,
            } => (entries, visible),
        };
        let prefix = format!("{}{relative_prefix}", self.prefix);
        let start = visible
            .partition_point(|index| entries.get(*index).is_some_and(|entry| entry.name < prefix));
        visible
            .iter()
            .skip(start)
            .filter_map(|index| entries.get(*index))
            .take_while(move |entry| entry.name.starts_with(&prefix))
            .filter_map(|entry| Some((entry.name.strip_prefix(&self.prefix)?, entry)))
    }
}

pub(super) fn has_role(name: &str, role: FileRole, include_tests: bool) -> bool {
    let basename = name.rsplit('/').next().unwrap_or_default();
    match role {
        FileRole::DefineIndexTemplate | FileRole::ManifestTemplate | FileRole::NotesTemplate => {
            let Some(template) = name.strip_prefix("templates/") else {
                return false;
            };
            let (dirs, _) = template.rsplit_once('/').unwrap_or(("", template));
            if !include_tests && within_template_tests(dirs) {
                return false;
            }
            match role {
                FileRole::DefineIndexTemplate => true,
                FileRole::ManifestTemplate => {
                    !basename.starts_with('_') && extension_is_one_of(basename, &["yaml", "yml"])
                }
                FileRole::NotesTemplate => basename == "NOTES.txt",
                FileRole::StaticCrd | FileRole::FilesGetSource => false,
            }
        }
        FileRole::StaticCrd => extension_is_one_of(basename, &["json", "yaml", "yml"]),
        FileRole::FilesGetSource => {
            let (dirs, _) = name.rsplit_once('/').unwrap_or(("", name));
            !dirs.split('/').any(excluded_files_directory)
                && !matches!(
                    basename,
                    ".helmignore"
                        | "Chart.lock"
                        | "Chart.yaml"
                        | "values.schema.json"
                        | "values.yaml"
                )
        }
    }
}

pub(super) fn excluded_files_directory(name: &str) -> bool {
    name.eq_ignore_ascii_case("charts") || name.eq_ignore_ascii_case("templates")
}

fn within_template_tests(dirs: &str) -> bool {
    let mut in_templates = true;
    for dir in dirs.split('/') {
        if in_templates && dir.eq_ignore_ascii_case("tests") {
            return true;
        }
        in_templates = dir.eq_ignore_ascii_case("templates");
    }
    false
}

fn extension_is_one_of(name: &str, extensions: &[&str]) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            extensions
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
#[path = "tests/files.rs"]
mod tests;
