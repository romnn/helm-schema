use camino::Utf8Path;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::io::Read;
use std::path::Path;
use thiserror::Error;
use vfs::VfsPath;

#[derive(Debug, Error)]
pub enum WalkError {
    #[error("vfs error: {0}")]
    Vfs(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug)]
pub struct WalkOptions {
    pub include_hidden: bool,
    pub standard_filters: bool,
    pub respect_gitignore: bool,
}
impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            standard_filters: true,
            respect_gitignore: true,
        }
    }
}

#[derive(Debug)]
pub struct VfsDirEntry {
    pub path: VfsPath,
    // pub is_dir: bool,
    // pub is_file: bool,
}

impl std::ops::Deref for VfsDirEntry {
    type Target = VfsPath;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

pub struct VfsWalkBuilder {
    root: VfsPath,
    opts: WalkOptions,
}

impl VfsWalkBuilder {
    pub fn new(root: VfsPath) -> Self {
        Self {
            root,
            opts: WalkOptions::default(),
        }
    }
    pub fn hidden(mut self, yes: bool) -> Self {
        self.opts.include_hidden = yes;
        self
    }

    pub fn standard_filters(mut self, yes: bool) -> Self {
        self.opts.standard_filters = yes;
        self
    }

    pub fn git_ignore(mut self, yes: bool) -> Self {
        self.opts.respect_gitignore = yes;
        self
    }

    pub fn build(self) -> Result<VfsWalk, WalkError> {
        Ok(VfsWalk::new(self.root, self.opts)?)
    }
}

struct Frame {
    dir: VfsPath,
    // children are collected eagerly for deterministic order
    children: Vec<VfsPath>,
    idx: usize,
    gitignore: Option<Gitignore>,
}

pub struct VfsWalk {
    root: VfsPath,
    opts: WalkOptions,
    stack: Vec<Frame>,
}

impl VfsWalk {
    fn new(root: VfsPath, opts: WalkOptions) -> Result<Self, WalkError> {
        if !root.exists().map_err(|e| WalkError::Vfs(e.to_string()))? {
            return Err(WalkError::Vfs(format!(
                "root does not exist: {}",
                root.as_str()
            )));
        }
        let mut me = Self {
            root: root.clone(),
            opts,
            stack: Vec::new(),
        };
        let top = me.new_frame(root)?;
        me.stack.push(top);
        Ok(me)
    }

    fn new_frame(&self, dir: VfsPath) -> Result<Frame, WalkError> {
        let mut children = Vec::new();
        for entry in dir.read_dir().map_err(|e| WalkError::Vfs(e.to_string()))? {
            children.push(entry);
        }
        // Deterministic: sort by UTF-8 path
        children.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        // Load .gitignore if required
        let gitignore = if self.opts.respect_gitignore {
            let gi_path = dir
                .join(".gitignore")
                .map_err(|e| WalkError::Vfs(e.to_string()))?;
            if gi_path
                .exists()
                .map_err(|e| WalkError::Vfs(e.to_string()))?
                && gi_path
                    .is_file()
                    .map_err(|e| WalkError::Vfs(e.to_string()))?
            {
                let mut s = String::new();
                gi_path
                    .open_file()
                    .map_err(|e| WalkError::Vfs(e.to_string()))?
                    .read_to_string(&mut s)?;
                let mut b = GitignoreBuilder::new(Path::new(dir.as_str()));
                for line in s.lines() {
                    // treat entire file as patterns; ignore errors for malformed lines
                    let _ = b.add_line(None, line);
                }
                b.build().ok()
            } else {
                None
            }
        } else {
            None
        };

        Ok(Frame {
            dir,
            children,
            idx: 0,
            gitignore,
        })
    }

    fn is_hidden_name(&self, name: &str) -> bool {
        name.starts_with('.')
    }

    fn standard_skip(&self, entry: &VfsPath) -> bool {
        if !self.opts.standard_filters {
            return false;
        }
        let name = Utf8Path::new(entry.as_str()).file_name().unwrap_or("");
        matches!(
            name,
            ".git" | ".hg" | ".svn" | "CVS" | "node_modules" | "target"
        )
    }

    fn ignored_by_git(&self, path: &VfsPath) -> Result<bool, WalkError> {
        if !self.opts.respect_gitignore {
            return Ok(false);
        }
        // consult frames from top to bottom; first match wins
        // Using absolute path here since GitignoreBuilder was created with abs base
        let p = Path::new(path.as_str());
        for f in self.stack.iter().rev() {
            if let Some(gi) = &f.gitignore {
                if gi
                    .matched_path_or_any_parents(
                        p,
                        path.is_dir().map_err(|e| WalkError::Vfs(e.to_string()))?,
                    )
                    .is_ignore()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl Iterator for VfsWalk {
    type Item = Result<VfsDirEntry, WalkError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let top = self.stack.last_mut()?;
            if top.idx >= top.children.len() {
                // leave this directory
                self.stack.pop();
                continue;
            }
            let path = top.children[top.idx].clone();
            top.idx += 1;

            let name = Utf8Path::new(path.as_str()).file_name().unwrap_or("");
            if !self.opts.include_hidden && self.is_hidden_name(name) {
                continue;
            }
            if self.standard_skip(&path) {
                continue;
            }
            match self.ignored_by_git(&path) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => return Some(Err(e)),
            }

            // yield entry; push dir frames lazily (pre-order)
            let is_dir = match path.is_dir() {
                Ok(b) => b,
                Err(e) => return Some(Err(WalkError::Vfs(e.to_string()))),
            };
            // let is_file = match path.is_file() {
            //     Ok(b) => b,
            //     Err(e) => return Some(Err(WalkError::Vfs(e.to_string()))),
            // };
            if is_dir {
                // descend with new frame (captures directory's .gitignore)
                match self.new_frame(path.clone()) {
                    Ok(frame) => self.stack.push(frame),
                    Err(e) => return Some(Err(e)),
                }
            }
            return Some(Ok(VfsDirEntry {
                path,
                // is_dir,
                // is_file,
            }));
        }
    }
}
