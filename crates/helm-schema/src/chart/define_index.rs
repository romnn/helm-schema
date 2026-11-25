use helm_schema_ast::DefineIndex;
use tracing::instrument;

use super::FileRole;
use super::types::ChartContext;
use crate::error::EngineResult;

#[instrument(skip_all)]
pub fn build_define_index(charts: &[ChartContext]) -> EngineResult<DefineIndex> {
    let mut index = DefineIndex::new();

    for chart in charts {
        for file in chart.files.files_with_role(FileRole::DefineIndexTemplate) {
            let name = file.template_name();
            // Helm parses only a library chart's partials (`engine.go:592`).
            if chart.is_library && !is_partial(&name) {
                continue;
            }
            index.add_chart_template_source(
                &chart.template_namespace,
                &template_execution_name(chart, &name),
                file.source()?,
            );
        }

        for file in chart.files.files_with_role(FileRole::FilesGetSource) {
            if let Some(name) = file.text_name() {
                index.add_chart_file(&chart.template_namespace, name, &file.bytes);
            }
        }
    }

    Ok(index)
}

/// The name Helm registers this template under.
///
/// Helm keeps one global template namespace whose keys are
/// `<root chart>/[charts/<dependency key>]*/<chart-relative path>`. Using
/// that exact name as the index key makes template identity the same fact
/// for the analyzer and for Helm: two charts shipping `templates/x.yaml`
/// stay two distinct templates, and a computed include resolves to the
/// executing chart's own file.
pub(crate) fn template_execution_name(chart: &ChartContext, name: &str) -> String {
    format!("{}/{name}", chart.template_namespace)
}

/// Whether a template is a partial: its file name starts with `_`.
fn is_partial(name: &str) -> bool {
    name.rsplit('/')
        .next()
        .is_some_and(|file| file.starts_with('_'))
}
