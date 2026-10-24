//! Records schema-byte and finalized-IR hashes for every admission generation job.
//!
//! Run with the suite directory, the CLI binary, and a destination for the retained bytes.
//! Build this driver against each tree being compared; the CLI and IR must come from that tree.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{DeclaredTypes as AuthoringDeclaredTypes, SchemaProfile};
use helm_schema_test_support::admission::{self, DeclaredTypes};
use helm_schema_test_support::generate::generate_options_at;
use helm_schema_test_support::registry::ChartRecipe;
use helm_schema_test_support::source_digest::sha256_hex;

#[tracing::instrument]
fn main() -> eyre::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let root = PathBuf::from(arguments.next().ok_or_eyre("missing suite")?);
    let binary = PathBuf::from(arguments.next().ok_or_eyre("missing binary")?);
    let output = PathBuf::from(arguments.next().ok_or_eyre("missing output")?);
    fs::create_dir_all(&output)?;
    let suite = admission::read_suite(&root)?;
    let bundle = root
        .parent()
        .ok_or_eyre("missing testdata")?
        .join("provider-bundle");
    let jobs: BTreeSet<_> = suite
        .cells
        .iter()
        .map(|cell| (cell.chart.clone(), cell.declared_types))
        .collect();
    println!("chart\tdeclared_types\tschema_sha256\tir_sha256");
    for (index, (chart, mode)) in jobs.into_iter().enumerate() {
        let destination = output.join(index.to_string());
        fs::create_dir_all(&destination)?;
        let schema = destination.join("schema.json");
        let chart_path = root.join(&chart);
        let arguments: Vec<_> = admission::generation_arguments(mode)
            .into_iter()
            .map(|argument| {
                argument
                    .replace("<chart>", &chart_path.to_string_lossy())
                    .replace("<bundle>", &bundle.to_string_lossy())
                    .replace("<out>", &schema.to_string_lossy())
            })
            .collect();
        let result = Command::new(&binary)
            .env_clear()
            .env("HOME", &destination)
            .env("XDG_CACHE_HOME", &destination)
            .env("XDG_CONFIG_HOME", &destination)
            .args(arguments)
            .output()?;
        eyre::ensure!(
            result.status.success(),
            "{chart}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut recipe = ChartRecipe::corpus("", SchemaProfile::Full);
        recipe.authoring.declared_types = match mode {
            DeclaredTypes::Annotate => AuthoringDeclaredTypes::Annotate,
            DeclaredTypes::Assert => AuthoringDeclaredTypes::Assert,
        };
        let document =
            AnalysisSession::new(generate_options_at(&chart_path, &recipe)).contract_document()?;
        let ir = serde_json::to_vec_pretty(&document)?;
        fs::write(destination.join("ir.json"), &ir)?;
        println!(
            "{chart}\t{}\t{}\t{}",
            mode.as_arg(),
            sha256_hex(&fs::read(schema)?),
            sha256_hex(&ir)
        );
    }
    Ok(())
}
