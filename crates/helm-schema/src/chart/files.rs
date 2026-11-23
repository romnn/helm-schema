use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bstr::ByteSlice;
use flate2::read::MultiGzDecoder;
use serde::Deserialize as _;
use vfs::VfsPath;

use super::directory::Directory;
use crate::error::{CliError, EngineResult};
use crate::load_budget::read_to_end_capped;

pub(super) const MAX_FILE_SIZE: usize = 5 * 1024 * 1024;
const MAX_ARCHIVE_SIZE: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub(crate) enum FileRole {
    ManifestTemplate,
    DefineIndexTemplate,
    StaticCrd,
    FilesGetSource,
    NotesTemplate,
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedChart {
    entries: Arc<LoadedEntries>,
    directory: Option<Arc<Directory>>,
    pub(crate) path: String,
    include_tests: bool,
    last: Arc<BTreeMap<Vec<u8>, usize>>,
}

#[derive(Debug, Default)]
struct LoadedEntries {
    files: Vec<Entry>,
    mounts: BTreeMap<Vec<u8>, Vec<Entry>>,
}

#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) file: LoadedFile,
    pub(super) source: SourcePath,
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedFile {
    pub(crate) name: Vec<u8>,
    pub(crate) path: String,
    pub(crate) bytes: Arc<[u8]>,
}

impl LoadedFile {
    pub(super) fn new(name: Vec<u8>, path: String, bytes: Arc<[u8]>) -> Self {
        let bytes = if let Some(data) = bytes.strip_prefix(b"\xef\xbb\xbf") {
            Arc::from(data)
        } else {
            bytes
        };
        Self { name, path, bytes }
    }

    pub(crate) fn text_name(&self) -> Option<&str> {
        std::str::from_utf8(&self.name).ok()
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
    Virtual(VfsPath),
    ArchiveMember {
        archive: Arc<Self>,
        original_name: Box<[u8]>,
    },
}

impl SourcePath {
    fn label(&self) -> String {
        match self {
            Self::Native(path) => path.display().to_string(),
            Self::Virtual(path) => path.as_str().to_owned(),
            Self::ArchiveMember {
                archive,
                original_name,
            } => format!("{}:{}", archive.label(), original_name.as_bstr()),
        }
    }
}

impl LoadedChart {
    pub(crate) fn read_directory(
        directory: Arc<Directory>,
        include_tests: bool,
    ) -> EngineResult<Self> {
        let path = directory.path().to_owned();
        let entries = directory.entries()?;
        Ok(Self::from_entries(
            entries,
            path,
            Some(directory),
            include_tests,
        ))
    }

    pub(crate) fn read_archive(
        input: impl Read,
        source: SourcePath,
        include_tests: bool,
    ) -> EngineResult<Self> {
        let path = source.label();
        let entries = archive_entries(input, &path, &Arc::new(source))?;
        let chart = Self::from_entries(entries, String::new(), None, include_tests);
        if chart.file("Chart.yaml").is_none() {
            return Err(CliError::NoChartYamlInArchive { archive: path });
        }
        Ok(chart)
    }

    fn from_entries(
        entries: Vec<Entry>,
        path: String,
        directory: Option<Arc<Directory>>,
        include_tests: bool,
    ) -> Self {
        let mut loaded = LoadedEntries::default();
        let mut last = BTreeMap::new();
        for mut entry in entries {
            if let Some(member) = entry.file.name.strip_prefix(b"charts/")
                && extension(member) != b".prov"
            {
                let mount = member.split(|byte| *byte == b'/').next().unwrap_or(member);
                if !matches!(mount.first(), Some(b'_' | b'.')) {
                    let group = loaded.mounts.entry(mount.to_owned()).or_default();
                    entry.file.name = member.to_owned();
                    group.push(entry);
                }
            } else {
                if !matches!(
                    entry.file.name.as_slice(),
                    b"requirements.yaml" | b"requirements.lock"
                ) {
                    last.insert(entry.file.name.clone(), loaded.files.len());
                }
                loaded.files.push(entry);
            }
        }
        Self {
            entries: Arc::new(loaded),
            directory,
            path,
            include_tests,
            last: Arc::new(last),
        }
    }

    pub(crate) fn analysis_view(&self, include_tests: bool) -> Self {
        Self {
            include_tests,
            ..self.clone()
        }
    }

    pub(super) fn set_api_version(&mut self, version: Option<&str>) {
        let mut v1 = matches!(version, None | Some("" | "v1"));
        for (index, entry) in self.entries.files.iter().enumerate() {
            match entry.file.name.as_slice() {
                b"requirements.yaml" => {
                    if let Some(version) = entry
                        .file
                        .source()
                        .ok()
                        .and_then(helm_schema_syntax::decoded_yaml_value)
                        .and_then(|doc| {
                            doc.as_object()?
                                .iter()
                                .rfind(|(key, _)| key.eq_ignore_ascii_case("apiVersion"))
                                .map(|(_, value)| value.clone())
                        })
                        .filter(|value| !value.is_null())
                    {
                        v1 = version.as_str() == Some("v1");
                    }
                }
                b"requirements.lock" => {}
                _ => continue,
            }
            if v1 {
                Arc::make_mut(&mut self.last).insert(entry.file.name.clone(), index);
            }
        }
    }

    pub(crate) fn file_path(&self, name: &str) -> String {
        format!("{}/{name}", self.path)
    }

    pub(crate) fn filename(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or_default()
    }

    pub(crate) fn manifest(&self) -> EngineResult<&LoadedFile> {
        self.file("Chart.yaml").ok_or_else(|| {
            vfs::VfsError::from(vfs::error::VfsErrorKind::FileNotFound)
                .with_context(|| format!("Could not read path '{}'", self.file_path("Chart.yaml")))
                .into()
        })
    }

    pub(crate) fn file(&self, name: &str) -> Option<&LoadedFile> {
        self.entries
            .files
            .iter()
            .map(|entry| &entry.file)
            .rfind(|file| file.name == name.as_bytes())
    }

    pub(crate) fn load_local_file(&self, path: &Path) -> EngineResult<Option<Arc<[u8]>>> {
        match &self.directory {
            Some(directory) => directory.local_file(path),
            None => Ok(None),
        }
    }

    pub(crate) fn lint_values(&self) -> Option<serde_yaml::Value> {
        let raw = self.directory.as_ref()?.lint_values()?;
        let doc =
            serde_yaml::Value::deserialize(serde_yaml::Deserializer::from_slice(&raw).next()?)
                .ok()?;
        match doc {
            serde_yaml::Value::Null => Some(serde_yaml::Value::Mapping(serde_yaml::Mapping::new())),
            serde_yaml::Value::Mapping(_) => Some(doc),
            _ => None,
        }
    }

    pub(crate) fn files_with_role(&self, role: FileRole) -> impl Iterator<Item = &LoadedFile> {
        self.entries
            .files
            .iter()
            .enumerate()
            .filter_map(move |(index, entry)| {
                let file = &entry.file;
                let selected = matches!(role, FileRole::StaticCrd)
                    || self.last.get(&file.name) == Some(&index);
                (selected && self.has_role(&file.name, role)).then_some(file)
            })
    }

    fn has_role(&self, name: &[u8], role: FileRole) -> bool {
        let template = name.strip_prefix(b"templates/");
        let enabled =
            || template.is_some_and(|name| self.include_tests || !within_template_tests(name));
        match role {
            FileRole::DefineIndexTemplate => enabled(),
            FileRole::ManifestTemplate | FileRole::NotesTemplate => {
                enabled()
                    && !basename(name).starts_with(b"_")
                    && name.ends_with(b"NOTES.txt") == matches!(role, FileRole::NotesTemplate)
            }
            FileRole::StaticCrd => {
                name.starts_with(b"crds/")
                    && [b".json".as_slice(), b".yaml", b".yml"]
                        .iter()
                        .any(|ext| extension(name).eq_ignore_ascii_case(ext))
            }
            FileRole::FilesGetSource => {
                template.is_none()
                    && !matches!(
                        name,
                        b"Chart.yaml" | b"Chart.lock" | b"values.yaml" | b"values.schema.json"
                    )
            }
        }
    }

    pub(crate) fn visit_vendored_charts(
        &self,
        mut visit: impl FnMut(Self) -> EngineResult<()>,
    ) -> EngineResult<()> {
        for (mount, entries) in &self.entries.mounts {
            let path = self.file_path(&format!("charts/{}", String::from_utf8_lossy(mount)));
            if extension(mount) == b".tgz" {
                let Some(package) = entries.first() else {
                    continue;
                };
                if package.file.name != *mount {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("expected archive {path}, got {}", package.file.path),
                    )
                    .into());
                }
                visit(Self::read_archive(
                    package.file.bytes.as_ref(),
                    package.source.clone(),
                    self.include_tests,
                )?)?;
            } else {
                let entries = entries
                    .iter()
                    .filter_map(|entry| {
                        let (_, name) = entry.file.name.split_once_str(b"/")?;
                        let mut entry = entry.clone();
                        entry.file.name = name.to_owned();
                        Some(entry)
                    })
                    .collect();
                visit(Self::from_entries(
                    entries,
                    path,
                    self.directory.clone(),
                    self.include_tests,
                ))?;
            }
        }
        Ok(())
    }
}

fn archive_entries(
    input: impl Read,
    path: &str,
    source: &Arc<SourcePath>,
) -> EngineResult<Vec<Entry>> {
    let mut archive = tar::Archive::new(MultiGzDecoder::new(input));
    let mut entries = Vec::new();
    let mut remaining = MAX_ARCHIVE_SIZE;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        let original_name = entry.path_bytes();
        let old_directory =
            entry.header().as_old().linkflag.first() == Some(&0) && original_name.ends_with(b"/");
        if kind.is_dir()
            || old_directory
            || kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
        {
            continue;
        }
        let original_name = original_name.into_owned().into_boxed_slice();
        let name = archive_name(path, &original_name)?;
        if entry.size() >= remaining as u64 {
            return Err(CliError::LoadBudgetExceeded {
                subject: format!("expanded {path}"),
                limit_bytes: MAX_ARCHIVE_SIZE,
            });
        }
        let data = read_to_end_capped(
            &mut entry,
            MAX_FILE_SIZE,
            format!("expanded {path}/{}", name.as_bstr()),
        )?;
        remaining -= data.len();
        entries.push(Entry {
            file: LoadedFile::new(
                name.clone(),
                format!("/{}", name.as_bstr()),
                Arc::from(data),
            ),
            source: SourcePath::ArchiveMember {
                archive: Arc::clone(source),
                original_name,
            },
        });
    }
    Ok(entries)
}

fn archive_name(archive: &str, raw: &[u8]) -> EngineResult<Vec<u8>> {
    let separator = if raw.contains(&b'\\') { b'\\' } else { b'/' };
    let error = || CliError::UnsafeArchiveEntryPath {
        archive: archive.to_owned(),
        entry_path: String::from_utf8_lossy(raw).into_owned(),
    };
    let (wrapper, member) = raw.split_once_str(&[separator]).ok_or_else(error)?;
    if wrapper == b"Chart.yaml"
        || matches!(member.first(), Some(first) if *first == b'/' || *first == separator)
    {
        return Err(error());
    }
    let mut parts = Vec::new();
    for part in member.split(|byte| *byte == separator || *byte == b'/') {
        match part {
            b"." | b"" => {}
            b".." if parts.last().is_some_and(|part| *part != b"..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let name = parts.join(&b'/');
    let drive_path =
        matches!(name.as_slice(), [letter, b':', b'/', ..] if letter.is_ascii_alphabetic());
    if name.is_empty() || name.starts_with(b"..") || drive_path {
        Err(error())
    } else {
        Ok(name)
    }
}

fn basename(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b'/').next().unwrap_or(name)
}

fn extension(name: &[u8]) -> &[u8] {
    let basename = basename(name);
    basename
        .rfind(b".")
        .and_then(|dot| basename.get(dot..))
        .unwrap_or_default()
}

fn within_template_tests(template: &[u8]) -> bool {
    let (dirs, _) = template.rsplit_once_str(b"/").unwrap_or((b"", template));
    let mut in_templates = true;
    for dir in dirs.split(|byte| *byte == b'/') {
        if in_templates && dir.eq_ignore_ascii_case(b"tests") {
            return true;
        }
        in_templates = dir.eq_ignore_ascii_case(b"templates");
    }
    false
}

#[cfg(test)]
#[path = "tests/files.rs"]
mod tests;
