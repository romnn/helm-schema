use tracing::instrument;
use vfs::VfsPath;

use crate::chart_files::{self, FileRole};
use crate::error::CliResult;

#[instrument(skip_all)]
pub fn list_manifest_templates(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    files_with_role(chart_dir, include_tests, FileRole::ManifestTemplate)
}

pub(super) fn files_with_role(
    chart_dir: &VfsPath,
    include_tests: bool,
    role: FileRole,
) -> CliResult<Vec<VfsPath>> {
    Ok(chart_files::list_chart_files(chart_dir, include_tests)?
        .into_iter()
        .filter(|file| file.has_role(role))
        .map(|file| file.path)
        .collect())
}
