use std::path::Path;

use vfs::VfsPath;

use crate::chart::discovery::{extract_chart_archive, is_chart_archive};
use crate::error::{CliError, EngineResult};
use crate::load_budget::LoadBudget;

/// Open root chart directory shared by config discovery and chart analysis.
#[derive(Debug, Clone)]
pub struct RootChartSource {
    chart_dir: VfsPath,
}

impl RootChartSource {
    /// Opens a physical chart directory or extracts a packaged chart archive.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be inspected, is neither a
    /// directory nor a supported chart archive, or archive extraction fails.
    pub fn open(path: &Path, load_budget: LoadBudget) -> EngineResult<Self> {
        let metadata = std::fs::metadata(path)?;
        let chart_dir = if metadata.is_dir() {
            let path = path.to_string_lossy();
            VfsPath::new(vfs::PhysicalFS::new(path.as_ref()))
        } else if metadata.is_file()
            && path
                .file_name()
                .is_some_and(|name| is_chart_archive(&name.to_string_lossy()))
        {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let parent = parent.to_string_lossy();
            let root = VfsPath::new(vfs::PhysicalFS::new(parent.as_ref()));
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .ok_or_else(|| {
                    CliError::CliValidation(format!(
                        "chart archive path has no file name: {}",
                        path.display()
                    ))
                })?;
            extract_chart_archive(&root.join(file_name.as_ref())?, load_budget)?
        } else {
            return Err(CliError::CliValidation(format!(
                "chart source must be a directory or .tgz/.tar.gz archive: {}",
                path.display()
            )));
        };

        Ok(Self { chart_dir })
    }

    /// Returns the root VFS directory used for both config and chart loading.
    #[must_use]
    pub fn chart_dir(&self) -> &VfsPath {
        &self.chart_dir
    }

    /// Consumes the source and returns its root VFS directory.
    #[must_use]
    pub fn into_chart_dir(self) -> VfsPath {
        self.chart_dir
    }
}
