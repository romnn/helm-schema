use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;

use vfs::VfsPath;

use crate::chart::{Directory, LoadedChart, SourcePath};
use crate::error::{CliError, EngineResult};
use crate::load_budget::LoadBudget;

/// Root chart input shared by config discovery and chart analysis.
#[derive(Debug, Clone)]
pub struct RootChartSource(Source);

#[derive(Debug, Clone)]
enum Source {
    Directory(Arc<Directory>),
    Archive(LoadedChart),
}

impl From<VfsPath> for RootChartSource {
    fn from(path: VfsPath) -> Self {
        Self::directory(path)
    }
}

impl RootChartSource {
    /// Uses a virtual-filesystem directory as the chart source.
    #[must_use]
    pub fn directory(path: VfsPath) -> Self {
        Self(Source::Directory(Arc::new(Directory::new(path, None))))
    }

    /// Opens a physical chart directory or captures a packaged chart archive.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be inspected, is neither a
    /// directory nor a supported chart archive, or the archive cannot be read.
    pub fn open(path: &Path, _load_budget: LoadBudget) -> EngineResult<Self> {
        let metadata = fs::metadata(path)?;
        if metadata.is_dir() {
            return Ok(Self(Source::Directory(Arc::new(Directory::new(
                VfsPath::new(vfs::PhysicalFS::new(path)),
                Some(path),
            )))));
        }
        if metadata.is_file() {
            return Ok(Self(Source::Archive(LoadedChart::read_archive(
                File::open(path)?,
                SourcePath::Native(path.to_path_buf()),
                true,
            )?)));
        }
        Err(CliError::CliValidation(format!(
            "chart source must be a directory or gzip archive: {}",
            path.display()
        )))
    }

    /// Reads a root file for input preparation, such as CLI config discovery.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read as UTF-8.
    pub fn read_root_file(&mut self, name: &str) -> EngineResult<Option<String>> {
        match &self.0 {
            Source::Directory(dir) => dir
                .direct(name)?
                .map(|bytes| {
                    std::str::from_utf8(&bytes).map(str::to_owned).map_err(|_| {
                        CliError::NonUtf8ChartSource {
                            path: format!("{}/{name}", dir.path()),
                        }
                    })
                })
                .transpose(),
            Source::Archive(chart) => chart
                .file(name)
                .map(|file| file.source().map(str::to_owned))
                .transpose(),
        }
    }

    pub(crate) fn load(&self, include_tests: bool) -> EngineResult<LoadedChart> {
        match &self.0 {
            Source::Directory(dir) => LoadedChart::read_directory(dir.for_session(), include_tests),
            Source::Archive(chart) => Ok(chart.analysis_view(include_tests)),
        }
    }

    pub(crate) fn logical_path(&self) -> &str {
        match &self.0 {
            Source::Directory(dir) => dir.path(),
            Source::Archive(chart) => chart.path.as_str(),
        }
    }
}
