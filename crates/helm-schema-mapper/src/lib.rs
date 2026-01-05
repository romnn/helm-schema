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

#[derive(Debug, Clone)]
pub struct GenerateValuesSchemaOptions {
    pub add_values_yaml_baseline: bool,
    pub compose_subcharts: bool,
    pub ingest_values_schema_json: bool,
}

impl Default for GenerateValuesSchemaOptions {
    fn default() -> Self {
        Self {
            add_values_yaml_baseline: true,
            compose_subcharts: true,
            ingest_values_schema_json: false,
        }
    }
}

pub fn generate_values_schema_for_chart_vyt_with_options<P: schema::VytSchemaProvider>(
    chart: &helm_schema_chart::ChartSummary,
    provider: &P,
    options: &GenerateValuesSchemaOptions,
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

    let mut out = if processed_any {
        schema::generate_values_schema_vyt(&uses, provider)
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

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

    if options.add_values_yaml_baseline && !values_docs.is_empty() {
        schema::add_values_yaml_baseline(&mut out, &values_docs);
    }

    if options.ingest_values_schema_json {
        let chart_root = chart
            .values_files
            .first()
            .map(|p| p.parent())
            .or_else(|| chart.templates.first().map(|p| p.parent().parent()))
            .or_else(|| chart.crds.first().map(|p| p.parent().parent()))
            .or_else(|| {
                chart.subcharts.first().map(|sc| match &sc.location {
                    helm_schema_chart::model::SubchartLocation::Directory { path } => {
                        path.parent().parent()
                    }
                    helm_schema_chart::model::SubchartLocation::Archive { tgz_path, .. } => {
                        tgz_path.parent().parent()
                    }
                })
            });

        if let Some(root) = chart_root {
            if let Ok(schema_path) = root.join("values.schema.json") {
                if schema_path.exists().unwrap_or(false) {
                    if let Ok(raw) = schema_path.read_to_string().wrap_err_with(|| {
                        format!("read values.schema.json {}", schema_path.as_str())
                    }) {
                        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw)
                            .wrap_err_with(|| {
                                format!("parse values.schema.json {}", schema_path.as_str())
                            })
                        {
                            schema::add_json_schema_baseline_additive(&mut out, &doc);
                        }
                    }
                }
            }
        }
    }

    if options.compose_subcharts {
        for sc in &chart.subcharts {
            let values_key = chart
                .dependencies
                .iter()
                .find(|d| d.name == sc.name || d.alias.as_deref() == Some(sc.name.as_str()))
                .map(|d| d.alias.clone().unwrap_or_else(|| d.name.clone()))
                .unwrap_or_else(|| sc.name.clone());

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
                    respect_gitignore: false,
                    include_hidden: false,
                    ..Default::default()
                },
            ) {
                Ok(c) => c,
                Err(_) => continue,
            };

            match generate_values_schema_for_chart_vyt_with_options(&sub_chart, provider, options) {
                Ok(sub_schema) => {
                    schema::add_json_schema_baseline_additive_at_path(
                        &mut out,
                        &values_key,
                        &sub_schema,
                    );
                }
                Err(_) => {
                    if !options.add_values_yaml_baseline {
                        continue;
                    }

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
                        schema::add_values_yaml_baseline_under(
                            &mut out,
                            &values_key,
                            &sub_values_docs,
                        );
                    }
                }
            }
        }
    }

    Ok(out)
}

pub fn generate_values_schema_for_chart_vyt(
    chart: &helm_schema_chart::ChartSummary,
) -> color_eyre::eyre::Result<serde_json::Value> {
    let provider = schema::UpstreamThenDefaultVytSchemaProvider::default();
    let options = GenerateValuesSchemaOptions::default();
    generate_values_schema_for_chart_vyt_with_options(chart, &provider, &options)
}
