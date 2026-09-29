use std::path::PathBuf;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, ValuesPath,
};
use helm_schema_ir::{ContractIr, ContractUse, Guard, GuardValue, ValueKind, YamlPath};
use test_util::prelude::sim_assert_eq;

use crate::explain::{ExplainFormat, generation_report};

fn scalar_use(guards: Vec<Guard>) -> ContractUse {
    ContractUse::new(
        ValuesPath::parse("name"),
        YamlPath(vec!["data".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        guards,
        None,
    )
}

fn expected_report(name: &str) -> eyre::Result<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/tests/fixtures/explain")
        .join(name);
    std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))
}

/// The first overlay of `name` tests a path that has no evidence and no
/// declared default, so lowering skips it; the recorded overlay keeps its
/// original position and the report resolves its guards from the owning
/// vector. Contract analysis records every guard path it reads, so the
/// unsupported overlay is placed at the signal boundary.
#[test]
fn overlay_reports_preserve_original_indices_and_guards() -> eyre::Result<()> {
    let finalized = ContractIr::from_contract_uses(vec![scalar_use(vec![Guard::Eq {
        path: ValuesPath::parse("mode"),
        value: GuardValue::string("on"),
    }])])
    .finalize();
    let mut evidence_by_path = finalized
        .schema_signals()
        .schema_evidence_by_value_path()
        .clone();
    let name_evidence = evidence_by_path
        .get_mut(&ValuesPath::parse("name"))
        .ok_or_eyre("name evidence")?;
    let lowered = name_evidence
        .conditional_overlays
        .first()
        .ok_or_eyre("name overlay")?
        .clone();
    name_evidence.conditional_overlays.insert(
        0,
        ConditionalPathOverlay::new(
            vec![ConditionalGuard::Truthy {
                path: ValuesPath::parse("absent.flag"),
            }],
            lowered.evidence.clone(),
            lowered.preserve_base_schema,
            lowered.flavor,
        ),
    );
    let signals = &ContractSchemaSignals::new(evidence_by_path, Vec::new());
    let provider = helm_schema_k8s::Chain::new(Vec::new());
    let generated = helm_schema_gen::generate_values_schema_with_report(
        helm_schema_gen::ValuesSchemaInput::new(signals, &provider),
    );
    let path = ValuesPath::parse("name");
    let decision = generated.generation_decisions.path(&path);
    let evidence = signals.evidence_for(&path);

    sim_assert_eq!(
        have: evidence.map(|evidence| evidence.conditional_overlays.len()),
        want: Some(2)
    );
    sim_assert_eq!(
        have: decision.map(|decision| decision
            .overlays
            .iter()
            .map(|overlay| overlay.overlay)
            .collect::<Vec<_>>()),
        want: Some(vec![1])
    );
    sim_assert_eq!(
        have: generation_report(&path, decision, evidence, ExplainFormat::Json)?,
        want: expected_report("skipped-overlay.name.json")?
    );
    sim_assert_eq!(
        have: generation_report(&path, decision, evidence, ExplainFormat::Text)?,
        want: expected_report("skipped-overlay.name.txt")?
    );
    Ok(())
}
