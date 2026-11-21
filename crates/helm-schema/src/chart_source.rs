use std::path::{Path, PathBuf};

use vfs::VfsPath;

use crate::chart::discovery::is_chart_archive;
use crate::chart::{LoadedChart, PreparedRootFile, SourcePath};
use crate::error::{CliError, EngineResult};
use crate::load_budget::{LoadBudget, read_to_end_capped};

/// Root chart input shared by config discovery and chart analysis.
#[derive(Debug, Clone)]
pub struct RootChartSource(Source);

#[derive(Debug, Clone)]
enum Source {
    Directory {
        path: VfsPath,
        native_path: Option<PathBuf>,
        prepared_file: Option<PreparedRootFile>,
    },
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
        Self(Source::Directory {
            path,
            native_path: None,
            prepared_file: None,
        })
    }

    /// Opens a physical chart directory or captures a packaged chart archive.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be inspected, is neither a
    /// directory nor a supported chart archive, or the archive cannot be read.
    pub fn open(path: &Path, load_budget: LoadBudget) -> EngineResult<Self> {
        let metadata = std::fs::metadata(path)?;
        if metadata.is_dir() {
            return Ok(Self(Source::Directory {
                path: VfsPath::new(vfs::PhysicalFS::new(path)),
                native_path: Some(path.to_path_buf()),
                prepared_file: None,
            }));
        }
        if let Some(name) = path.file_name().map(|name| name.to_string_lossy())
            && metadata.is_file()
            && is_chart_archive(&name)
        {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let parent = parent.to_string_lossy();
            let root = VfsPath::new(vfs::PhysicalFS::new(parent.as_ref()));
            let archive = root.join(name.as_ref())?;
            let bytes = read_to_end_capped(
                &mut archive.open_file()?,
                load_budget.max_chart_archive_bytes,
                archive.as_str(),
            )?;
            return Ok(Self(Source::Archive(LoadedChart::read_archive(
                &bytes,
                archive.as_str(),
                SourcePath::Native(path.to_path_buf()),
                load_budget,
                true,
            )?)));
        }

        Err(CliError::CliValidation(format!(
            "chart source must be a directory or .tgz/.tar.gz archive: {}",
            path.display()
        )))
    }

    /// Reads a root file for input preparation, such as CLI config discovery.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read as UTF-8.
    pub fn read_root_file(&mut self, name: &str) -> EngineResult<Option<String>> {
        match &mut self.0 {
            Source::Directory {
                path,
                native_path: _,
                prepared_file,
            } => {
                if let Some(file) = prepared_file.as_ref().filter(|file| file.name == name) {
                    return Ok(Some(file.source.clone()));
                }
                let path = path.join(name)?;
                if path.is_file()? {
                    let source = path.read_to_string()?;
                    *prepared_file = Some(PreparedRootFile {
                        name: name.to_string(),
                        source: source.clone(),
                    });
                    return Ok(Some(source));
                }
                Ok(None)
            }
            Source::Archive(chart) => chart
                .file(name)?
                .map(|file| file.source().map(str::to_owned))
                .transpose(),
        }
    }

    pub(crate) fn load(&self, include_tests: bool) -> EngineResult<LoadedChart> {
        match &self.0 {
            Source::Directory {
                path,
                native_path,
                prepared_file,
            } => Ok(LoadedChart::read_directory(
                path,
                native_path.as_deref(),
                prepared_file.as_ref(),
                include_tests,
            )?),
            Source::Archive(chart) => Ok(chart.analysis_view(include_tests)),
        }
    }

    pub(crate) fn logical_path(&self) -> &str {
        match &self.0 {
            Source::Directory {
                path,
                native_path: _,
                prepared_file: _,
            } => path.as_str(),
            Source::Archive(chart) => chart.path(),
        }
    }
}
