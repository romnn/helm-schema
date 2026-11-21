use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use vfs::VfsPath;

use super::files::{FileRole, LoadedFile, PreparedRootFile, SourcePath, has_role};
use crate::error::{CliError, EngineResult};
use crate::load_budget::read_to_end_capped;

#[derive(Debug)]
pub(super) struct Directory {
    pub(super) path: VfsPath,
    source: Arc<DirectorySource>,
    roles: OnceLock<Vec<RoleFile>>,
}

#[derive(Debug)]
struct DirectorySource {
    root: VfsPath,
    native_root: Option<PathBuf>,
    listings: Mutex<BTreeMap<String, Arc<[VfsPath]>>>,
    bytes: Mutex<BTreeMap<PathBuf, Arc<[u8]>>>,
}

#[derive(Debug)]
struct RoleFile {
    file: LoadedFile,
    roles: BTreeSet<FileRole>,
}

impl Directory {
    pub(super) fn new(
        root: &VfsPath,
        native_root: Option<&Path>,
        prepared_file: Option<&PreparedRootFile>,
    ) -> EngineResult<Self> {
        let source = Arc::new(DirectorySource {
            root: root.clone(),
            native_root: native_root
                .map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf())),
            listings: Mutex::default(),
            bytes: Mutex::default(),
        });
        if let Some(prepared) = prepared_file {
            let path = root.join(&prepared.name)?;
            lock(&source.bytes).insert(source.key(&path), Arc::from(prepared.source.as_bytes()));
        }
        Ok(Self {
            path: root.clone(),
            source,
            roles: OnceLock::new(),
        })
    }

    pub(super) fn subchart(&self, path: VfsPath) -> Self {
        Self {
            path,
            source: Arc::clone(&self.source),
            roles: OnceLock::new(),
        }
    }

    pub(super) fn children(&self, dir: &VfsPath) -> EngineResult<Arc<[VfsPath]>> {
        let mut listings = lock(&self.source.listings);
        if let Some(entries) = listings.get(dir.as_str()) {
            return Ok(Arc::clone(entries));
        }
        let entries = Arc::from(dir.read_dir()?.collect::<Vec<_>>());
        listings.insert(dir.as_str().to_string(), Arc::clone(&entries));
        Ok(entries)
    }

    pub(super) fn file(&self, name: &str) -> EngineResult<Option<LoadedFile>> {
        let path = self.path.join(name)?;
        if !path.is_file()? {
            return Ok(None);
        }
        Ok(Some(self.load_text(&path)?))
    }

    pub(super) fn load_local_file(&self, path: &Path) -> EngineResult<Option<Arc<[u8]>>> {
        let Some(root) = self.source.native_root.as_ref() else {
            return Ok(None);
        };
        let Ok(path) = path.canonicalize() else {
            return Ok(std::path::absolute(path)
                .ok()
                .and_then(|path| lock(&self.source.bytes).get(&path).cloned()));
        };
        let mut bytes = lock(&self.source.bytes);
        if let Some(data) = bytes.get(&path) {
            return Ok(Some(Arc::clone(data)));
        }
        if !path.starts_with(root) {
            return Ok(None);
        }
        let data = Arc::from(std::fs::read(&path)?);
        bytes.insert(path, Arc::clone(&data));
        Ok(Some(data))
    }

    pub(super) fn source_path(&self, file: &VfsPath) -> SourcePath {
        let relative = self.source.relative(file);
        match &self.source.native_root {
            Some(root) => SourcePath::Native(root.join(relative)),
            None => SourcePath::Virtual {
                root: self.source.root.clone(),
                path: PathBuf::from(relative),
            },
        }
    }

    pub(super) fn read(
        &self,
        file: &VfsPath,
        archive_limit: Option<usize>,
    ) -> EngineResult<Arc<[u8]>> {
        self.read_once(file, || {
            let mut input = file.open_file()?;
            if let Some(limit) = archive_limit {
                read_to_end_capped(&mut input, limit, file.as_str())
            } else {
                let mut data = Vec::new();
                input.read_to_end(&mut data)?;
                Ok(data)
            }
        })
    }

    fn read_once(
        &self,
        file: &VfsPath,
        read: impl FnOnce() -> EngineResult<Vec<u8>>,
    ) -> EngineResult<Arc<[u8]>> {
        let mut bytes = lock(&self.source.bytes);
        let key = self.source.key(file);
        if let Some(data) = bytes.get(&key) {
            return Ok(Arc::clone(data));
        }
        let data = Arc::from(read()?);
        bytes.insert(key, Arc::clone(&data));
        Ok(data)
    }

    fn loaded_file(&self, path: &VfsPath, bytes: Arc<[u8]>) -> LoadedFile {
        LoadedFile {
            name: relative_to(&self.path, path).to_owned(),
            path: path.as_str().to_owned(),
            bytes,
        }
    }

    fn load_text(&self, path: &VfsPath) -> EngineResult<LoadedFile> {
        let bytes = self.read_once(path, || Ok(path.read_to_string()?.into_bytes()))?;
        Ok(self.loaded_file(path, bytes))
    }

    pub(super) fn required_manifest(&self) -> EngineResult<LoadedFile> {
        self.load_text(&self.path.join("Chart.yaml")?)
    }

    pub(super) fn prepare_roles(&self, include_tests: bool) -> EngineResult<()> {
        if self.roles.get().is_some() {
            return Ok(());
        }

        let mut selected = BTreeMap::new();
        for (name, role) in [
            ("templates", FileRole::DefineIndexTemplate),
            ("crds", FileRole::StaticCrd),
        ] {
            let dir = self.path.join(name)?;
            if dir.is_dir()? {
                self.select(&dir, role, include_tests, &mut selected)?;
            }
        }
        self.select(
            &self.path,
            FileRole::FilesGetSource,
            include_tests,
            &mut selected,
        )?;

        let files = selected
            .into_values()
            .map(|(path, roles)| {
                Ok(RoleFile {
                    file: self.loaded_file(&path, self.read(&path, None)?),
                    roles,
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let _ = self.roles.set(files);
        Ok(())
    }

    pub(super) fn files_with_role(&self, role: FileRole) -> EngineResult<Vec<LoadedFile>> {
        let roles = self
            .roles
            .get()
            .ok_or_else(|| CliError::ChartRolesNotPrepared {
                path: self.path.as_str().to_owned(),
            })?;
        Ok(roles
            .iter()
            .filter(|entry| entry.roles.contains(&role))
            .map(|entry| entry.file.clone())
            .collect())
    }

    fn select(
        &self,
        dir: &VfsPath,
        role: FileRole,
        include_tests: bool,
        out: &mut BTreeMap<String, (VfsPath, BTreeSet<FileRole>)>,
    ) -> EngineResult<()> {
        for path in self.children(dir)?.iter() {
            if path.is_dir()? {
                let name = path.filename();
                let skip = match role {
                    FileRole::DefineIndexTemplate => {
                        !include_tests
                            && name.eq_ignore_ascii_case("tests")
                            && dir.filename().eq_ignore_ascii_case("templates")
                    }
                    FileRole::FilesGetSource => super::files::excluded_files_directory(&name),
                    FileRole::StaticCrd | FileRole::ManifestTemplate | FileRole::NotesTemplate => {
                        false
                    }
                };
                if !skip {
                    self.select(path, role, include_tests, out)?;
                }
            } else if path.is_file()?
                && has_role(relative_to(&self.path, path), role, include_tests)
            {
                let (_, roles) = out
                    .entry(path.as_str().to_string())
                    .or_insert_with(|| (path.clone(), BTreeSet::new()));
                roles.insert(role);
                if role == FileRole::DefineIndexTemplate {
                    for role in [FileRole::ManifestTemplate, FileRole::NotesTemplate] {
                        if has_role(relative_to(&self.path, path), role, include_tests) {
                            roles.insert(role);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl DirectorySource {
    fn relative<'a>(&self, file: &'a VfsPath) -> &'a str {
        relative_to(&self.root, file)
    }

    fn key(&self, file: &VfsPath) -> PathBuf {
        let Some(root) = self.native_root.as_ref() else {
            return PathBuf::from(file.as_str());
        };
        let path = root.join(self.relative(file));
        path.canonicalize().unwrap_or(path)
    }
}

fn relative_to<'a>(dir: &VfsPath, file: &'a VfsPath) -> &'a str {
    let path = file.as_str();
    path.strip_prefix(dir.as_str())
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or(path)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
