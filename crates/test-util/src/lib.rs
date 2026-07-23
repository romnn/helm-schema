//! Shared fixtures, matchers, and setup helpers for workspace tests.

use std::path::PathBuf;
use std::sync::OnceLock;

use color_eyre::eyre::{self, WrapErr, eyre};
use vfs::VfsPath;

/// S-expression fixtures and parsing utilities.
pub mod sexpr;

/// Common imports for workspace tests.
pub mod prelude {
    pub use crate::matchers::*;
    pub use crate::sexpr::{ParseError as SExprParseError, SExpr};
    pub use crate::write;
    pub use crate::{Builder, LogLevel};
    pub use googletest::{assert_that, matcher::MatcherBase, matchers::*};
    pub use similar_asserts::assert_eq as sim_assert_eq;
}

/// Identifies helper and template fixtures used to construct a define index.
#[derive(Debug, Clone, Copy)]
pub struct DefineSourceSpec<'a> {
    /// Paths of helper templates relative to the workspace test-data directory.
    pub helper_templates: &'a [&'a str],
    /// Helper-template directories paired with the file extension to load.
    pub helper_template_dirs: &'a [(&'a str, &'a str)],
    /// Template names paired with paths relative to the workspace test-data directory.
    pub file_sources: &'a [(&'a str, &'a str)],
}

/// A named fixture source loaded from the workspace test-data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedDefineSource {
    /// Stable logical path used for source identity and provenance.
    pub path: String,
    /// Normalized UTF-8 source text.
    pub source: String,
}

impl DefineSourceSpec<'_> {
    /// Loads all configured sources from the workspace test-data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured fixture cannot be read.
    pub fn load(self) -> eyre::Result<Vec<LoadedDefineSource>> {
        let mut sources = self
            .helper_templates
            .iter()
            .map(|path| {
                Ok(LoadedDefineSource {
                    path: (*path).to_string(),
                    source: read_testdata(path)?,
                })
            })
            .collect::<eyre::Result<Vec<_>>>()?;
        for (dir, extension) in self.helper_template_dirs {
            sources.extend(read_testdata_dir(dir, extension)?);
        }
        for (name, path) in self.file_sources {
            sources.push(LoadedDefineSource {
                path: (*name).to_string(),
                source: read_testdata(path)?,
            });
        }
        sources.sort_by(|left, right| left.path.cmp(&right.path));

        Ok(sources)
    }
}

/// Returns the workspace root directory via the `CARGO_WORKSPACE_DIR` env var
/// set in `.cargo/config.toml`.
///
/// # Panics
///
/// Panics if `CARGO_WORKSPACE_DIR` is not set.
#[must_use]
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

/// Returns the path to the workspace `testdata/` directory.
#[must_use]
pub fn workspace_testdata() -> PathBuf {
    workspace_root().join("testdata")
}

/// Reads a file relative to the workspace `testdata/` directory.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn read_testdata(relative_path: &str) -> eyre::Result<String> {
    let path = workspace_testdata().join(relative_path);
    let source = std::fs::read_to_string(&path)
        .wrap_err_with(|| format!("read test fixture {}", path.display()))?;
    Ok(source.replace("\r\n", "\n"))
}

/// Reads named files with the given extension from a test-data directory.
///
/// Returns an empty `Vec` if the directory does not exist.
/// Source identities use forward-slash paths relative to `testdata/`, and the
/// returned sources are ordered by those identities.
///
/// # Errors
///
/// Returns an error when the directory or one of its matching files cannot be read.
pub fn read_testdata_dir(
    relative_dir: &str,
    extension: &str,
) -> eyre::Result<Vec<LoadedDefineSource>> {
    let dir = workspace_testdata().join(relative_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).wrap_err_with(|| format!("read {}", dir.display())),
    };
    let mut sources = Vec::new();
    for entry in entries {
        let entry = entry.wrap_err_with(|| format!("read entry in {}", dir.display()))?;
        if entry.path().extension().is_some_and(|e| e == extension) {
            let path = entry.path();
            let content = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("read test fixture {}", path.display()))?;
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();
            sources.push(LoadedDefineSource {
                path: format!("{}/{filename}", relative_dir.trim_end_matches('/')),
                source: content.replace("\r\n", "\n"),
            });
        }
    }

    sources.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(sources)
}

/// Write `data` into the virtual filesystem at `path`, creating parent directories as needed.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written to.
pub fn write(path: &VfsPath, data: impl AsRef<[u8]>) -> eyre::Result<VfsPath> {
    let _ = path.parent().create_dir_all();
    let mut file = path.create_file()?;
    file.write_all(data.as_ref())?;
    Ok(path.clone())
}

/// Tracing level used by the test setup builder.
pub type LogLevel = tracing::metadata::Level;

static INIT_EYRE: OnceLock<std::result::Result<(), String>> = OnceLock::new();

/// Keeps test-scoped resources alive until the end of a test.
#[derive(Default)]
pub struct TestGuard {
    // trace_guard: Option<crate::trace::TraceGuard>,
}

/// Configures shared diagnostics and tracing for a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Builder {
    setup_tracing: bool,
    trace_timestamps: bool,
    install_eyre: bool,
    // trace_file: Option<PathBuf>,
    env_filter: Option<String>,
    log_level: LogLevel,
    // color_choice: ColorChoice,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            setup_tracing: true,
            trace_timestamps: false,
            install_eyre: true,
            env_filter: None,
            // trace_file: None,
            log_level: LogLevel::DEBUG,
            // color_choice: ColorChoice::Auto,
        }
    }
}

impl Builder {
    /// Initialize test.
    ///
    /// This ensures `color_eyre` is setup once and env variables are read.
    ///
    /// # Errors
    ///
    /// Returns an error if `color_eyre` installation fails.
    pub fn build(self) -> eyre::Result<TestGuard> {
        let test_guard = TestGuard::default();

        if self.install_eyre {
            let installation =
                INIT_EYRE.get_or_init(|| color_eyre::install().map_err(|error| error.to_string()));
            if let Err(error) = installation {
                return Err(eyre!(error.clone())).wrap_err("install color-eyre test hook");
            }
        }
        if self.setup_tracing {
            // let trace_options = crate::trace::Options {
            //     color_choice: self.color_choice,
            //     log_level: self.log_level,
            //     env_filter: self.env_filter,
            //     trace_path: self.trace_file,
            //     with_time: self.trace_timestamps,
            // };
            // let trace_guard = setup_tracing(trace_options).expect("failed to setup tracing");
            // test_guard.trace_guard = Some(trace_guard);
        }
        // if self.source_dotfiles {
        //     env::source_env_files().expect("failed to source dotfiles");
        // }

        Ok(test_guard)
    }

    /// Toggle setting up tracing inside the test.
    #[must_use]
    pub fn with_tracing(mut self, enabled: bool) -> Self {
        self.setup_tracing = enabled;
        self
    }

    /// Toggle log level for tracing inside the test.
    #[must_use]
    pub fn with_log_level(mut self, log_level: impl Into<LogLevel>) -> Self {
        self.log_level = log_level.into();
        self
    }

    /// Toggle installation of `color_eyre`.
    #[must_use]
    pub fn with_eyre(mut self, enabled: bool) -> Self {
        self.install_eyre = enabled;
        self
    }

    /// Configure the tracing subscribers env filter.
    ///
    /// Requires tracing to be enabled with `Self::with_tracing`.
    #[must_use]
    pub fn with_env_filter(mut self, filter: impl Into<String>) -> Self {
        self.env_filter = Some(filter.into());
        self
    }
}

/// Create a new builder.
#[must_use]
pub fn builder() -> Builder {
    Builder::default()
}

/// Matchers for paths in virtual filesystems.
pub mod matchers {
    use googletest::matchers::{ContainsMatcher, contains, predicate};
    use vfs::VfsPath;

    /// Matches a collection containing the requested virtual path.
    #[must_use]
    pub fn contains_path(
        path: &str,
    ) -> ContainsMatcher<impl googletest::matcher::Matcher<&VfsPath>> {
        contains(matches_path(path))
    }

    /// Matches a virtual path by its normalized string representation.
    #[must_use]
    pub fn matches_path(path: &str) -> impl googletest::matcher::Matcher<&VfsPath> {
        predicate(move |p: &VfsPath| p.as_str() == path)
    }
}
