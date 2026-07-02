use helm_schema_ast::DefineIndex;
use tracing::instrument;
use vfs::VfsPath;

use super::types::ChartContext;
use super::{FileRole, list_chart_files};
use crate::error::CliResult;

#[instrument(skip_all)]
pub fn build_define_index(charts: &[ChartContext], include_tests: bool) -> CliResult<DefineIndex> {
    let mut index = DefineIndex::new();

    for chart in charts {
        let chart_files = list_chart_files(&chart.chart_dir, include_tests)?;

        for path in chart_files
            .iter()
            .filter(|file| file.has_role(FileRole::DefineIndexTemplate))
            .map(|file| &file.path)
        {
            index.add_file_source(
                &define_source_logical_path(chart, path),
                &path.read_to_string()?,
            );
        }

        for path in chart_files
            .iter()
            .filter(|file| file.has_role(FileRole::FilesGetSource))
            .map(|file| &file.path)
        {
            index.add_file_source(
                &files_get_relative_path(&chart.chart_dir, path),
                &path.read_to_string()?,
            );
        }
    }

    Ok(index)
}

fn chart_relative_path(chart_dir: &VfsPath, path: &VfsPath) -> String {
    let abs = path.as_str();
    let root = chart_dir.as_str().trim_end_matches('/');
    abs.strip_prefix(root)
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or(abs)
        .to_string()
}

fn define_source_logical_path(chart: &ChartContext, path: &VfsPath) -> String {
    let relative = chart_relative_path(&chart.chart_dir, path);
    if chart.values_prefix.is_empty() {
        return relative;
    }

    // Helper-body structural analysis rebuilds define bodies from the logical
    // file-source map. Paths must therefore stay unique across sibling charts:
    // plain `templates/_helpers.tpl` would make library helper sources
    // overwrite each other even though the define AST remains global.
    format!("charts/{}/{relative}", chart.values_prefix.join("/charts/"))
}

fn files_get_relative_path(chart_dir: &VfsPath, path: &VfsPath) -> String {
    let abs = path.as_str();
    let root = chart_dir.as_str().trim_end_matches('/');
    abs.strip_prefix(root)
        .and_then(|path| path.strip_prefix('/'))
        .or_else(|| abs.find("/files/").map(|index| &abs[(index + 1)..]))
        .or_else(|| abs.find("files/").map(|index| &abs[index..]))
        .unwrap_or(abs)
        .to_string()
}
