use helm_schema_ast::DefineIndex;
use tracing::instrument;

use super::FileRole;
use super::types::ChartContext;
use crate::error::EngineResult;

#[instrument(skip_all)]
pub fn build_define_index(charts: &[ChartContext]) -> EngineResult<DefineIndex> {
    // The loading stage precedes UTF-8 validation and defaults decoding.
    for chart in charts {
        chart.files.prepare_roles()?;
    }
    let mut index = DefineIndex::new();

    for chart in charts {
        for file in chart.files.files_with_role(FileRole::DefineIndexTemplate)? {
            index.add_chart_template_source(
                &chart.template_namespace,
                &template_execution_name(chart, &file.name),
                file.source()?,
            );
        }

        for file in chart.files.files_with_role(FileRole::FilesGetSource)? {
            index.add_chart_file(&chart.template_namespace, &file.name, &file.bytes);
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
