use color_eyre::eyre;
use std::sync::Once;
use vfs::VfsPath;

pub mod prelude {
    pub use crate::matchers::*;
    pub use crate::write;
    pub use crate::{Builder, LogLevel};
    pub use googletest::{assert_that, matcher::MatcherBase, matchers::*};
    pub use similar_asserts::assert_eq as sim_assert_eq;
}

// fn str_paths<'a>(paths: impl IntoIterator<Item = &'a VfsPath>) -> Vec<&'a str> {
//     paths.into_iter().map(|p| p.as_str()).collect()
// }

pub fn write(path: &VfsPath, data: impl AsRef<[u8]>) -> eyre::Result<()> {
    let _ = path.parent().create_dir_all();
    let mut file = path.create_file()?;
    file.write_all(data.as_ref())?;
    Ok(())
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
    /// Initialize test
    ///
    /// This ensures `color_eyre` is setup once and env variables are read.
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
    pub fn with_tracing(mut self, enabled: bool) -> Self {
        self.setup_tracing = enabled;
        self
    }

    /// Toggle log level for tracing inside the test.
    pub fn with_log_level(mut self, log_level: impl Into<LogLevel>) -> Self {
        self.log_level = log_level.into();
        self
    }

    // /// Set color choice for tracing.
    // pub fn with_color_choice(mut self, color_choice: impl Into<ColorChoice>) -> Self {
    //     self.color_choice = color_choice.into();
    //     self
    // }

    /// Toggle installation of `color_eyre`.
    pub fn with_eyre(mut self, enabled: bool) -> Self {
        self.install_eyre = enabled;
        self
    }

    /// Configure the tracing subscribers env filter.
    ///
    /// Requires tracing to be enabled with `Self::with_tracing`.
    // pub fn with_env_filter(mut self, filter: Option<impl Into<String>>) -> Self {
    pub fn with_env_filter(mut self, filter: impl Into<String>) -> Self {
        self.env_filter = Some(filter.into());
        self
    }
}

/// Create a new builder.
pub fn builder() -> Builder {
    Builder::default()
}

pub mod matchers {
    use googletest::matchers::*;
    use vfs::VfsPath;

    pub fn contains_path<'a>(
        path: &'a str,
    ) -> ContainsMatcher<impl googletest::matcher::Matcher<&'a VfsPath>> {
        contains(matches_path(path))
    }

    pub fn matches_path<'a>(path: &'a str) -> impl googletest::matcher::Matcher<&'a VfsPath> {
        predicate(move |p: &VfsPath| p.as_str() == path)
    }
}
