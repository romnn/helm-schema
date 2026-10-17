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
    assert!(!capped.coverage.failures.is_empty());
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
    assert!(!controls.is_empty());
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
    let overlay = json!({"gate":true,"payload":1});
    let mut row = context_row(source.path(), &schemas, overlay.clone())?;
    row.context_overlay_sha256 = sha256_hex(&serde_json::to_vec(&context_overlay)?);
    row.context_overlay = context_overlay;
    let screened = screen(&ScreenRequest {
        chart: source.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[row],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
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
        .ok_or_eyre("context raw flip absent")?;
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        source.path(),
    )?;
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
        &schemas,
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
        error.contains("tightening rejects a lint document"),
        "{error}"
    );
    sim_assert_eq!(
        have: context_probe_verdict(template, json!({}))?,
        want: Ok(super::HelmFlipVerdict::TighteningMatchedKubernetesRejection)
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
