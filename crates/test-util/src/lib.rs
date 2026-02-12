use std::path::PathBuf;
use std::sync::Once;

use color_eyre::eyre;
use vfs::VfsPath;

pub mod sexpr;

pub mod prelude {
    pub use crate::matchers::*;
    pub use crate::sexpr::{ParseError as SExprParseError, SExpr};
    pub use crate::write;
    pub use crate::{Builder, LogLevel};
    pub use googletest::{assert_that, matcher::MatcherBase, matchers::*};
    pub use similar_asserts::assert_eq as sim_assert_eq;
}

/// Returns the workspace root directory via the `CARGO_WORKSPACE_DIR` env var
/// set in `.cargo/config.toml`.
///
/// # Panics
///
/// Panics if `CARGO_WORKSPACE_DIR` is not set.
#[must_use]
pub fn workspace_root() -> PathBuf {
    PathBuf::from(
        std::env::var("CARGO_WORKSPACE_DIR")
            .expect("CARGO_WORKSPACE_DIR must be set in .cargo/config.toml"),
    )
}

/// Returns the path to the workspace `testdata/` directory.
#[must_use]
pub fn workspace_testdata() -> PathBuf {
    workspace_root().join("testdata")
}

/// Reads a file relative to the workspace `testdata/` directory.
///
/// # Panics
///
/// Panics if the file cannot be read.
#[must_use]
pub fn read_testdata(relative_path: &str) -> String {
    let path = workspace_testdata().join(relative_path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Reads all files with the given extension from a directory relative to
/// the workspace `testdata/` directory.
///
/// Returns an empty `Vec` if the directory does not exist.
#[must_use]
pub fn read_testdata_dir(relative_dir: &str, extension: &str) -> Vec<String> {
    let dir = workspace_testdata().join(relative_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|e| e == extension)
            && let Ok(content) = std::fs::read_to_string(entry.path())
        {
            out.push(content);
        }
    }
    out
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

pub type LogLevel = tracing::metadata::Level;

static INIT_EYRE: Once = Once::new();

#[derive(Default)]
pub struct TestGuard {
    // trace_guard: Option<crate::trace::TraceGuard>,
}

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
    /// # Panics
    ///
    /// Panics if `color_eyre` installation fails.
    pub fn build(self) -> TestGuard {
        let test_guard = TestGuard::default();

        if self.install_eyre {
            INIT_EYRE.call_once(|| {
                color_eyre::install().expect("failed to install eyre");
            });
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

        test_guard
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

pub mod matchers {
    use googletest::matchers::{ContainsMatcher, contains, predicate};
    use vfs::VfsPath;

    #[must_use]
    pub fn contains_path(
        path: &str,
    ) -> ContainsMatcher<impl googletest::matcher::Matcher<&VfsPath>> {
        contains(matches_path(path))
    }

    #[must_use]
    pub fn matches_path(path: &str) -> impl googletest::matcher::Matcher<&VfsPath> {
        predicate(move |p: &VfsPath| p.as_str() == path)
    }
}
