//! Pinned Helm receipt for placements after the release namespace and split verdicts.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::acceptance::adjudicate::{
    FailureKind, HelmFlipVerdict, NAMESPACE_UNCERTAINTY, adjudicate_flip,
};
use helm_schema_test_support::acceptance::discovery::ProfileSchemas;
use helm_schema_test_support::helm::adjudication::{
    KUBERNETES_RELEASE, OfflineKubernetesValidator, PinnedHelmChart, RENDER_NAMESPACE,
    SECOND_RENDER_NAMESPACE,
};
use helm_schema_test_support::helm::invocation::{HelmRunner, preserve_failure};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[test]
#[ignore = "requires the pinned Helm 4.2.3 engine"]
fn two_namespace_adjudication_has_a_helm_4_2_3_receipt() -> eyre::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/namespace-adjudication");
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, &source)?;
    let kubernetes = OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        KUBERNETES_RELEASE,
    )?;
    let first = chart.adjudicate(&json!({}))?;
    let second = chart.second_namespace().adjudicate(&json!({}))?;
    assert!(first.rendered.success());
    assert!(second.rendered.success());
    assert!(
        String::from_utf8(first.rendered.stdout)?
            .contains(&format!("namespace: {RENDER_NAMESPACE}"))
    );
    assert!(
        String::from_utf8(second.rendered.stdout)?
            .contains(&format!("namespace: {SECOND_RENDER_NAMESPACE}"))
    );
    assert_ne!(first.rendered.record.key, second.rendered.record.key);
    let typed = json!({"properties": {"payload": {"type": "string"}}});
    let profiles = ProfileSchemas::compile(&typed, &json!({}), json!({"payload": "valid"}))?;
    let (equal, _) = adjudicate_flip(
        &chart,
        &json!({"payload": true}),
        &profiles,
        &kubernetes,
        None,
    )?;
    sim_assert_eq!(have: equal, want: HelmFlipVerdict::CandidateAcceptsHelmAborts);
    let excludes_split = json!({"properties": {"payload": {"not": {"const": "split"}}}});
    let profiles =
        ProfileSchemas::compile(&excludes_split, &json!({}), json!({"payload": "valid"}))?;
    let (split, evidence) = adjudicate_flip(
        &chart,
        &json!({"payload": "split"}),
        &profiles,
        &kubernetes,
        None,
    )?;
    sim_assert_eq!(have: split, want: HelmFlipVerdict::EnvironmentDependentAcceptance(vec![NAMESPACE_UNCERTAINTY.into()]));
    let profiles =
        ProfileSchemas::compile(&json!({}), &excludes_split, json!({"payload": "valid"}))?;
    let err = adjudicate_flip(
        &chart,
        &json!({"payload": "split"}),
        &profiles,
        &kubernetes,
        None,
    )
    .err()
    .ok_or_eyre("namespace-dependent tightening was counted as matched")?;
    sim_assert_eq!(have: FailureKind::of(&err), want: FailureKind::UnattributedRejection);
    eprintln!(
        "Helm 4.2.3 namespace receipt: {}",
        preserve_failure(&evidence)
            .join("namespace-verdicts.json")
            .display()
    );
    Ok(())
}

#[test]
#[ignore = "requires the pinned Helm 4.2.3 engine"]
fn namespace_dependent_lint_acceptances_remain_undecided() -> eyre::Result<()> {
    use helm_schema_test_support::acceptance::adjudicate::adjudicate_lint_flip;
    use helm_schema_test_support::acceptance::screen::{
        LintSchemas, ScreenedDocument, ScreenedProbe,
    };
    use test_util::helm_values::{AcceptanceDocument, AcceptanceInputs};

    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/namespace-adjudication");
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, &source)?;
    let kubernetes = OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        KUBERNETES_RELEASE,
    )?;
    let baseline = json!({"properties": {"payload": {"not": {"const": "split"}}}});
    let schemas = LintSchemas::compile(&baseline, &json!({}), None)?;
    let inputs = AcceptanceInputs::load(&source)?;
    let overlay = json!({"payload": "split"});
    for kind in AcceptanceDocument::ALL {
        let document = ScreenedDocument {
            kind,
            lint_control: false,
            values: inputs.compose(overlay.clone(), kind)?.root,
            defaults: Some(inputs.compose(json!({}), kind)?.root),
            kubernetes_version: "1.29.0".into(),
            origins: vec!["namespace-receipt".into()],
            values_file: serde_json::to_vec(&overlay)?,
            control_overlays: vec![json!({})],
        };
        let probe = ScreenedProbe {
            name: format!("namespace-dependent-{kind:?}"),
            overlay: overlay.clone(),
            document: Some(document.clone()),
        };
        let (verdict, _) = adjudicate_lint_flip(&chart, &probe, &document, &schemas, &kubernetes)?;
        sim_assert_eq!(have: verdict, want: HelmFlipVerdict::EnvironmentDependentAcceptance(vec![NAMESPACE_UNCERTAINTY.into()]));
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Helm 4.2.3 engine"]
fn resident_namespace_processes_do_not_leak_between_requests() -> eyre::Result<()> {
    use helm_schema_test_support::helm::invocation::{Cacheability, TemplateRequest};
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/namespace-adjudication");
    let runner = HelmRunner::shared()?;
    let chart = PinnedHelmChart::prepare(runner, &source)?;
    let case = test_util::scratch::ScratchDir::new("namespace-server-isolation")?;
    let policy = Cacheability::bypass("exercise live resident reuse across namespaces".into());
    for namespace in [
        RENDER_NAMESPACE,
        SECOND_RENDER_NAMESPACE,
        RENDER_NAMESPACE,
        SECOND_RENDER_NAMESPACE,
    ] {
        let request = TemplateRequest {
            namespace,
            chart: chart.render_tree(),
            values: b"{}",
            kubernetes_version: "1.29.0",
        };
        let render = runner.template(&request, case.path(), "render", &policy)?;
        assert!(render.success());
        assert!(String::from_utf8(render.stdout)?.contains(&format!("namespace: {namespace}")));
        // Lint reports Helm's fail as a diagnostic even when its overall exit succeeds.
        let request = TemplateRequest {
            values: br#"{"payload":"split"}"#,
            ..request
        };
        let lint = runner.lint(&request, case.path(), "lint", &policy)?;
        let log = String::from_utf8(lint.stdout)? + &String::from_utf8(lint.stderr)?;
        sim_assert_eq!(have: log.contains("payload depends on the release namespace"), want: namespace == RENDER_NAMESPACE);
    }
    Ok(())
}
