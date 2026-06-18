use std::io::Read;

use helm_schema_engine::helpers::DefineIndex;
use helm_schema_engine::parse::TreeSitterParser;
use tracing::instrument;
use vfs::VfsPath;

use super::files::files_with_role;
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
            add_template_source(&mut index, chart, path)?;
        }

        for path in chart_files
            .iter()
            .filter(|file| file.has_role(FileRole::FilesGetSource))
            .map(|file| &file.path)
        {
            add_files_get_source(&mut index, &chart.chart_dir, path)?;
        }
    }

    Ok(index)
}

#[instrument(skip_all)]
pub fn list_template_sources_for_define_index(
    chart_dir: &VfsPath,
    include_tests: bool,
) -> CliResult<Vec<VfsPath>> {
    files_with_role(chart_dir, include_tests, FileRole::DefineIndexTemplate)
}

fn add_template_source(
    index: &mut DefineIndex,
    chart: &ChartContext,
    path: &VfsPath,
) -> CliResult<()> {
    let mut source = String::new();
    path.open_file()?.read_to_string(&mut source)?;
    index.add_source(&TreeSitterParser, &source)?;
    index.add_file_source(&define_source_logical_path(chart, path), &source);
    Ok(())
}

fn add_files_get_source(
    index: &mut DefineIndex,
    chart_dir: &VfsPath,
    path: &VfsPath,
) -> CliResult<()> {
    let mut source = String::new();
    path.open_file()?.read_to_string(&mut source)?;
    index.add_file_source(&files_get_relative_path(chart_dir, path), &source);
    Ok(())
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
