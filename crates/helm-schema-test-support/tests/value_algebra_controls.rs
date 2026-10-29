//! Version-1 observation controls and the persistent history they enter:
//! registry and grounding verification, the runner protocol's envelope
//! checks, scoring, and a real `init → C01 → C02 → C03` chain whose
//! later-discovered controls are journaled, backfilled with each
//! checkpoint's retained runner, and still caught when displaced.
//!
//! This binary doubles as a synthetic protocol runner
//! ([`tests::value_algebra_controls::protocol_v1`]): a wrapper script sets
//! its answers, so two scripts are two versioned runner builds. A fake Helm
//! script renders each chart's `rendered/` sidecar, so groundings reproduce.
//! Synthetic runners, fake Helm and hand-written captures are protocol
//! fixtures, never admissible checkpoint evidence. The scripts need a Unix
//! shell, so the binary is Unix-only.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::admission;
use helm_schema_test_support::helm::invocation::find_helmsweep;
use helm_schema_test_support::manifest::BuildProvenance;
use helm_schema_test_support::registry::PROVIDER_BUNDLE;
use helm_schema_test_support::source_digest::sha256_hex;
use helm_schema_test_support::value_algebra_diff::admission::AdmissionRecord;
use helm_schema_test_support::value_algebra_diff::build::{BinaryRecord, BuildReceipt, dir_sha256};
use helm_schema_test_support::value_algebra_diff::capture::VerifiedDump;
use helm_schema_test_support::value_algebra_diff::commands::{
    self, AmendmentReview, CompareRequest, LiveOptions, Verdict,
};
use helm_schema_test_support::value_algebra_diff::compare::{Class, LedgerRow, SeedReview};
use helm_schema_test_support::value_algebra_diff::controls::{
    self, AbortPhase, ChartTree, ControlCase, ControlRegistry, ControlResult, EngineMode,
    ExactCoverage, ExecutionEvidence, GroundCoverage, Observation, ProjectionStatus, Query,
    RegistryBuilder, Role, Runner, Score, Selection, Termination, UnknownReason, UnsupportedReason,
};
use helm_schema_test_support::value_algebra_diff::history::{
    FORMAT_VERSION, HistoryLock, Receipt, Universe,
};
use helm_schema_test_support::value_algebra_diff::revision::{
    self, CaptureManifest, open_capture, record_controls,
};
use helm_schema_test_support::value_algebra_diff::{DiffError, EquivalenceCertificate, json_diff};
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

        /// Answers from `SYNTHETIC_CONTROL_ANSWERS` (`file#pointer`, or
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
                    Query::OutputV1 { file, pointer, .. }
                    | Query::AliasV1 { file, pointer, .. } => {
                        format!("{file}#{pointer}")
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

/// The toJson snapshot template of the required regression test.
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

/// Pinned Helm 4.2.3's render of [`SNAPSHOT`].
const SNAPSHOT_RENDERED: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: snapshot
    data:
      snapshot: "{\"x\":\"old\"}"
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

/// Pinned Helm 4.2.3's render of [`ALIAS`].
const ALIAS_RENDERED: &str = indoc! {"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: alias
    data:
      visible: true
"};

const LITERAL: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    data:
      greeting: "hello world"
"#};

/// The fake pinned Helm: `version`, or the chart's `rendered/` sidecar for
/// `--show-only`, or the sidecar abort diagnostic.
const FAKE_HELM: &str = indoc! {r#"
    #!/bin/sh
    if [ "$1" = version ]; then echo 'fake helm v4.2.3'; exit 0; fi
    chart="$3"
    show=""
    while [ $# -gt 0 ]; do
      if [ "$1" = --show-only ]; then show="$2"; fi
      shift
    done
    if [ -f "$chart/rendered/abort.txt" ]; then /bin/cat "$chart/rendered/abort.txt" >&2; exit 1; fi
    if [ -n "$show" ]; then
      printf -- '---\n# Source: c/%s\n' "$show"
      /bin/cat "$chart/rendered/$show"
    fi
    exit 0
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

fn unsupported() -> ControlResult {
    ControlResult::Unsupported {
        query_kind: "alias_v1".into(),
        mode: EngineMode::Render,
        reason: UnsupportedReason::PrivateHookUnavailable,
    }
}

fn output_query(file: &str, pointer: &str) -> Query {
    Query::OutputV1 {
        file: file.into(),
        document: 0,
        pointer: pointer.into(),
    }
}

fn alias_query(file: &str) -> Query {
    Query::AliasV1 {
        file: file.into(),
        document: 0,
        pointer: "/data/visible".into(),
        relation: controls::AliasRelation::WriteVisible,
    }
}

/// One control: its id, chart files (with the `rendered/` sidecar) and query.
struct Spec {
    id: &'static str,
    files: Vec<(&'static str, &'static str)>,
    query: Query,
}

fn literal() -> Spec {
    Spec {
        id: "literal-output",
        files: vec![
            CHART_YAML,
            ("templates/cm.yaml", LITERAL),
            ("rendered/templates/cm.yaml", LITERAL),
        ],
        query: output_query("templates/cm.yaml", "/data/greeting"),
    }
}

fn direct_abort() -> Spec {
    Spec {
        id: "direct-abort",
        files: vec![
            CHART_YAML,
            ("templates/fail.yaml", "{{ fail \"boom\" }}\n"),
            (
                "rendered/abort.txt",
                "Error: execution error at (c/templates/fail.yaml:1:3): boom\n",
            ),
        ],
        query: Query::TerminationV1 {
            scope: controls::TerminationScope::Render,
        },
    }
}

fn static_alias() -> Spec {
    let template = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        data:
          visible: true
    "};
    Spec {
        id: "static-alias",
        files: vec![
            CHART_YAML,
            ("templates/static.yaml", template),
            ("rendered/templates/static.yaml", template),
        ],
        query: alias_query("templates/static.yaml"),
    }
}

fn snapshot() -> Spec {
    Spec {
        id: "serialized-map-snapshot",
        files: vec![
            CHART_YAML,
            ("templates/snapshot.yaml", SNAPSHOT),
            ("rendered/templates/snapshot.yaml", SNAPSHOT_RENDERED),
        ],
        query: output_query("templates/snapshot.yaml", "/data/snapshot"),
    }
}

fn late_literal() -> Spec {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          late: "yes, late"
    "#};
    Spec {
        id: "late-literal",
        files: vec![
            CHART_YAML,
            ("templates/late.yaml", template),
            ("rendered/templates/late.yaml", template),
        ],
        query: output_query("templates/late.yaml", "/data/late"),
    }
}

fn write_alias() -> Spec {
    Spec {
        id: "write-visible-alias",
        files: vec![
            CHART_YAML,
            ("templates/alias.yaml", ALIAS),
            ("rendered/templates/alias.yaml", ALIAS_RENDERED),
        ],
        query: alias_query("templates/alias.yaml"),
    }
}

fn fake_helm(dir: &Path) -> eyre::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("helm");
    if !path.exists() {
        std::fs::write(&path, FAKE_HELM)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(path)
}

/// The scored case of `spec`, grounded by running `helm`.
fn grounded(
    builder: &mut RegistryBuilder,
    helm: &Path,
    spec: &Spec,
    scratch: &Path,
) -> eyre::Result<ControlCase> {
    let input = builder.render_input(&spec.files, "")?;
    let grounding = builder.ground(helm, &input, &spec.query, &scratch.join(spec.id))?;
    Ok(ControlCase {
        format_version: controls::PROTOCOL_VERSION,
        id: spec.id.into(),
        role: Role::Scored,
        input,
        query: Some(spec.query.clone()),
        claim: None,
        grounding,
    })
}

/// A registry of `specs` at the new directory `root`, grounded by `helm`.
fn registry(root: &Path, helm: &Path, specs: &[Spec]) -> eyre::Result<ControlRegistry> {
    let mut builder = RegistryBuilder::new(root)?;
    let scratch = root.with_extension("ground");
    for spec in specs {
        let case = grounded(&mut builder, helm, spec, &scratch)?;
        builder.case(&case)?;
    }
    Ok(builder.finish()?)
}

/// The base registry: one control of each version-1 query.
fn base(root: &Path, helm: &Path) -> eyre::Result<ControlRegistry> {
    registry(root, helm, &[literal(), direct_abort(), static_alias()])
}

/// A synthetic runner script `dir/name` answering `answers` by `file#pointer`.
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

/// A runner answering the base controls correctly, and `snapshot` and
/// `visible` for the later controls.
fn revision_runner(
    dir: &Path,
    name: &str,
    snapshot: &str,
    visible: ControlResult,
) -> eyre::Result<PathBuf> {
    runner(
        dir,
        name,
        "answer",
        &[
            (
                "templates/cm.yaml#/data/greeting",
                exact(output("hello world")),
            ),
            (
                "termination",
                exact(Observation::Termination {
                    outcome: Termination::Abort {
                        phase: AbortPhase::TemplateEval,
                    },
                }),
            ),
            ("templates/static.yaml#/data/visible", exact(alias(true))),
            (
                "templates/snapshot.yaml#/data/snapshot",
                exact(output(snapshot)),
            ),
            ("templates/alias.yaml#/data/visible", visible),
        ],
    )
}

/// The admission records exactly as the suite states main.
fn recorded_admissions() -> eyre::Result<Vec<AdmissionRecord>> {
    let root = test_util::workspace_testdata().join(admission::SUITE_DIR);
    let suite = admission::read_suite(&root)?;
    let mut records = Vec::new();
    for cell in &suite.cells {
        let coalesced = suite
            .coalesced
            .get(&cell.id)
            .ok_or_eyre("no coalesced row")?;
        records.push(AdmissionRecord {
            cell: cell.id.clone(),
            declared_types: cell.declared_types.as_arg().to_string(),
            verdict: if cell.main == admission::Verdict::Accept {
                "accept"
            } else {
                "reject"
            }
            .into(),
            violations: Vec::new(),
            inputs_sha256: coalesced.inputs_sha256.clone(),
            document_sha256: coalesced.document_sha256.clone(),
        });
    }
    records.sort();
    Ok(records)
}

/// A hand-written capture at the new directory `root`: a one-chart dump of
/// `schema`, its export, the suite's recorded admission verdicts and the
/// controls `registry` evaluated by `runner`.
fn capture(
    root: &Path,
    schema: &[u8],
    registry: &Path,
    runner: &Path,
    helm: &Path,
) -> eyre::Result<PathBuf> {
    let provenance = BuildProvenance::compiled();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin)?;
    let mut binaries = BTreeMap::new();
    for name in [
        "corpus_generation",
        "contract_export",
        "helm-schema",
        "protocol-runner",
    ] {
        let path = bin.join(name);
        if name == "protocol-runner" {
            std::fs::copy(runner, &path)?;
        } else {
            std::fs::write(&path, format!("synthetic {name}"))?;
        }
        binaries.insert(
            name.to_string(),
            BinaryRecord {
                sha256: sha256_hex(&std::fs::read(&path)?),
                cargo_path: "synthetic".into(),
                linked: Vec::new(),
            },
        );
    }
    std::fs::write(bin.join("cargo-artifacts.jsonl"), b"")?;
    let receipt = BuildReceipt {
        format_version: FORMAT_VERSION,
        commands: Vec::new(),
        rustc: "synthetic".into(),
        cargo: "synthetic".into(),
        environment: BTreeMap::new(),
        source_before_sha256: "synthetic".into(),
        source_after_sha256: "synthetic".into(),
        provenance: provenance.clone(),
        artifacts_sha256: sha256_hex(b""),
        binaries,
    };
    std::fs::write(bin.join("build.json"), serde_json::to_vec_pretty(&receipt)?)?;
    let dump_dir = root.join("dump");
    write_dump(&dump_dir, schema)?;
    let dump = VerifiedDump::open(&dump_dir)?;
    let export = root.join("export");
    std::fs::create_dir_all(&export)?;
    std::fs::write(export.join("0000.contract.json"), b"{}\n")?;
    std::fs::write(export.join("0000.schema.json"), schema)?;
    let key = dump.keys().next().ok_or_eyre("empty dump")?.to_string();
    let export_manifest = json!({"format_version": 1, "build": provenance, "entries": [{
        "key": key,
        "contract": {"file": "0000.contract.json", "sha256": sha256_hex(b"{}\n")},
        "schema": {"file": "0000.schema.json", "sha256": sha256_hex(schema)},
    }]});
    std::fs::write(
        export.join("export.json"),
        serde_json::to_vec_pretty(&export_manifest)?,
    )?;
    std::fs::write(root.join("parity.jsonl"), b"{\"synthetic\":true}\n")?;
    let mut admissions = Vec::new();
    for record in recorded_admissions()? {
        admissions.extend(serde_json::to_vec(&record)?);
        admissions.push(b'\n');
    }
    std::fs::write(root.join("admission.jsonl"), &admissions)?;
    let controls = record_controls(root, registry, &provenance, helm)?;
    let file_sha = |path: &Path| -> eyre::Result<String> { Ok(sha256_hex(&std::fs::read(path)?)) };
    let testdata = test_util::workspace_testdata();
    let manifest = CaptureManifest {
        format_version: FORMAT_VERSION,
        scope: revision::SCOPE.into(),
        commit: "c0ffee".into(),
        tree: "synthetic".into(),
        checkout: "/nonexistent/checkout".into(),
        status: String::new(),
        synthetic: false,
        build_sha256: file_sha(&bin.join("build.json"))?,
        provenance,
        dump_manifest_sha256: dump.manifest_sha256().to_string(),
        export_sha256: file_sha(&export.join("export.json"))?,
        parity_sha256: file_sha(&root.join("parity.jsonl"))?,
        admission_suite_sha256: dir_sha256(&testdata.join(admission::SUITE_DIR))?,
        admission_sha256: file_sha(&root.join("admission.jsonl"))?,
        provider_sha256: dir_sha256(&testdata.join(PROVIDER_BUNDLE))?,
        helmsweep_sha256: file_sha(&find_helmsweep()?)?,
        helmsweep_version: "synthetic".into(),
        helm_sha256: file_sha(helm)?,
        kubernetes_release: "v1.29.0-standalone-strict".into(),
        suite_sha256: revision::suite_sha256(&dump)?,
        contexts_sha256: write_roster(&root.join("contexts.json"))?,
        controls,
    };
    std::fs::write(
        root.join("capture.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    open_capture(root)?;
    Ok(root.to_path_buf())
}

/// Writes the fixture's one-row context roster to `path`; returns its
/// content identity.
fn write_roster(path: &Path) -> eyre::Result<String> {
    let empty = sha256_hex(b"{}");
    let row = json!({
        "id": "deployed-defaults", "chart": common::CHART, "overlay": {},
        "transport": "values-file", "kubernetes_version": "1.29.0",
        "chart_sha256": "", "baseline_sha256": "", "candidate_sha256": "",
        "canonical_overlay_sha256": empty, "values_file": "{}\n",
        "values_file_sha256": sha256_hex(b"{}\n"),
        "context_overlay": {}, "context_overlay_sha256": empty,
    });
    std::fs::write(
        path,
        serde_json::to_vec(&json!({"version": 2, "rows": [row]}))?,
    )?;
    Ok(sha256_hex(&std::fs::read(path)?))
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
fn a_registry_binds_its_cases_blobs_and_groundings() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-registry")?;
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let root = scratch.path().join("registry");
    let opened = registry(&root, &helm, &[literal()])?;
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
    let evidence = root.join("blobs").join(&case.grounding.evidence_sha256);
    let original = std::fs::read(&evidence)?;
    std::fs::write(&evidence, b"{}")?;
    let message = identity_message(ControlRegistry::open(&root))?;
    assert!(message.contains("content mismatch"), "{message}");
    std::fs::write(&evidence, &original)?;
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

/// A grounding must be the pinned render of exactly its case's input and
/// derive exactly its claimed observation.
#[test]
fn groundings_are_resolved_and_rederived() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-grounding")?;
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let grounding_scratch = scratch.path().join("ground");
    let case = grounded(&mut builder, &helm, &literal(), &grounding_scratch)?;
    case.check_grounding(&builder.blobs())?;
    let mut claims_other = case.clone();
    claims_other.grounding.alternatives = vec![output("goodbye")];
    let message = identity_message(claims_other.check_grounding(&builder.blobs()))?;
    assert!(message.contains("derives another observation"), "{message}");
    let mut other_input = case.clone();
    other_input.input.kubernetes_version = "v1.30.0".into();
    let message = identity_message(other_input.check_grounding(&builder.blobs()))?;
    assert!(
        message.contains("not the pinned render of this input"),
        "{message}"
    );
    let mut unresolved = case.clone();
    unresolved.grounding.evidence_sha256 = "1".repeat(64);
    identity_message(unresolved.check_grounding(&builder.blobs()))?;
    // An unrecognized diagnostic is no observation.
    let spec = Spec {
        files: vec![
            CHART_YAML,
            ("templates/cm.yaml", LITERAL),
            ("rendered/abort.txt", "Error: mystery\n"),
        ],
        ..literal()
    };
    let input = builder.render_input(&spec.files, "")?;
    let message = request_message(builder.ground(
        &helm,
        &input,
        &spec.query,
        &grounding_scratch.join("mystery"),
    ))?;
    assert!(
        message.contains("unrecognized Helm diagnostic"),
        "{message}"
    );
    Ok(())
}

#[test]
fn case_shapes_and_the_supported_domain_are_checked() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-cases")?;
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let base = grounded(
        &mut builder,
        &helm,
        &literal(),
        &scratch.path().join("ground"),
    )?;
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
    let mut empty = base.clone();
    empty.grounding.alternatives = Vec::new();
    for (label, case) in [
        ("lint", lint),
        ("document 1", second_document),
        ("claim", with_claim),
        ("grounding with query", grounding_with_query),
        ("answers another query", other_answer),
        ("unsorted", unsorted),
        ("empty grounding", empty),
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
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let base = grounded(
        &mut builder,
        &helm,
        &literal(),
        &scratch.path().join("ground"),
    )?;
    let mut regrounded = base.clone();
    regrounded.grounding.evidence_sha256 = "1".repeat(64);
    sim_assert_eq!(have: regrounded.witness_id()?, want: base.witness_id()?);
    let mut changed_query = base.clone();
    changed_query.query = Some(output_query("templates/cm.yaml", "/data/other"));
    sim_assert_eq!(have: changed_query.witness_id()? == base.witness_id()?, want: false);
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
        let helm = fake_helm(&scratch.path().join("helm"))?;
        let registry = registry(&scratch.path().join("registry"), &helm, &[literal()])?;
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

    fn runner(&self, name: &str, mode: &str, answer: ControlResult) -> eyre::Result<Runner> {
        let path = runner(
            &self.scratch.path().join("bin"),
            name,
            mode,
            &[("templates/cm.yaml#/data/greeting", answer)],
        )?;
        Ok(Runner::new(&path)?)
    }

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
    let blobs = protocol.registry.root.join("blobs");
    let unknown_query = json!({"kind": "output_v2", "file": "templates/cm.yaml"});
    let timeout = Duration::from_secs(60);
    let response = protocol.observe(&good, "unknown", &blobs, &unknown_query, timeout)??;
    sim_assert_eq!(have: response.result, want: ControlResult::Unsupported {
        query_kind: "output_v2".into(),
        mode: EngineMode::Render,
        reason: UnsupportedReason::QueryNotImplemented,
    });
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
    let message = infrastructure_message(protocol.observe(
        &hung,
        "sleep",
        &blobs,
        &protocol.query,
        Duration::from_secs(2),
    )?)?;
    assert!(message.contains("timed out"), "{message}");
    let crossed = protocol.runner(
        "crossed",
        "answer",
        exact(Observation::Termination {
            outcome: Termination::Normal,
        }),
    )?;
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
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let case = grounded(
        &mut builder,
        &helm,
        &snapshot(),
        &scratch.path().join("ground"),
    )?;
    let (old, new) = (
        exact(output(r#"{"x":"old"}"#)),
        exact(output(r#"{"x":"new"}"#)),
    );
    let other = exact(output(r#"{"x":"other"}"#));
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
    sim_assert_eq!(have: class(&old, &unsupported())?, want: (Class::Unadjudicated, None));
    sim_assert_eq!(have: class(&unsupported(), &old)?, want: (Class::Unadjudicated, None));
    let failure = ControlResult::InfrastructureFailure {
        code: controls::InfrastructureCode::Timeout,
        diagnostic: "late".into(),
    };
    infrastructure_message(controls::classify_observation(&case, &old, &failure))?;
    // A gap's representation does not matter, an exact answer's does.
    let (left, right) = (
        controls::classify_observation(&case, &old, &unknown())?,
        controls::classify_observation(
            &case,
            &old,
            &ControlResult::AbstractUnknown {
                reason: UnknownReason::PathCoverage,
                evidence: ExecutionEvidence {
                    evaluator: "another".into(),
                    result_sha256: "1".repeat(64),
                    gaps: Vec::new(),
                    projection: ProjectionStatus::ReadOnly,
                },
            },
        )?,
    );
    sim_assert_eq!(have: left.answer_sha256, want: right.answer_sha256);
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

/// A capture keeps every scored observation of its registry, refuses a
/// registry that does not score every query kind, and fails verification
/// once its retained runner changes.
#[test]
fn a_capture_retains_its_runner_and_answers() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-capture")?;
    let root = scratch.path();
    let helm = fake_helm(&root.join("helm"))?;
    let complete = base(&root.join("registry"), &helm)?;
    let correct = revision_runner(
        &root.join("bin"),
        "runner",
        r#"{"x":"old"}"#,
        exact(alias(true)),
    )?;
    let path = capture(
        &root.join("capture"),
        b"{}",
        &complete.root,
        &correct,
        &helm,
    )?;
    let opened = open_capture(&path)?;
    sim_assert_eq!(have: opened.controls.observations.len(), want: 3);
    let partial = registry(&root.join("partial"), &helm, &[literal()])?;
    let message = identity_message(record_controls(
        &root.join("partial-capture"),
        &partial.root,
        &BuildProvenance::compiled(),
        &helm,
    ))?;
    assert!(message.contains("not every version-1 query"), "{message}");
    std::fs::write(path.join("bin/protocol-runner"), b"#!/bin/sh\nexit 0\n")?;
    identity_message(open_capture(&path))?;
    Ok(())
}

/// The history driven through its public commands.
struct Chain {
    root: PathBuf,
    history: PathBuf,
    helmsweep: PathBuf,
    reviewer: String,
    attempts: usize,
}

impl Chain {
    fn live(&self, out: &Path) -> LiveOptions {
        LiveOptions {
            helmsweep: Some(self.helmsweep.clone()),
            selection: None,
            scratch: out.join("scratch"),
        }
    }

    fn head(&self) -> eyre::Result<PathBuf> {
        let head = Universe::new(self.history.join("universe")).gate_head()?;
        Ok(self.history.join(head.receipt))
    }

    fn out(&mut self, label: &str) -> PathBuf {
        self.attempts += 1;
        self.root.join(format!("{:02}-{label}", self.attempts))
    }

    /// Compares `candidate` as the next checkpoint with `ledger` rows.
    fn compare(
        &mut self,
        label: &str,
        main: &Path,
        candidate: &Path,
        ledger: &[LedgerRow],
        seed: Option<&[LedgerRow]>,
        certificates: &[EquivalenceCertificate],
    ) -> eyre::Result<(PathBuf, Result<Verdict, DiffError>)> {
        let out = self.out(label);
        let files = self.root.join(format!("{label}-inputs-{}", self.attempts));
        std::fs::create_dir_all(&files)?;
        std::fs::write(files.join("ledger.json"), serde_json::to_vec(ledger)?)?;
        let seed_review = match seed {
            Some(rows) => {
                let review = SeedReview {
                    format_version: FORMAT_VERSION,
                    reviewer: self.reviewer.clone(),
                    rows: rows.to_vec(),
                };
                std::fs::write(files.join("seed.json"), serde_json::to_vec(&review)?)?;
                Some(files.join("seed.json"))
            }
            None => None,
        };
        let request = CompareRequest {
            main: main.to_path_buf(),
            candidate: candidate.to_path_buf(),
            history: self.history.clone(),
            previous: self.head()?,
            ledger: files.join("ledger.json"),
            out: out.clone(),
            seed_review,
            certificates: certificates.to_vec(),
            live: self.live(&out),
        };
        let verdict = commands::compare(&request);
        Ok((out, verdict))
    }

    /// Amends the history with the one batch journaled after the head.
    fn amend(&mut self, seed_extension: &[LedgerRow]) -> eyre::Result<Result<Verdict, DiffError>> {
        let previous = self.head()?;
        let receipt = Receipt::read(&previous)?;
        let universe = Universe::new(self.history.join("universe")).load()?;
        let batch = universe.batches.last().ok_or_eyre("no batches")?.clone();
        eyre::ensure!(
            !receipt.witness_batch_hashes.contains(&batch),
            "no new batch to amend"
        );
        let out = self.out("amend");
        let review = AmendmentReview {
            format_version: FORMAT_VERSION,
            reviewer: self.reviewer.clone(),
            batch_sha256: batch.clone(),
            seed_extension: seed_extension.to_vec(),
        };
        let review_path = self.root.join(format!("review-{}.json", self.attempts));
        std::fs::write(&review_path, serde_json::to_vec(&review)?)?;
        Ok(commands::amend(
            &self.history,
            &previous,
            &batch,
            &review_path,
            &out,
            &self.live(&out),
        ))
    }
}

/// Ledger rows for every regression an output directory measured.
fn rows(out: &Path) -> eyre::Result<Vec<LedgerRow>> {
    Ok(commands::read_outcomes(out)?
        .regressions()
        .into_iter()
        .map(|key| LedgerRow {
            witness: key.witness,
            direction: key.direction,
            owner: "R3b".into(),
            cause: "the reviewed seed".into(),
            repair_checkpoint: 5,
            answer_sha256: key.answer_sha256,
        })
        .collect())
}

fn problems(verdict: Result<Verdict, DiffError>) -> eyre::Result<Vec<String>> {
    match verdict? {
        Verdict::Fail(problems) => Ok(problems),
        Verdict::Pass => eyre::bail!("expected a failing verdict"),
    }
}

fn every_file(root: &Path) -> eyre::Result<BTreeMap<PathBuf, String>> {
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

/// Main's schema in the chain test.
const MAIN_SCHEMA: &[u8] = br#"{"type":"object"}"#;

/// C02's reviewed seed tightening.
const TIGHTENED: &[u8] =
    br#"{"type":"object","properties":{"ingress":{"properties":{"enabled":{"const":false}}}}}"#;

/// The state the chain test carries from one checkpoint to the next.
struct Story {
    chain: Chain,
    helm: PathBuf,
    bin: PathBuf,
    first: ControlRegistry,
    wrong: PathBuf,
    main: PathBuf,
    c01: PathBuf,
    c02: PathBuf,
    seed: Vec<LedgerRow>,
    certificates: [EquivalenceCertificate; 1],
}

/// The answers of a runner that answers the base registry, except `visible`.
fn base_answers(visible: ControlResult) -> Vec<(&'static str, ControlResult)> {
    vec![
        (
            "templates/cm.yaml#/data/greeting",
            exact(output("hello world")),
        ),
        (
            "termination",
            exact(Observation::Termination {
                outcome: Termination::Abort {
                    phase: AbortPhase::TemplateEval,
                },
            }),
        ),
        ("templates/static.yaml#/data/visible", visible),
    ]
}

fn write_history_plan(root: &Path, main: &Path) -> eyre::Result<PathBuf> {
    let plan = root.join("plan.json");
    let checkpoints = (1..=10)
        .map(|ordinal| json!({"ordinal": ordinal, "subject": format!("C{ordinal:02}"), "kind": if ordinal == 3 { "semantic" } else { "representation" }}))
        .collect::<Vec<_>>();
    std::fs::write(
        &plan,
        serde_json::to_vec(&json!({
            "format_version": 1, "history_id": "r3b-test", "main_commit": "c0ffee",
            "checkpoints": checkpoints, "seed_ordinal": 2, "reviewer": "reviewer",
            "context_roster_sha256": sha256_hex(&std::fs::read(main.join("contexts.json"))?),
        }))?,
    )?;
    Ok(plan)
}

/// Genesis, an unsupported C01 refused, C01, an unreviewed C02 refused and
/// the reviewed C02.
fn through_c02(root: &Path, late_seed_loss: bool) -> eyre::Result<Story> {
    let helm = fake_helm(&root.join("helm"))?;
    let bin = root.join("bin");
    let first = base(&root.join("registry-1"), &helm)?;
    let correct = revision_runner(&bin, "correct", r#"{"x":"old"}"#, exact(alias(true)))?;
    let shaky = runner(&bin, "unsupported", "answer", &base_answers(unsupported()))?;
    let wrong = revision_runner(&bin, "wrong", r#"{"x":"new"}"#, exact(alias(false)))?;
    let main = capture(
        &root.join("main"),
        MAIN_SCHEMA,
        &first.root,
        &correct,
        &helm,
    )?;
    let c01_unsupported = capture(
        &root.join("c01-unsupported"),
        MAIN_SCHEMA,
        &first.root,
        &shaky,
        &helm,
    )?;
    let c01 = capture(&root.join("c01"), MAIN_SCHEMA, &first.root, &correct, &helm)?;
    let seed_runner = revision_runner(
        &bin,
        "seed-runner",
        if late_seed_loss {
            r#"{"x":"new"}"#
        } else {
            r#"{"x":"old"}"#
        },
        exact(alias(true)),
    )?;
    let c02 = capture(
        &root.join("c02"),
        TIGHTENED,
        &first.root,
        &seed_runner,
        &helm,
    )?;
    let plan = write_history_plan(root, &main)?;
    let mut chain = Chain {
        root: root.to_path_buf(),
        history: root.join("history"),
        helmsweep: find_helmsweep()?,
        reviewer: "reviewer".into(),
        attempts: 0,
    };

    // Genesis accepts main's unchanged inherited obligations.
    let genesis_live = chain.live(&chain.history);
    sim_assert_eq!(have: commands::init(&main, &plan, &chain.history, &genesis_live)?, want: Verdict::Pass);

    // C01 must equal main: an unsupported observation blocks it.
    let (_, verdict) = chain.compare("c01-unsupported", &main, &c01_unsupported, &[], None, &[])?;
    let found = problems(verdict)?;
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("UNADJUDICATED static-alias")),
        "{found:?}"
    );
    let (_, verdict) = chain.compare("c01", &main, &c01, &[], None, &[])?;
    sim_assert_eq!(have: verdict?, want: Verdict::Pass);

    // C02's tightening is a changed artifact: it needs the reviewer's certificate.
    let parse = |bytes: &[u8]| -> eyre::Result<Value> { Ok(serde_json::from_slice(bytes)?) };
    let certificates = [EquivalenceCertificate {
        key: "chart/dict-config".into(),
        baseline_sha256: sha256_hex(MAIN_SCHEMA),
        candidate_sha256: sha256_hex(TIGHTENED),
        pointers: json_diff(&parse(MAIN_SCHEMA)?, &parse(TIGHTENED)?),
        witnesses: Vec::new(),
        reviewer: "reviewer".into(),
        reasoning: "the reviewed seed tightening".into(),
    }];
    // The seed checkpoint without a seed review is refused before it runs.
    let (attempt, verdict) = chain.compare("c02-no-seed", &main, &c02, &[], None, &certificates)?;
    let Err(DiffError::Request(message)) = verdict else {
        eyre::bail!("a seed checkpoint without --seed-review ran: {verdict:?}");
    };
    assert!(message.contains("needs --seed-review"), "{message}");
    sim_assert_eq!(have: attempt.exists(), want: false);
    let (attempt, verdict) =
        chain.compare("c02-unreviewed", &main, &c02, &[], Some(&[]), &certificates)?;
    let found = problems(verdict)?;
    assert!(
        found
            .iter()
            .any(|problem| problem.starts_with("unreviewed seed regression")),
        "{found:?}"
    );
    let seed = rows(&attempt)?;
    sim_assert_eq!(have: seed.is_empty(), want: false);
    let (_, verdict) = chain.compare("c02", &main, &c02, &seed, Some(&seed), &certificates)?;
    sim_assert_eq!(have: verdict?, want: Verdict::Pass);
    Ok(Story {
        chain,
        helm,
        bin,
        first,
        wrong,
        main,
        c01,
        c02,
        seed,
        certificates,
    })
}

/// Asserts that `found` reports `witness` as a new regression.
fn reports_regression(found: &[String], witness: &str) {
    assert!(
        found
            .iter()
            .any(|problem| problem.starts_with(&format!("new regression {witness}"))),
        "{witness}: {found:?}"
    );
}

/// C03 discovers the controls, amends history, regresses, and still
/// regresses when displaced from its registry.
fn c03_regresses(story: &mut Story, root: &Path) -> eyre::Result<()> {
    let Story {
        chain,
        helm,
        first,
        wrong,
        main,
        c01,
        c02,
        seed,
        certificates,
        ..
    } = story;
    let second = registry(
        &root.join("registry-2"),
        helm,
        &[
            literal(),
            direct_abort(),
            static_alias(),
            snapshot(),
            write_alias(),
        ],
    )?;
    let c03 = capture(&root.join("c03"), TIGHTENED, &second.root, wrong, helm)?;
    let receipts_before = every_file(&chain.history.join("transactions"))?;
    let captures_before = (every_file(main)?, every_file(c01)?, every_file(c02)?);
    let (_, verdict) = chain.compare("c03-discovery", main, &c03, seed, None, certificates)?;
    let found = problems(verdict)?;
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("backfill history with amend")),
        "{found:?}"
    );
    sim_assert_eq!(have: chain.amend(&[])??, want: Verdict::Pass);
    let receipts_after = every_file(&chain.history.join("transactions"))?;
    for (path, hash) in &receipts_before {
        sim_assert_eq!(have: receipts_after.get(path), want: Some(hash));
    }
    sim_assert_eq!(have: (every_file(main)?, every_file(c01)?, every_file(c02)?), want: captures_before);
    let backfilled = std::fs::read_dir(&chain.root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.to_string_lossy().ends_with("-amend"))
        .max()
        .ok_or_eyre("no amendment output")?;
    let c02_observations = commands::read_outcomes(&backfilled.join("checkpoint-02"))?.observations;
    let snapshot_answer = c02_observations
        .iter()
        .find(|outcome| outcome.case == "serialized-map-snapshot")
        .ok_or_eyre("the snapshot was not backfilled")?;
    sim_assert_eq!(have: snapshot_answer.candidate.clone(), want: exact(output(r#"{"x":"old"}"#)));

    // C03's own answers regress although no schema byte changed since C02.
    let (attempt, verdict) = chain.compare("c03", main, &c03, seed, None, certificates)?;
    let found = problems(verdict)?;
    for case in ["serialized-map-snapshot", "write-visible-alias"] {
        let witness = second.cases.get(case).ok_or_eyre("case")?.witness_id()?;
        reports_regression(&found, &witness);
    }
    let table = commands::read_outcomes(&attempt)?;
    sim_assert_eq!(have: table.observations.iter().filter(|outcome| outcome.class == Class::Regression).count(), want: 2);

    // Displaced: a checkpoint whose registry no longer holds the controls
    // still answers them with its retained runner and still regresses.
    let c03b = capture(&root.join("c03b"), TIGHTENED, &first.root, wrong, helm)?;
    let (_, verdict) = chain.compare("c03-displaced", main, &c03b, seed, None, certificates)?;
    let witness = second
        .cases
        .get("serialized-map-snapshot")
        .ok_or_eyre("case")?
        .witness_id()?;
    reports_regression(&problems(verdict)?, &witness);
    Ok(())
}

/// A comparison that cannot judge everything publishes nothing: C04's
/// runner cannot answer the journaled snapshot, so neither its discoveries
/// nor a receipt reach the history, and the lock is released for a rerun.
/// A held lock refuses a second writer before it reads anything.
fn c04_failing_publishes_nothing(story: &mut Story, root: &Path) -> eyre::Result<()> {
    let third = registry(
        &root.join("registry-3"),
        &story.helm,
        &[literal(), direct_abort(), static_alias(), late_literal()],
    )?;
    let mut answers = base_answers(exact(alias(true)));
    answers.push(("templates/late.yaml#/data/late", exact(output("yes, late"))));
    let forgetful = runner(&story.bin, "forgetful", "answer", &answers)?;
    let c04 = capture(
        &root.join("c04"),
        TIGHTENED,
        &third.root,
        &forgetful,
        &story.helm,
    )?;
    let history = story.chain.history.clone();
    let before = every_file(&history)?;
    let (_, verdict) = story.chain.compare(
        "c04-failing",
        &story.main,
        &c04,
        &story.seed,
        None,
        &story.certificates,
    )?;
    let message = infrastructure_message(verdict)?;
    assert!(
        message.contains("control serialized-map-snapshot"),
        "{message}"
    );
    // Content-addressed blobs it stored stay unreferenced; nothing else moved.
    let published = |files: BTreeMap<PathBuf, String>| {
        files
            .into_iter()
            .filter(|(path, _)| {
                !path.starts_with(history.join("universe/blobs"))
                    && !path.starts_with(history.join("universe/batches"))
            })
            .collect::<BTreeMap<_, _>>()
    };
    sim_assert_eq!(have: published(every_file(&history)?), want: published(before));
    let held = HistoryLock::acquire(&history)?;
    let (_, verdict) = story.chain.compare(
        "c04-locked",
        &story.main,
        &c04,
        &story.seed,
        None,
        &story.certificates,
    )?;
    assert!(identity_message(verdict)?.contains("locked"));
    drop(held);
    Ok(())
}

/// The required regression, through the real history: byte-identical
/// schemas between C02 and C03, the snapshot and alias controls discovered
/// only at C03, journaled, backfilled across main, C01 and C02 with their
/// retained runners (the current runner answers otherwise), and the C03
/// regression caught even when C03's own registry no longer holds them.
#[test]
fn non_schema_output_regression_replays_with_identical_schema() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-chain")?;
    let root = scratch.path().to_path_buf();
    let mut story = through_c02(&root, false)?;
    c03_regresses(&mut story, &root)?;
    c04_failing_publishes_nothing(&mut story, &root)?;

    let interrupted = story.chain.history.join("transactions/interrupted-staging");
    std::fs::create_dir_all(&interrupted)?;
    std::fs::write(interrupted.join("receipt.json"), b"partial")?;
    // The chain verifies; a changed payload file or a lost retained runner does not.
    let head = story.chain.head()?;
    sim_assert_eq!(have: commands::verify_history(&story.chain.history, &head)?, want: Verdict::Pass);
    let seal = Receipt::read(&head)?.seal_sha256;
    let table = story
        .chain
        .history
        .join("transactions")
        .join(&seal)
        .join("payload")
        .join("checkpoint-01/outcomes.jsonl");
    let original = std::fs::read(&table)?;
    std::fs::write(&table, b"")?;
    identity_message(commands::verify_history(&story.chain.history, &head))?;
    std::fs::write(&table, &original)?;
    std::fs::remove_file(story.c02.join("bin/protocol-runner"))?;
    identity_message(commands::verify_history(&story.chain.history, &head))?;
    // A successor is refused on the broken chain before it compares.
    let (_, verdict) = story.chain.compare(
        "c04-broken-chain",
        &story.main,
        &story.c02,
        &story.seed,
        None,
        &story.certificates,
    )?;
    identity_message(verdict)?;
    Ok(())
}

/// Version-1 observations serialize as the protocol documents them.
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

/// A runner's blob root holds exactly its case's input closure: never the
/// grounding evidence, nor Helm's output unless those bytes are an input
/// themselves (an empty stderr is the empty values file).
#[test]
fn runners_never_see_the_grounding() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-stage")?;
    let root = scratch.path();
    let helm = fake_helm(&root.join("helm"))?;
    let registry = base(&root.join("registry"), &helm)?;
    let blobs = registry.blobs();
    for case in registry.cases.values() {
        let staged = root.join("staged").join(&case.id);
        controls::stage_blobs(&blobs, case, &staged)?;
        let have = std::fs::read_dir(&staged)?
            .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
            .collect::<eyre::Result<std::collections::BTreeSet<_>>>()?;
        let inputs = blobs.input_closure(&case.input)?;
        sim_assert_eq!(have: have.clone(), want: inputs.clone());
        let evidence = case.check_grounding(&blobs)?;
        sim_assert_eq!(have: have.contains(&case.grounding.evidence_sha256), want: false);
        for output in [&evidence.stdout_sha256, &evidence.stderr_sha256] {
            sim_assert_eq!(have: have.contains(output), want: inputs.contains(output));
        }
    }
    Ok(())
}

/// Grounding checks defaults and original values with null-deletion semantics.
#[test]
fn grounding_verifies_the_composed_document() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-composition")?;
    let helm = fake_helm(&scratch.path().join("helm"))?;
    let mut builder = RegistryBuilder::new(&scratch.path().join("registry"))?;
    let mut spec = literal();
    spec.files
        .push(("values.yaml", "nested: {drop: 1, keep: 2}\n"));
    let input = builder.render_input(&spec.files, "nested: {drop: null}\n")?;
    sim_assert_eq!(have: builder.blobs().chart(&input)?.coalesced, want: json!({"nested": {"keep": 2}}));
    let grounding = builder.ground(&helm, &input, &spec.query, &scratch.path().join("ground"))?;
    let mut case = ControlCase {
        format_version: controls::PROTOCOL_VERSION,
        id: spec.id.into(),
        role: Role::Scored,
        input,
        query: Some(spec.query),
        claim: None,
        grounding,
    };
    case.check_grounding(&builder.blobs())?;
    case.input.coalesced_document_sha256 = builder.blob(b"{}")?;
    assert!(
        identity_message(case.check_grounding(&builder.blobs()))?.contains("coalesced document")
    );
    Ok(())
}

/// Later amendments retain a seed extension accepted by an earlier amendment.
#[test]
fn a_second_amendment_retains_the_reviewed_seed_extension() -> eyre::Result<()> {
    let scratch = ScratchDir::new("vac-second-amendment")?;
    let root = scratch.path();
    let mut story = through_c02(root, true)?;
    let registry2 = registry(
        &root.join("registry-2"),
        &story.helm,
        &[literal(), direct_abort(), static_alias(), snapshot()],
    )?;
    let candidate = capture(
        &root.join("discover-snapshot"),
        TIGHTENED,
        &registry2.root,
        &story.wrong,
        &story.helm,
    )?;
    let (_, verdict) = story.chain.compare(
        "discover-snapshot",
        &story.main,
        &candidate,
        &story.seed,
        None,
        &story.certificates,
    )?;
    problems(verdict)?;
    let batch = Universe::new(story.chain.history.join("universe"))
        .load()?
        .batches
        .pop()
        .ok_or_eyre("batch")?;
    let review_path = root.join("empty-extension.json");
    std::fs::write(
        &review_path,
        serde_json::to_vec(&AmendmentReview {
            format_version: FORMAT_VERSION,
            reviewer: story.chain.reviewer.clone(),
            batch_sha256: batch.clone(),
            seed_extension: vec![],
        })?,
    )?;
    let out = story.chain.out("measure-extension");
    let previous = story.chain.head()?;
    problems(commands::amend(
        &story.chain.history,
        &previous,
        &batch,
        &review_path,
        &out,
        &story.chain.live(&out),
    ))?;
    let original = story
        .seed
        .iter()
        .map(|row| &row.witness)
        .collect::<std::collections::BTreeSet<_>>();
    let extension = rows(&out.join("checkpoint-02"))?
        .into_iter()
        .filter(|row| !original.contains(&row.witness))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: extension.len(), want: 1);
    sim_assert_eq!(have: story.chain.amend(&extension)??, want: Verdict::Pass);
    let registry3 = registry(
        &root.join("registry-3"),
        &story.helm,
        &[
            literal(),
            direct_abort(),
            static_alias(),
            snapshot(),
            write_alias(),
        ],
    )?;
    let candidate = capture(
        &root.join("discover-alias"),
        TIGHTENED,
        &registry3.root,
        &story.wrong,
        &story.helm,
    )?;
    let (_, verdict) = story.chain.compare(
        "discover-alias",
        &story.main,
        &candidate,
        &story.seed,
        None,
        &story.certificates,
    )?;
    problems(verdict)?;
    sim_assert_eq!(have: story.chain.amend(&[])??, want: Verdict::Pass);
    sim_assert_eq!(have: commands::verify_history(&story.chain.history, &story.chain.head()?)?, want: Verdict::Pass);
    Ok(())
}
