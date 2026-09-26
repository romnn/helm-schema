use helm_schema_ast::DefineIndex;
use tracing::instrument;
use vfs::VfsPath;

use super::types::ChartContext;
use super::{FileRole, LoadedChartCorpus};
use crate::error::EngineResult;

#[instrument(skip_all)]
pub fn build_define_index(
    charts: &[ChartContext],
    corpus: &LoadedChartCorpus,
) -> EngineResult<DefineIndex> {
    let mut index = DefineIndex::new();

    for chart in charts {
        let chart_files = corpus.chart(chart)?;

        for path in chart_files.files_with_role(FileRole::DefineIndexTemplate) {
            index.add_file_source(&template_execution_name(chart, &path.path), path.source()?);
        }

        for path in chart_files.files_with_role(FileRole::FilesGetSource) {
            if let Some(source) = path.utf8_source() {
                index.add_files_get_source(
                    &files_get_relative_path(&chart.chart_dir, &path.path),
                    source,
                );
            }
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

/// The name Helm registers this template under.
///
/// Helm keeps one global template namespace whose keys are
/// `<root chart>/[charts/<dependency key>]*/<chart-relative path>`. Using
/// that exact name as the index key makes template identity the same fact
/// for the analyzer and for Helm: two charts shipping `templates/x.yaml`
/// stay two distinct templates, and a computed include resolves to the
/// executing chart's own file.
fn template_execution_name(chart: &ChartContext, path: &VfsPath) -> String {
    format!(
        "{}/{}",
        chart.template_namespace,
        chart_relative_path(&chart.chart_dir, path)
    )
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
