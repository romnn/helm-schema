//! Generation-decision explanations through the public session API.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Barrier, Mutex};
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::explain::{
    BaseOwner, BaseOwnerDecision, BaseOwnerRule, ConditionalBaseEffect, ContainmentCheck,
    ContainmentDecision, ContainmentShortCircuit, EmissionOrigin, ExplainFormat, ImplicationRef,
    IndependentQualification, JsonSchemaType, PathGenerationDecision, QualifiedContract,
};
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use helm_schema_test_support::generate::generate_options;
use helm_schema_test_support::registry::ChartRecipe;
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

/// The span `helm_schema_gen` opens once per emitter run.
const GENERATION_SPAN: &str = "generate_values_schema_with_report";

#[path = "../examples/explain_values.rs"]
#[allow(
    dead_code,
    reason = "the example's `main` is its process entry; tests call `run`"
)]
mod explain_values_example;

/// Templates of the literal-guard chart: each branch guards `name` with a
/// literal whose type or spelling a text rendering could lose.
const LITERAL_TEMPLATES: [(&str, &str); 4] = [
    ("mode", r#"eq .Values.mode "false""#),
    ("flag", "eq .Values.flag false"),
    ("level", r#"eq .Values.level "1""#),
    ("separator", r#"eq .Values.separator "a\"b\nc""#),
];

/// The paths the literal-guard chart reads.
const LITERAL_PATHS: [&str; 5] = ["name", "mode", "flag", "level", "separator"];

fn corpus_session(chart: &'static str) -> AnalysisSession {
    AnalysisSession::new(generate_options(&ChartRecipe::corpus(
        chart,
        SchemaProfile::Full,
    )))
}

/// A parent whose dependency splices its whole values root, and a parent
/// path read only through `dig`.
fn dependency_root_session() -> eyre::Result<AnalysisSession> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: child
                version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "customRules: {}\n")?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: root
            data:
              for: {{ dig "Alert" "for" "15m" .Values.customRules | quote }}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        indoc! {"
            feature:
              enabled: false
              name: x
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: child
            data:
              values: |
            {{ toYaml .Values | indent 4 }}
            {{- if .Values.feature.enabled }}
              name: {{ .Values.feature.name | quote }}
            {{- end }}
        "},
    )?;
    Ok(AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            k8s_schema_cache_dir: Some(test_util::cold_provider_cache_root("k8s")?),
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            disable_k8s_schemas: true,
            ..Default::default()
        },
    }))
}

/// The literal-guard chart, with its templates created in declaration
/// order or reversed.
fn literal_chart_session(reversed: bool) -> eyre::Result<AnalysisSession> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: literals
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {r#"
            name: demo
            mode: "off"
            flag: true
            level: "2"
            separator: plain
        "#},
    )?;
    let mut templates = LITERAL_TEMPLATES.to_vec();
    if reversed {
        templates.reverse();
    }
    for (name, condition) in templates {
        test_util::write(
            &chart_dir.join(format!("templates/{name}.yaml"))?,
            formatdoc! {"
                {{{{- if {condition} }}}}
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: {name}
                data:
                  value: {{{{ .Values.name | quote }}}}
                {{{{- end }}}}
            "},
        )?;
    }
    Ok(AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            k8s_schema_cache_dir: Some(test_util::cold_provider_cache_root("k8s")?),
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            disable_k8s_schemas: true,
            ..Default::default()
        },
    }))
}

/// Every JSON and text report of the literal-guard chart, in `paths` order.
fn literal_reports(session: &AnalysisSession, paths: &[&str]) -> eyre::Result<Vec<String>> {
    let mut reports = Vec::new();
    for path in paths {
        reports.push(session.explain_generation(path, ExplainFormat::Json)?);
        reports.push(session.explain_generation(path, ExplainFormat::Text)?);
    }
    Ok(reports)
}

/// Runs the `explain_values` example in JSON mode over the provider bundle.
struct ExampleOutput {
    stdout: String,
    stderr: String,
}

fn run_explain_values_example(
    chart: &str,
    paths: &[&str],
    raw_contract: bool,
) -> eyre::Result<ExampleOutput> {
    let args = std::iter::once(test_util::workspace_testdata().join("charts").join(chart))
        .map(|chart| chart.to_string_lossy().into_owned())
        .chain(paths.iter().map(|path| (*path).to_string()))
        .collect::<Vec<_>>();
    let options = explain_values_example::ExplainOptions {
        json: true,
        raw_contract,
        provider_bundle: test_util::workspace_testdata().join("provider-bundle"),
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    explain_values_example::run(&args, &options, &mut stdout, &mut stderr)
        .map_err(|error| eyre::eyre!("explain_values failed: {error}"))?;
    Ok(ExampleOutput {
        stdout: String::from_utf8(stdout)?,
        stderr: String::from_utf8(stderr)?,
    })
}

fn expected_report(name: &str) -> eyre::Result<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/explain")
        .join(name);
    std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))
}

fn decision(session: &AnalysisSession, path: &str) -> eyre::Result<PathGenerationDecision> {
    session
        .explain(path)?
        .generation
        .ok_or_eyre(format!("no generation decisions for {path}"))
}

fn types<const N: usize>(types: [JsonSchemaType; N]) -> BTreeSet<JsonSchemaType> {
    BTreeSet::from(types)
}

fn requirement(index: usize) -> ImplicationRef {
    ImplicationRef {
        origin: EmissionOrigin::RequirementImplication,
        index,
    }
}

fn backprojection(index: usize) -> ImplicationRef {
    ImplicationRef {
        origin: EmissionOrigin::Backprojection,
        index,
    }
}

#[test]
fn redis_exporter_service_type_report_is_pinned() -> eyre::Result<()> {
    let session = corpus_session("prometheus-redis-exporter");

    sim_assert_eq!(
        have: session.explain_generation("service.type", ExplainFormat::Json)?,
        want: expected_report("prometheus-redis-exporter.service.type.json")?
    );
    sim_assert_eq!(
        have: session.explain_generation(".Values.service.type", ExplainFormat::Text)?,
        want: expected_report("prometheus-redis-exporter.service.type.txt")?
    );
    Ok(())
}

#[test]
fn jaeger_common_annotations_report_is_pinned() -> eyre::Result<()> {
    let session = corpus_session("jaeger");

    sim_assert_eq!(
        have: session.explain_generation("commonAnnotations", ExplainFormat::Json)?,
        want: expected_report("jaeger.commonAnnotations.json")?
    );
    sim_assert_eq!(
        have: session.explain_generation("commonAnnotations", ExplainFormat::Text)?,
        want: expected_report("jaeger.commonAnnotations.txt")?
    );
    Ok(())
}

#[test]
fn unobserved_path_reports_its_status() -> eyre::Result<()> {
    let session = dependency_root_session()?;

    sim_assert_eq!(
        have: session.explain_generation("absent.path", ExplainFormat::Json)?,
        want: indoc! {r#"
            {
              "coverage": {
                "emission": "not_recorded",
                "generation": "recorded",
                "inputs": "not_recorded",
                "interpreter": "not_recorded",
                "providers": "not_recorded",
                "signals": "not_recorded",
                "sources": "not_recorded"
              },
              "format_version": 1,
              "generation": null,
              "path": "absent.path",
              "references": {
                "overlays": [],
                "requirement_implications": []
              },
              "scope": "generation_decisions",
              "status": "not_observed"
            }
        "#}
    );
    sim_assert_eq!(have: session.explain("absent.path")?.generation, want: None);
    Ok(())
}

#[test]
fn explanation_adds_the_recorded_decision_to_the_contract_evidence() -> eyre::Result<()> {
    let session = corpus_session("prometheus-redis-exporter");
    let explanation = session.explain("service.type")?;
    let recorded = session
        .resolved_contract()?
        .generation_decisions
        .path(&helm_schema_core::ValuesPath::parse("service.type"))
        .cloned();

    assert!(
        !explanation.exact_uses.is_empty(),
        "expected exact uses beside the generation decision: {explanation:#?}"
    );
    sim_assert_eq!(have: explanation.generation, want: recorded);
    Ok(())
}

#[test]
fn base_owner_decisions_cover_every_owner() -> eyre::Result<()> {
    let redis = corpus_session("prometheus-redis-exporter");
    let nack = corpus_session("nack");
    let dependency_root = dependency_root_session()?;
    let cases = [
        (
            &redis,
            "service.type",
            BaseOwner::Resolved,
            BaseOwnerRule::Unconditional,
            QualifiedContract::Discarded,
        ),
        (
            &redis,
            "image.tag",
            BaseOwner::Serialized,
            BaseOwnerRule::SerializedRender,
            QualifiedContract::NotQualified,
        ),
        (
            &redis,
            "customLabels",
            BaseOwner::ResolvedUnclosed,
            BaseOwnerRule::ConditionalTarget,
            QualifiedContract::Discarded,
        ),
        (
            &redis,
            "auth.redisPassword",
            BaseOwner::Empty,
            BaseOwnerRule::ConditionalTarget,
            QualifiedContract::NotQualified,
        ),
        (
            &dependency_root,
            "child",
            BaseOwner::UnknownObject,
            BaseOwnerRule::PathlessDependencyRoot,
            QualifiedContract::NotQualified,
        ),
        (
            &nack,
            "jetstream.image.pullPolicy",
            BaseOwner::IndependentContract,
            BaseOwnerRule::OwningAncestor,
            QualifiedContract::Retained,
        ),
        (
            &redis,
            "image.pullSecrets.*",
            BaseOwner::OwnedByAncestor,
            BaseOwnerRule::GuardedCollectionMember,
            QualifiedContract::NotQualified,
        ),
    ];

    for (session, path, owner, rule, qualified_contract) in cases {
        sim_assert_eq!(
            have: decision(session, path)?.base_owner,
            want: Some(BaseOwnerDecision {
                owner,
                rule,
                qualified_contract,
            }),
            "{path}"
        );
    }
    Ok(())
}

#[test]
fn containment_checks_record_operands_or_the_short_circuit() -> eyre::Result<()> {
    let redis = corpus_session("prometheus-redis-exporter");
    let dependency_root = dependency_root_session()?;
    let not_evaluated = |implication, short_circuit_reason, base_effect| ContainmentDecision {
        implication,
        check: ContainmentCheck::NotEvaluated {
            short_circuit_reason,
        },
        base_effect,
    };

    // The network-policy port backprojects a non-null requirement that the
    // integer-or-string structural domain does not contain.
    sim_assert_eq!(
        have: decision(&redis, "service.port")?.containment_checks,
        want: vec![
            not_evaluated(
                backprojection(0),
                ContainmentShortCircuit::UnguardedRequirement,
                ConditionalBaseEffect::None,
            ),
            ContainmentDecision {
                implication: backprojection(1),
                check: ContainmentCheck::Evaluated {
                    structural_types: types([JsonSchemaType::Integer, JsonSchemaType::String]),
                    requirement_types: types([
                        JsonSchemaType::Array,
                        JsonSchemaType::Boolean,
                        JsonSchemaType::Integer,
                        JsonSchemaType::Number,
                        JsonSchemaType::Object,
                        JsonSchemaType::String,
                    ]),
                    contains: false,
                },
                base_effect: ConditionalBaseEffect::Own,
            },
        ]
    );
    sim_assert_eq!(
        have: decision(&redis, "service")?.containment_checks,
        want: vec![
            not_evaluated(
                requirement(0),
                ContainmentShortCircuit::IncompleteMemberHost,
                ConditionalBaseEffect::Require,
            ),
            not_evaluated(
                backprojection(0),
                ContainmentShortCircuit::UnguardedRequirement,
                ConditionalBaseEffect::None,
            ),
            not_evaluated(
                backprojection(1),
                ContainmentShortCircuit::EmptyStructuralDomain,
                ConditionalBaseEffect::Own,
            ),
        ]
    );
    sim_assert_eq!(
        have: decision(&redis, "auth")?.containment_checks,
        want: vec![not_evaluated(
            requirement(0),
            ContainmentShortCircuit::UnguardedRequirement,
            ConditionalBaseEffect::None,
        )]
    );
    sim_assert_eq!(
        have: decision(&redis, "image.pullSecrets")?.containment_checks,
        want: vec![not_evaluated(
            requirement(0),
            ContainmentShortCircuit::SelfTruthyGuard,
            ConditionalBaseEffect::Own,
        )]
    );
    sim_assert_eq!(
        have: decision(&redis, "auth.redisPasswordFile")?.containment_checks,
        want: vec![
            not_evaluated(
                requirement(0),
                ContainmentShortCircuit::EmptyStructuralDomain,
                ConditionalBaseEffect::Own,
            ),
            not_evaluated(
                backprojection(0),
                ContainmentShortCircuit::EmptyStructuralDomain,
                ConditionalBaseEffect::Own,
            ),
        ]
    );
    sim_assert_eq!(
        have: decision(&dependency_root, "customRules")?.containment_checks,
        want: vec![not_evaluated(
            requirement(0),
            ContainmentShortCircuit::SelfPresenceTypeArm,
            ConditionalBaseEffect::Own,
        )]
    );
    Ok(())
}

#[test]
fn independent_contract_qualification_is_recorded() -> eyre::Result<()> {
    let redis = corpus_session("prometheus-redis-exporter");
    let qualification = |path| -> eyre::Result<IndependentQualification> {
        Ok(decision(&redis, path)?
            .base
            .ok_or_eyre(format!("{path} was not resolved"))?
            .independent_contract)
    };

    assert!(
        matches!(
            qualification("service.type")?,
            IndependentQualification::Qualified { .. }
        ),
        "service.type has a strict string consumer and a provider slot"
    );
    sim_assert_eq!(
        have: qualification("auth")?,
        want: IndependentQualification::NoIndependentConsumer
    );
    sim_assert_eq!(
        have: qualification("extraArgs.*")?,
        want: IndependentQualification::WildcardPath
    );
    Ok(())
}

/// Counts emitter runs through the span the generator already opens.
struct GenerationRuns(AtomicUsize);

impl tracing::Subscriber for GenerationRuns {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.name() == GENERATION_SPAN
    }

    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        if span.metadata().name() == GENERATION_SPAN {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, _event: &tracing::Event<'_>) {}

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

#[test]
fn concurrent_and_repeated_queries_run_generation_once() -> eyre::Result<()> {
    const THREADS: usize = 4;
    let runs = std::sync::Arc::new(GenerationRuns(AtomicUsize::new(0)));
    let dispatch = tracing::Dispatch::new(std::sync::Arc::clone(&runs));
    let session = corpus_session("prometheus-redis-exporter");
    let start = Barrier::new(THREADS);

    std::thread::scope(|scope| -> eyre::Result<()> {
        let workers = (0..THREADS)
            .map(|_| {
                scope.spawn(|| {
                    tracing::dispatcher::with_default(&dispatch, || {
                        start.wait();
                        session.generated_schema().map(|_| ())
                    })
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker
                .join()
                .map_err(|_| eyre::eyre!("generation worker panicked"))??;
        }
        Ok(())
    })?;
    tracing::dispatcher::with_default(&dispatch, || -> eyre::Result<()> {
        session.explain("service.type")?;
        session.explain("service.type")?;
        session.explain("service.port")?;
        Ok(())
    })?;

    sim_assert_eq!(have: runs.0.load(Ordering::SeqCst), want: 1);
    Ok(())
}

#[test]
fn text_report_preserves_guard_literal_types_and_escaping() -> eyre::Result<()> {
    let session = literal_chart_session(false)?;

    sim_assert_eq!(
        have: session.explain_generation("name", ExplainFormat::Json)?,
        want: expected_report("literals.name.json")?
    );
    sim_assert_eq!(
        have: session.explain_generation("name", ExplainFormat::Text)?,
        want: expected_report("literals.name.txt")?
    );
    Ok(())
}

#[test]
fn jaeger_guard_references_follow_implication_index_order() -> eyre::Result<()> {
    let session = corpus_session("jaeger");
    let json: serde_json::Value = serde_json::from_str(
        &session.explain_generation("commonAnnotations", ExplainFormat::Json)?,
    )?;
    let json_indices = json
        .pointer("/references/requirement_implications")
        .and_then(serde_json::Value::as_array)
        .ok_or_eyre("requirement implication references")?
        .iter()
        .map(|reference| reference.get("index").and_then(serde_json::Value::as_u64))
        .collect::<Option<Vec<_>>>()
        .ok_or_eyre("reference index")?;
    let text = session.explain_generation("commonAnnotations", ExplainFormat::Text)?;
    let text_indices = text
        .split_once("references:")
        .ok_or_eyre("text references section")?
        .1
        .lines()
        .filter_map(|line| line.trim().strip_prefix("index: "))
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()?;

    sim_assert_eq!(have: json_indices, want: (0..12).collect::<Vec<u64>>());
    sim_assert_eq!(have: text_indices, want: (0..12).collect::<Vec<u64>>());
    Ok(())
}

#[test]
fn reports_are_deterministic_across_sessions_discovery_and_concurrent_queries() -> eyre::Result<()>
{
    const THREADS: usize = 4;
    let baseline = literal_reports(&literal_chart_session(false)?, &LITERAL_PATHS)?;
    let mut reversed_paths = LITERAL_PATHS;
    reversed_paths.reverse();
    let mut reversed = literal_reports(&literal_chart_session(true)?, &reversed_paths)?;
    // Undo the query order: reports come in (JSON, text) pairs per path.
    let mut pairs = reversed
        .chunks(2)
        .map(<[String]>::to_vec)
        .collect::<Vec<_>>();
    pairs.reverse();
    reversed = pairs.concat();

    let runs = std::sync::Arc::new(GenerationRuns(AtomicUsize::new(0)));
    let dispatch = tracing::Dispatch::new(std::sync::Arc::clone(&runs));
    let session = literal_chart_session(false)?;
    let start = Barrier::new(THREADS);
    let (session, dispatch, start) = (&session, &dispatch, &start);
    let (json, text) = std::thread::scope(|scope| -> eyre::Result<(String, String)> {
        let query = |format| {
            scope.spawn(move || {
                tracing::dispatcher::with_default(dispatch, || {
                    start.wait();
                    session.explain_generation("name", format)
                })
            })
        };
        let json = query(ExplainFormat::Json);
        let text = query(ExplainFormat::Text);
        let resolved = scope.spawn(|| {
            tracing::dispatcher::with_default(dispatch, || {
                start.wait();
                session.resolved_contract().map(|_| ())
            })
        });
        let generated = scope.spawn(|| {
            tracing::dispatcher::with_default(dispatch, || {
                start.wait();
                session.generated_schema().map(|_| ())
            })
        });
        let panicked = |_| eyre::eyre!("query worker panicked");
        resolved.join().map_err(panicked)??;
        generated.join().map_err(panicked)??;
        Ok((
            json.join().map_err(panicked)??,
            text.join().map_err(panicked)??,
        ))
    })?;

    sim_assert_eq!(have: reversed, want: baseline.clone());
    sim_assert_eq!(
        have: vec![json, text],
        want: baseline.get(..2).map(<[String]>::to_vec).unwrap_or_default()
    );
    sim_assert_eq!(have: runs.0.load(Ordering::SeqCst), want: 1);
    Ok(())
}

#[test]
fn explain_values_json_stdout_is_one_report() -> eyre::Result<()> {
    let expected = expected_report("schema-emission-controls.version.json")?;
    for raw_contract in [false, true] {
        let output =
            run_explain_values_example("schema-emission-controls", &["version"], raw_contract)?;
        let document: serde_json::Value = serde_json::from_str(&output.stdout)
            .wrap_err_with(|| format!("stdout is one JSON document: {}", output.stdout))?;

        sim_assert_eq!(have: output.stdout, want: expected.clone());
        sim_assert_eq!(
            have: document.get("path").and_then(serde_json::Value::as_str),
            want: Some("version")
        );
        assert!(
            output.stderr.contains("terminal: "),
            "terminal clauses go to stderr: {}",
            output.stderr
        );
        sim_assert_eq!(
            have: output.stderr.contains("contract: "),
            want: raw_contract
        );
    }
    Ok(())
}

#[test]
fn explain_values_json_stdout_is_parseable_for_terminal_and_multiple_paths() -> eyre::Result<()> {
    let pinned = expected_report("prometheus-redis-exporter.service.type.json")?;
    let single = run_explain_values_example("prometheus-redis-exporter", &["service.type"], false)?;
    let several = run_explain_values_example(
        "prometheus-redis-exporter",
        &["service.type", "service.port"],
        false,
    )?;
    let reports: Vec<serde_json::Value> = serde_json::from_str(&several.stdout)
        .wrap_err_with(|| format!("stdout is one JSON array: {}", several.stdout))?;

    sim_assert_eq!(have: single.stdout, want: pinned.clone());
    sim_assert_eq!(have: reports.len(), want: 2);
    sim_assert_eq!(
        have: reports.first().cloned(),
        want: Some(serde_json::from_str::<serde_json::Value>(&pinned)?)
    );
    sim_assert_eq!(
        have: reports
            .get(1)
            .and_then(|report| report.get("path"))
            .and_then(serde_json::Value::as_str),
        want: Some("service.port")
    );
    Ok(())
}

/// On the first generation span of its thread, waits until another thread
/// has entered `generated_schema` (and so waits on the resolved contract this
/// thread is initializing), then queries `generated_schema` itself.
struct ReenteringCallback {
    session: std::sync::Arc<AnalysisSession>,
    initializing: Mutex<mpsc::Sender<()>>,
    other_entered: Mutex<mpsc::Receiver<()>>,
    fired: std::sync::atomic::AtomicBool,
    outcome: Mutex<Option<Result<(), helm_schema::CliError>>>,
}

impl tracing::Subscriber for ReenteringCallback {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.name() == GENERATION_SPAN
    }

    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        if span.metadata().name() == GENERATION_SPAN && !self.fired.swap(true, Ordering::SeqCst) {
            let _ = lock(&self.initializing).send(());
            let entered = lock(&self.other_entered).recv_timeout(Duration::from_secs(30));
            // The other thread now holds the generated-schema cache and is
            // about to wait on the resolved contract this thread holds.
            std::thread::sleep(Duration::from_millis(500));
            let outcome = entered
                .map_err(|_| helm_schema::CliError::NoChartsDiscovered)
                .and_then(|()| self.session.generated_schema().map(|_| ()));
            *lock(&self.outcome) = Some(outcome);
        }
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, _event: &tracing::Event<'_>) {}

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn session_callback_reentry_across_phases_fails_without_deadlock() -> eyre::Result<()> {
    let session = std::sync::Arc::new(literal_chart_session(false)?);
    let (initializing_sender, initializing) = mpsc::channel();
    let (entered, other_entered) = mpsc::channel();
    let callback = std::sync::Arc::new(ReenteringCallback {
        session: std::sync::Arc::clone(&session),
        initializing: Mutex::new(initializing_sender),
        other_entered: Mutex::new(other_entered),
        fired: std::sync::atomic::AtomicBool::new(false),
        outcome: Mutex::new(None),
    });
    let dispatch = tracing::Dispatch::new(std::sync::Arc::clone(&callback));
    let (resolved_sender, resolved) = mpsc::channel();
    let resolving_session = std::sync::Arc::clone(&session);
    std::thread::spawn(move || {
        let outcome = tracing::dispatcher::with_default(&dispatch, || {
            resolving_session.resolved_contract().map(|_| ())
        });
        let _ = resolved_sender.send(outcome.map_err(|error| error.to_string()));
    });
    initializing
        .recv_timeout(Duration::from_secs(60))
        .wrap_err("resolving thread never entered generation")?;
    let (generated_sender, generated) = mpsc::channel();
    let generating_session = std::sync::Arc::clone(&session);
    std::thread::spawn(move || {
        let _ = entered.send(());
        let outcome = generating_session.generated_schema().map(|_| ());
        let _ = generated_sender.send(outcome.map_err(|error| error.to_string()));
    });

    let resolved = resolved
        .recv_timeout(Duration::from_secs(60))
        .wrap_err("the resolving query deadlocked")?;
    let generated = generated
        .recv_timeout(Duration::from_secs(60))
        .wrap_err("the generating query deadlocked")?;
    let reentry = lock(&callback.outcome).take();

    assert!(
        matches!(
            reentry,
            Some(Err(helm_schema::CliError::ReentrantSessionQuery {
                requested: "generated schema",
                initializing: "resolved contract",
            }))
        ),
        "the callback query must fail before blocking: {reentry:?}"
    );
    sim_assert_eq!(have: resolved, want: Ok(()));
    sim_assert_eq!(have: generated, want: Ok(()));
    Ok(())
}
