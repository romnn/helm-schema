//! The differential gate's public contracts: verified dump pairs, the
//! persistent witness universe and its chain, sealed receipts, witness
//! classification and the construction policy.

use std::collections::BTreeSet;
use std::path::PathBuf;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::acceptance::discovery::ProbeInstance;
use helm_schema_test_support::acceptance::screen::{ScreenedDocument, ScreenedProbe};
use helm_schema_test_support::acceptance::{
    AdjudicationFailure, AdjudicationRecord, FailureKind, HelmFlipVerdict, ScreenedWitness,
};
use helm_schema_test_support::manifest::{MANIFEST_FILE, Manifest};
use helm_schema_test_support::value_algebra_diff::DiffError;
use helm_schema_test_support::value_algebra_diff::capture::{DumpPair, VerifiedDump};
use helm_schema_test_support::value_algebra_diff::compare::{
    ChartIdentity, Class, ConstructionRequest, LedgerRow, OracleAnswer, OutcomeTable, PriorSets,
    RegressionKey, SeedReview, WitnessOutcome, classify, construction_policy, replay_witness,
    witness_record,
};
use helm_schema_test_support::value_algebra_diff::history::{
    Attempt, BlobStore, HistoryPlan, Receipt, ReceiptKind, Universe, UniverseState, WitnessRecord,
    canonical_json,
};
use serde_json::{Value, json};
use test_util::helm_values::AcceptanceDocument;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

mod common;

use common::{CHART, write_dump};

fn identity_error<T: std::fmt::Debug>(result: Result<T, DiffError>) -> eyre::Result<String> {
    match result {
        Err(DiffError::Identity(message)) => Ok(message),
        other => eyre::bail!("expected an identity error, got {other:?}"),
    }
}

#[test]
fn a_dump_is_verified_against_its_own_manifest() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-dump")?;
    let root = scratch.path().join("dump");
    write_dump(&root, br#"{"type":"object"}"#)?;
    let dump = VerifiedDump::open(&root)?;
    sim_assert_eq!(have: dump.keys().collect::<Vec<_>>(), want: vec![format!("chart/{CHART}").as_str()]);
    let schema = dump.schema(&format!("chart/{CHART}"))?;
    sim_assert_eq!(have: schema.bytes, want: br#"{"type":"object"}"#.to_vec());

    // A changed artifact byte, a foreign file and a stale manifest are refused.
    let artifact = root.join(format!("helm-schema.cli.chart-corpus.{CHART}.schema.json"));
    std::fs::write(&artifact, br#"{"type":"array"}"#)?;
    assert!(identity_error(VerifiedDump::open(&root))?.contains("differs from its manifest"));
    write_dump(&root, br#"{"type":"object"}"#)?;
    std::fs::write(root.join("stray.json"), b"{}")?;
    assert!(identity_error(VerifiedDump::open(&root))?.contains("does not list"));
    std::fs::remove_file(root.join("stray.json"))?;
    let mut manifest: Manifest = serde_json::from_slice(&std::fs::read(root.join(MANIFEST_FILE))?)?;
    manifest.harness_version += 1;
    std::fs::write(root.join(MANIFEST_FILE), serde_json::to_vec(&manifest)?)?;
    assert!(identity_error(VerifiedDump::open(&root))?.contains("harness version"));
    Ok(())
}

#[test]
fn a_dump_pair_needs_the_same_entries_and_the_comparators_inputs() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-pair")?;
    let (baseline, candidate) = (scratch.path().join("main"), scratch.path().join("cand"));
    write_dump(&baseline, b"{}")?;
    write_dump(&candidate, br#"{"type":"object"}"#)?;
    let testdata = test_util::workspace_testdata();
    let pair = DumpPair::new(
        VerifiedDump::open(&baseline)?,
        VerifiedDump::open(&candidate)?,
        &testdata,
    )?;
    let artifacts = pair.artifacts();
    sim_assert_eq!(have: artifacts.len(), want: 1);

    // Another recipe input digest is another experiment.
    let mut manifest: Manifest =
        serde_json::from_slice(&std::fs::read(candidate.join(MANIFEST_FILE))?)?;
    for entry in &mut manifest.artifacts {
        entry.inputs_sha256 = "0".repeat(64);
    }
    std::fs::write(
        candidate.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest)?,
    )?;
    let message = identity_error(DumpPair::new(
        VerifiedDump::open(&baseline)?,
        VerifiedDump::open(&candidate)?,
        &testdata,
    ))?;
    assert!(message.contains("disagree on the inputs"), "{message}");

    // A manifest swapped into the other dump no longer matches its bytes.
    write_dump(&candidate, br#"{"type":"object"}"#)?;
    std::fs::copy(baseline.join(MANIFEST_FILE), candidate.join(MANIFEST_FILE))?;
    assert!(identity_error(VerifiedDump::open(&candidate))?.contains("differs from its manifest"));
    Ok(())
}

#[test]
fn canonical_json_orders_members_by_key() {
    let mut forward = serde_json::Map::new();
    forward.insert("b".into(), json!(1));
    forward.insert("a".into(), json!({"d": 1, "c": 2}));
    let mut backward = serde_json::Map::new();
    backward.insert("a".into(), json!({"c": 2, "d": 1}));
    backward.insert("b".into(), json!(1));
    sim_assert_eq!(
        have: canonical_json(&Value::Object(forward)),
        want: canonical_json(&Value::Object(backward))
    );
}

#[test]
fn blobs_are_rehashed_on_every_read() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-blobs")?;
    let blobs = BlobStore::new(scratch.path().join("blobs"));
    let id = blobs.put(b"exact bytes")?;
    sim_assert_eq!(have: blobs.get(&id)?, want: b"exact bytes".to_vec());
    let path = scratch
        .path()
        .join("blobs")
        .join(id.get(..2).ok_or_eyre("short id")?)
        .join(&id);
    std::fs::write(&path, b"other bytes")?;
    assert!(identity_error(blobs.get(&id))?.contains("changed"));
    Ok(())
}

/// A template-lane witness of `overlay` over `defaults`.
fn template_witness(name: &str, overlay: Value, defaults: &Value) -> ScreenedWitness {
    let composed = test_util::helm_values::coalesce_tables(&overlay, defaults);
    ScreenedWitness {
        chart: CHART.into(),
        probe: ScreenedProbe {
            name: name.into(),
            overlay: overlay.clone(),
            document: None,
        },
        instance: Some(ProbeInstance::SparseOverride(overlay)),
        composed,
        kubernetes_version: "1.29.0".into(),
        baseline_accepts: true,
        candidate_accepts: true,
        screened: false,
        transition: false,
    }
}

fn chart_identity(defaults: &Value) -> ChartIdentity {
    ChartIdentity {
        chart_input_sha256: "a".repeat(64),
        recipe_sha256: "b".repeat(64),
        defaults: defaults.clone(),
    }
}

#[test]
fn witness_records_rebuild_their_exact_documents() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-records")?;
    let blobs = BlobStore::new(scratch.path().join("blobs"));
    let defaults = json!({"enabled": false, "ingress": {"enabled": false}});
    let witness = template_witness("enabled <- true", json!({"enabled": true}), &defaults);
    let record = witness_record(&witness, &chart_identity(&defaults), &blobs, "attempt:test")?;
    record.verify(&blobs)?;
    let rebuilt = replay_witness(&record, &blobs)?;
    sim_assert_eq!(have: &rebuilt.composed, want: &witness.composed);
    sim_assert_eq!(have: &rebuilt.probe.overlay, want: &witness.probe.overlay);

    let document = ScreenedWitness {
        probe: ScreenedProbe {
            name: "document=LintRaw".into(),
            overlay: json!({"enabled": true}),
            document: Some(ScreenedDocument {
                kind: AcceptanceDocument::LintRaw,
                lint_control: true,
                values: json!({"enabled": true}),
                defaults: None,
                kubernetes_version: "1.29.0".into(),
                origins: vec!["dependency:root".into()],
                values_file: b"enabled: true\n".to_vec(),
                control_overlays: vec![json!({})],
            }),
        },
        instance: None,
        composed: json!({"enabled": true}),
        ..witness.clone()
    };
    let record = witness_record(
        &document,
        &chart_identity(&defaults),
        &blobs,
        "attempt:test",
    )?;
    let rebuilt = replay_witness(&record, &blobs)?;
    let rebuilt_document = rebuilt.probe.document.ok_or_eyre("document lane lost")?;
    sim_assert_eq!(have: rebuilt_document.values_file, want: b"enabled: true\n".to_vec());
    sim_assert_eq!(have: rebuilt_document.origins, want: vec!["dependency:root".to_string()]);

    // The identity excludes the name: the same experiment found twice is one witness.
    let renamed = template_witness("another name", json!({"enabled": true}), &defaults);
    let renamed = witness_record(
        &renamed,
        &chart_identity(&defaults),
        &blobs,
        "attempt:other",
    )?;
    let first = witness_record(&witness, &chart_identity(&defaults), &blobs, "attempt:test")?;
    sim_assert_eq!(have: renamed.id, want: first.id);
    Ok(())
}

fn records(blobs: &BlobStore, overlays: &[Value]) -> eyre::Result<Vec<WitnessRecord>> {
    let defaults = json!({"enabled": false});
    overlays
        .iter()
        .map(|overlay| {
            Ok(witness_record(
                &template_witness("probe", overlay.clone(), &defaults),
                &chart_identity(&defaults),
                blobs,
                "attempt:test",
            )?)
        })
        .collect()
}

fn attempt(reason: &str) -> Attempt {
    Attempt {
        run_id: "run".into(),
        capture_hash: None,
        reason: reason.into(),
    }
}

#[test]
fn the_universe_only_grows_along_its_chain() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-universe")?;
    let universe = Universe::new(scratch.path().join("universe"));
    let mut state = universe.load()?;
    let lock = universe.lock()?;
    assert!(identity_error(universe.lock())?.contains("locked"));
    let first = records(
        universe.blobs(),
        &[json!({"enabled": true}), json!({"enabled": null})],
    )?;
    let first_batch = universe
        .append(&lock, &mut state, first, attempt("genesis"))?
        .ok_or_eyre("no batch")?;
    let second = records(universe.blobs(), &[json!({"enabled": "x"})])?;
    universe.append(&lock, &mut state, second, attempt("discovery"))?;
    drop(lock);
    let loaded = universe.load()?;
    sim_assert_eq!(have: loaded.witnesses.len(), want: 3);
    sim_assert_eq!(have: &loaded.head, want: &state.head);
    sim_assert_eq!(have: loaded.batch_hashes().len(), want: 2);

    // A stale writer cannot append after the head moved.
    let mut outdated = UniverseState {
        witnesses: loaded.witnesses.clone(),
        journal: loaded.journal.clone(),
        head: None,
    };
    let lock = universe.lock()?;
    assert!(
        identity_error(universe.append(
            &lock,
            &mut outdated,
            records(universe.blobs(), &[json!({"enabled": 1})])?,
            attempt("discovery")
        ))?
        .contains("moved")
    );
    drop(lock);

    // Rolling the head back, dropping a batch or editing one breaks the chain.
    let head = scratch.path().join("universe/head");
    let current = std::fs::read(&head)?;
    let journal = std::fs::read_to_string(scratch.path().join("universe/journal.jsonl"))?;
    let first_record = journal.lines().next().ok_or_eyre("empty journal")?;
    let rolled_back = helm_schema_test_support::source_digest::sha256_hex(first_record.as_bytes());
    std::fs::write(&head, rolled_back)?;
    assert!(identity_error(universe.load())?.contains("head"));
    std::fs::write(&head, current)?;
    let batch = scratch
        .path()
        .join(format!("universe/batches/{first_batch}.jsonl"));
    let bytes = std::fs::read(&batch)?;
    std::fs::remove_file(&batch)?;
    assert!(universe.load().is_err());
    let mut edited = bytes.clone();
    edited.extend_from_slice(b"\n");
    std::fs::write(&batch, &edited)?;
    assert!(identity_error(universe.load())?.contains("changed"));
    std::fs::write(&batch, &bytes)?;
    universe.load()?;
    Ok(())
}

fn receipt() -> Receipt {
    Receipt {
        format_version: 1,
        tool_version_and_hash: "tool".into(),
        run_id: "run".into(),
        history_id: "history".into(),
        kind: ReceiptKind::Genesis,
        checkpoint_ordinal: 0,
        checkpoint_commit: None,
        capture_hash: None,
        baseline_capture_hash: "main".into(),
        suite_hash: "suite".into(),
        controls_hash: None,
        context_roster_hash: None,
        provider_hash: "provider".into(),
        helm_identity: "helm".into(),
        predecessor_receipt_sha256: None,
        seed_ledger_sha256: None,
        universe_parent_hash: None,
        universe_hash: "universe".into(),
        universe_journal_head_sha256: None,
        witness_batch_hashes: Vec::new(),
        artifact_manifest_hashes: Vec::new(),
        coverage_hash: "coverage".into(),
        proof_index_hash: "proof".into(),
        regression_set_hash: "regressions".into(),
        earned_closure_set_hash: "closures".into(),
        full_outcome_table_hash: "outcomes".into(),
        control_runner_manifest_hash: None,
        observation_results_hash: None,
        gate_mode: "construction".into(),
        landability: "NON-LANDABLE".into(),
        exit_code: 0,
        seal_sha256: String::new(),
    }
}

#[test]
fn receipts_are_sealed_and_never_replaced() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-receipt")?;
    let path = scratch.path().join("receipt.json");
    assert!(identity_error(receipt().write(&path))?.contains("not sealed"));
    let sealed = receipt().seal()?;
    sealed.write(&path)?;
    sim_assert_eq!(have: Receipt::read(&path)?, want: sealed.clone());
    assert!(identity_error(sealed.write(&path))?.contains("already exists"));
    let mut tampered: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    tampered["exit_code"] = json!(1);
    let other = scratch.path().join("tampered.json");
    std::fs::write(&other, serde_json::to_vec(&tampered)?)?;
    assert!(identity_error(Receipt::read(&other))?.contains("seal"));
    Ok(())
}

#[test]
fn a_plan_declares_ten_checkpoints_seeded_at_two() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vad-plan")?;
    let path = scratch.path().join("plan.json");
    let plan = |ordinals: Vec<u32>, seed: u32| {
        json!({
            "format_version": 1, "history_id": "r3b", "main_commit": "main",
            "checkpoints": ordinals.into_iter().map(|ordinal| json!({"ordinal": ordinal, "subject": "s", "kind": "semantic"})).collect::<Vec<_>>(),
            "seed_ordinal": seed, "reviewer": "reviewer",
        })
    };
    std::fs::write(&path, serde_json::to_vec(&plan((1..=10).collect(), 2))?)?;
    sim_assert_eq!(have: HistoryPlan::read(&path)?.checkpoints.len(), want: 10);
    for (ordinals, seed) in [
        ((1..=9).collect(), 2),
        ([1, 2, 4, 5, 6, 7, 8, 9, 10, 11].to_vec(), 2),
        ((1..=10).collect(), 3),
    ] {
        std::fs::write(&path, serde_json::to_vec(&plan(ordinals, seed))?)?;
        assert!(matches!(
            HistoryPlan::read(&path),
            Err(DiffError::Request(_))
        ));
    }
    Ok(())
}

fn verdict_witness(baseline: bool, candidate: bool, transition: bool) -> ScreenedWitness {
    ScreenedWitness {
        baseline_accepts: baseline,
        candidate_accepts: candidate,
        screened: transition,
        transition,
        ..template_witness("probe", json!({}), &json!({}))
    }
}

fn adjudicated(outcome: Result<HelmFlipVerdict, (FailureKind, &str)>) -> AdjudicationRecord {
    AdjudicationRecord {
        case: format!("{CHART}: probe"),
        outcome: outcome
            .map(|verdict| (verdict, PathBuf::from("/evidence")))
            .map_err(|(kind, message)| AdjudicationFailure {
                kind,
                message: message.into(),
            }),
    }
}

/// One classification case: the witness's verdicts, its adjudication and
/// the expected class, oracle answer and direction.
struct Case {
    verdicts: (bool, bool, bool),
    adjudication: Option<Result<HelmFlipVerdict, (FailureKind, &'static str)>>,
    class: Class,
    oracle: OracleAnswer,
    direction: Option<&'static str>,
}

fn check(cases: Vec<Case>) {
    for Case {
        verdicts: (baseline, candidate, transition),
        adjudication,
        class,
        oracle,
        direction,
    } in cases
    {
        let witness = verdict_witness(baseline, candidate, transition);
        let record = adjudication.map(adjudicated);
        let outcome = classify("id", &witness, record.as_ref());
        sim_assert_eq!(
            have: (outcome.class, outcome.oracle, outcome.direction.as_deref()),
            want: (class, oracle, direction)
        );
    }
}

fn case(
    verdicts: (bool, bool, bool),
    adjudication: Option<Result<HelmFlipVerdict, (FailureKind, &'static str)>>,
    class: Class,
    oracle: OracleAnswer,
    direction: Option<&'static str>,
) -> Case {
    Case {
        verdicts,
        adjudication,
        class,
        oracle,
        direction,
    }
}

#[test]
fn verdicts_classify_against_the_pinned_oracle() {
    check(vec![
        case(
            (true, true, false),
            None,
            Class::Identical,
            OracleAnswer::NotAsked,
            None,
        ),
        case(
            (true, false, true),
            None,
            Class::Unadjudicated,
            OracleAnswer::NotAsked,
            None,
        ),
        case(
            (true, false, true),
            Some(Ok(HelmFlipVerdict::TighteningMatchedHelmAbort)),
            Class::Closure,
            OracleAnswer::Rejects,
            Some("rejects-oracle-rejection"),
        ),
        case(
            (false, true, true),
            Some(Ok(HelmFlipVerdict::CandidateAcceptsKubernetesRejects)),
            Class::Regression,
            OracleAnswer::Rejects,
            Some("accepts-oracle-rejection"),
        ),
        case(
            (false, true, true),
            Some(Ok(HelmFlipVerdict::LooseningMatchedKubernetesValidation)),
            Class::Closure,
            OracleAnswer::Accepts,
            Some("accepts-oracle-acceptance"),
        ),
        case(
            (true, false, true),
            Some(Ok(HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy)),
            Class::Policy,
            OracleAnswer::Policy,
            None,
        ),
        case(
            (false, true, true),
            Some(Ok(HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
                "crd".into(),
            ]))),
            Class::Unadjudicated,
            OracleAnswer::Undecided,
            None,
        ),
        case(
            (true, false, true),
            Some(Ok(HelmFlipVerdict::Collapsed)),
            Class::Identical,
            OracleAnswer::NotAsked,
            None,
        ),
        // Helm's coalesced document disagreeing with the screened one decides nothing.
        case(
            (true, true, true),
            Some(Ok(HelmFlipVerdict::TighteningMatchedHelmAbort)),
            Class::Unadjudicated,
            OracleAnswer::Rejects,
            None,
        ),
    ]);
}

#[test]
fn adjudication_failures_classify_by_their_kind() {
    let unattributed = Some(Err((FailureKind::UnattributedRejection, "renders cleanly")));
    check(vec![
        case(
            (true, false, true),
            unattributed.clone(),
            Class::Regression,
            OracleAnswer::Accepts,
            Some("rejects-oracle-acceptance"),
        ),
        case(
            (false, false, false),
            unattributed.clone(),
            Class::Inherited,
            OracleAnswer::Accepts,
            None,
        ),
        case(
            (false, false, true),
            unattributed,
            Class::Regression,
            OracleAnswer::Accepts,
            Some("changed-failure"),
        ),
        case(
            (false, false, false),
            Some(Err((FailureKind::Undecided, "inherited lint errors"))),
            Class::Identical,
            OracleAnswer::Undecided,
            None,
        ),
        case(
            (true, false, true),
            Some(Err((FailureKind::Infrastructure, "helm crashed"))),
            Class::Infrastructure,
            OracleAnswer::Undecided,
            None,
        ),
    ]);
}

fn outcome(witness: &str, class: Class, direction: Option<&str>) -> WitnessOutcome {
    WitnessOutcome {
        witness: witness.into(),
        case: format!("{CHART}: {witness}"),
        baseline_accepts: true,
        candidate_accepts: true,
        transition: false,
        oracle: OracleAnswer::NotAsked,
        class,
        direction: direction.map(str::to_string),
        detail: None,
    }
}

fn row(witness: &str, direction: &str) -> LedgerRow {
    LedgerRow {
        witness: witness.into(),
        direction: direction.into(),
        owner: "owner".into(),
        cause: "cause".into(),
        repair_checkpoint: 5,
    }
}

fn key(witness: &str, direction: &str) -> RegressionKey {
    RegressionKey {
        witness: witness.into(),
        direction: direction.into(),
    }
}

/// The construction policy's semantic problems for `ordinal` over `table`.
fn problems(
    ordinal: u32,
    semantic: bool,
    table: &OutcomeTable,
    prior: &PriorSets,
    ledger: &[LedgerRow],
    seed: Option<&SeedReview>,
) -> Result<Vec<String>, DiffError> {
    Ok(construction_policy(&ConstructionRequest {
        ordinal,
        semantic,
        table,
        prior,
        ledger,
        seed,
        seed_ordinal: 2,
        reviewer: "reviewer",
    })?
    .semantic)
}

#[test]
fn checkpoint_one_must_equal_main_and_only_checkpoint_two_seeds() -> eyre::Result<()> {
    let genesis = PriorSets::default();
    let identical = OutcomeTable::new(vec![
        outcome("a", Class::Identical, None),
        outcome("b", Class::Inherited, None),
    ]);
    sim_assert_eq!(have: problems(1, false, &identical, &genesis, &[], None)?, want: Vec::<String>::new());
    let closed = OutcomeTable::new(vec![outcome("a", Class::Closure, Some("x"))]);
    sim_assert_eq!(have: problems(1, false, &closed, &genesis, &[], None)?.len(), want: 1);

    let seed = SeedReview {
        format_version: 1,
        reviewer: "reviewer".into(),
        rows: vec![row("r", "accepts-oracle-rejection")],
    };
    // C01 cannot seed, and C02 cannot run without its review.
    assert!(matches!(
        problems(1, false, &identical, &genesis, &[], Some(&seed)),
        Err(DiffError::Request(_))
    ));
    let c01 = PriorSets {
        ordinal: 1,
        ..PriorSets::default()
    };
    let seeded = OutcomeTable::new(vec![outcome(
        "r",
        Class::Regression,
        Some("accepts-oracle-rejection"),
    )]);
    assert!(matches!(
        problems(2, true, &seeded, &c01, &[], None),
        Err(DiffError::Request(_))
    ));
    sim_assert_eq!(have: problems(2, true, &seeded, &c01, &seed.rows, Some(&seed))?, want: Vec::<String>::new());

    // The seed is the exact measured set, reviewed by the plan's reviewer.
    let unreviewed = OutcomeTable::new(vec![
        outcome("r", Class::Regression, Some("accepts-oracle-rejection")),
        outcome("s", Class::Regression, Some("rejects-oracle-acceptance")),
    ]);
    sim_assert_eq!(
        have: problems(2, true, &unreviewed, &c01, &seed.rows, Some(&seed))?,
        want: vec![
            "unreviewed seed regression s (rejects-oracle-acceptance)".to_string(),
            "regression s (rejects-oracle-acceptance) is not in the ledger".to_string(),
        ]
    );
    let stranger = SeedReview {
        reviewer: "someone".into(),
        ..seed.clone()
    };
    sim_assert_eq!(
        have: problems(2, true, &seeded, &c01, &seed.rows, Some(&stranger))?,
        want: vec!["the seed review is not a version-1 review by the plan's reviewer".to_string()]
    );

    // A second seed at a later checkpoint is refused.
    let c02 = PriorSets {
        ordinal: 2,
        regressions: BTreeSet::from([key("r", "accepts-oracle-rejection")]),
        closures: BTreeSet::new(),
    };
    assert!(matches!(
        problems(3, true, &seeded, &c02, &seed.rows, Some(&seed)),
        Err(DiffError::Request(_))
    ));
    Ok(())
}

#[test]
fn later_checkpoints_only_shrink_the_regression_set() -> eyre::Result<()> {
    let c02 = PriorSets {
        ordinal: 2,
        regressions: BTreeSet::from([key("r", "accepts-oracle-rejection")]),
        closures: BTreeSet::from(["c".to_string()]),
    };
    let ledger = [row("r", "accepts-oracle-rejection")];
    let kept = OutcomeTable::new(vec![
        outcome("r", Class::Regression, Some("accepts-oracle-rejection")),
        outcome("c", Class::Closure, Some("x")),
    ]);
    // A representation checkpoint may keep the set; a semantic one must progress.
    sim_assert_eq!(have: problems(3, false, &kept, &c02, &ledger, None)?, want: Vec::<String>::new());
    sim_assert_eq!(have: problems(3, true, &kept, &c02, &ledger, None)?.len(), want: 1);

    // Equal counts with exchanged ids fail, as does a changed failure direction.
    let swapped = OutcomeTable::new(vec![
        outcome("s", Class::Regression, Some("accepts-oracle-rejection")),
        outcome("c", Class::Closure, Some("x")),
    ]);
    let swapped_problems = problems(
        3,
        false,
        &swapped,
        &c02,
        &[row("s", "accepts-oracle-rejection")],
        None,
    )?;
    assert!(
        swapped_problems
            .iter()
            .any(|problem| problem.starts_with("new regression s")),
        "{swapped_problems:?}"
    );
    let redirected = OutcomeTable::new(vec![
        outcome("r", Class::Regression, Some("changed-failure")),
        outcome("c", Class::Closure, Some("x")),
    ]);
    sim_assert_eq!(
        have: problems(3, false, &redirected, &c02, &[row("r", "changed-failure")], None)?,
        want: vec!["new regression r (changed-failure)".to_string()]
    );

    // A lost closure fails; a removed regression shrinks the set.
    let lost = OutcomeTable::new(vec![
        outcome("r", Class::Regression, Some("accepts-oracle-rejection")),
        outcome("c", Class::Identical, None),
    ]);
    assert!(
        problems(3, false, &lost, &c02, &ledger, None)?.contains(&"lost closure c".to_string())
    );
    let repaired = OutcomeTable::new(vec![
        outcome("r", Class::Identical, None),
        outcome("c", Class::Closure, Some("x")),
    ]);
    sim_assert_eq!(have: problems(3, true, &repaired, &c02, &[], None)?, want: Vec::<String>::new());

    // The ledger names the set exactly: stale rows and additions fail.
    sim_assert_eq!(
        have: problems(3, true, &repaired, &c02, &ledger, None)?,
        want: vec!["stale ledger row r (accepts-oracle-rejection)".to_string()]
    );
    sim_assert_eq!(
        have: problems(3, false, &kept, &c02, &[], None)?,
        want: vec!["regression r (accepts-oracle-rejection) is not in the ledger".to_string()]
    );

    // Ordinals cannot skip, and unadjudicated outcomes fail.
    assert!(matches!(
        problems(4, false, &kept, &c02, &ledger, None),
        Err(DiffError::Identity(_))
    ));
    let unadjudicated = OutcomeTable::new(vec![
        outcome("r", Class::Regression, Some("accepts-oracle-rejection")),
        outcome("c", Class::Closure, Some("x")),
        outcome("u", Class::Unadjudicated, None),
    ]);
    sim_assert_eq!(have: problems(3, false, &unadjudicated, &c02, &ledger, None)?.len(), want: 1);
    Ok(())
}

/// An unusable Kubernetes control (resources no pinned schema decides) is
/// an undecided oracle, never an infrastructure failure, through context.
#[test]
fn an_unusable_kubernetes_control_is_undecided() -> eyre::Result<()> {
    use color_eyre::eyre::WrapErr as _;
    use helm_schema_test_support::helm::adjudication::UnusableControl;
    let unusable: eyre::Result<()> = Err(UnusableControl(
        "unusable defaults Kubernetes control: uncertainty: document 9".into(),
    )
    .into());
    let Err(error) = unusable.wrap_err("case=document=LintRaw") else {
        eyre::bail!("the control error vanished");
    };
    sim_assert_eq!(have: FailureKind::of(&error), want: FailureKind::Undecided);
    sim_assert_eq!(have: FailureKind::of(&eyre::eyre!("helm exited")), want: FailureKind::Infrastructure);
    Ok(())
}
