//! Independent helper-effect certificate census over canonical whole-chart generation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_ast::DefineIndex;
use helm_schema_test_support::generate::chart_schema;
use helm_schema_test_support::registry::{ArtifactId, ChartId, GenerationRecipe};
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use tracing::field::{Field, Visit};
use tracing_subscriber::prelude::*;

use crate::analysis_db::IrAnalysisDb;

#[derive(Deserialize)]
struct CertificateEvent {
    source: String,
    local_templates: BTreeSet<String>,
    required: bool,
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Observation {
    Certificate(CertificateEvent),
}

#[derive(Default)]
struct Census {
    certificates: BTreeMap<CertificateKey, bool>,
    comparisons: usize,
    required: usize,
    read_only: usize,
    certificate_mismatches: usize,
    errors: Vec<String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CertificateKey {
    source: String,
    local_templates: BTreeSet<String>,
}

impl Census {
    fn observe(&mut self, observation: Observation) {
        match observation {
            Observation::Certificate(CertificateEvent {
                source,
                local_templates,
                required,
            }) => {
                let key = CertificateKey {
                    source,
                    local_templates,
                };
                let may_write = *self.certificates.entry(key).or_insert_with_key(|key| {
                    let db = IrAnalysisDb::new(&DefineIndex::new());
                    let parsed = db.parsed_source(&key.source, None, "stage0-certificate");
                    parsed.root.syntax().is_none_or(|syntax| {
                        super::write_certificate::may_write(
                            &key.source,
                            syntax,
                            &key.local_templates,
                        )
                    })
                });
                self.comparisons += 1;
                self.required += usize::from(required);
                self.read_only += usize::from(!may_write);
                self.certificate_mismatches += usize::from(required && !may_write);
            }
        }
    }

    fn report(&self, chart: &str) -> Value {
        json!({"chart": chart,
            "certificate_comparisons": self.comparisons, "certificate_required": self.required,
            "certificate_read_only": self.read_only,
            "certificate_mismatches": self.certificate_mismatches, "errors": self.errors})
    }
}

#[derive(Clone)]
struct CensusLayer(Arc<Mutex<Census>>);

#[derive(Default)]
struct Payload {
    json: String,
}

impl Visit for Payload {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "payload" {
            self.json = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "payload" {
            self.json = value.to_owned();
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CensusLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "helm_schema_ir::stage0" {
            return;
        }
        let Self(census) = self;
        let mut payload = Payload::default();
        event.record(&mut payload);
        let mut census = census.lock().unwrap_or_else(PoisonError::into_inner);
        match serde_json::from_str(&payload.json) {
            Ok(observation) => census.observe(observation),
            Err(err) => census.errors.push(format!("{err:#}")),
        }
    }
}

#[test]
#[ignore = "canonical eight-chart Stage 0 census; run one chart per process when memory is bounded"]
fn stage0_chart_census() -> eyre::Result<()> {
    let census = Arc::new(Mutex::new(Census::default()));
    let subscriber = tracing_subscriber::registry().with(
        CensusLayer(Arc::clone(&census)).with_filter(tracing_subscriber::filter::filter_fn(
            |metadata| metadata.target() == "helm_schema_ir::stage0",
        )),
    );
    tracing::subscriber::set_global_default(subscriber)?;
    let selected = std::env::var("STAGE0_CENSUS_CHART").ok();
    let mut reports = Vec::new();
    for chart in [
        ChartId::Airflow,
        ChartId::ArgoCd,
        ChartId::BitnamiRedis,
        ChartId::Cilium,
        ChartId::Datadog,
        ChartId::KubePrometheusStack,
        ChartId::PostgresqlHa,
        ChartId::Vault,
    ] {
        let GenerationRecipe::Chart(recipe) = ArtifactId::Chart(chart).spec().recipe else {
            eyre::bail!("census chart lacks a canonical whole-chart recipe");
        };
        if selected
            .as_ref()
            .is_some_and(|selected| selected != recipe.chart)
        {
            continue;
        }
        *census.lock().unwrap_or_else(PoisonError::into_inner) = Census::default();
        chart_schema(&recipe).wrap_err_with(|| format!("generate {}", recipe.chart))?;
        let census = census.lock().unwrap_or_else(PoisonError::into_inner);
        let report = census.report(recipe.chart);
        eprintln!("stage0 {report}");
        reports.push(report);
        if let Some(output) = std::env::var_os("STAGE0_CENSUS_OUT") {
            std::fs::write(
                output,
                serde_json::to_vec_pretty(&json!({"charts": &reports}))?,
            )?;
        }
        eyre::ensure!(
            census.comparisons > 0,
            "census observed no certificate comparisons"
        );
        sim_assert_eq!(have: census.certificate_mismatches, want: 0);
        sim_assert_eq!(have: &census.errors, want: &Vec::<String>::new());
    }
    eyre::ensure!(!reports.is_empty(), "no canonical census chart selected");
    if let Some(output) = std::env::var_os("STAGE0_CENSUS_OUT") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&json!({"charts": reports}))?,
        )?;
    }
    Ok(())
}
