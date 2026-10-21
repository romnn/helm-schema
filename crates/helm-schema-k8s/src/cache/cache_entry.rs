//! One schema file's place in a managed cache root, and whether its name may
//! be written.
//!
//! Cache roots are shared across machines (the vendored provider bundle is
//! committed, user caches get copied), so every writer applies the
//! strictest platform's rules on every platform: a name Windows could not
//! create is never written anywhere.

use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;

use super::layout::k8s_cache_path;

/// Longest component written: below the common 255-byte file-name limit
/// with room for the `.not-found`, `.meta` and temp-file suffixes writers
/// append.
const MAX_COMPONENT_BYTES: usize = 200;

/// Device names Windows reserves regardless of extension.
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Why a cache name is not portable to every platform. Writers skip such an
/// entry; the reason never escapes as an error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NonPortableCacheName {
    /// A component is empty.
    #[error("empty path component")]
    Empty,
    /// A component holds a byte outside the lowercase-normalised token set.
    #[error("component {component:?} contains {character:?}, outside the portable set [a-z0-9._-]")]
    Character {
        /// The offending component.
        component: String,
        /// Its first byte outside the portable set.
        character: char,
    },
    /// A component starts or ends with `.`.
    #[error("component {component:?} starts or ends with '.'")]
    DotEdge {
        /// The offending component.
        component: String,
    },
    /// A component's stem is a reserved Windows device name.
    #[error("component {component:?} is a reserved Windows device name")]
    ReservedDevice {
        /// The offending component.
        component: String,
    },
    /// A component is too long once writers append their suffixes.
    #[error("component {component:?} is longer than {MAX_COMPONENT_BYTES} bytes")]
    TooLong {
        /// The offending component.
        component: String,
    },
}

/// A cache file path whose every component below the root is portable.
/// Only [`CacheEntry::writable`] makes one, so cache writers that take it
/// cannot write a non-portable name.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PortableCachePath<'a>(&'a Path);

impl PortableCachePath<'_> {
    pub(crate) fn path(&self) -> &Path {
        self.0
    }
}

/// `<root>/<source_id>/<dir>/<file>`: one schema file in a managed cache,
/// where `dir` is the K8s version directory or the CRD API group.
#[derive(Debug)]
pub(crate) struct CacheEntry {
    root: PathBuf,
    relative: String,
    local: PathBuf,
    portability: Result<(), NonPortableCacheName>,
}

impl CacheEntry {
    pub(crate) fn new(root: &Path, source_id: &str, dir: &str, file: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            relative: format!("{source_id}/{dir}/{file}"),
            local: k8s_cache_path(root, source_id, dir, file),
            portability: [source_id, dir, file]
                .into_iter()
                .try_for_each(check_portable_component),
        }
    }

    /// The entry's path when its name is portable. A non-portable entry is
    /// neither read nor written on any platform: no platform's cache could
    /// hold it, so it behaves as a cache miss everywhere alike.
    pub(crate) fn writable(&self) -> Result<PortableCachePath<'_>, &NonPortableCacheName> {
        match &self.portability {
            Ok(()) => Ok(PortableCachePath(&self.local)),
            Err(reason) => Err(reason),
        }
    }

    /// The warning for a write this entry's name made the cache skip.
    pub(crate) fn skipped_write(&self, reason: &NonPortableCacheName) -> Diagnostic {
        Diagnostic::CacheWriteSkipped {
            cache_root: self.root.display().to_string(),
            entry: self.relative.clone(),
            reason: reason.to_string(),
        }
    }
}

fn check_portable_component(component: &str) -> Result<(), NonPortableCacheName> {
    let owned = || component.to_string();
    if component.is_empty() {
        return Err(NonPortableCacheName::Empty);
    }
    if component.len() > MAX_COMPONENT_BYTES {
        return Err(NonPortableCacheName::TooLong { component: owned() });
    }
    if let Some(character) = component.chars().find(|character| {
        !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-'))
    }) {
        return Err(NonPortableCacheName::Character {
            component: owned(),
            character,
        });
    }
    if component.starts_with('.') || component.ends_with('.') {
        return Err(NonPortableCacheName::DotEdge { component: owned() });
    }
    let stem = component.split('.').next().unwrap_or(component);
    if RESERVED_DEVICE_NAMES.contains(&stem) {
        return Err(NonPortableCacheName::ReservedDevice { component: owned() });
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/cache_entry.rs"]
mod tests;
