//! Version-1 observation controls: registry verification, the runner
//! protocol's envelope checks, scoring, capture retention and historical
//! backfill with retained runners.
//!
//! This binary doubles as a synthetic protocol runner
//! ([`tests::value_algebra_controls::protocol_v1`]): a wrapper script sets
//! its answers, so two scripts are two versioned runner builds. Synthetic
//! runners are protocol fixtures, never admissible checkpoint evidence.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::manifest::BuildProvenance;
use helm_schema_test_support::source_digest::sha256_hex;
use helm_schema_test_support::value_algebra_diff::DiffError;
use helm_schema_test_support::value_algebra_diff::capture::{DumpPair, VerifiedDump};
use helm_schema_test_support::value_algebra_diff::commands::{
    self, Capture, CaptureManifest, ControlsRequest, backfill_observation, observation_outcomes,
    open_capture, record_controls,
};
use helm_schema_test_support::value_algebra_diff::compare::{Class, OutcomeTable, RegressionKey};
use helm_schema_test_support::value_algebra_diff::controls::{
    self, AbortPhase, ChartTree, ControlCase, ControlInput, ControlRegistry, ControlResult,
    EngineMode, ExactCoverage, ExecutionEvidence, GroundCoverage, Grounding, Observation,
    ProjectionStatus, Query, RegistryBuilder, Role, Runner, Score, Selection, Termination,
    UnknownReason, UnsupportedReason,
};
use helm_schema_test_support::value_algebra_diff::history::FORMAT_VERSION;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

mod common;

use common::write_dump;

/// The synthetic runner.
mod tests {
    pub mod value_algebra_controls {
        use std::collections::BTreeMap;

        use color_eyre::eyre;
        use helm_schema_test_support::source_digest::sha256_hex;
        use helm_schema_test_support::value_algebra_diff::controls::{
            self, Capabilities, ControlResult, EngineMode, Query,
        };

        /// Answers from `SYNTHETIC_CONTROL_ANSWERS` (query pointer, or
        /// `termination`, to result) as the build `SYNTHETIC_RUNNER_SCRIPT`
        /// hashes to; `SYNTHETIC_CONTROL_MODE` selects a misbehavior.
        #[test]
        #[ignore = "a synthetic control runner; the control tests invoke it through a script"]
        fn protocol_v1() -> eyre::Result<()> {
            let script = std::env::var("SYNTHETIC_RUNNER_SCRIPT")?;
            let mode = std::env::var("SYNTHETIC_CONTROL_MODE")?;
            match mode.as_str() {
                "fail" => eyre::bail!("the synthetic runner fails as asked"),
                "sleep" => std::thread::sleep(std::time::Duration::from_secs(30)),
                "silent" => return Ok(()),
                _ => {}
            }
            let build = if mode == "wrong-build" {
                "0".repeat(64)
            } else {
                sha256_hex(&std::fs::read(&script)?)
            };
            let answers: BTreeMap<String, ControlResult> =
                serde_json::from_str(&std::env::var("SYNTHETIC_CONTROL_ANSWERS")?)?;
            let capabilities = Capabilities {
                queries: controls::QUERY_KINDS
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                modes: vec![EngineMode::Render],
                cache_modes: vec![controls::CacheMode::Default],
            };
            controls::serve(build, &capabilities, |_, _, query| {
                let key = match query {
                    Query::OutputV1 { pointer, .. } | Query::AliasV1 { pointer, .. } => {
                        pointer.clone()
                    }
                    Query::TerminationV1 { .. } => "termination".to_string(),
                };
                answers
                    .get(&key)
                    .cloned()
                    .unwrap_or(ControlResult::InfrastructureFailure {
                        code: controls::InfrastructureCode::BadRequest,
                        diagnostic: format!("no synthetic answer for {key}"),
                    })
            })?;
            Ok(())
        }
    }
}

const CHART_YAML: (&str, &str) = (
    "Chart.yaml",
    indoc! {"
        apiVersion: v2
        name: c
        version: 0.1.0
    "},
);

/// The snapshot template of the required regression test.
const SNAPSHOT: &str = indoc! {r#"
    {{- $d := dict "x" "old" -}}
    {{- $saved := toJson $d -}}
    {{- $_ := set $d "x" "new" -}}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: snapshot
    data:
      snapshot: {{ $saved | quote }}
"#};

/// The write-visible alias template.
const ALIAS: &str = indoc! {r#"
    {{- $a := dict "x" "old" -}}
    {{- $b := $a -}}
    {{- $_ := set $a "x" "new" -}}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: alias
    data:
      visible: {{ eq (get $b "x") "new" }}
"#};

const LITERAL: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    data:
      greeting: "hello world"
"#};

fn output(text: &str) -> Observation {
    Observation::Output {
        selection: controls::output_value(text.as_bytes()),
    }
}

fn alias(value: bool) -> Observation {
    Observation::Alias {
        selection: Selection::Value { value },
    }
}

fn exact(observation: Observation) -> ControlResult {
    ControlResult::Exact {
        alternatives: vec![observation],
        coverage: ExactCoverage::Complete,
    }
}

fn unknown() -> ControlResult {
    ControlResult::AbstractUnknown {
        reason: UnknownReason::RepresentationGap,
        evidence: ExecutionEvidence {
            evaluator: "synthetic".into(),
            result_sha256: "0".repeat(64),
            gaps: vec!["synthetic gap".into()],
            projection: ProjectionStatus::ReadOnly,
        },
    }
}

fn output_query(pointer: &str) -> Query {
    Query::OutputV1 {
        file: "templates/cm.yaml".into(),
        document: 0,
        pointer: pointer.into(),
    }
}

/// A scored render case grounded completely in `ground`.
fn scored(id: &str, input: ControlInput, query: Query, ground: Observation) -> ControlCase {
    ControlCase {
        format_version: controls::PROTOCOL_VERSION,
        id: id.into(),
        role: Role::Scored,
        input,
        query: Some(query),
        claim: None,
        grounding: Grounding {
            evidence_sha256: sha256_hex(
                format!("synthetic pinned-Helm 4.2.3 evidence for {id}").as_bytes(),
            ),
            coverage: GroundCoverage::Complete,
            alternatives: vec![ground],
        },
    }
}

/// The literal control.
fn literal_case(builder: &mut RegistryBuilder) -> eyre::Result<ControlCase> {
    let input = builder.render_input(
        &[CHART_YAML, ("templates/cm.yaml", LITERAL)],
        "{}\n",
        &json!({}),
    )?;
    Ok(scored(
        "literal-output",
        input,
        output_query("/data/greeting"),
        output("hello world"),
    ))
}

/// The snapshot control; Helm 4.2.3 renders `{"x":"old"}`.
fn snapshot_case(builder: &mut RegistryBuilder) -> eyre::Result<ControlCase> {
    let input = builder.render_input(
        &[CHART_YAML, ("templates/cm.yaml", SNAPSHOT)],
        "{}\n",
        &json!({}),
    )?;
    Ok(scored(
        "serialized-map-snapshot",
        input,
        output_query("/data/snapshot"),
        output(r#"{"x":"old"}"#),
    ))
}

/// The alias control; Helm 4.2.3 renders `true`.
fn alias_case(builder: &mut RegistryBuilder) -> eyre::Result<ControlCase> {
    let input = builder.render_input(
        &[CHART_YAML, ("templates/cm.yaml", ALIAS)],
        "{}\n",
        &json!({}),
    )?;
    let query = Query::AliasV1 {
        file: "templates/cm.yaml".into(),
        document: 0,
        pointer: "/data/visible".into(),
        relation: controls::AliasRelation::WriteVisible,
    };
    Ok(scored("write-visible-alias", input, query, alias(true)))
}

/// A registry of `cases` at the new directory `root`.
fn registry(
    root: &Path,
    cases: &[fn(&mut RegistryBuilder) -> eyre::Result<ControlCase>],
) -> eyre::Result<ControlRegistry> {
    let mut builder = RegistryBuilder::new(root)?;
    for case in cases {
        let case = case(&mut builder)?;
        builder.case(&case)?;
    }
    Ok(builder.finish()?)
}

/// A synthetic runner script `dir/name` answering `answers` by pointer.
fn runner(
    dir: &Path,
    name: &str,
    mode: &str,
    answers: &[(&str, ControlResult)],
) -> eyre::Result<PathBuf> {
    let answers = answers
        .iter()
        .map(|(key, result)| ((*key).to_string(), result.clone()))
        .collect::<BTreeMap<_, _>>();
    let answers = serde_json::to_string(&answers)?;
    eyre::ensure!(
        !answers.contains('\''),
        "answers must not contain a single quote"
    );
    let exe = std::env::current_exe()?;
    let script = format!(
        "#!/bin/sh\nexport SYNTHETIC_RUNNER_SCRIPT=\"$0\"\nexport SYNTHETIC_CONTROL_MODE='{mode}'\n\
         export SYNTHETIC_CONTROL_ANSWERS='{answers}'\nexec '{}' \"$@\"\n",
        exe.display()
    );
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    std::fs::write(&path, script)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// A synthetic capture at the new directory `root`: a one-chart dump of
/// `schema` and the controls `registry` evaluated by `runner`.
fn capture(root: &Path, schema: &[u8], registry: &Path, runner: &Path) -> eyre::Result<Capture> {
    let dump_dir = root.join("dump");
    write_dump(&dump_dir, schema)?;
    let dump = VerifiedDump::open(&dump_dir)?;
    std::fs::create_dir_all(root.join("bin"))?;
    std::fs::write(root.join("bin/corpus_generation"), b"synthetic producer")?;
    let controls = record_controls(
        root,
        &ControlsRequest {
            registry: registry.to_path_buf(),
            runner: runner.to_path_buf(),
        },
        &BuildProvenance::compiled(),
    )?;
    let manifest = CaptureManifest {
        format_version: FORMAT_VERSION,
        scope: "schema-dumps-v1".into(),
        commit: "synthetic".into(),
        tree: "synthetic".into(),
        checkout: "/nonexistent/checkout".into(),
        status: String::new(),
        synthetic: true,
        source_sha256: dump.build().source_sha256.clone(),
        producer_sha256: sha256_hex(b"synthetic producer"),
        producer_build: dump.build().clone(),
        dump_manifest_sha256: dump.manifest_sha256().to_string(),
        helmsweep_sha256: "synthetic".into(),
        helmsweep_version: "synthetic".into(),
        kubernetes_release: "synthetic".into(),
        suite_sha256: commands::suite_sha256(&dump)?,
        contexts_sha256: None,
        controls: Some(controls),
    };
    std::fs::write(
        root.join("capture.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(open_capture(root)?)
}

/// Every file under `root` with its digest.
fn tree_digest(root: &Path) -> eyre::Result<BTreeMap<PathBuf, String>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(path.clone(), sha256_hex(&std::fs::read(&path)?));
            }
        }
    }
    Ok(files)
}

fn identity_message<T: std::fmt::Debug>(result: Result<T, DiffError>) -> eyre::Result<String> {
    match result {
        Err(DiffError::Identity(message)) => Ok(message),
        other => eyre::bail!("expected an identity error, got {other:?}"),
    }
}

fn infrastructure_message<T: std::fmt::Debug>(
    result: Result<T, DiffError>,
) -> eyre::Result<String> {
    match result {
        Err(DiffError::Infrastructure(message)) => Ok(message),
        other => eyre::bail!("expected an infrastructure error, got {other:?}"),
    }
}

fn request_message<T: std::fmt::Debug>(result: Result<T, DiffError>) -> eyre::Result<String> {
    match result {
        Err(DiffError::Request(message)) => Ok(message),
        other => eyre::bail!("expected a request error, got {other:?}"),
    }
}

#[test]
fn a_registry_binds_its_cases_and_blobs() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-registry")?;
    let root = scratch.path().join("registry");
    let opened = registry(&root, &[literal_case])?;
    sim_assert_eq!(have: opened.cases.keys().collect::<Vec<_>>(), want: vec!["literal-output"]);

    // A stray blob, a changed blob and a changed case are refused.
    std::fs::write(root.join("blobs").join("0".repeat(64)), b"stray")?;
    let message = identity_message(ControlRegistry::open(&root))?;
    assert!(
        message.contains("differ from the manifest's blob index"),
        "{message}"
    );
    std::fs::remove_file(root.join("blobs").join("0".repeat(64)))?;
    let case = opened.cases.get("literal-output").ok_or_eyre("case")?;
    let blob = root.join("blobs").join(&case.input.original_values_sha256);
    std::fs::write(&blob, b"{changed: true}\n")?;
    let message = identity_message(ControlRegistry::open(&root))?;
    assert!(message.contains("content mismatch"), "{message}");
    std::fs::write(&blob, b"{}\n")?;
    ControlRegistry::open(&root)?;
    let case_file = root.join("cases/literal-output.json");
    let mut bytes = std::fs::read(&case_file)?;
    bytes.push(b'\n');
    std::fs::write(&case_file, bytes)?;
    let message = identity_message(ControlRegistry::open(&root))?;
    assert!(message.contains("case bytes mismatch"), "{message}");

    // Chart tree names are normalized and relative.
    for name in [
        "../escape.yaml",
        "/abs.yaml",
        "templates/./x.yaml",
        "templates/",
    ] {
        let tree = json!({"format_version": 1, "files": {name: "0".repeat(64)}});
        request_message(ChartTree::parse(&serde_json::to_vec(&tree)?))?;
    }
    Ok(())
}

#[test]
fn case_shapes_and_the_supported_domain_are_checked() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-cases")?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let base = literal_case(&mut builder)?;
    base.validate()?;
    let mut lint = base.clone();
    lint.input.mode = EngineMode::Lint;
    let mut second_document = base.clone();
    second_document.query = Some(Query::OutputV1 {
        file: "templates/cm.yaml".into(),
        document: 1,
        pointer: "/data/greeting".into(),
    });
    let mut with_claim = base.clone();
    with_claim.claim = Some("a claim".into());
    let mut grounding_with_query = base.clone();
    grounding_with_query.role = Role::HelmGroundingOnly;
    grounding_with_query.claim = Some("raw text".into());
    let mut other_answer = base.clone();
    other_answer.grounding.alternatives = vec![Observation::Termination {
        outcome: Termination::Normal,
    }];
    let mut unsorted = base.clone();
    unsorted.grounding.alternatives = vec![output("b"), output("a")];
    let mut sampled_empty = base.clone();
    sampled_empty.grounding.alternatives = Vec::new();
    for (label, case) in [
        ("lint", lint),
        ("document 1", second_document),
        ("claim", with_claim),
        ("grounding with query", grounding_with_query),
        ("answers another query", other_answer),
        ("unsorted", unsorted),
        ("empty grounding", sampled_empty),
    ] {
        let message = request_message(case.validate())?;
        assert!(
            message.starts_with("literal-output: "),
            "{label}: {message}"
        );
    }
    let mut grounding_only = base;
    grounding_only.role = Role::HelmGroundingOnly;
    grounding_only.query = None;
    grounding_only.claim = Some("raw full-file bytes are outside version 1".into());
    grounding_only.validate()?;
    request_message(grounding_only.witness_id())?;
    Ok(())
}

#[test]
fn the_witness_is_the_input_and_query_identity() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-witness")?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let base = literal_case(&mut builder)?;
    let mut regrounded = base.clone();
    regrounded.grounding.evidence_sha256 = "1".repeat(64);
    sim_assert_eq!(have: regrounded.witness_id()?, want: base.witness_id()?);
    let mut requeried = base.clone();
    requeried.query = Some(output_query("/data/other"));
    sim_assert_eq!(have: requeried.witness_id()? == base.witness_id()?, want: false);
    let mut released = base.clone();
    released.input.release.namespace = "other".into();
    sim_assert_eq!(have: released.witness_id()? == base.witness_id()?, want: false);
    Ok(())
}

/// One registry, its literal case and a directory of synthetic runners.
struct Protocol {
    scratch: ScratchDir,
    registry: ControlRegistry,
    query: Value,
}

impl Protocol {
    fn new(label: &str) -> eyre::Result<Self> {
        let scratch = ScratchDir::new(label)?;
        let registry = registry(&scratch.path().join("registry"), &[literal_case])?;
        let case = registry.cases.get("literal-output").ok_or_eyre("case")?;
        let query = serde_json::to_value(case.scored_query().ok_or_eyre("query")?)?;
        Ok(Self {
            scratch,
            registry,
            query,
        })
    }

    fn case(&self) -> eyre::Result<&ControlCase> {
        self.registry.cases.get("literal-output").ok_or_eyre("case")
    }

    /// A synthetic runner in `mode` answering `answer` for the literal case.
    fn runner(&self, name: &str, mode: &str, answer: ControlResult) -> eyre::Result<Runner> {
        let path = runner(
            &self.scratch.path().join("bin"),
            name,
            mode,
            &[("/data/greeting", answer)],
        )?;
        Ok(Runner::new(&path)?)
    }

    /// `runner`'s answer to `query` over `blobs` in the run directory `run`.
    fn observe(
        &self,
        runner: &Runner,
        run: &str,
        blobs: &Path,
        query: &Value,
        timeout: Duration,
    ) -> eyre::Result<Result<controls::ControlResponseV1, DiffError>> {
        let run_dir = self.scratch.path().join("runs").join(run);
        Ok(runner.observe(&run_dir, blobs, &self.case()?.input, query, timeout))
    }

    /// `runner`'s answer to the literal query.
    fn literal(
        &self,
        runner: &Runner,
        run: &str,
    ) -> eyre::Result<Result<controls::ControlResponseV1, DiffError>> {
        self.observe(
            runner,
            run,
            &self.registry.root.join("blobs"),
            &self.query,
            Duration::from_secs(60),
        )
    }
}

#[test]
fn the_runner_protocol_answers_well_formed_requests() -> eyre::Result<()> {
    let protocol = Protocol::new("vac-protocol-good")?;
    let good = protocol.runner("good", "answer", exact(output("hello world")))?;
    let describe = good.describe(&protocol.scratch.path().join("runs/describe"))?;
    sim_assert_eq!(have: describe.runner_build_id, want: good.build_id.clone());
    sim_assert_eq!(have: describe.build, want: BuildProvenance::compiled());
    let response = protocol.literal(&good, "good")??;
    sim_assert_eq!(have: response.result, want: exact(output("hello world")));
    sim_assert_eq!(have: response.input_sha256, want: protocol.case()?.input.sha256()?);

    // An unknown variant is unsupported, never a guess or an error.
    let blobs = protocol.registry.root.join("blobs");
    let unknown_query = json!({"kind": "output_v2", "file": "templates/cm.yaml"});
    let timeout = Duration::from_secs(60);
    let response = protocol.observe(&good, "unknown", &blobs, &unknown_query, timeout)??;
    sim_assert_eq!(have: response.result, want: ControlResult::Unsupported {
        query_kind: "output_v2".into(),
        mode: EngineMode::Render,
        reason: UnsupportedReason::QueryNotImplemented,
    });

    // A run directory is never reused.
    identity_message(protocol.literal(&good, "good")?)?;
    Ok(())
}

#[test]
fn the_runner_protocol_refuses_broken_exchanges() -> eyre::Result<()> {
    let protocol = Protocol::new("vac-protocol-broken")?;
    let answer = exact(output("hello world"));
    let good = protocol.runner("good", "answer", answer.clone())?;
    let blobs = protocol.registry.root.join("blobs");
    let timeout = Duration::from_secs(60);

    // A malformed query and mismatched input blobs are infrastructure.
    let malformed = json!({"kind": "output_v1", "file": "templates/cm.yaml", "document": 0});
    let message = infrastructure_message(protocol.observe(
        &good,
        "malformed",
        &blobs,
        &malformed,
        timeout,
    )?)?;
    assert!(message.contains("BadRequest"), "{message}");
    let corrupt = protocol.scratch.path().join("corrupt-blobs");
    std::fs::create_dir_all(&corrupt)?;
    for entry in std::fs::read_dir(&blobs)? {
        let entry = entry?;
        std::fs::copy(entry.path(), corrupt.join(entry.file_name()))?;
    }
    let document = &protocol.case()?.input.coalesced_document_sha256;
    std::fs::write(corrupt.join(document), b"{\"x\":1}")?;
    let message = infrastructure_message(protocol.observe(
        &good,
        "corrupt",
        &corrupt,
        &protocol.query,
        timeout,
    )?)?;
    assert!(message.contains("InputMismatch"), "{message}");

    // Another build's answer, no response, a failing runner and a hung runner.
    let wrong = protocol.runner("wrong-build", "wrong-build", answer.clone())?;
    let message = identity_message(protocol.literal(&wrong, "wrong")?)?;
    assert!(message.contains("runner_mismatch"), "{message}");
    let silent = protocol.runner("silent", "silent", answer.clone())?;
    let message = infrastructure_message(protocol.literal(&silent, "silent")?)?;
    assert!(message.contains("no response"), "{message}");
    let failing = protocol.runner("fail", "fail", answer.clone())?;
    let message = infrastructure_message(protocol.literal(&failing, "fail")?)?;
    assert!(message.contains("runner exited with"), "{message}");
    let hung = protocol.runner("sleep", "sleep", answer)?;
    let short = Duration::from_secs(2);
    let message = infrastructure_message(protocol.observe(
        &hung,
        "sleep",
        &blobs,
        &protocol.query,
        short,
    )?)?;
    assert!(message.contains("timed out"), "{message}");

    // An exact answer must answer the query, sorted and unique.
    let termination = exact(Observation::Termination {
        outcome: Termination::Normal,
    });
    let crossed = protocol.runner("crossed", "answer", termination)?;
    let message = infrastructure_message(protocol.literal(&crossed, "crossed")?)?;
    assert!(message.contains("answers another query"), "{message}");
    let reversed = ControlResult::Exact {
        alternatives: vec![output("b"), output("a")],
        coverage: ExactCoverage::Complete,
    };
    let unsorted = protocol.runner("unsorted", "answer", reversed)?;
    let message = infrastructure_message(protocol.literal(&unsorted, "unsorted")?)?;
    assert!(message.contains("sorted and unique"), "{message}");
    Ok(())
}

#[test]
fn observations_score_against_complete_grounds_only() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-score")?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let case = snapshot_case(&mut builder)?;
    let (old, new) = (
        exact(output(r#"{"x":"old"}"#)),
        exact(output(r#"{"x":"new"}"#)),
    );
    let other = exact(output(r#"{"x":"other"}"#));
    let unsupported = ControlResult::Unsupported {
        query_kind: "output_v1".into(),
        mode: EngineMode::Render,
        reason: UnsupportedReason::PrivateHookUnavailable,
    };
    let class = |main: &ControlResult,
                 candidate: &ControlResult|
     -> eyre::Result<(Class, Option<String>)> {
        let outcome = controls::classify_observation(&case, main, candidate)?;
        Ok((outcome.class, outcome.direction))
    };
    let regression = |direction: &str| (Class::Regression, Some(direction.to_string()));
    sim_assert_eq!(have: class(&old, &old)?, want: (Class::Identical, None));
    sim_assert_eq!(have: class(&old, &new)?, want: regression("observation:correct-to-wrong"));
    sim_assert_eq!(have: class(&old, &unknown())?, want: regression("observation:correct-to-unknown"));
    sim_assert_eq!(have: class(&unknown(), &new)?, want: regression("observation:unknown-to-wrong"));
    sim_assert_eq!(have: class(&new, &other)?, want: regression("observation:changed-wrong"));
    sim_assert_eq!(have: class(&new, &new)?, want: (Class::Inherited, None));
    sim_assert_eq!(have: class(&unknown(), &unknown())?, want: (Class::Inherited, None));
    sim_assert_eq!(have: class(&new, &unknown())?, want: (Class::Inherited, None));
    sim_assert_eq!(have: class(&unknown(), &old)?, want: (Class::Closure, None));
    sim_assert_eq!(have: class(&new, &old)?, want: (Class::Closure, None));
    sim_assert_eq!(have: class(&old, &unsupported)?, want: (Class::Unadjudicated, None));
    sim_assert_eq!(have: class(&unsupported, &old)?, want: (Class::Unadjudicated, None));
    let failure = ControlResult::InfrastructureFailure {
        code: controls::InfrastructureCode::Timeout,
        diagnostic: "late".into(),
    };
    infrastructure_message(controls::classify_observation(&case, &old, &failure))?;

    // Sampled grounds never score correctness or earn a closure.
    let mut sampled = case.clone();
    sampled.grounding.coverage = GroundCoverage::Sampled;
    let sampled_class = |main: &ControlResult, candidate: &ControlResult| -> eyre::Result<Class> {
        Ok(controls::classify_observation(&sampled, main, candidate)?.class)
    };
    sim_assert_eq!(have: sampled_class(&unknown(), &old)?, want: Class::Unadjudicated);
    sim_assert_eq!(have: sampled_class(&old, &new)?, want: Class::Unadjudicated);
    sim_assert_eq!(have: sampled_class(&old, &old)?, want: Class::Identical);
    sim_assert_eq!(have: controls::score(&old, &sampled.grounding)?, want: Score::Ungrounded);
    Ok(())
}

/// A runner answering the literal control correctly, `snapshot` for the
/// snapshot control and `visible` for the alias control.
fn revision_runner(
    bin: &Path,
    name: &str,
    snapshot: &str,
    visible: ControlResult,
) -> eyre::Result<PathBuf> {
    let answers = [
        ("/data/greeting", exact(output("hello world"))),
        ("/data/snapshot", exact(output(snapshot))),
        ("/data/visible", visible),
    ];
    runner(bin, name, "answer", &answers)
}

/// Main and candidate captured with byte-identical schemas, before the
/// snapshot and alias controls exist: main answers them correctly, the
/// candidate wrongly.
fn captured_revisions(root: &Path) -> eyre::Result<(Capture, Capture)> {
    let bin = root.join("bin");
    let main_runner = revision_runner(&bin, "main-runner", r#"{"x":"old"}"#, exact(alias(true)))?;
    let candidate_runner = revision_runner(
        &bin,
        "candidate-runner",
        r#"{"x":"new"}"#,
        exact(alias(false)),
    )?;
    let first = registry(&root.join("registry-1"), &[literal_case])?;
    let schema = br#"{"type":"object"}"#;
    let main = capture(&root.join("main"), schema, &first.root, &main_runner)?;
    let candidate = capture(
        &root.join("candidate"),
        schema,
        &first.root,
        &candidate_runner,
    )?;
    let pair = DumpPair::new(
        VerifiedDump::open(&main.root.join("dump"))?,
        VerifiedDump::open(&candidate.root.join("dump"))?,
        &test_util::workspace_testdata(),
    )?;
    let identical = pair
        .artifacts()
        .iter()
        .all(|artifact| artifact.baseline_sha256 == artifact.candidate_sha256);
    sim_assert_eq!(have: identical, want: true);
    let captured = observation_outcomes(&main, &candidate)?;
    sim_assert_eq!(
        have: captured.iter().map(|outcome| (outcome.case.as_str(), outcome.class)).collect::<Vec<_>>(),
        want: vec![("literal-output", Class::Identical)]
    );
    Ok((main, candidate))
}

/// The required regression: byte-identical schemas, a later-discovered
/// snapshot control, and per-revision answers from retained runners.
#[test]
fn non_schema_output_regression_replays_with_identical_schema() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-replay")?;
    let root = scratch.path();
    let (main, candidate) = captured_revisions(root)?;

    // The controls are discovered later; the current runner answers
    // something else and is never consulted.
    let second = registry(
        &root.join("registry-2"),
        &[literal_case, snapshot_case, alias_case],
    )?;
    let current = Runner::new(&revision_runner(
        &root.join("bin"),
        "current-runner",
        r#"{"x":"current"}"#,
        unknown(),
    )?)?;
    let snapshot = second
        .cases
        .get("serialized-map-snapshot")
        .ok_or_eyre("snapshot")?;
    let current_answer = controls::observe_case(
        &current,
        &root.join("current-run"),
        &second.root.join("blobs"),
        snapshot,
    )?;
    sim_assert_eq!(have: current_answer.response.result, want: exact(output(r#"{"x":"current"}"#)));
    let before = (tree_digest(&main.root)?, tree_digest(&candidate.root)?);
    let mut regressions = Vec::new();
    for (case_id, main_answer, candidate_answer) in [
        (
            "serialized-map-snapshot",
            exact(output(r#"{"x":"old"}"#)),
            exact(output(r#"{"x":"new"}"#)),
        ),
        (
            "write-visible-alias",
            exact(alias(true)),
            exact(alias(false)),
        ),
    ] {
        let replay = |capture: &Capture, side: &str| {
            backfill_observation(
                capture,
                &second,
                case_id,
                &root.join(format!("backfill-{side}-{case_id}")),
            )
        };
        let (replayed_main, replayed_candidate) =
            (replay(&main, "main")?, replay(&candidate, "candidate")?);
        sim_assert_eq!(have: replayed_main.response.result, want: main_answer);
        sim_assert_eq!(have: replayed_candidate.response.result, want: candidate_answer);
        let main_build = &main
            .controls
            .as_ref()
            .ok_or_eyre("controls")?
            .runner
            .build_id;
        sim_assert_eq!(have: &replayed_main.response.runner_build_id, want: main_build);
        let case = second.cases.get(case_id).ok_or_eyre("case")?;
        let outcome = controls::classify_observation(
            case,
            &replayed_main.response.result,
            &replayed_candidate.response.result,
        )?;
        let direction = Some("observation:correct-to-wrong".to_string());
        sim_assert_eq!(have: (outcome.class, outcome.direction.clone()), want: (Class::Regression, direction));
        regressions.push(outcome);
    }
    sim_assert_eq!(have: (tree_digest(&main.root)?, tree_digest(&candidate.root)?), want: before);

    // The regressions enter the outcome table's regression set although no
    // schema byte changed.
    let mut table = OutcomeTable::new(Vec::new());
    table.observations = regressions;
    let alias = second
        .cases
        .get("write-visible-alias")
        .ok_or_eyre("alias")?;
    let want = [snapshot, alias]
        .into_iter()
        .map(|case| {
            Ok(RegressionKey {
                witness: case.witness_id()?,
                direction: "observation:correct-to-wrong".into(),
            })
        })
        .collect::<Result<_, DiffError>>()?;
    sim_assert_eq!(have: table.regressions(), want: want);
    Ok(())
}

/// A missing input blob or a missing retained runner blocks replay.
#[test]
fn replay_needs_every_blob_and_the_retained_runner() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-replay-blocked")?;
    let root = scratch.path();
    let (main, candidate) = captured_revisions(root)?;
    let second = registry(&root.join("registry-2"), &[literal_case, snapshot_case])?;
    let snapshot = second
        .cases
        .get("serialized-map-snapshot")
        .ok_or_eyre("snapshot")?;
    let pruned = controls::copy_registry(&second, &root.join("registry-pruned"))?;
    let document = &snapshot.input.coalesced_document_sha256;
    std::fs::remove_file(pruned.root.join("blobs").join(document))?;
    let without_blob = backfill_observation(
        &candidate,
        &pruned,
        &snapshot.id,
        &root.join("without-blob"),
    );
    let message = identity_message(without_blob)?;
    assert!(message.contains(document.as_str()), "{message}");
    let message = identity_message(ControlRegistry::open(&pruned.root))?;
    assert!(message.contains("blob index"), "{message}");
    std::fs::remove_file(main.root.join("controls/runner/protocol-runner"))?;
    let without_runner =
        backfill_observation(&main, &second, &snapshot.id, &root.join("without-runner"));
    let message = identity_message(without_runner)?;
    assert!(message.contains("retained runner"), "{message}");
    identity_message(open_capture(&main.root))?;
    Ok(())
}

#[test]
fn grounding_only_cases_are_never_observed_or_scored() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-grounding")?;
    let root = scratch.path();
    let mut builder = RegistryBuilder::new(&root.join("registry"))?;
    let mut probe = literal_case(&mut builder)?;
    probe.id = "raw-text-probe".into();
    probe.role = Role::HelmGroundingOnly;
    probe.query = None;
    probe.claim = Some("full-file bytes are outside version 1".into());
    builder.case(&probe)?;
    let literal = literal_case(&mut builder)?;
    builder.case(&literal)?;
    let registry = builder.finish()?;
    let answers = [("/data/greeting", exact(output("hello world")))];
    let runner_path = runner(&root.join("bin"), "runner", "answer", &answers)?;
    let main = capture(&root.join("main"), b"{}", &registry.root, &runner_path)?;
    let observed = main
        .controls
        .as_ref()
        .ok_or_eyre("controls")?
        .observations
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    sim_assert_eq!(have: observed, want: vec!["literal-output".to_string()]);
    let outcomes = observation_outcomes(&main, &main)?;
    sim_assert_eq!(have: outcomes.len(), want: 1);
    sim_assert_eq!(
        have: controls::coverage(&outcomes, &registry),
        want: json!({"scored": 1, "grounding_only": 1, "candidate_scores": {"exact_correct": 1}})
    );
    Ok(())
}

/// A version-1 runner can answer an abort as a not-emitted selection.
#[test]
fn abort_selections_round_trip() -> eyre::Result<()> {
    let observation = Observation::Output {
        selection: Selection::NotEmitted {
            phase: AbortPhase::TemplateEval,
        },
    };
    let value: Value = serde_json::to_value(&observation)?;
    sim_assert_eq!(have: value.clone(), want: json!({"kind": "output", "selection": {"kind": "not_emitted", "phase": "template_eval"}}));
    sim_assert_eq!(have: serde_json::from_value::<Observation>(value)?, want: observation);
    let termination = Observation::Termination {
        outcome: Termination::Abort {
            phase: AbortPhase::RenderYaml,
        },
    };
    let value = serde_json::to_value(&termination)?;
    sim_assert_eq!(have: value.clone(), want: json!({"kind": "termination", "outcome": "abort", "phase": "render_yaml"}));
    sim_assert_eq!(have: serde_json::from_value::<Observation>(value)?, want: termination);
    Ok(())
}

/// A retained runner that cannot evaluate a later-discovered query leaves
/// that observation unadjudicated: neither the current runner nor the
/// Helm ground fills in for it.
#[test]
fn an_unsupported_historical_runner_cannot_be_bypassed() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-unsupported")?;
    let root = scratch.path();
    let bin = root.join("bin");
    let unsupported = ControlResult::Unsupported {
        query_kind: "output_v1".into(),
        mode: EngineMode::Render,
        reason: UnsupportedReason::PrivateHookUnavailable,
    };
    let answers = [
        ("/data/greeting", exact(output("hello world"))),
        ("/data/snapshot", unsupported.clone()),
    ];
    let old_runner = runner(&bin, "old-runner", "answer", &answers)?;
    let new_runner = revision_runner(&bin, "new-runner", r#"{"x":"old"}"#, exact(alias(true)))?;
    let first = registry(&root.join("registry-1"), &[literal_case])?;
    let main = capture(&root.join("main"), b"{}", &first.root, &old_runner)?;
    let candidate = capture(&root.join("candidate"), b"{}", &first.root, &new_runner)?;
    let second = registry(&root.join("registry-2"), &[literal_case, snapshot_case])?;
    let snapshot = second
        .cases
        .get("serialized-map-snapshot")
        .ok_or_eyre("snapshot")?;
    let replayed_main = backfill_observation(&main, &second, &snapshot.id, &root.join("main-run"))?;
    let replayed_candidate = backfill_observation(
        &candidate,
        &second,
        &snapshot.id,
        &root.join("candidate-run"),
    )?;
    sim_assert_eq!(have: replayed_main.response.result.clone(), want: unsupported);
    let outcome = controls::classify_observation(
        snapshot,
        &replayed_main.response.result,
        &replayed_candidate.response.result,
    )?;
    sim_assert_eq!(have: outcome.class, want: Class::Unadjudicated);
    let mut table = OutcomeTable::new(Vec::new());
    table.observations = vec![outcome];
    sim_assert_eq!(have: table.closures().len(), want: 0);
    Ok(())
}
