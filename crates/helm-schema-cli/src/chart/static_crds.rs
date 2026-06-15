use std::io::Read;

use helm_schema_k8s::LocalSchemaUniverse;
use serde::Deserialize;
use tracing::instrument;

use super::files::files_with_role;
use super::types::ChartContext;
use crate::chart_files::FileRole;
use crate::error::CliResult;

#[instrument(skip_all)]
pub fn collect_static_crd_universe(charts: &[ChartContext]) -> CliResult<LocalSchemaUniverse> {
    let mut universe = LocalSchemaUniverse::default();

    for chart in charts {
        for path in files_with_role(&chart.chart_dir, false, FileRole::StaticCrd)? {
            let mut source = String::new();
            path.open_file()?.read_to_string(&mut source)?;

            for document in serde_yaml::Deserializer::from_str(&source) {
                let document = serde_json::Value::deserialize(document)?;
                if !document.is_null() {
                    universe.insert_crd_document(document);
                }
            }
        }
    }

    Ok(universe)
}
