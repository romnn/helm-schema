use std::io::Read;
use std::path::Path;

use serde_json::Value;
use tracing::instrument;

use super::ChartContext;
use crate::error::CliResult;
use crate::fetch_policy::FetchPolicy;
use crate::flatten;

pub(crate) const GENERATED_SCHEMA_MARKER_KEY: &str = "x-helm-schema-generated";

/// One shipped `values.schema.json` constraint scoped to the chart prefix it
/// validates inside the composed root values document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScopedValuesSchemaConstraint {
    pub(crate) values_prefix: Vec<String>,
    pub(crate) schema: Value,
}

#[instrument(skip_all)]
pub(crate) fn load_shipped_values_schema_constraints(
    charts: &[ChartContext],
    fetch_policy: FetchPolicy,
) -> CliResult<Vec<ScopedValuesSchemaConstraint>> {
    let mut constraints = Vec::new();

    for chart in charts {
        let schema_path = chart.chart_dir.join("values.schema.json")?;
        if !schema_path.is_file()? {
            continue;
        }

        let mut bytes = Vec::new();
        schema_path.open_file()?.read_to_end(&mut bytes)?;
        let mut schema: Value = serde_json::from_slice(&bytes)?;

        // Avoid feeding a previous helm-schema output back into the next run as
        // if it were chart-authored source. Some deployment workflows write
        // generated output to `values.schema.json` in-place, and without this
        // guard a second run recursively ingests its own synthesized provider
        // defs as input constraints.
        if schema
            .get(GENERATED_SCHEMA_MARKER_KEY)
            .and_then(Value::as_bool)
            .is_some_and(|marker| marker)
        {
            continue;
        }

        // When the chart lives on the physical filesystem, bundle any refs
        // against the schema's own directory before composing it into the
        // generated root document. In-memory VFS-backed tests fall back to the
        // raw document, which still covers the common no-ref case.
        let physical_path = Path::new(schema_path.as_str());
        if physical_path.exists() {
            let base_dir = physical_path.parent().unwrap_or_else(|| Path::new("."));
            schema = flatten::bundle_refs(schema, base_dir, fetch_policy)?;
        }

        constraints.push(ScopedValuesSchemaConstraint {
            values_prefix: chart.values_prefix.clone(),
            schema,
        });
    }

    Ok(constraints)
}
