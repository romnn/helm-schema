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

    let provider = schema::UpstreamThenDefaultVytSchemaProvider::default();
    let mut out = schema::generate_values_schema_vyt(&uses, &provider);

    let mut values_docs: Vec<serde_yaml::Value> = Vec::new();
    for p in &chart.values_files {
        let Ok(raw) = p
            .read_to_string()
            .wrap_err_with(|| format!("read values {}", p.as_str()))
        else {
            continue;
        };

        let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&raw)
            .wrap_err_with(|| format!("parse values yaml {}", p.as_str()))
        else {
            continue;
        };

        values_docs.push(doc);
    }

    if !values_docs.is_empty() {
        schema::add_values_yaml_baseline(&mut out, &values_docs);
    }

    // Step 2: Additively compose subchart values.yaml under the subchart key.
    for sc in &chart.subcharts {
        let sub_root = match &sc.location {
            helm_schema_chart::model::SubchartLocation::Directory { path } => path.clone(),
            helm_schema_chart::model::SubchartLocation::Archive { tgz_path, inner_root } => {
                let Ok(p) = helm_schema_chart::archive_subchart_root(tgz_path, inner_root) else {
                    continue;
                };
                p
            }
        };

        let sub_chart = match helm_schema_chart::load_chart(
            &sub_root,
            &helm_schema_chart::LoadOptions {
                include_tests: false,
                recurse_subcharts: true,
                auto_extract_tgz: true,
                ..Default::default()
            },
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut sub_values_docs: Vec<serde_yaml::Value> = Vec::new();
        for p in &sub_chart.values_files {
            let Ok(raw) = p
                .read_to_string()
                .wrap_err_with(|| format!("read values {}", p.as_str()))
            else {
                continue;
            };

            let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&raw)
                .wrap_err_with(|| format!("parse values yaml {}", p.as_str()))
            else {
                continue;
            };

            sub_values_docs.push(doc);
        }

        if !sub_values_docs.is_empty() {
            schema::add_values_yaml_baseline_under(&mut out, &sc.name, &sub_values_docs);
        }
    }

    Ok(out)
}
