#![allow(warnings)]

pub mod analyze;
pub mod sanitize;
pub mod values;
pub mod yaml_path;
pub mod yaml_sink;
pub mod schema;
pub mod vyt;

pub use analyze::{Role, ValueUse, analyze_template_file};
pub use yaml_path::YamlPath;

pub fn generate_values_schema_for_chart_vyt(
    chart: &helm_schema_chart::ChartSummary,
) -> color_eyre::eyre::Result<serde_json::Value> {
    use color_eyre::eyre::WrapErr;

    let mut defs = vyt::DefineIndex::default();
    for p in &chart.templates {
        let Ok(src) = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))
        else {
            continue;
        };
        let _ = vyt::extend_define_index_from_str(&mut defs, &src);
    }
    let defs = std::sync::Arc::new(defs);

    let mut uses: Vec<vyt::VYUse> = Vec::new();
    let mut processed_any = false;
    for p in &chart.templates {
        let Ok(src) = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))
        else {
            continue;
        };

        let Some(parsed) = helm_schema_template::parse::parse_gotmpl_document(&src) else {
            continue;
        };

        processed_any = true;
        let mut u = vyt::VYT::new(src)
            .with_defines(std::sync::Arc::clone(&defs))
            .run(&parsed.tree);
        uses.append(&mut u);
    }

    if !processed_any {
        return Err(color_eyre::eyre::eyre!("no templates could be processed"));
    }

    let provider = schema::DefaultVytSchemaProvider::default();
    Ok(schema::generate_values_schema_vyt(&uses, &provider))
}
