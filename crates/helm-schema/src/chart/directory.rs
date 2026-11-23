use std::collections::BTreeMap;
use std::fs;
use std::io::{Error, ErrorKind, Read as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use vfs::{VfsFileType, VfsPath, error::VfsErrorKind};

use super::files::{Entry, LoadedFile, MAX_FILE_SIZE, SourcePath};
use super::ignore::Rules;
use crate::error::{CliError, EngineResult};

type Bytes = Arc<[u8]>;

#[derive(Debug, Clone)]
enum DirectEntry {
    Missing,
    Directory,
    File(Bytes),
}

impl DirectEntry {
    fn into_file(self) -> Option<Bytes> {
        match self {
            Self::File(bytes) => Some(bytes),
            Self::Missing | Self::Directory => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Directory {
    root: VfsPath,
    native_root: Option<PathBuf>,
    direct: Mutex<BTreeMap<String, DirectEntry>>,
    native: Mutex<BTreeMap<PathBuf, Bytes>>,
}

impl Directory {
    pub(crate) fn new(root: VfsPath, native_root: Option<&Path>) -> Self {
        Self {
            root,
            native_root: native_root
                .map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf())),
            direct: Mutex::default(),
            native: Mutex::default(),
        }
    }

    pub(crate) fn for_session(&self) -> Arc<Self> {
        let direct = lock(&self.direct);
        Arc::new(Self {
            root: self.root.clone(),
            native_root: self.native_root.clone(),
            direct: Mutex::new(direct.clone()),
            native: Mutex::new(lock(&self.native).clone()),
        })
    }

    pub(crate) fn path(&self) -> &str {
        self.root.as_str()
    }

    pub(crate) fn direct(&self, name: &str) -> EngineResult<Option<Bytes>> {
        Ok(self.lookup(name)?.into_file())
    }

    fn lookup(&self, name: &str) -> EngineResult<DirectEntry> {
        let mut direct = lock(&self.direct);
        if let Some(entry) = direct.get(name) {
            return Ok(entry.clone());
        }
        let path = self.root.join(name)?;
        let found = match path.metadata() {
            Ok(metadata) if metadata.file_type == VfsFileType::Directory => DirectEntry::Directory,
            Ok(_) => {
                let bytes = if let Some(root) = &self.native_root {
                    let path = root.join(path.as_str().trim_start_matches('/'));
                    if !fs::metadata(&path)?.is_file() {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot load irregular chart file {name}"),
                        )
                        .into());
                    }
                    self.native_bytes(&path)?
                } else {
                    vfs_bytes(&path)?
                };
                DirectEntry::File(bytes)
            }
            Err(error) if matches!(error.kind(), VfsErrorKind::FileNotFound) => {
                DirectEntry::Missing
            }
            Err(error) => return Err(error.into()),
        };
        direct.insert(name.to_owned(), found.clone());
        Ok(found)
    }

    pub(super) fn local_file(&self, path: &Path) -> EngineResult<Option<Bytes>> {
        let Some(root) = &self.native_root else {
            return Ok(None);
        };
        let path = std::path::absolute(path)?;
        if let Some(bytes) = lock(&self.native).get(&path).cloned() {
            return Ok(Some(bytes));
        }
        let path = path.canonicalize().unwrap_or(path);
        if path.starts_with(root) {
            return self.native_bytes(&path).map(Some);
        }
        Ok(None)
    }

    fn native_bytes(&self, path: &Path) -> EngineResult<Bytes> {
        let mut native = lock(&self.native);
        if let Some(bytes) = native.get(path) {
            return Ok(Arc::clone(bytes));
        }
        let bytes = Arc::from(fs::read(path)?);
        native.insert(path.to_owned(), Arc::clone(&bytes));
        Ok(bytes)
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn entries(&self) -> EngineResult<Vec<Entry>> {
        let ignore = match self.lookup(".helmignore")? {
            DirectEntry::File(bytes) => Some(bytes),
            DirectEntry::Missing => None,
            DirectEntry::Directory => {
                return Err(
                    Error::new(ErrorKind::InvalidData, ".helmignore is a directory").into(),
                );
            }
        };
        let rules = Rules::parse(ignore.as_deref().unwrap_or_default())?;
        let _ = self.direct("values.yaml");
        let mut entries = Vec::new();
        // Source charts can supply a manifest template for the input boundary to prepare.
        if !rules.ignores(b"Chart.yaml", false)
            && !self.root.join("Chart.yaml")?.is_file()?
            && let Some(bytes) = self.direct("Chart.template.yaml")?
        {
            let source = match &self.native_root {
                Some(root) => SourcePath::Native(root.join("Chart.template.yaml")),
                None => SourcePath::Virtual(self.root.join("Chart.template.yaml")?),
            };
            self.push(&mut entries, b"Chart.yaml".to_vec(), source, bytes);
        }
        if let Some(root) = &self.native_root {
            self.walk_native(root, b"", &rules, &mut entries)?;
        } else {
            self.walk_vfs(&self.root, &rules, &mut entries)?;
        }
        Ok(entries)
    }

    fn walk_native(
        &self,
        dir: &Path,
        prefix: &[u8],
        rules: &Rules,
        out: &mut Vec<Entry>,
    ) -> EngineResult<()> {
        let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let metadata = fs::metadata(entry.path())?;
            let basename = entry.file_name();
            let mut name = prefix.to_vec();
            name.extend_from_slice(basename.as_encoded_bytes());
            if rules.ignores(&name, metadata.is_dir()) {
                continue;
            }
            if metadata.is_dir() {
                name.push(b'/');
                self.walk_native(&entry.path(), &name, rules, out)?;
            } else {
                if !metadata.is_file() {
                    return Err(CliError::CliValidation(format!(
                        "cannot load irregular chart file {}",
                        entry.path().display()
                    )));
                }
                let path = entry.path();
                check_file_size(metadata.len(), || path.display().to_string())?;
                let bytes = self.native_bytes(&path)?;
                self.push(out, name, SourcePath::Native(path), bytes);
            }
        }
        Ok(())
    }

    fn walk_vfs(&self, dir: &VfsPath, rules: &Rules, out: &mut Vec<Entry>) -> EngineResult<()> {
        let mut entries: Vec<_> = dir.read_dir()?.collect();
        entries.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));
        for entry in entries {
            let name = entry
                .as_str()
                .strip_prefix(self.root.as_str())
                .unwrap_or(entry.as_str())
                .trim_start_matches('/');
            let metadata = entry.metadata()?;
            let directory = metadata.file_type == VfsFileType::Directory;
            if rules.ignores(name.as_bytes(), directory) {
                continue;
            }
            if directory {
                self.walk_vfs(&entry, rules, out)?;
            } else {
                check_file_size(metadata.len, || entry.as_str().to_owned())?;
                let captured = lock(&self.direct)
                    .get(name)
                    .cloned()
                    .and_then(DirectEntry::into_file);
                let bytes = match captured {
                    Some(bytes) => bytes,
                    None => vfs_bytes(&entry)?,
                };
                self.push(
                    out,
                    name.as_bytes().to_vec(),
                    SourcePath::Virtual(entry),
                    bytes,
                );
            }
        }
        Ok(())
    }

    pub(super) fn lint_values(&self) -> Option<Bytes> {
        lock(&self.direct).get("values.yaml").cloned()?.into_file()
    }

    fn push(&self, out: &mut Vec<Entry>, name: Vec<u8>, source: SourcePath, bytes: Bytes) {
        let path = format!("{}/{}", self.path(), String::from_utf8_lossy(&name));
        out.push(Entry {
            file: LoadedFile::new(name, path, bytes),
            source,
        });
    }
}

fn vfs_bytes(path: &VfsPath) -> EngineResult<Bytes> {
    let mut bytes = Vec::new();
    path.open_file()?.read_to_end(&mut bytes)?;
    Ok(Arc::from(bytes))
}

fn check_file_size(size: u64, subject: impl FnOnce() -> String) -> EngineResult<()> {
    if size > MAX_FILE_SIZE as u64 {
        return Err(CliError::LoadBudgetExceeded {
            subject: subject(),
            limit_bytes: MAX_FILE_SIZE,
        });
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
