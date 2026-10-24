//! Regression controls for dependency activation and document-specific lint evidence.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::{Value, json};
use test_util::helm_values::{AcceptanceDocument, AcceptanceInputs, acceptance_values};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use super::lint_probes::{
    ContextRoster, ContextRow, ImportedRoster, LintSchemas, ScreenRequest, screen,
};
use helm_schema_test_support::helm::invocation::tree_sha256;
use helm_schema_test_support::source_digest::sha256_hex;

fn corpus_chart(name: &str) -> PathBuf {
    test_util::workspace_testdata().join("charts").join(name)
}

fn document_group_chart(source: &Path) -> eyre::Result<super::LiveChart> {
    fs::write(
        source.join("Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: grouped-observation
            version: 1.0.0
        "},
    )?;
    fs::write(source.join("values.yaml"), "enabled: false\n")?;
    fs::create_dir(source.join("templates"))?;
    // Random annotations force fresh lint observations without changing resource validity.
    fs::write(
        source.join("templates/config.yaml"),
        indoc::indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: grouped-observation
              annotations:
                nonce: {{ randAlphaNum 8 | quote }}
            data:
              token: "ok"
        "#},
    )?;
    let baseline = json!({"properties": {"enabled": {"const": false}}});
    let candidate = json!({});
    let overlay = json!({"enabled": true});
    let inputs = AcceptanceInputs::load(source)?;
    let mut flips = Vec::new();
    for kind in [
        AcceptanceDocument::Template,
        AcceptanceDocument::LintRaw,
        AcceptanceDocument::LintCoalescedTwice,
    ] {
        flips.push(super::ScreenedProbe {
            name: format!("document={kind:?}"),
            overlay: overlay.clone(),
            document: Some(super::lint_probes::ScreenedDocument {
                kind,
                lint_control: false,
                values: inputs.compose(overlay.clone(), kind)?.root,
                defaults: Some(inputs.compose(json!({}), kind)?.root),
                kubernetes_version: "1.29.0".into(),
                origins: vec![format!("origin-{kind:?}")],
                values_file: serde_json::to_vec(&overlay)?,
                control_overlays: vec![json!({"context": format!("{kind:?}")})],
            }),
        });
    }
    Ok(super::LiveChart {
        label: "grouped-observation".into(),
        path: source.to_owned(),
        profiles: super::ProfileSchemas::compile(&baseline, &candidate, json!({"enabled": false}))?,
        flips,
        lint_schemas: LintSchemas::compile(&baseline, &candidate, None)?,
        helm: std::sync::Arc::default(),
        annotation: None,
    })
}

#[cfg(unix)]
#[test]
fn nondeterministic_controls_are_observed_again_after_the_threshold() -> eyre::Result<()> {
    use super::helm_invocation::{HelmRunner, find_helm};
    use super::lint_adjudication::{
        ControlObservations, GroupObservations, adjudicate_lint_observation,
    };
    use helm_schema_test_support::helm::adjudication::LintProbeRequest;
    use std::os::unix::fs::PermissionsExt as _;

    let source = ScratchDir::new("control-threshold")?;
    let mut live = document_group_chart(source.path())?;
    fs::write(
        source.path().join("templates/config.yaml"),
        indoc::indoc! {r#"
        {{ $_ := now }}
        {{ if and (not .Values.enabled) .Values.expired }}{{ fail "control expired" }}{{ end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: threshold
    "#},
    )?;
    // The test double advances the template's clock input through a file, without sleeping.
    let clock = ScratchDir::new("control-clock")?;
    let expired = clock.path().join("expired");
    let program = clock.path().join("helm");
    fs::write(
        &program,
        indoc::formatdoc! {r#"
        #!/bin/sh
        if [ "$1" != version ] && [ -f '{expired}' ]; then
          set -- "$@" --set expired=true
        fi
        exec '{helm}' "$@"
    "#, expired = expired.display(), helm = find_helm()?.display()},
    )?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    let runner = Box::leak(Box::new(HelmRunner::with_program(
        &clock.path().join("runner"),
        true,
        program,
    )?));
    let chart = super::PinnedHelmChart::prepare(runner, source.path())?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let controls = ControlObservations::default();
    let probe = live.flips.first_mut().ok_or_eyre("document absent")?;
    for group in 0..2 {
        let document = probe.document.as_mut().ok_or_eyre("document absent")?;
        document.kind = AcceptanceDocument::LintRaw;
        document.values = json!({"enabled":true,"group":group});
        document.values_file = serde_json::to_vec(&document.values)?;
        document.control_overlays = vec![json!({})];
        let lint = chart.adjudicate_lint(&LintProbeRequest {
            values: &document.values_file,
            kubernetes_version: &document.kubernetes_version,
            baseline_schema: &live.lint_schemas.baseline,
            candidate_schema: &live.lint_schemas.candidate,
        })?;
        let document = probe.document.as_ref().ok_or_eyre("document absent")?;
        let mut observations = GroupObservations::new(&chart, document, lint, &controls);
        let result = adjudicate_lint_observation(
            probe,
            document,
            &live.lint_schemas,
            &kubernetes,
            &mut observations,
        );
        if group == 0 {
            let (verdict, _) = result?;
            sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
            fs::write(&expired, "threshold reached")?;
        } else {
            let err = result
                .err()
                .ok_or_eyre("expired control certified another group's acceptance")?;
            assert!(
                err.to_string()
                    .contains("unusable defaults/context render control"),
                "{err:#}"
            );
        }
    }
    Ok(())
}

#[test]
fn live_document_group_shares_one_fresh_lint_observation_and_keeps_origins() -> eyre::Result<()> {
    let source = ScratchDir::new("grouped-lint-observation")?;
    let live = document_group_chart(source.path())?;
    let chart = live
        .helm
        .chart(&live.path)
        .map_err(|err| eyre::eyre!("{err}"))?;
    assert!(matches!(
        chart.render_cacheability().replay(),
        super::helm_invocation::Replay::Bypass(_)
    ));
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let groups = super::probe_groups(&live);
    sim_assert_eq!(have: &groups, want: &vec![vec![0, 1, 2]]);
    let group = groups.first().ok_or_eyre("document group absent")?;
    let results = super::adjudicate_probe_group(&live, group, Some(&kubernetes));
    sim_assert_eq!(have: results.iter().map(|result| result.flip).collect::<Vec<_>>(), want: vec![0, 1, 2]);
    for (result, probe) in results.into_iter().zip(&live.flips) {
        let document = probe.document.as_ref().ok_or_eyre("document absent")?;
        sim_assert_eq!(have: result.document_kind, want: Some(document.kind));
        let (verdict, directory) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
        sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
        let evidence: Value =
            serde_json::from_slice(&fs::read(directory.join("schema-verdicts.json"))?)?;
        sim_assert_eq!(have: &evidence["origins"], want: &json!(document.origins));
        sim_assert_eq!(have: &evidence["kubernetes_contexts"][0]["overlay"], want: &document.control_overlays[0]);
    }
    let invocations = chart.invocations();
    let payload_bytes = u64::try_from(serde_json::to_vec(&json!({"enabled": true}))?.len())?;
    let payload_lints: Vec<_> = invocations
        .iter()
        .filter(|record| record.stage.starts_with("lint-") && record.input_bytes == payload_bytes)
        .map(|record| record.stage.as_str())
        .collect();
    sim_assert_eq!(have: payload_lints, want: vec!["lint-baseline", "lint-candidate", "lint-schema-free"]);
    // Defaults are observed once; the three distinct contexts remain separate observations.
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-baseline").count(), want: 1);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-candidate").count(), want: 1);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-schema-free").count(), want: 5);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "render").count(), want: 5);

    // Nondeterministic controls are observed again with each independent payload group.
    for result in super::adjudicate_probe_group(&live, group, Some(&kubernetes)) {
        let (verdict, _) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
        sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
    }
    let invocations = chart.invocations();
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-baseline").count(), want: 2);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-candidate").count(), want: 2);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-schema-free").count(), want: 10);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "render").count(), want: 10);
    Ok(())
}

#[test]
fn retained_controls_reload_large_observations_from_evidence() -> eyre::Result<()> {
    let source = ScratchDir::new("control-retained-bytes")?;
    let mut live = document_group_chart(source.path())?;
    let padding = "x".repeat(3 * 1024 * 1024);
    fs::write(
        source.path().join("values.yaml"),
        serde_json::to_vec(&json!({"enabled":false,"padding":padding}))?,
    )?;
    fs::write(
        source.path().join("templates/config.yaml"),
        indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: retained-bytes
        data:
          padding: {{ .Values.padding | quote }}
    "},
    )?;
    for probe in &mut live.flips {
        probe
            .document
            .as_mut()
            .ok_or_eyre("document absent")?
            .control_overlays = vec![json!({})];
    }
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let first = super::adjudicate_probe_group(&live, &[0], Some(&kubernetes))
        .pop()
        .ok_or_eyre("outcome absent")?;
    let (verdict, directory) = first.outcome.map_err(|err| eyre::eyre!("{err}"))?;
    sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
    let pool = directory
        .parent()
        .and_then(Path::parent)
        .ok_or_eyre("probe pool absent")?;
    let mut removed = 0;
    // Losing retained evidence must fail closed instead of using an in-memory copy of its megabytes.
    for entry in fs::read_dir(pool)? {
        let path = entry?.path();
        let render = path.join("render.yaml");
        if render.is_file() && fs::read(path.join("values.json"))? == b"{}" {
            assert!(fs::metadata(&render)?.len() > 3 * 1024 * 1024);
            fs::remove_file(render)?;
            removed += 1;
        }
    }
    sim_assert_eq!(have: removed, want: 1);
    let second = super::adjudicate_probe_group(&live, &[0], Some(&kubernetes))
        .pop()
        .ok_or_eyre("outcome absent")?;
    assert!(
        second.outcome.is_err(),
        "retained control survived without its disk observation"
    );
    Ok(())
}

#[test]
fn live_group_control_failure_affects_only_its_applicable_document() -> eyre::Result<()> {
    let source = ScratchDir::new("grouped-control-failure")?;
    let mut live = document_group_chart(source.path())?;
    fs::write(
        source.path().join("templates/config.yaml"),
        indoc::indoc! {r#"
            {{ if eq (.Values.context | default "") "bad" }}{{ fail "bad context" }}{{ end }}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: grouped-observation
              annotations:
                nonce: {{ randAlphaNum 8 | quote }}
            data:
              token: "ok"
        "#},
    )?;
    for probe in &mut live.flips {
        let document = probe.document.as_mut().ok_or_eyre("document absent")?;
        document.control_overlays = vec![json!({"context":
            if document.kind == AcceptanceDocument::LintRaw { "bad" } else { "shared" }
        })];
    }
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let results = super::adjudicate_probe_group(&live, &[0, 1, 2], Some(&kubernetes));
    sim_assert_eq!(have: results.iter().map(|result| result.flip).collect::<Vec<_>>(), want: vec![0, 1, 2]);
    for (result, probe) in results.into_iter().zip(&live.flips) {
        let document = probe.document.as_ref().ok_or_eyre("document absent")?;
        if document.kind == AcceptanceDocument::LintRaw {
            let err = result
                .outcome
                .err()
                .ok_or_eyre("bad context was accepted")?;
            assert!(
                err.contains("unusable defaults/context render control"),
                "{err}"
            );
        } else {
            let (verdict, directory) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
            sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
            let evidence: Value =
                serde_json::from_slice(&fs::read(directory.join("schema-verdicts.json"))?)?;
            sim_assert_eq!(have: &evidence["origins"], want: &json!(document.origins));
            sim_assert_eq!(have: &evidence["kubernetes_contexts"][0]["overlay"], want: &json!({"context":"shared"}));
            let overlays: Vec<_> = evidence["controls"]
                .as_array()
                .ok_or_eyre("controls absent")?
                .iter()
                .map(|control| control["overlay"].clone())
                .collect();
            sim_assert_eq!(have: overlays, want: vec![json!({"context":"shared"}), json!({})]);
        }
    }
    let chart = live
        .helm
        .chart(&live.path)
        .map_err(|err| eyre::eyre!("{err}"))?;
    let invocations = chart.invocations();
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-schema-free").count(), want: 4);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "render").count(), want: 4);
    Ok(())
}

#[test]
fn live_group_payload_controls_share_only_exact_values_bytes() -> eyre::Result<()> {
    let source = ScratchDir::new("grouped-control-bytes")?;
    let mut live = document_group_chart(source.path())?;
    for probe in &mut live.flips {
        probe
            .document
            .as_mut()
            .ok_or_eyre("document absent")?
            .control_overlays = vec![json!({"enabled": true})];
    }
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    for pretty in [false, true] {
        if pretty {
            for probe in &mut live.flips {
                probe
                    .document
                    .as_mut()
                    .ok_or_eyre("document absent")?
                    .values_file = serde_json::to_vec_pretty(&json!({"enabled": true}))?;
            }
        }
        for result in super::adjudicate_probe_group(&live, &[0, 1, 2], Some(&kubernetes)) {
            let (verdict, _) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
            sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
        }
        let chart = live
            .helm
            .chart(&live.path)
            .map_err(|err| eyre::eyre!("{err}"))?;
        let invocations = chart.invocations();
        // Compact payload and context share one observation; pretty payload bytes do not.
        let expected = if pretty { 5 } else { 2 };
        sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-schema-free").count(), want: expected);
        sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "render").count(), want: expected);
    }
    Ok(())
}

#[test]
fn repeated_deterministic_documents_keep_fresh_decode_evidence() -> eyre::Result<()> {
    let source = ScratchDir::new("grouped-deterministic-evidence")?;
    let mut live = document_group_chart(source.path())?;
    let template = source.path().join("templates/config.yaml");
    fs::write(
        &template,
        fs::read_to_string(&template)?.replace("{{ randAlphaNum 8 | quote }}", "\"fixed\""),
    )?;
    let probe = live.flips.first_mut().ok_or_eyre("document absent")?;
    probe
        .document
        .as_mut()
        .ok_or_eyre("document absent")?
        .control_overlays = vec![json!({})];
    let document = probe.document.as_ref().ok_or_eyre("document absent")?;
    let chart = live
        .helm
        .chart(&live.path)
        .map_err(|err| eyre::eyre!("{err}"))?;
    assert!(matches!(
        chart.render_cacheability().replay(),
        super::helm_invocation::Replay::Cacheable
    ));
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let mut directories = BTreeSet::new();
    for _ in 0..2 {
        let (verdict, directory) =
            super::adjudicate_lint_flip(chart, probe, document, &live.lint_schemas, &kubernetes)?;
        sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
        let evidence: Value =
            serde_json::from_slice(&fs::read(directory.join("schema-verdicts.json"))?)?;
        sim_assert_eq!(have: &evidence["document_kind"], want: &json!(document.kind));
        sim_assert_eq!(have: evidence["kubernetes_contexts"].as_array().map(Vec::len), want: Some(1));
        sim_assert_eq!(have: &evidence["kubernetes_contexts"][0]["overlay"], want: &json!({}));
        directories.insert(directory);
    }
    // The deterministic lint is reusable, but each independent adjudication owns its decoder files.
    sim_assert_eq!(have: directories.len(), want: 2);
    let invocations = chart.invocations();
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "lint-baseline").count(), want: 1);
    sim_assert_eq!(have: invocations.iter().filter(|record| record.stage == "render").count(), want: 4);
    Ok(())
}

#[test]
fn live_document_groups_keep_exact_bytes_versions_and_template_probes_separate() -> eyre::Result<()>
{
    let source = ScratchDir::new("grouped-lint-identity")?;
    let mut live = document_group_chart(source.path())?;
    let mut different_bytes = live
        .flips
        .first()
        .ok_or_eyre("first document absent")?
        .clone();
    different_bytes
        .document
        .as_mut()
        .ok_or_eyre("document absent")?
        .values_file
        .push(b'\n');
    live.flips.insert(1, different_bytes);
    let mut different_version = live
        .flips
        .first()
        .ok_or_eyre("first document absent")?
        .clone();
    different_version
        .document
        .as_mut()
        .ok_or_eyre("document absent")?
        .kubernetes_version = "1.30.0".into();
    live.flips.insert(3, different_version);
    let mut template = live
        .flips
        .first()
        .ok_or_eyre("first document absent")?
        .clone();
    template.document = None;
    live.flips.insert(2, template.clone());
    live.flips.push(template);
    sim_assert_eq!(have: super::probe_groups(&live), want: vec![vec![0, 3, 5], vec![1], vec![2], vec![4], vec![6]]);
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    std::thread::scope(|scope| -> eyre::Result<()> {
        let first =
            scope.spawn(|| super::adjudicate_probe_group(&live, &[0, 3, 5], Some(&kubernetes)));
        let second = scope.spawn(|| super::adjudicate_probe_group(&live, &[1], Some(&kubernetes)));
        for thread in [first, second] {
            for result in thread
                .join()
                .map_err(|_| eyre::eyre!("adjudication thread failed"))?
            {
                let (verdict, _) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
                sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
            }
        }
        Ok(())
    })?;
    let chart = live
        .helm
        .chart(&live.path)
        .map_err(|err| eyre::eyre!("{err}"))?;
    // Concurrent nondeterministic groups observe their own controls.
    sim_assert_eq!(have: chart.invocations().iter().filter(|record| record.stage == "render").count(), want: 8);
    for result in super::adjudicate_probe_group(&live, &[4], Some(&kubernetes)) {
        let (verdict, _) = result.outcome.map_err(|err| eyre::eyre!("{err}"))?;
        sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
    }
    // Another Kubernetes version requires its own payload, defaults, and applicable context.
    sim_assert_eq!(have: chart.invocations().iter().filter(|record| record.stage == "render").count(), want: 11);
    sim_assert_eq!(have: chart.invocations().iter().filter(|record| record.stage == "lint-schema-free").count(), want: 11);
    Ok(())
}

#[test]
fn live_grouped_outcomes_fold_in_original_probe_order() -> eyre::Result<()> {
    let results = [0, 3, 5, 1, 2, 4, 6]
        .into_iter()
        .map(|flip| {
            let case = format!("flip {flip}");
            (
                (0, flip + 1),
                super::AcceptanceResult::Probe(Box::new(super::AdjudicatedProbe {
                    flip,
                    case: case.clone(),
                    document_kind: Some(AcceptanceDocument::LintRaw),
                    kubernetes_version: "1.29.0".into(),
                    input: None,
                    outcome: Err(case),
                    adjudication_ms: 0,
                })),
            )
        })
        .collect();
    let comparison = super::fold_results(results, true)?;
    sim_assert_eq!(have: comparison.helm_adjudication.adjudication_failures.into_iter().map(|failure| failure.reason).collect::<Vec<_>>(), want: (0..7).map(|flip| format!("flip {flip}")).collect::<Vec<_>>());
    sim_assert_eq!(have: comparison.helm_adjudication.examined_contexts.len(), want: 7);
    Ok(())
}

fn activation_schemas() -> eyre::Result<LintSchemas> {
    LintSchemas::compile(
        &json!({}),
        &json!({
            "if": {"required": ["redis-ha"], "properties": {"redis-ha": {
                "required": ["enabled"], "properties": {"enabled": {"const": true}}
            }}},
            "then": {"properties": {"redis-ha": {"required": ["image"]}}}
        }),
        None,
    )
}

#[test]
fn oauth2_proxy_activation_exposes_raw_lint_restriction_template_cannot_see() -> eyre::Result<()> {
    let chart = corpus_chart("oauth2-proxy");
    let schemas = activation_schemas()?;
    let result = screen(&ScreenRequest {
        chart: &chart,
        schemas: &schemas,
        kubernetes_version: "1.33.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    result.coverage.validate()?;
    let overlay = json!({"redis-ha": {"enabled": true}});
    let probe = result
        .flips
        .iter()
        .find(|probe| probe.overlay == overlay)
        .ok_or_eyre("activation row did not screen the raw lint restriction")?;
    let document = probe
        .document
        .as_ref()
        .ok_or_eyre("lint document identity lost")?;
    sim_assert_eq!(have: document.kind, want: AcceptanceDocument::LintRaw);
    let mut expected: Value =
        serde_yaml::from_str(&fs::read_to_string(chart.join("values.yaml"))?)?;
    expected["redis-ha"]["enabled"] = json!(true);
    sim_assert_eq!(have: &document.values, want: &expected);
    assert!(
        document
            .origins
            .iter()
            .any(|origin| origin.contains("Enable"))
    );
    let template = acceptance_values(&chart, overlay.clone(), AcceptanceDocument::Template)?;
    assert!(!schemas.profiles.screen_document(&template.root));
    assert!(template.root["redis-ha"].get("image").is_some());
    assert!(!result.flips.iter().any(|probe| {
        probe.overlay == overlay
            && probe
                .document
                .as_ref()
                .is_some_and(|document| document.kind == AcceptanceDocument::Template)
    }));
    Ok(())
}

#[test]
fn openebs_mayastor_scope_null_raw_document_deletes_the_entire_scope() -> eyre::Result<()> {
    let chart = corpus_chart("openebs");
    let mut expected: Value =
        serde_yaml::from_str(&fs::read_to_string(chart.join("values.yaml"))?)?;
    expected
        .as_object_mut()
        .ok_or_eyre("openebs values are not an object")?
        .remove("mayastor")
        .ok_or_eyre("openebs no longer declares mayastor defaults")?;
    let inputs = AcceptanceInputs::load(&chart)?;
    let probe = inputs
        .dependency_probes()?
        .into_iter()
        .find(|probe| {
            probe.kind == test_util::helm_values::DependencyProbeKind::ScopeNull
                && probe.scope == ["mayastor"]
        })
        .ok_or_eyre("dependency inventory omitted the mayastor scope-null control")?;
    sim_assert_eq!(have: &probe.overlay, want: &json!({
        "mayastor": null,
        "engines": {"replicated": {"mayastor": {"enabled": true}}}
    }));
    let raw = inputs.compose(probe.overlay, AcceptanceDocument::LintRaw)?;
    sim_assert_eq!(have: raw.root, want: expected);
    sim_assert_eq!(have: raw.dependencies.len(), want: 0);
    Ok(())
}

#[test]
fn openebs_mayastor_condition_null_distinguishes_both_lint_documents() -> eyre::Result<()> {
    let chart = corpus_chart("openebs");
    let inputs = AcceptanceInputs::load(&chart)?;
    let overlay = json!({"engines": {"replicated": {"mayastor": {"enabled": null}}}});
    assert!(
        inputs
            .dependency_probes()?
            .iter()
            .any(|probe| probe.overlay == overlay)
    );
    let mut raw_expected: Value =
        serde_yaml::from_str(&fs::read_to_string(chart.join("values.yaml"))?)?;
    raw_expected["engines"]["replicated"]["mayastor"]
        .as_object_mut()
        .ok_or_eyre("mayastor condition defaults missing")?
        .remove("enabled")
        .ok_or_eyre("mayastor enabled default missing")?;
    sim_assert_eq!(have: inputs.compose(overlay.clone(), AcceptanceDocument::LintRaw)?.root,
        want: raw_expected);
    let twice_expected = inputs
        .compose(json!({}), AcceptanceDocument::LintCoalescedTwice)?
        .root;
    sim_assert_eq!(have: inputs.compose(overlay.clone(), AcceptanceDocument::LintCoalescedTwice)?.root,
        want: twice_expected);
    let mut template_expected = inputs
        .compose(json!({}), AcceptanceDocument::Template)?
        .root;
    template_expected["engines"]["replicated"]["mayastor"]
        .as_object_mut()
        .ok_or_eyre("mayastor condition control missing")?
        .remove("enabled")
        .ok_or_eyre("mayastor enabled control missing")?;
    sim_assert_eq!(have: inputs.compose(overlay, AcceptanceDocument::Template)?.root,
        want: template_expected);
    Ok(())
}

fn null_chart() -> eyre::Result<ScratchDir> {
    let chart = ScratchDir::new("lint-null-documents")?;
    fs::create_dir(chart.path().join("templates"))?;
    fs::write(
        chart.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: openebs-null-control
        version: 1.0.0
    "},
    )?;
    fs::write(
        chart.path().join("values.yaml"),
        indoc::indoc! {"
        mayastor:
          enabled: true
          replicas: 3
        keep: 7
    "},
    )?;
    Ok(chart)
}

#[test]
fn null_document_flip_is_retained_even_when_template_has_no_flip() -> eyre::Result<()> {
    let chart = null_chart()?;
    let inputs = AcceptanceInputs::load(chart.path())?;
    let overlay = json!({"mayastor": {"replicas": null}});
    for (kind, expected) in [
        (
            AcceptanceDocument::LintRaw,
            json!({"mayastor": {"enabled": true}, "keep": 7}),
        ),
        (
            AcceptanceDocument::LintCoalescedTwice,
            json!({"mayastor": {"enabled": true, "replicas": 3}, "keep": 7}),
        ),
        (
            AcceptanceDocument::Template,
            json!({"mayastor": {"enabled": true}, "keep": 7}),
        ),
    ] {
        sim_assert_eq!(have: inputs.compose(overlay.clone(), kind)?.root, want: expected);
    }
    let schemas = LintSchemas::compile(
        &json!({}),
        &json!({"properties": {
            "mayastor": {"properties": {"replicas": {"maximum": 2}}}
        }}),
        None,
    )?;
    let row = context_row(chart.path(), &schemas, overlay)?;
    let result = screen(&ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[row],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let flips: Vec<_> = result
        .flips
        .iter()
        .filter(|probe| probe.overlay == json!({"mayastor": {"replicas": null}}))
        .map(|probe| probe.document.as_ref().map(|document| document.kind))
        .collect();
    sim_assert_eq!(have: flips, want: vec![Some(AcceptanceDocument::LintCoalescedTwice)]);
    Ok(())
}

fn context_row(chart: &Path, schemas: &LintSchemas, overlay: Value) -> eyre::Result<ContextRow> {
    let values_file = serde_json::to_string(&overlay)?;
    Ok(ContextRow {
        id: "activation".into(),
        chart: "control".into(),
        transport: "values-file".into(),
        kubernetes_version: "1.29.0".into(),
        chart_sha256: tree_sha256(chart)?,
        baseline_sha256: sha256_hex(&schemas.baseline),
        candidate_sha256: sha256_hex(&schemas.candidate),
        canonical_overlay_sha256: sha256_hex(&serde_json::to_vec(&overlay)?),
        values_file_sha256: sha256_hex(values_file.as_bytes()),
        values_file,
        context_overlay: json!({}),
        context_overlay_sha256: sha256_hex(b"{}"),
        overlay,
    })
}

#[test]
fn lint_screen_is_deterministic_and_budget_exhaustion_fails_closed() -> eyre::Result<()> {
    let chart = null_chart()?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["absent"]}), None)?;
    let request = ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    };
    let first = screen(&request)?;
    let second = screen(&request)?;
    first.coverage.validate()?;
    sim_assert_eq!(have: first.coverage, want: second.coverage);
    sim_assert_eq!(have: first.flips.iter().map(|probe| &probe.name).collect::<Vec<_>>(),
        want: second.flips.iter().map(|probe| &probe.name).collect::<Vec<_>>());
    let capped = screen(&ScreenRequest {
        available_probes: 2,
        ..request
    })?;
    assert!(capped.coverage.validate().is_err());
    assert_ne!(capped.coverage.failures, [] as [String; 0]);
    sim_assert_eq!(have: capped.coverage.rows_screened, want: 0);
    Ok(())
}

#[test]
fn context_roster_rejects_stale_unknown_unsupported_and_duplicate_inputs() -> eyre::Result<()> {
    let chart = null_chart()?;
    let schemas = LintSchemas::compile(&json!({}), &json!({}), None)?;
    let row = context_row(chart.path(), &schemas, json!({}))?;
    let roster_path = chart.path().join("context-roster.json");
    let selected = BTreeSet::from(["control".to_string()]);
    fs::write(
        &roster_path,
        serde_json::to_vec(&ContextRoster {
            version: 2,
            rows: vec![row.clone()],
        })?,
    )?;
    let imported = ImportedRoster::read(&roster_path, &selected)?;
    sim_assert_eq!(have: imported.rows.get("control").map(Vec::len), want: Some(1));
    sim_assert_eq!(have: imported.sha256, want: sha256_hex(&fs::read(&roster_path)?));
    for (rows, expected_error) in [
        (
            {
                let mut changed = row.clone();
                changed.chart = "unknown".into();
                vec![changed]
            },
            "unselected chart",
        ),
        (
            {
                let mut changed = row.clone();
                changed.transport = "set-json".into();
                vec![changed]
            },
            "unsupported context transport",
        ),
        (
            {
                let mut changed = row.clone();
                changed.canonical_overlay_sha256 = "stale".into();
                vec![changed]
            },
            "overlay hash mismatch",
        ),
        (vec![row.clone(), row.clone()], "duplicate context row id"),
    ] {
        fs::write(
            &roster_path,
            serde_json::to_vec(&ContextRoster { version: 2, rows })?,
        )?;
        let failure = ImportedRoster::read(&roster_path, &selected)
            .err()
            .ok_or_eyre("invalid roster accepted")?;
        assert!(failure.to_string().contains(expected_error), "{failure}");
    }
    // Bind stale-input checks to the actual screen, where chart/schema bytes are available.
    for field in ["chart", "baseline", "candidate"] {
        let mut stale = row.clone();
        match field {
            "chart" => stale.chart_sha256 = "stale".into(),
            "baseline" => stale.baseline_sha256 = "stale".into(),
            _ => stale.candidate_sha256 = "stale".into(),
        }
        let error = screen(&ScreenRequest {
            chart: chart.path(),
            schemas: &schemas,
            kubernetes_version: "1.29.0",
            imported: &[stale],
            roster_sha256: None,
            available_probes: usize::MAX,
        })
        .err()
        .ok_or_eyre("stale context hashes accepted")?;
        assert!(error.to_string().contains("input hashes mismatch"));
    }
    Ok(())
}

#[test]
fn legacy_roster_identity_does_not_excuse_contextual_lint_flip() -> eyre::Result<()> {
    let chart = corpus_chart("oauth2-proxy");
    let schemas = activation_schemas()?;
    let result = screen(&ScreenRequest {
        chart: &chart,
        schemas: &schemas,
        kubernetes_version: "1.33.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let legacy = super::Probe {
        path: "redis-ha.enabled",
        value: "true",
    };
    assert!(super::names(
        "oauth2-proxy",
        &legacy,
        "oauth2-proxy: redis-ha.enabled <- true"
    ));
    assert!(!result.flips.is_empty());
    for probe in result.flips {
        assert!(!super::names(
            "oauth2-proxy",
            &legacy,
            &format!("oauth2-proxy: {}", probe.name)
        ));
    }
    Ok(())
}

#[test]
fn lint_verdicts_are_accounted_as_adjudicated_outcomes() {
    let mut coverage = super::HelmAdjudicationCoverage::default();
    for (verdict, accepts) in [
        (
            super::HelmFlipVerdict::TighteningMatchedLintRejection,
            false,
        ),
        (super::HelmFlipVerdict::LooseningMatchedLintValidation, true),
        (super::HelmFlipVerdict::CandidateAcceptsLintRejects, true),
    ] {
        sim_assert_eq!(have: coverage.record_verdict(verdict, "lint-context".into(), None), want: Some(accepts));
    }
    sim_assert_eq!(have: coverage.flips_adjudicated, want: 3);
    sim_assert_eq!(have: coverage.tightenings_matched_lint_rejection, want: 1);
    sim_assert_eq!(have: coverage.loosenings_matched_lint_validation, want: 1);
    sim_assert_eq!(have: coverage.false_acceptances.iter().map(|observed| observed.rejection).collect::<Vec<_>>(),
        want: vec![super::Rejection::LintRejects]);
}

#[test]
fn real_lint_rejecting_a_renderable_raw_document_is_a_false_rejection() -> eyre::Result<()> {
    let source = null_chart()?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["missing"]}), None)?;
    let screen = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let probe = screen
        .flips
        .iter()
        .find(|probe| {
            probe.overlay == json!({})
                && probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.kind == AcceptanceDocument::LintRaw)
        })
        .ok_or_eyre("raw defaults flip absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
    let cache = ScratchDir::new("lint-empty-kubernetes-cache")?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        cache.path(),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let error = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe.document.as_ref().ok_or_eyre("raw document absent")?,
        &schemas,
        &kubernetes,
    )
    .err()
    .ok_or_eyre("false rejection was accepted")?;
    assert!(
        error
            .to_string()
            .contains("tightening rejects a lint document"),
        "{error}"
    );
    Ok(())
}

#[test]
fn inherited_schema_free_parse_error_cannot_match_a_tightening() -> eyre::Result<()> {
    let source = null_chart()?;
    fs::create_dir_all(source.path().join("templates"))?;
    fs::write(
        source.path().join("templates/invalid.yaml"),
        "{{ missingSemanticControlFunction }}",
    )?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["missing"]}), None)?;
    let screen = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let probe = screen
        .flips
        .iter()
        .find(|probe| {
            probe.overlay == json!({})
                && probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.kind == AcceptanceDocument::LintRaw)
        })
        .ok_or_eyre("raw defaults flip absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
    let cache = ScratchDir::new("lint-semantic-kubernetes-cache")?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        cache.path(),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let error = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe.document.as_ref().ok_or_eyre("raw document absent")?,
        &schemas,
        &kubernetes,
    )
    .err()
    .ok_or_eyre("inherited parse error certified an unrelated tightening")?;
    assert!(error.to_string().contains("control"), "{error}");
    Ok(())
}

fn raw_flip_error(
    chart_path: &Path,
    schemas: &LintSchemas,
    overlay: &Value,
) -> eyre::Result<String> {
    let screened = screen(&ScreenRequest {
        chart: chart_path,
        schemas,
        kubernetes_version: "1.33.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let probe = screened
        .flips
        .iter()
        .find(|probe| {
            &probe.overlay == overlay
                && probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.kind == AcceptanceDocument::LintRaw)
        })
        .ok_or_eyre("raw flip absent")?;
    let chart =
        super::PinnedHelmChart::prepare(super::helm_invocation::HelmRunner::shared()?, chart_path)?;
    let kubernetes = super::OfflineKubernetesValidator::with_crd_catalog(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        &test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let error = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe.document.as_ref().ok_or_eyre("raw document absent")?,
        schemas,
        &kubernetes,
    )
    .err()
    .ok_or_eyre("unrelated chart failure certified a tightening")?;
    Ok(error.to_string())
}

#[test]
fn inherited_library_template_abort_cannot_match_a_lint_tightening() -> eyre::Result<()> {
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["missing"]}), None)?;
    let error = raw_flip_error(&corpus_chart("common"), &schemas, &json!({}))?;
    assert!(error.contains("control"), "{error}");
    Ok(())
}

fn write_inherited_filename_error(chart: &Path) -> eyre::Result<()> {
    fs::create_dir_all(chart.join("templates"))?;
    fs::write(
        chart.join("templates/config.json"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: fixed-extension-control
        data:
          fixed: valid
    "},
    )?;
    Ok(())
}

#[test]
fn inherited_lint_filename_error_cannot_match_a_tightening() -> eyre::Result<()> {
    let source = null_chart()?;
    write_inherited_filename_error(source.path())?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["missing"]}), None)?;
    let error = raw_flip_error(source.path(), &schemas, &json!({}))?;
    assert!(error.contains("inherited"), "{error}");
    assert_portable_control_evidence(&error)?;
    Ok(())
}

fn assert_portable_control_evidence(error: &str) -> eyre::Result<()> {
    let (_, directory) = error
        .rsplit_once("; evidence=")
        .ok_or_eyre("no preserved evidence path")?;
    let directory = Path::new(directory);
    let verdict: Value =
        serde_json::from_slice(&fs::read(directory.join("schema-verdicts.json"))?)?;
    let controls = verdict
        .get("controls")
        .and_then(Value::as_array)
        .ok_or_eyre("no controls in evidence")?;
    assert_ne!(controls.as_slice(), [] as [Value; 0]);
    for control in controls {
        for field in ["lint_evidence", "render_evidence"] {
            let relative = Path::new(
                control
                    .get(field)
                    .and_then(Value::as_str)
                    .ok_or_eyre("control path absent")?,
            );
            assert!(!relative.is_absolute());
            let bundle = directory.join(relative);
            let mut records = 0;
            for entry in fs::read_dir(&bundle)? {
                let path = entry?.path();
                if !path.to_string_lossy().ends_with(".invocation.json") {
                    continue;
                }
                records += 1;
                let record: Value = serde_json::from_slice(&fs::read(path)?)?;
                for input in ["chart", "values"] {
                    let relative = Path::new(
                        record
                            .get(input)
                            .and_then(Value::as_str)
                            .ok_or_eyre("control input absent")?,
                    );
                    assert!(!relative.is_absolute());
                    assert!(bundle.join(relative).exists());
                }
            }
            assert!(records > 0);
        }
    }
    Ok(())
}

#[test]
fn oauth_activation_stays_a_false_rejection_with_inherited_lint_error() -> eyre::Result<()> {
    let source = ScratchDir::new("oauth-inherited-lint-control")?;
    test_util::scratch::copy_tree(&corpus_chart("oauth2-proxy"), source.path())?;
    write_inherited_filename_error(source.path())?;
    let error = raw_flip_error(
        source.path(),
        &activation_schemas()?,
        &json!({"redis-ha":{"enabled":true}}),
    )?;
    assert!(
        error.contains("tightening rejects a lint document"),
        "{error}"
    );
    assert!(error.contains("inherited"), "{error}");
    Ok(())
}

fn context_probe_verdict(
    template: &str,
    context_overlay: Value,
) -> eyre::Result<Result<super::HelmFlipVerdict, String>> {
    let source = null_chart()?;
    fs::write(
        source.path().join("values.yaml"),
        "{\"gate\":false,\"payload\":0}",
    )?;
    fs::write(source.path().join("templates/config.yaml"), template)?;
    let schemas = LintSchemas::compile(
        &json!({}),
        &json!({"properties":{"payload":{"maximum":0}}}),
        None,
    )?;
    screened_context_verdict(
        source.path(),
        &schemas,
        &json!({"gate":true,"payload":1}),
        context_overlay,
    )
}

fn screened_context_verdict(
    source: &Path,
    schemas: &LintSchemas,
    overlay: &Value,
    context_overlay: Value,
) -> eyre::Result<Result<super::HelmFlipVerdict, String>> {
    screened_contexts_verdict(source, schemas, overlay, &[context_overlay])
}

fn screened_contexts_verdict(
    source: &Path,
    schemas: &LintSchemas,
    overlay: &Value,
    contexts: &[Value],
) -> eyre::Result<Result<super::HelmFlipVerdict, String>> {
    let mut rows = Vec::new();
    for (index, context) in contexts.iter().enumerate() {
        let mut row = context_row(source, schemas, overlay.clone())?;
        row.id = format!("context-{index}");
        row.context_overlay_sha256 = sha256_hex(&serde_json::to_vec(context)?);
        row.context_overlay = context.clone();
        rows.push(row);
    }
    let screened = screen(&ScreenRequest {
        chart: source,
        schemas,
        kubernetes_version: "1.29.0",
        imported: &rows,
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    screened.coverage.validate()?;
    let probe = screened
        .flips
        .iter()
        .find(|probe| {
            &probe.overlay == overlay
                && probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.kind == AcceptanceDocument::LintRaw)
        })
        .ok_or_eyre("context raw flip absent")?;
    let chart =
        super::PinnedHelmChart::prepare(super::helm_invocation::HelmRunner::shared()?, source)?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    Ok(super::adjudicate_lint_flip(
        &chart,
        probe,
        probe
            .document
            .as_ref()
            .ok_or_eyre("context raw document absent")?,
        schemas,
        &kubernetes,
    )
    .map(|(verdict, _)| verdict)
    .map_err(|err| err.to_string()))
}

#[test]
fn context_render_abort_cannot_justify_a_payload_tightening() -> eyre::Result<()> {
    let template = "{{ if .Values.gate }}{{ fail \"activation blocked\" }}{{ end }}";
    let error = context_probe_verdict(template, json!({"gate":true}))?
        .err()
        .ok_or_eyre("unusable context certified a payload tightening")?;
    assert!(
        error.contains("unusable defaults/context render control"),
        "{error}"
    );
    sim_assert_eq!(
        have: context_probe_verdict(template, json!({}))?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedHelmAbort)
    );
    Ok(())
}

#[test]
fn attributable_schema_free_lint_error_matches_a_tightening() -> eyre::Result<()> {
    sim_assert_eq!(
        have: context_probe_verdict("{{ if .Values.gate }}invalid scalar manifest{{ end }}", json!({}))?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedLintRejection)
    );
    Ok(())
}

#[test]
fn context_kubernetes_violation_cannot_justify_a_payload_tightening() -> eyre::Result<()> {
    let template = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: context-control
        {{ if .Values.gate }}
        data: []
        {{ else }}
        data: {}
        {{ end }}
    "};
    let error = context_probe_verdict(template, json!({"gate":true}))?
        .err()
        .ok_or_eyre("inherited Kubernetes violation certified a payload tightening")?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    sim_assert_eq!(
        have: context_probe_verdict(template, json!({}))?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedKubernetesRejection)
    );
    Ok(())
}

fn unusable_control_error(
    defaults: &Value,
    template: &str,
    context: Value,
    inherited_lint: bool,
) -> eyre::Result<String> {
    let source = null_chart()?;
    fs::write(
        source.path().join("values.yaml"),
        serde_json::to_vec(defaults)?,
    )?;
    fs::write(source.path().join("templates/config.yaml"), template)?;
    if inherited_lint {
        fs::write(source.path().join("templates/fixed.json"), "")?;
    }
    let schemas = LintSchemas::compile(
        &json!({}),
        &json!({"properties":{"payload":{"minimum":2}}}),
        None,
    )?;
    screened_context_verdict(
        source.path(),
        &schemas,
        &json!({"gate":true,"payload":1}),
        context,
    )?
    .err()
    .ok_or_eyre("unusable Kubernetes control certified a payload tightening")
}

const SCALAR_CONTEXT: &str = indoc::indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: context-control
    data: {{ if .Values.gate }}{{ .Values.payload | quote }}{{ else }}{}{{ end }}
"};

#[test]
fn unusable_changing_scalar_context_cannot_certify_tightening() -> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"gate":false,"payload":0}),
        SCALAR_CONTEXT,
        json!({"gate":true}),
        false,
    )?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    assert!(error.contains("/data"), "{error}");
    Ok(())
}

#[test]
fn unusable_changing_scalar_context_with_inherited_lint_cannot_certify_tightening()
-> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"gate":false,"payload":0}),
        SCALAR_CONTEXT,
        json!({"gate":true}),
        true,
    )?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    Ok(())
}

const UNCERTAIN_CONTROL: &str = indoc::indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: precision-control
      generation: {{ .Values.payload }}
    data:
      fixed: []
"};

#[test]
fn unusable_uncertain_defaults_cannot_certify_tightening() -> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"payload":9_007_199_254_740_992_u64}),
        UNCERTAIN_CONTROL,
        json!({}),
        false,
    )?;
    assert_preserved_control_uncertainty(&error)
}

#[test]
fn unusable_uncertain_defaults_with_inherited_lint_cannot_certify_tightening() -> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"payload":9_007_199_254_740_992_u64}),
        UNCERTAIN_CONTROL,
        json!({}),
        true,
    )?;
    assert_preserved_control_uncertainty(&error)
}

#[test]
fn unusable_uncertain_context_cannot_certify_tightening() -> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"payload":0}),
        UNCERTAIN_CONTROL,
        json!({"payload":9_007_199_254_740_992_u64}),
        false,
    )?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    assert!(error.contains("numeric magnitude"), "{error}");
    Ok(())
}

fn assert_preserved_control_uncertainty(error: &str) -> eyre::Result<()> {
    assert!(
        error.contains("unusable defaults Kubernetes control"),
        "{error}"
    );
    assert!(error.contains("numeric magnitude"), "{error}");
    let (_, directory) = error
        .rsplit_once("; evidence=")
        .ok_or_eyre("no evidence path")?;
    let evidence: Value = serde_json::from_slice(&fs::read(
        Path::new(directory).join("schema-verdicts.json"),
    )?)?;
    let failure = evidence
        .get("failure")
        .and_then(Value::as_str)
        .ok_or_eyre("no saved control failure")?;
    assert!(failure.contains("numeric magnitude"), "{failure}");
    Ok(())
}

#[test]
fn unusable_controls_are_checked_before_lint_rejection() -> eyre::Result<()> {
    let template = indoc::indoc! {r"
        {{ if eq (int .Values.payload) 1 }}invalid scalar manifest{{ else }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: precision-control
          generation: {{ .Values.payload }}
        {{ end }}
    "};
    let error = unusable_control_error(
        &json!({"payload":9_007_199_254_740_992_u64}),
        template,
        json!({}),
        false,
    )?;
    assert_preserved_control_uncertainty(&error)
}

#[test]
fn unusable_controls_are_checked_before_template_abort() -> eyre::Result<()> {
    let template = indoc::indoc! {r#"
        {{ if eq (int .Values.payload) 1 }}{{ fail "payload blocked" }}{{ end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: precision-control
          generation: {{ .Values.payload }}
    "#};
    let error = unusable_control_error(
        &json!({"payload":9_007_199_254_740_992_u64}),
        template,
        json!({}),
        false,
    )?;
    assert_preserved_control_uncertainty(&error)
}

#[test]
fn inherited_scalar_assertion_does_not_become_new_in_a_context() -> eyre::Result<()> {
    let error = unusable_control_error(
        &json!({"gate":true,"payload":0}),
        SCALAR_CONTEXT,
        json!({"payload":2}),
        false,
    )?;
    assert!(
        error.contains("tightening rejects a lint document"),
        "{error}"
    );
    assert!(!error.contains("unusable"), "{error}");
    Ok(())
}

fn relocation_tightening_verdict(
    template: &str,
) -> eyre::Result<Result<super::HelmFlipVerdict, String>> {
    let source = null_chart()?;
    fs::write(source.path().join("values.yaml"), r#"{"extra":false}"#)?;
    fs::write(source.path().join("templates/config.yaml"), template)?;
    let schemas = LintSchemas::compile(
        &json!({}),
        &json!({"properties":{"extra":{"const":false}}}),
        None,
    )?;
    screened_context_verdict(source.path(), &schemas, &json!({"extra":true}), json!({}))
}

fn assert_tightening_has_only_inherited_failures(template: &str) -> eyre::Result<()> {
    let error = relocation_tightening_verdict(template)?
        .err()
        .ok_or_eyre("an inherited failure certified a tightening")?;
    assert!(
        error.contains("tightening rejects a lint document"),
        "{error}"
    );
    assert!(!error.contains("unusable"), "{error}");
    let (_, directory) = error
        .rsplit_once("; evidence=")
        .ok_or_eyre("no evidence path")?;
    let evidence: Value = serde_json::from_slice(&fs::read(
        Path::new(directory).join("schema-verdicts.json"),
    )?)?;
    let contexts = evidence
        .get("kubernetes_contexts")
        .and_then(Value::as_array)
        .ok_or_eyre("no saved Kubernetes context comparisons")?;
    sim_assert_eq!(have: contexts.len(), want: 1);
    let context = contexts.first().ok_or_eyre("no saved context comparison")?;
    sim_assert_eq!(have: context.get("new_kubernetes_errors"), want: Some(&json!([])));
    sim_assert_eq!(
        have: context.get("inherited_kubernetes_errors").and_then(Value::as_array).ok_or_eyre("missing inherited errors")?.len(),
        want: 1,
    );
    Ok(())
}

#[test]
fn prepending_valid_container_cannot_turn_inherited_image_failure_into_tightening()
-> eyre::Result<()> {
    assert_tightening_has_only_inherited_failures(indoc::indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: container-insertion
        spec:
          containers:
            {{ if .Values.extra }}
            - name: good
              image: busybox
            {{ end }}
            - name: bad
              image: 42
    "})
}

#[test]
fn reordering_environment_cannot_turn_inherited_value_failure_into_tightening() -> eyre::Result<()>
{
    assert_tightening_has_only_inherited_failures(indoc::indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: environment-reordering
        spec:
          containers:
            - name: example
              image: busybox
              env:
                {{ if .Values.extra }}
                - name: VALID
                  value: valid
                {{ end }}
                - name: BROKEN
                  value: []
                {{ if not .Values.extra }}
                - name: VALID
                  value: valid
                {{ end }}
    "})
}

#[test]
fn renaming_permitted_data_key_cannot_turn_inherited_failure_into_tightening() -> eyre::Result<()> {
    assert_tightening_has_only_inherited_failures(indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: data-key-renaming
        data:
          {{ if .Values.extra }}renamed{{ else }}original{{ end }}: []
    "})
}

#[test]
fn changing_invalid_image_scalar_type_cannot_certify_tightening() -> eyre::Result<()> {
    assert_tightening_has_only_inherited_failures(indoc::indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: invalid-image-type
        spec:
          containers:
            - name: bad
              image: {{ if .Values.extra }}true{{ else }}42{{ end }}
    "})
}

#[test]
fn a_second_invalid_data_member_still_proves_a_new_violation() -> eyre::Result<()> {
    let template = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: added-data-failure
        data:
          original: []
          {{ if .Values.extra }}
          added: []
          {{ end }}
    "};
    sim_assert_eq!(
        have: relocation_tightening_verdict(template)?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedKubernetesRejection),
    );
    Ok(())
}

#[test]
fn moving_failure_between_fixed_data_properties_remains_a_new_violation() -> eyre::Result<()> {
    let template = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: fixed-property-failure
        data: {{ if .Values.extra }}{}{{ else }}[]{{ end }}
        binaryData: {{ if .Values.extra }}[]{{ else }}{}{{ end }}
    "};
    sim_assert_eq!(
        have: relocation_tightening_verdict(template)?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedKubernetesRejection),
    );
    Ok(())
}

fn structural_loosening_verdict(
    template: &str,
    context: Value,
) -> eyre::Result<Result<super::HelmFlipVerdict, String>> {
    let source = null_chart()?;
    fs::write(
        source.path().join("values.yaml"),
        r#"{"gate":false,"payload":0}"#,
    )?;
    fs::write(source.path().join("templates/config.yaml"), template)?;
    let schemas = LintSchemas::compile(
        &json!({"properties":{"payload":{"maximum":0}}}),
        &json!({}),
        None,
    )?;
    screened_context_verdict(
        source.path(),
        &schemas,
        &json!({"gate":true,"payload":1}),
        context,
    )
}

fn forbidden_member_template(existing: &str, added: &str) -> String {
    indoc::formatdoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: forbidden-members
        {existing}: present
        {{{{ if .Values.payload }}}}
        {added}: added
        {{{{ end }}}}
    "}
}

#[test]
fn added_forbidden_member_is_a_new_contextual_violation() -> eyre::Result<()> {
    let template = forbidden_member_template("existingUnexpected", "addedUnexpected");
    sim_assert_eq!(
        have: structural_loosening_verdict(&template, json!({"gate":true}))?,
        want: Ok(super::HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
    );
    Ok(())
}

#[test]
fn forbidden_x_does_not_hide_added_y_with_defaults_context() -> eyre::Result<()> {
    let template = forbidden_member_template("x", "y");
    sim_assert_eq!(
        have: structural_loosening_verdict(&template, json!({}))?,
        want: Ok(super::HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
    );
    Ok(())
}

#[test]
fn added_forbidden_member_makes_context_unusable() -> eyre::Result<()> {
    let template = forbidden_member_template("existingUnexpected", "addedUnexpected");
    let error = structural_loosening_verdict(&template, json!({"payload":1}))?
        .err()
        .ok_or_eyre("new forbidden member was inherited from defaults")?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    assert!(error.contains("/addedUnexpected"), "{error}");
    Ok(())
}

const REQUIRED_PROPERTY_SWAP: &str = indoc::indoc! {r"
    apiVersion: rbac.authorization.k8s.io/v1
    kind: RoleBinding
    metadata:
      name: required-swap
    roleRef:
      kind: Role
      {{ if .Values.payload }}
      apiGroup: rbac.authorization.k8s.io
      {{ else }}
      name: reader
      {{ end }}
"};

#[test]
fn changed_missing_required_property_is_a_new_contextual_violation() -> eyre::Result<()> {
    sim_assert_eq!(
        have: structural_loosening_verdict(REQUIRED_PROPERTY_SWAP, json!({}))?,
        want: Ok(super::HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
    );
    Ok(())
}

#[test]
fn changed_missing_required_property_makes_context_unusable() -> eyre::Result<()> {
    let error = structural_loosening_verdict(REQUIRED_PROPERTY_SWAP, json!({"payload":1}))?
        .err()
        .ok_or_eyre("different missing required property was inherited from defaults")?;
    assert!(
        error.contains("unusable context Kubernetes control"),
        "{error}"
    );
    assert!(error.contains("name"), "{error}");
    Ok(())
}

const CONTEXT_REDUCES_FAILURES: &str = indoc::indoc! {r"
        {{ $copies := 2 }}
        {{ if .Values.gate }}{{ $copies = 1 }}{{ end }}
        {{ if .Values.payload }}{{ $copies = 2 }}{{ end }}
        {{ range $index := until $copies }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: duplicate-{{ $index }}
        data:
          token: {{ $index }}
        {{ end }}
    "};

#[test]
fn payload_cannot_spend_defaults_violations_removed_by_context() -> eyre::Result<()> {
    // Defaults emit two invalid resources, the context one, and the payload two again.
    sim_assert_eq!(
        have: structural_loosening_verdict(CONTEXT_REDUCES_FAILURES, json!({"gate":true}))?,
        want: Ok(super::HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
    );
    Ok(())
}

#[test]
fn merged_contexts_cannot_borrow_each_others_failure_allowances() -> eyre::Result<()> {
    let source = null_chart()?;
    fs::write(
        source.path().join("values.yaml"),
        r#"{"gate":false,"payload":0}"#,
    )?;
    fs::write(
        source.path().join("templates/config.yaml"),
        CONTEXT_REDUCES_FAILURES,
    )?;
    let restrictive = json!({"properties":{"payload":{"maximum":0}}});
    let tightening = LintSchemas::compile(&json!({}), &restrictive, None)?;
    let loosening = LintSchemas::compile(&restrictive, &json!({}), None)?;
    // The same payload has a new failure relative to one origin, but none relative to the other.
    for contexts in [
        vec![json!({"gate":true}), json!({})],
        vec![json!({}), json!({"gate":true})],
        vec![json!({"gate":true}), json!({"gate":false})],
    ] {
        let overlay = json!({"gate":true,"payload":1});
        let error = screened_contexts_verdict(source.path(), &tightening, &overlay, &contexts)?
            .err()
            .ok_or_eyre("one context's rejection certified every origin")?;
        assert!(error.contains("every applicable context"), "{error}");
        sim_assert_eq!(
            have: screened_contexts_verdict(source.path(), &loosening, &overlay, &contexts)?,
            want: Ok(super::HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
        );
    }
    Ok(())
}

#[test]
fn builtin_defaults_context_survives_imported_execution_deduplication() -> eyre::Result<()> {
    let source = null_chart()?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required":["absent"]}), None)?;
    let mut row = context_row(source.path(), &schemas, json!({}))?;
    row.context_overlay = json!({"gate":true});
    row.context_overlay_sha256 = sha256_hex(&serde_json::to_vec(&row.context_overlay)?);
    let screened = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[row],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    screened.coverage.validate()?;
    let document = screened
        .flips
        .iter()
        .find(|probe| probe.overlay == json!({}))
        .and_then(|probe| probe.document.as_ref())
        .ok_or_eyre("deduplicated defaults flip absent")?;
    let contexts: BTreeSet<_> = document
        .control_overlays
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()?;
    sim_assert_eq!(have: contexts, want: BTreeSet::from(["{}".to_string(), r#"{"gate":true}"#.to_string()]));
    assert!(
        document
            .origins
            .iter()
            .any(|origin| origin.starts_with("dependency:"))
    );
    assert!(
        document
            .origins
            .iter()
            .any(|origin| origin.starts_with("roster:"))
    );
    Ok(())
}

#[test]
#[ignore = "maintenance: requires an external OAuth2 Proxy chart and historical baseline/candidate schemas"]
fn landing_lint_activation_is_screened_and_adjudicated() -> eyre::Result<()> {
    let chart_path = PathBuf::from(
        std::env::var_os(super::EXTERNAL_CHART_VAR)
            .ok_or_eyre("SCHEMA_ACCEPTANCE_EXTERNAL_CHART must name the OAuth2 Proxy chart")?,
    );
    let baseline_path = PathBuf::from(
        std::env::var_os(super::BASELINE_SCHEMA_VAR)
            .ok_or_eyre("SCHEMA_ACCEPTANCE_BASELINE_SCHEMA must name the baseline schema")?,
    );
    let candidate_path = PathBuf::from(std::env::var_os(super::CANDIDATE_SCHEMA_VAR).ok_or_eyre(
        "SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA must name the historical candidate schema",
    )?);
    let baseline_bytes = fs::read(&baseline_path)?;
    let candidate: Value = serde_json::from_slice(&fs::read(&candidate_path)?)?;
    let schemas = LintSchemas::compile_bytes(&baseline_bytes, &candidate, None)?;
    let version =
        helm_schema_test_support::helm::kubernetes_version::chart_kubernetes_version(&chart_path)?;
    let started = std::time::Instant::now();
    let screened = screen(&ScreenRequest {
        chart: &chart_path,
        schemas: &schemas,
        kubernetes_version: version,
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    screened.coverage.validate()?;
    let overlay = json!({"redis-ha": {"enabled": true}});
    let probe = screened
        .flips
        .iter()
        .find(|probe| {
            probe.overlay == overlay
                && probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.kind == AcceptanceDocument::LintRaw)
        })
        .ok_or_eyre("historical activation did not screen a raw lint flip")?;
    let document = probe
        .document
        .as_ref()
        .ok_or_eyre("screened raw document absent")?;
    let template = acceptance_values(&chart_path, overlay.clone(), AcceptanceDocument::Template)?;
    eyre::ensure!(
        !schemas.profiles.screen_document(&template.root),
        "historical activation also flips the template document; this is not the missed lint-only witness"
    );
    eprintln!(
        "HISTORICAL_LINT_SCREEN {}",
        serde_json::to_string(&json!({
            "chart": chart_path, "baseline": baseline_path, "candidate": candidate_path,
            "overlay": overlay, "document_kind": document.kind, "document": document.values,
            "coverage": screened.coverage, "screen_elapsed_ms": started.elapsed().as_millis(),
        }))?
    );
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        &chart_path,
    )?;
    let cache = std::env::var_os(super::K8S_CACHE_VAR).map_or_else(
        || test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        PathBuf::from,
    );
    let kubernetes = super::OfflineKubernetesValidator::with_crd_catalog(
        &cache,
        &test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let error = super::adjudicate_lint_flip(&chart, probe, document, &schemas, &kubernetes)
        .err()
        .ok_or_eyre("historical false rejection was accepted")?;
    eprintln!("HISTORICAL_LINT_ADJUDICATION {error:#}");
    eyre::ensure!(
        error
            .to_string()
            .contains("tightening rejects a lint document"),
        "historical witness did not reach a confirmed false rejection: {error:#}"
    );
    Ok(())
}

#[test]
fn live_lint_coverage_requires_every_screened_document_outcome() -> eyre::Result<()> {
    let chart = null_chart()?;
    let schemas = LintSchemas::compile(&json!({}), &json!({"required": ["absent"]}), None)?;
    let screened = screen(&ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let mut coverage = screened.coverage;
    coverage.validate()?;
    assert!(!screened.flips.is_empty());
    assert!(coverage.validate_live().is_err());
    for counts in coverage.documents.values_mut() {
        counts.adjudicated = counts.flips;
    }
    coverage.validate_live()?;
    let counts = coverage
        .documents
        .get_mut("LintRaw")
        .ok_or_eyre("raw coverage absent")?;
    counts.adjudication_failures = 1;
    assert!(coverage.validate_live().is_err());
    Ok(())
}

#[test]
fn live_global_coverage_cannot_lose_a_screened_flip() -> eyre::Result<()> {
    let mut coverage = super::HelmAdjudicationCoverage {
        enabled: true,
        screened_flips: 1,
        ..Default::default()
    };
    let error = super::validate_helm_adjudication_coverage(&coverage, &[], &[])
        .err()
        .ok_or_eyre("live coverage accepted a missing outcome")?;
    assert!(
        error
            .to_string()
            .contains("screened flip outcome accounting mismatch")
    );
    coverage.record_verdict(
        super::HelmFlipVerdict::Collapsed,
        "collapsed-control".into(),
        None,
    );
    super::validate_helm_adjudication_coverage(&coverage, &[], &[])?;
    coverage.screened_flips += 1;
    coverage.record_verdict(
        super::HelmFlipVerdict::TighteningMatchedLintRejection,
        "adjudicated-control".into(),
        None,
    );
    super::validate_helm_adjudication_coverage(&coverage, &[], &[])?;
    Ok(())
}

fn contextual_selector() -> super::known_false_acceptances::ContextSelector {
    super::known_false_acceptances::ContextSelector {
        document: AcceptanceDocument::LintRaw,
        overlay: r#"{"redis-ha":{"enabled":true},"payload":[]}"#,
        transport: super::known_false_acceptances::ContextTransport::ValuesFile,
        kubernetes_version: "1.33.0",
    }
}

#[test]
fn contextual_false_acceptance_authorizes_only_the_complete_identity() -> eyre::Result<()> {
    use super::known_false_acceptances::ContextFalseAcceptance;
    let selector = contextual_selector();
    let row = ContextFalseAcceptance {
        chart: "oauth2-proxy",
        selector,
        rejection: super::Rejection::LintRejects,
        baseline: super::Baseline::RejectsUnlikeItsDefaults,
        family: super::Family::Unfiled,
    };
    let case = selector.case(row.chart)?;
    let mut coverage = super::HelmAdjudicationCoverage::default();
    coverage.examined_contexts.insert(case.clone());
    coverage.record_verdict(
        super::HelmFlipVerdict::CandidateAcceptsLintRejects,
        case.clone(),
        None,
    );
    super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[row], &[])?;
    for changed in [
        super::known_false_acceptances::ContextSelector {
            document: AcceptanceDocument::LintCoalescedTwice,
            ..selector
        },
        super::known_false_acceptances::ContextSelector {
            overlay: r#"{"redis-ha":{"enabled":true}}"#,
            ..selector
        },
        super::known_false_acceptances::ContextSelector {
            kubernetes_version: "1.29.0",
            ..selector
        },
    ] {
        let wrong = ContextFalseAcceptance {
            chart: "oauth2-proxy",
            selector: changed,
            rejection: super::Rejection::LintRejects,
            baseline: super::Baseline::RejectsUnlikeItsDefaults,
            family: super::Family::Unfiled,
        };
        assert!(
            super::validate_helm_adjudication_coverage_with_contexts(
                &coverage,
                &[],
                &[],
                &[wrong],
                &[]
            )
            .is_err()
        );
    }
    let reordered = super::known_false_acceptances::ContextSelector {
        overlay: r#"{"payload":[],"redis-ha":{"enabled":true}}"#,
        ..selector
    };
    sim_assert_eq!(have: reordered.case("oauth2-proxy")?, want: case);
    Ok(())
}

#[test]
fn contextual_undecided_authorization_and_staleness_require_exact_examined_case() -> eyre::Result<()>
{
    use super::known_undecided_acceptances::ContextUndecidedAcceptance;
    let row = ContextUndecidedAcceptance {
        chart: "oauth2-proxy",
        selector: contextual_selector(),
        uncertain: &["unknown resource"],
    };
    let case = row.selector.case(row.chart)?;
    let rows = [row];
    let mut coverage = super::HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("oauth2-proxy".into());
    // Other cases of this chart do not establish that this precise context was fixed.
    super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[], &rows)?;
    coverage.examined_contexts.insert(case.clone());
    assert!(
        super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[], &rows)
            .is_err()
    );
    coverage.record_verdict(
        super::HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["unknown resource".into()]),
        case,
        None,
    );
    super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[], &rows)?;
    coverage
        .loosenings_with_uncertain_kubernetes
        .first_mut()
        .ok_or_eyre("uncertainty observation absent")?
        .uncertain = vec!["different uncertainty".into()];
    assert!(
        super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[], &rows)
            .is_err()
    );
    Ok(())
}

#[test]
fn contextual_false_acceptance_row_is_stale_only_after_its_case_is_examined() -> eyre::Result<()> {
    let row = super::known_false_acceptances::ContextFalseAcceptance {
        chart: "oauth2-proxy",
        selector: contextual_selector(),
        rejection: super::Rejection::LintRejects,
        baseline: super::Baseline::RejectsUnlikeItsDefaults,
        family: super::Family::Unfiled,
    };
    let case = row.selector.case(row.chart)?;
    let rows = [row];
    let mut coverage = super::HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("oauth2-proxy".into());
    super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &rows, &[])?;
    coverage.examined_contexts.insert(case);
    let error =
        super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &rows, &[])
            .err()
            .ok_or_eyre("examined fixed contextual row was retained")?;
    assert!(error.to_string().contains("no longer fail alike"));
    Ok(())
}

fn partial_override_chart() -> eyre::Result<ScratchDir> {
    let chart = ScratchDir::new("partial-override-lint-control")?;
    fs::create_dir_all(chart.path().join("charts/child"))?;
    fs::create_dir(chart.path().join("templates"))?;
    fs::write(
        chart.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: partial-override-control
        version: 1.0.0
        dependencies:
          - name: child
            version: 1.0.0
    "},
    )?;
    fs::write(chart.path().join("values.yaml"), "{}\n")?;
    fs::write(
        chart.path().join("charts/child/Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: child
        version: 1.0.0
    "},
    )?;
    fs::write(
        chart.path().join("charts/child/values.yaml"),
        serde_json::to_vec(&json!({"first": 1, "second": 2}))?,
    )?;
    Ok(chart)
}

#[test]
fn partial_override_audits_unchanged_rejection_and_balances_document_coverage() -> eyre::Result<()>
{
    let chart = partial_override_chart()?;
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let schemas = LintSchemas::compile(&restrictive, &restrictive, None)?;
    let result = screen(&ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    result.coverage.validate()?;
    sim_assert_eq!(have: result.coverage.builtin_rows, want: 3);
    sim_assert_eq!(have: result.coverage.distinct_overlays, want: 3);
    sim_assert_eq!(have: result.coverage.rows_screened, want: 3);
    for (kind, counts) in &result.coverage.documents {
        let skipped = usize::from(kind != "LintRaw");
        sim_assert_eq!(have: counts.screened, want: 3 - skipped);
        sim_assert_eq!(have: counts.not_validated, want: skipped);
    }
    sim_assert_eq!(have: result.flips.len(), want: 1);
    let probe = result.flips.first().ok_or_eyre("partial control absent")?;
    sim_assert_eq!(have: &probe.overlay, want: &json!({"child": {"first": 1}}));
    let document = probe
        .document
        .as_ref()
        .ok_or_eyre("partial document absent")?;
    sim_assert_eq!(have: document.kind, want: AcceptanceDocument::LintRaw);
    sim_assert_eq!(have: &document.origins, want: &vec!["dependency:[\"child\"]:PartialOverride".to_string()]);
    sim_assert_eq!(have: &document.control_overlays, want: &vec![json!({})]);
    sim_assert_eq!(have: schemas.profiles.document_verdicts(&document.values), want: (false, false));
    assert!(document.lint_control);
    assert!(
        raw_flip_error(chart.path(), &schemas, &probe.overlay)?
            .contains("tightening rejects a lint document")
    );
    Ok(())
}

/// Folds one chart's lint screen through the schema-only accounting.
fn schema_only_lint_accounting(
    baseline: &Value,
    candidate: &Value,
) -> eyre::Result<(super::ScreenedProbes, super::ProbeCoverage)> {
    let chart = partial_override_chart()?;
    let schemas = LintSchemas::compile(baseline, candidate, None)?;
    let lint = screen(&ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let mut screened = super::ScreenedProbes {
        probes_checked: 0,
        flips: Vec::new(),
        failures: Vec::new(),
        screened: Vec::new(),
    };
    let mut coverage = super::ProbeCoverage {
        label: "partial-override-control".to_string(),
        ..Default::default()
    };
    screened.append_lint(Ok(lint), &schemas, &mut coverage, false);
    Ok((screened, coverage))
}

/// Schema-only mode cannot run the forced audit, and identical verdicts are no
/// transition, so the audit is deferred to live mode instead of read as a flip.
#[test]
fn schema_only_defers_an_unchanged_forced_partial_override_audit() -> eyre::Result<()> {
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let (screened, coverage) = schema_only_lint_accounting(&restrictive, &restrictive)?;
    sim_assert_eq!(have: screened.flips, want: Vec::<String>::new());
    let lint = coverage.lint.ok_or_eyre("lint coverage absent")?;
    lint.validate()?;
    sim_assert_eq!(
        have: lint.deferred_live_audits,
        want: vec![
            "document=LintRaw transport=values-file kube=1.29.0 overlay={\"child\":{\"first\":1}}"
                .to_string()
        ]
    );
    Ok(())
}

/// A forced audit whose verdict or violations change is a transition and
/// stays a flip, which schema-only mode cannot adjudicate.
#[test]
fn schema_only_keeps_a_changed_forced_partial_override_audit_as_a_flip() -> eyre::Result<()> {
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let stricter = json!({"properties": {"child": {"required": ["second", "third"]}}});
    for (baseline, candidate, verdicts) in [
        (json!({}), restrictive.clone(), "before=true, after=false"),
        (
            restrictive.clone(),
            stricter.clone(),
            "before=false, after=false",
        ),
        // Both reject, but the candidate drops a reason: still a transition.
        (stricter, restrictive, "before=false, after=false"),
    ] {
        let (screened, coverage) = schema_only_lint_accounting(&baseline, &candidate)?;
        let lint = coverage.lint.ok_or_eyre("lint coverage absent")?;
        sim_assert_eq!(have: lint.deferred_live_audits, want: Vec::<String>::new());
        // The stricter pair also flips coalesced documents; only the raw audit is under test.
        let raw: Vec<_> = screened
            .flips
            .into_iter()
            .filter(|flip| flip.contains("document=LintRaw"))
            .collect();
        sim_assert_eq!(
            have: raw,
            want: vec![format!(
                "partial-override-control: document=LintRaw transport=values-file kube=1.29.0 \
                 overlay={{\"child\":{{\"first\":1}}}}: {verdicts}"
            )]
        );
    }
    Ok(())
}

#[test]
fn partial_override_restriction_is_matched_only_when_schema_free_lint_rejects() -> eyre::Result<()>
{
    let source = partial_override_chart()?;
    // Keep defaults usable: the tested override changes the rendered discriminator.
    fs::write(
        source.path().join("charts/child/values.yaml"),
        serde_json::to_vec(&json!({"first": true, "second": 2}))?,
    )?;
    fs::write(
        source.path().join("templates/control.yaml"),
        indoc::indoc! {"
            {{ if .Values.child.first }}invalid scalar manifest{{ end }}
    "},
    )?;
    fs::write(source.path().join("values.yaml"), "child: {first: false}\n")?;
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let schemas = LintSchemas::compile(&restrictive, &restrictive, None)?;
    let screened = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let probe = screened
        .flips
        .iter()
        .find(|probe| probe.overlay == json!({"child": {"first": true}}))
        .ok_or_eyre("partial control absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
    let cache = ScratchDir::new("partial-control-kubernetes")?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        cache.path(),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let (verdict, _) = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe
            .document
            .as_ref()
            .ok_or_eyre("partial document absent")?,
        &schemas,
        &kubernetes,
    )?;
    sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::TighteningMatchedLintRejection);
    Ok(())
}

#[test]
fn partial_override_loosened_schema_matches_live_lint_and_template() -> eyre::Result<()> {
    let source = partial_override_chart()?;
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let schemas = LintSchemas::compile(&restrictive, &json!({}), None)?;
    let screened = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    screened.coverage.validate()?;
    let probe = screened
        .flips
        .iter()
        .find(|probe| probe.overlay == json!({"child": {"first": 1}}))
        .ok_or_eyre("partial control absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
    let cache = ScratchDir::new("partial-loosening-kubernetes")?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        cache.path(),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let (verdict, _) = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe
            .document
            .as_ref()
            .ok_or_eyre("partial document absent")?,
        &schemas,
        &kubernetes,
    )?;
    sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::LooseningMatchedLintValidation);
    Ok(())
}

#[test]
fn partial_override_preserves_origins_on_dedup_and_fails_closed_at_document_budget()
-> eyre::Result<()> {
    let source = partial_override_chart()?;
    let path = source.path().join("Chart.yaml");
    let mut metadata: Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    metadata["dependencies"][0]["condition"] = json!("child.first");
    fs::write(path, serde_json::to_vec(&metadata)?)?;
    fs::write(
        source.path().join("charts/child/values.yaml"),
        serde_json::to_vec(&json!({"first": false, "second": 2}))?,
    )?;
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let schemas = LintSchemas::compile(&restrictive, &restrictive, None)?;
    let mut request = ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    };
    let screened = screen(&request)?;
    screened.coverage.validate()?;
    sim_assert_eq!(have: screened.coverage.builtin_rows, want: 6);
    sim_assert_eq!(have: screened.coverage.distinct_overlays, want: 5);
    sim_assert_eq!(have: screened.coverage.rows_screened, want: 6);
    let document = screened
        .flips
        .iter()
        .find(|probe| probe.overlay == json!({"child": {"first": true}}))
        .and_then(|probe| probe.document.as_ref())
        .ok_or_eyre("deduplicated partial row absent")?;
    sim_assert_eq!(have: document.origins.len(), want: 2);
    sim_assert_eq!(have: &document.control_overlays, want: &vec![json!({})]);
    request.available_probes = 14;
    let capped = screen(&request)?;
    assert!(capped.coverage.validate().is_err());
    assert!(
        capped
            .coverage
            .failures
            .iter()
            .any(|failure| failure.contains("cap would omit 5 overlays"))
    );
    Ok(())
}

#[test]
fn partial_override_template_abort_retains_the_existing_lint_oracle() -> eyre::Result<()> {
    let source = partial_override_chart()?;
    fs::write(
        source.path().join("charts/child/values.yaml"),
        serde_json::to_vec(&json!({"first": true, "second": 2}))?,
    )?;
    fs::write(source.path().join("values.yaml"), "child: {first: false}\n")?;
    // Helm's lint mode ignores fail; template execution still aborts.
    fs::write(
        source.path().join("templates/control.yaml"),
        indoc::indoc! {r#"
        {{ if .Values.child.first }}{{ fail "template-only failure" }}{{ end }}
    "#},
    )?;
    let restrictive = json!({"properties": {"child": {"required": ["second"]}}});
    let schemas = LintSchemas::compile(&restrictive, &restrictive, None)?;
    let screened = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    let probe = screened
        .flips
        .iter()
        .find(|probe| probe.overlay == json!({"child": {"first": true}}))
        .ok_or_eyre("partial control absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
    let cache = ScratchDir::new("partial-template-abort-kubernetes")?;
    let kubernetes = super::OfflineKubernetesValidator::new(
        cache.path(),
        super::helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let (verdict, _) = super::adjudicate_lint_flip(
        &chart,
        probe,
        probe
            .document
            .as_ref()
            .ok_or_eyre("partial document absent")?,
        &schemas,
        &kubernetes,
    )?;
    sim_assert_eq!(have: verdict, want: super::HelmFlipVerdict::TighteningMatchedHelmAbort);
    Ok(())
}
