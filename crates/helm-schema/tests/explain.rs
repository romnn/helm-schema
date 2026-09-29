//! Generation-decision explanations through the public session API.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};

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
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

/// The span `helm_schema_gen` opens once per emitter run.
const GENERATION_SPAN: &str = "generate_values_schema_with_report";

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
                "overlays": {},
                "requirement_implications": {}
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
