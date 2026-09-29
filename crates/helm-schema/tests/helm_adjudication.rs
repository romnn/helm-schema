//! Checks exact Helm values and offline rendered-resource adjudication.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use helm_schema_k8s::cache::{CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, k8s_cache_path};
use indoc::{formatdoc, indoc};
use serde_json::json;
use test_util::helm_values::AcceptanceDocument;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use helm_schema_test_support::helm::adjudication as helm_adjudication;
use helm_schema_test_support::helm::invocation as helm_invocation;
use helm_schema_test_support::helm::kubernetes_version::chart_kubernetes_version;

use helm_adjudication::{KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart};
use helm_invocation::{Outcome, stage_totals};

fn write_chart(root: &Path, name: &str, values: &str) -> eyre::Result<()> {
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        formatdoc! {"
            apiVersion: v2
            name: {name}
            version: 1.0.0
        "},
    )?;
    fs::write(root.join("values.yaml"), values)?;
    Ok(())
}

#[test]
fn unchanged_unknown_resources_pair_with_their_defaults() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "differential", "token: original\n")?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        indoc! {r"
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: sample
        spec:
          token: {{ .Values.token }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    assert!(matches!(
        validator.validate(
            helm_invocation::HelmRunner::shared()?,
            &probe.rendered.stdout
        )?,
        KubernetesVerdict::Uncertain(_)
    ));
    let unchanged = validator.compare_with_defaults(&chart, &probe)?;
    assert!(
        unchanged.uncertain.is_empty() && unchanged.new_violations.is_empty(),
        "{unchanged:?}"
    );
    let changed = chart.adjudicate(&json!({"token": "changed"}))?;
    sim_assert_eq!(
        have: validator.compare_with_defaults(&chart, &changed)?.uncertain,
        want: vec!["document 0: example.test/v1/Unknown sample: pinned resource schema not found".to_string()],
    );
    Ok(())
}

/// Every Helm child a probe runs is measured under its stage, beside its outputs.
#[test]
fn every_helm_child_is_recorded_under_its_stage() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "measured", "token: original\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/configmap.yaml"),
        indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: measured
        data:
          token: {{ .Values.token }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"token": "changed"}))?;
    OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
        .compare_with_defaults(&chart, &probe)?;
    let records = chart.invocations();
    let stages: Vec<(String, usize)> = stage_totals(&records)
        .into_iter()
        .map(|(stage, totals)| (stage, totals.invocations))
        .collect();
    sim_assert_eq!(
        have: stages,
        want: vec![
            ("coalesce".to_string(), 2),
            ("control".to_string(), 1),
            ("decode".to_string(), 2),
            ("render".to_string(), 1),
        ],
    );
    eyre::ensure!(
        records.iter().all(|record| record.max_rss_bytes > 0),
        "every child reports its own peak memory: {records:?}"
    );
    sim_assert_eq!(
        have: chart.peak_child_rss(),
        want: records.iter().map(|record| record.max_rss_bytes).max().unwrap_or(0),
    );
    eyre::ensure!(
        probe.evidence_dir.join("render.invocation.json").is_file()
            && probe
                .evidence_dir
                .join("decode/decode.invocation.json")
                .is_file(),
        "measurements are retained beside the outputs"
    );
    Ok(())
}

/// A chart that reads the clock never replays its renders, while its
/// coalescence and decoding, which run fixed templates, stay replayable.
#[test]
fn only_renders_of_a_nondeterministic_chart_bypass_replay() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "clock", "token: original\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/configmap.yaml"),
        indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: clock
        data:
          token: {{ .Values.token }}
          year: {{ now | date "2006" | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"token": "changed"}))?;
    OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
        .compare_with_defaults(&chart, &probe)?;
    let mut outcomes = BTreeMap::new();
    for record in chart.invocations() {
        outcomes.insert(record.stage, record.outcome);
    }
    let bypass = Outcome::Bypassed("templates/configmap.yaml: calls date (and 1 more)".to_string());
    sim_assert_eq!(
        have: outcomes,
        want: BTreeMap::from([
            ("coalesce".to_string(), Outcome::Executed),
            ("control".to_string(), bypass.clone()),
            ("decode".to_string(), Outcome::Executed),
            ("render".to_string(), bypass),
        ]),
    );
    Ok(())
}

/// Compiled schemas are shared by bundle content, never by location: a
/// bundle with the same content elsewhere reuses them, and a bundle with
/// different content compiles its own. A bundle that changes afterwards is
/// reported.
#[test]
fn compiled_schemas_are_shared_by_bundle_content() -> eyre::Result<()> {
    let strict = json!({"type": "object", "properties": {
        "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
        "metadata": {"properties": {"name": {"type": "string"}}}
    }});
    let [original, copy, different] = [
        ScratchDir::new("helm_adjudication")?,
        ScratchDir::new("helm_adjudication")?,
        ScratchDir::new("helm_adjudication")?,
    ];
    write_cached_schema(original.path(), "configmap-v1.json", &strict)?;
    write_cached_schema(copy.path(), "configmap-v1.json", &strict)?;
    let loose = json!({"properties": {
        "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}
    }});
    write_cached_schema(different.path(), "configmap-v1.json", &loose)?;
    let original =
        OfflineKubernetesValidator::new(original.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let copy_validator =
        OfflineKubernetesValidator::new(copy.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let different =
        OfflineKubernetesValidator::new(different.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let document = br"{apiVersion: v1, kind: ConfigMap, metadata: {name: 1}}";
    assert!(matches!(
        original.validate(helm_invocation::HelmRunner::shared()?, document)?,
        KubernetesVerdict::Invalid(_)
    ));
    // The copy can no longer compile anything itself; it judges by the
    // schema compiled for the identical original.
    fs::remove_dir_all(copy.path())?;
    fs::create_dir(copy.path())?;
    assert!(matches!(
        copy_validator.validate(helm_invocation::HelmRunner::shared()?, document)?,
        KubernetesVerdict::Invalid(_)
    ));
    sim_assert_eq!(have: different.validate(helm_invocation::HelmRunner::shared()?, document)?, want: KubernetesVerdict::Valid);
    assert!(original.verify_bundles_unchanged().is_ok());
    assert!(copy_validator.verify_bundles_unchanged().is_err());
    Ok(())
}

/// Identical documents pair one to one, so a copy the defaults render lacks
/// stays unproved even when it repeats a paired document.
#[test]
fn duplicate_documents_pair_with_multiplicity() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    for (defaults, overlay, uncertain) in [(1, 2, 1), (2, 1, 0), (2, 2, 0), (1, 3, 2)] {
        let root = ScratchDir::new("helm_adjudication")?;
        write_chart(root.path(), "duplicates", &format!("copies: {defaults}\n"))?;
        fs::write(
            root.path().join("templates/resources.yaml"),
            indoc! {r"
            {{ range until (int .Values.copies) }}
            ---
            apiVersion: example.test/v1
            kind: Unknown
            metadata:
              namespace: example
              name: duplicate
            {{ end }}
        "},
        )?;
        let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
        let probe = chart.adjudicate(&json!({"copies": overlay}))?;
        let comparison =
            OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
                .compare_with_defaults(&chart, &probe)?;
        sim_assert_eq!(have: comparison.uncertain.len(), want: uncertain);
    }
    Ok(())
}

#[test]
fn a_changed_resource_violation_is_new_beside_an_unchanged_unknown() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "invalid", "token: valid\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/resources.yaml"),
        indoc! {r"
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: same
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: invalid
        data:
          token: {{ .Values.token }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"token": true}))?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let comparison = validator.compare_with_defaults(&chart, &probe)?;
    assert!(
        comparison.uncertain.is_empty() && comparison.inherited_violations.is_empty(),
        "{comparison:?}"
    );
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(ToString::to_string).collect::<Vec<_>>(),
        want: vec![r#"document 0: v1/ConfigMap invalid: /data/token: true is not of type "string""#.to_string()],
    );
    Ok(())
}

/// The defaults render's violations are keyed by resource type, pointer and
/// violated assertion, so a changed document inherits them wherever it lands.
#[test]
fn defaults_violations_are_keyed_by_resource_not_position() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "defaults", "extra: false\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/resources.yaml"),
        indoc! {r#"
        {{- if .Values.extra }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: extra
        ---
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: invalid
          labels:
            extra: "{{ .Values.extra }}"
        data:
          token: true
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let probe = chart.adjudicate(&json!({"extra": true}))?;
    let comparison = validator.compare_with_defaults(&chart, &probe)?;
    assert!(
        comparison.new_violations.is_empty() && comparison.uncertain.is_empty(),
        "{comparison:?}"
    );
    sim_assert_eq!(
        have: comparison.inherited_violations.iter().map(ToString::to_string).collect::<Vec<_>>(),
        want: vec![r#"document 1: v1/ConfigMap invalid: /data/token: true is not of type "string""#.to_string()],
    );
    Ok(())
}

/// A changed document carrying another invalid value at the defaults'
/// violating pointer violates the assertion anew: the offending value is
/// part of the violation.
#[test]
fn a_different_invalid_value_is_a_new_violation() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "revalued", "token: true\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: revalued
        data:
          token: {{ .Values.token }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"token": 5}))?;
    let comparison =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
            .compare_with_defaults(&chart, &probe)?;
    assert!(
        comparison.inherited_violations.is_empty() && comparison.uncertain.is_empty(),
        "{comparison:?}"
    );
    sim_assert_eq!(
        have: comparison.new_violations.iter().map(ToString::to_string).collect::<Vec<_>>(),
        want: vec![r#"document 0: v1/ConfigMap revalued: /data/token: 5 is not of type "string""#.to_string()],
    );
    Ok(())
}

/// A violation is identified by resource type, pointer and violated
/// assertion, not by the resource's name: a probe that only renames a
/// resource (`nameOverride`) renders the defaults' own violation.
#[test]
fn a_renamed_resource_inherits_the_defaults_violations() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    let cache = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "renamed", "name: original\n")?;
    write_configmap_schema(cache.path())?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
        data:
          token: true
    "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"name": "renamed"}))?;
    let comparison =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
            .compare_with_defaults(&chart, &probe)?;
    assert!(
        comparison.new_violations.is_empty() && comparison.uncertain.is_empty(),
        "{comparison:?}"
    );
    sim_assert_eq!(have: comparison.inherited_violations.len(), want: 1);
    Ok(())
}

/// A number beyond Helm's exact integer range hides what the document was,
/// so a changed one stays undecided.
#[test]
fn inexact_documents_stay_uncertain() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "uncertain", "{}")?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        indoc! {"
            apiVersion: example.test/v1
            kind: Unknown
            metadata:
              name: inexact
            value: 9007199254740993
        "},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    assert!(matches!(
        validator.validate(
            helm_invocation::HelmRunner::shared()?,
            &probe.rendered.stdout
        )?,
        KubernetesVerdict::Uncertain(_)
    ));
    let comparison = validator.compare_with_defaults(&chart, &probe)?;
    sim_assert_eq!(have: comparison.uncertain.len(), want: 1);
    Ok(())
}

fn write_configmap_schema(cache: &Path) -> eyre::Result<()> {
    write_cached_schema(
        cache,
        "configmap-v1.json",
        &json!({
            "type": "object", "properties": {
                "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
                "data": {"type": "object", "additionalProperties": {"type": "string"}}
            }
        }),
    )
}

#[test]
fn coalescence_preserves_null_ownership_before_render_mutation() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "parent", "owned: 1\n")?;
    write_chart(&root.path().join("charts/child"), "child", "owned: 2\n")?;
    fs::write(
        root.path().join("templates/config.yaml"),
        indoc! {r#"
        {{- $_ := set .Values "mutated" true -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          mutated: {{ .Values.mutated | quote }}
          kubernetes: {{ .Capabilities.KubeVersion.Version | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    sim_assert_eq!(have: chart.adjudicate(&json!({}))?.values.ok_or_eyre("Helm aborted coalescence")?.as_json(), want: &json!({"owned": 1, "child": {"owned": 2, "global": {}}}));

    // Only defaults owned by the consuming chart delete user nulls.
    let overlay = json!({"owned": null, "absent": null, "child": {"owned": null, "absent": null}});
    let expected = json!({"absent": null, "child": {"absent": null, "global": {}}});
    let probe = chart.adjudicate(&overlay)?;
    sim_assert_eq!(have: probe.values.as_ref().ok_or_eyre("Helm aborted coalescence")?.as_json(), want: &expected);
    eyre::ensure!(
        probe.rendered.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    let rendered: serde_json::Value = serde_yaml::from_slice(&probe.rendered.stdout)?;
    sim_assert_eq!(have: rendered, want: json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "example"}, "data": {"mutated": "true", "kubernetes": "v1.29.0"}}));
    sim_assert_eq!(have: serde_json::from_slice::<serde_json::Value>(&fs::read(probe.evidence_dir.join("coalesced.json"))?)?, want: expected);
    sim_assert_eq!(have: serde_json::from_slice::<serde_json::Value>(&fs::read(probe.evidence_dir.join("values.json"))?)?, want: overlay);
    Ok(())
}

/// Helm validates three different documents, and the Rust port composes
/// each: a root schema `{"const": D}` passes exactly the check that
/// validates `D`. Lint's values rule keeps the values file's nulls, lint's
/// template rule coalesces twice and refills the deleted defaults, and
/// `helm template` coalesces once.
#[test]
fn helm_validates_the_three_documents_the_port_composes() -> eyre::Result<()> {
    let source = test_util::workspace_testdata().join("helm-values/nulls");
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, &source)?;
    let overlay = json!({
        "owned": null, "absent": null, "defaultNull": null,
        "nested": {"owned": null, "absent": null, "defaultNull": null},
    });
    let mut documents = BTreeMap::new();
    for (kind, document) in chart.acceptance_documents(&overlay) {
        documents.insert(kind, document?);
    }
    let distinct: BTreeSet<String> = documents.values().map(ToString::to_string).collect();
    sim_assert_eq!(have: distinct.len(), want: AcceptanceDocument::ALL.len());
    for (kind, document) in &documents {
        sim_assert_eq!(
            have: helm_schema_acceptance(&source, &overlay, &json!({"const": document}))?,
            want: BTreeSet::from([*kind]),
            "the {kind:?} document"
        );
    }
    Ok(())
}

/// The documents whose schema check Helm v4.2.3 passes when `schema` is the
/// root chart's `values.schema.json`: lint's values and template rules, and
/// `helm template`.
fn helm_schema_acceptance(
    chart: &Path,
    overlay: &serde_json::Value,
    schema: &serde_json::Value,
) -> eyre::Result<BTreeSet<AcceptanceDocument>> {
    let work = ScratchDir::new("helm_adjudication")?;
    let copy = work.path().join("chart");
    copy_dir(chart, &copy)?;
    fs::write(copy.join("values.schema.json"), serde_json::to_vec(schema)?)?;
    let values = work.path().join("values.json");
    fs::write(&values, serde_json::to_vec(overlay)?)?;
    let mut accepted = BTreeSet::from(AcceptanceDocument::ALL);

    let lint = Command::new("helm")
        .envs(test_util::scratch::temp_env()?)
        .arg("lint")
        .arg(&copy)
        .arg("-f")
        .arg(&values)
        .output()?;
    let report = String::from_utf8_lossy(&lint.stdout);
    for line in report.lines() {
        if line.starts_with("[ERROR] values.yaml:") {
            accepted.remove(&AcceptanceDocument::LintRaw);
        } else if line.starts_with("[ERROR] templates/: values don't meet the specifications") {
            accepted.remove(&AcceptanceDocument::LintCoalescedTwice);
        } else if line.starts_with("[ERROR]") {
            eyre::bail!("helm lint failed for another reason: {report}");
        }
    }

    let template = Command::new("helm")
        .envs(test_util::scratch::temp_env()?)
        .args(["template", "acceptance"])
        .arg(&copy)
        .arg("-f")
        .arg(&values)
        .output()?;
    if !template.status.success() {
        let stderr = String::from_utf8_lossy(&template.stderr);
        eyre::ensure!(
            stderr.contains("values don't meet the specifications"),
            "helm template failed for another reason: {stderr}"
        );
        accepted.remove(&AcceptanceDocument::Template);
    }
    Ok(accepted)
}

fn copy_dir(from: &Path, to: &Path) -> eyre::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[test]
fn packed_dependencies_are_sanitized_recursively_without_changing_sources() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "parent", "{}")?;
    fs::rename(
        root.path().join("Chart.yaml"),
        root.path().join("Chart.template.yaml"),
    )?;
    fs::write(root.path().join("values.schema.json"), "false")?;
    fs::create_dir_all(root.path().join("templates/tests"))?;
    fs::write(
        root.path().join("templates/tests/fail.yaml"),
        "{{ fail \"excluded root test\" }}",
    )?;

    let child = ScratchDir::new("helm_adjudication")?;
    write_chart(child.path(), "child", "childDefault: 3\n")?;
    fs::write(child.path().join("values.schema.json"), "false")?;
    fs::create_dir_all(child.path().join("templates/tests"))?;
    fs::write(
        child.path().join("templates/tests/fail.yaml"),
        "{{ fail \"excluded child test\" }}",
    )?;
    let grandchild = ScratchDir::new("helm_adjudication")?;
    write_chart(grandchild.path(), "grandchild", "leaf: 4\n")?;
    fs::write(grandchild.path().join("values.schema.json"), "false")?;
    pack_chart(
        grandchild.path(),
        &child.path().join("charts/grandchild.tgz"),
        "grandchild",
    )?;
    let archive = root.path().join("charts/child.tgz");
    pack_chart(child.path(), &archive, "child")?;
    let original = fs::read(&archive)?;

    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    sim_assert_eq!(have: probe.values.as_ref().ok_or_eyre("Helm aborted coalescence")?.as_json(), want: &json!({"child": {"childDefault": 3, "global": {}, "grandchild": {"leaf": 4, "global": {}}}}));
    sim_assert_eq!(have: fs::read(&archive)?, want: original);
    sim_assert_eq!(have: fs::read_to_string(root.path().join("values.schema.json"))?, want: "false");
    assert!(!root.path().join("Chart.yaml").exists());
    let render_tree = &chart.render_tree().path();
    assert!(!render_tree.join("charts/child").exists());
    let render = archive_files(&fs::read(render_tree.join("charts/child.tgz"))?)?;
    assert!(!render.contains_key(Path::new("child/values.schema.json")));
    assert!(
        !render
            .keys()
            .any(|path| path.starts_with("child/templates/tests"))
    );
    let grandchild = archive_files(
        render
            .get(Path::new("child/charts/grandchild.tgz"))
            .ok_or_eyre("missing nested archive")?,
    )?;
    assert!(!grandchild.contains_key(Path::new("grandchild/values.schema.json")));
    let coalesce = archive_files(&fs::read(
        chart.coalesce_tree().path().join("charts/child.tgz"),
    )?)?;
    assert!(
        !coalesce
            .keys()
            .any(|path| path.starts_with("child/templates"))
    );
    Ok(())
}

/// Preparation hashes and repacks only chart content: two preparations of
/// the same chart from copies with different timestamps, made at different
/// times, yield identical trees and archive bytes.
#[test]
fn preparation_is_independent_of_timestamps_and_preparation_time() -> eyre::Result<()> {
    let child = ScratchDir::new("helm_adjudication")?;
    write_chart(child.path(), "child", "childDefault: 3\n")?;
    fs::write(
        child.path().join("templates/config.yaml"),
        "kind: ConfigMap\n",
    )?;
    let mut prepared = Vec::new();
    for (index, mtime) in [(0_u64, 1_000_000_000_u64), (1, 1_700_000_000)] {
        let root = ScratchDir::new("helm_adjudication")?;
        write_chart(root.path(), "parent", "{}")?;
        fs::create_dir_all(root.path().join("charts/local/templates"))?;
        write_chart(&root.path().join("charts/local"), "local", "leaf: 1\n")?;
        pack_chart(child.path(), &root.path().join("charts/child.tgz"), "child")?;
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
        for file in [
            "Chart.yaml",
            "values.yaml",
            "charts/child.tgz",
            "charts/local/values.yaml",
        ] {
            fs::File::options()
                .write(true)
                .open(root.path().join(file))?
                .set_modified(time)?;
        }
        if index == 1 {
            // Archive headers carry whole seconds; a later preparation must not differ.
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
        let archive = fs::read(chart.render_tree().path().join("charts/child.tgz"))?;
        prepared.push((
            chart.render_tree().sha256().to_string(),
            chart.coalesce_tree().sha256().to_string(),
            archive,
        ));
    }
    let [first, second] = prepared.as_slice() else {
        eyre::bail!("expected two preparations");
    };
    sim_assert_eq!(have: second, want: first);
    Ok(())
}

fn archive_files(bytes: &[u8]) -> eyre::Result<BTreeMap<std::path::PathBuf, Vec<u8>>> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut files = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_file() {
            let path = entry.path()?.into_owned();
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            files.insert(path, contents);
        }
    }
    Ok(files)
}

fn pack_chart(root: &Path, archive: &Path, name: &str) -> eyre::Result<()> {
    fs::create_dir_all(archive.parent().ok_or_eyre("archive has no parent")?)?;
    let encoder = GzEncoder::new(fs::File::create(archive)?, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.append_dir_all(name, root)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

#[test]
fn render_abort_keeps_the_exact_document_and_evidence() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "aborting", "{}")?;
    fs::write(
        root.path().join("templates/fail.yaml"),
        "{{ fail \"intentional abort\" }}",
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({"unknown": null}))?;
    sim_assert_eq!(have: probe.values.as_ref().ok_or_eyre("Helm aborted coalescence")?.as_json(), want: &json!({"unknown": null}));
    assert!(!probe.rendered.success());
    assert!(
        fs::read_to_string(probe.evidence_dir.join("render.stderr"))?.contains("intentional abort")
    );
    Ok(())
}

#[test]
fn offline_validator_distinguishes_invalid_resources_from_missing_schemas() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let schema = json!({"type": "object", "properties": {
        "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
        "metadata": {"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}
    }, "required": ["apiVersion", "kind", "metadata"]});
    write_cached_schema(cache.path(), "configmap-v1.json", &schema)?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let valid = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "valid"}});
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, &serde_json::to_vec(&valid)?)?, want: KubernetesVerdict::Valid);

    // A List wrapper must not hide its contained resource's typed violation.
    let list = json!({"apiVersion": "v1", "kind": "List", "items": [
        {"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": true}}
    ]});
    let KubernetesVerdict::Invalid(errors) = validator.validate(
        helm_invocation::HelmRunner::shared()?,
        &serde_json::to_vec(&list)?,
    )?
    else {
        eyre::bail!("wrongly typed ConfigMap name was not rejected");
    };
    eyre::ensure!(
        errors.len() == 1
            && errors
                .iter()
                .any(|error| error.location == "document 0/items/0"
                    && error.key.instance_path == "/metadata/name"),
        "unexpected evidence: {errors:?}"
    );
    let missing = json!({"apiVersion": "example.test/v1", "kind": "Missing", "metadata": {"name": "unknown"}});
    let KubernetesVerdict::Uncertain(errors) = validator.validate(
        helm_invocation::HelmRunner::shared()?,
        &serde_json::to_vec(&missing)?,
    )?
    else {
        eyre::bail!("missing offline schema was treated as a verdict");
    };
    sim_assert_eq!(have: errors, want: vec!["document 0: example.test/v1/Missing unknown: pinned resource schema not found".to_string()]);
    Ok(())
}

#[test]
fn unresolved_schema_references_do_not_fetch_or_prove_validity() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}, "allOf": [{"$ref": "https://invalid.example.test/not-cached.json"}]}),
    )?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let document =
        json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "unknown"}});
    let KubernetesVerdict::Uncertain(errors) = validator.validate(
        helm_invocation::HelmRunner::shared()?,
        &serde_json::to_vec(&document)?,
    )?
    else {
        eyre::bail!("missing reference was treated as a resource verdict");
    };
    eyre::ensure!(
        errors.iter().any(
            |error| error.contains("original resource schema did not compile")
                && error.contains("not-cached.json")
        ),
        "missing reference evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn helm_yaml_decoding_preserves_boolean_keys_octal_scalars_and_unicode_boundaries()
-> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "café"}, "data": {"true": 10, "plain": true, "quoted": "on", "config": "---\nyes\n"}});
    // An exact schema makes every decoded scalar and key part of the assertion.
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let source = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: café
        data:
          yes: 012
          plain: on
          quoted: 'on'
          config: |
            ---
            yes
    "};
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?, want: KubernetesVerdict::Valid);
    let repeated = format!("{source}{source}");
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, repeated.as_bytes())?, want: KubernetesVerdict::Valid);
    let typed_name = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: yes
    "};
    let KubernetesVerdict::Invalid(errors) = validator.validate(
        helm_invocation::HelmRunner::shared()?,
        format!("{source}{typed_name}").as_bytes(),
    )?
    else {
        eyre::bail!("YAML 1.1 boolean name was not rejected");
    };
    eyre::ensure!(
        errors.len() == 1 && errors.iter().all(|error| error.location == "document 1"),
        "unexpected evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn helm_yaml_decoder_errors_remain_uncertain() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    // Helm fromYaml reports sequence roots through its Error member.
    let source = "[one, two]";
    let KubernetesVerdict::Uncertain(errors) =
        validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?
    else {
        eyre::bail!("unsupported YAML document was treated as a resource verdict");
    };
    eyre::ensure!(
        errors
            .iter()
            .any(|error| error.contains("Helm fromYaml reported an error")),
        "missing decoding evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn raw_document_transport_preserves_unicode_line_breaks() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "transport"}, "data": {"nel": "before\nafter\n", "ls": "before\u{2028}after\n", "ps": "before\u{2029}after\n"}});
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let source = formatdoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: transport
        data:
          nel: |
            before{nel}    after
          ls: |
            before{ls}    after
          ps: |
            before{ps}    after
    ", nel = '\u{85}', ls = '\u{2028}', ps = '\u{2029}'};
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    // A values-file round trip folds these characters before fromYaml can read the document.
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?, want: KubernetesVerdict::Valid);
    Ok(())
}

#[test]
fn kubernetes_document_framing_preserves_unicode_yaml_and_physical_boundaries() -> eyre::Result<()>
{
    let cache = ScratchDir::new("helm_adjudication")?;
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"properties": {
            "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
            "metadata": {"properties": {"name": {"type": "string"}}}
        }}),
    )?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let lines = [
        "# comment",
        "apiVersion: v1",
        "kind: ConfigMap",
        "metadata:",
        "  name: false",
    ];
    let invalid = validator.validate(
        helm_invocation::HelmRunner::shared()?,
        lines.join("\n").as_bytes(),
    )?;
    eyre::ensure!(
        matches!(invalid, KubernetesVerdict::Invalid(_)),
        "control must reject"
    );

    // Unicode YAML breaks terminate comments even though they do not frame separate resources.
    for separator in [
        "\u{85}",
        "\u{2028}",
        "\u{2029}",
        "\r\u{85}",
        "\r\u{2028}",
        "\r\u{2029}",
    ] {
        sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, lines.join(separator).as_bytes())?, want: invalid);
        let source = formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata: {{name: valid}}{separator}---{separator}apiVersion: v1{separator}kind: ConfigMap{separator}metadata: {{name: false}}
        "};
        sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?, want: KubernetesVerdict::Valid);
    }

    // A physical separator introduces the second resource; its trailing comment is allowed.
    let source = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata: {name: valid}
        --- # second
        apiVersion: v1
        kind: ConfigMap
        metadata: {name: false}
    "};
    let KubernetesVerdict::Invalid(errors) =
        validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?
    else {
        eyre::bail!("physical separator hid the second resource");
    };
    eyre::ensure!(
        errors.iter().all(|error| error.location == "document 1"),
        "{errors:?}"
    );
    assert!(
        validator
            .validate(helm_invocation::HelmRunner::shared()?, b"---invalid\n")
            .is_err()
    );
    Ok(())
}

#[test]
fn integers_that_helm_normalization_can_round_remain_uncertain() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "number"}, "value": 9_007_199_254_740_993_u64});
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    let source = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: number
        value: 9007199254740993
    "};
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, source.as_bytes())?, want: KubernetesVerdict::Uncertain(vec!["document 0: numeric magnitude exceeds Helm fromYaml's exact integer range".to_string()]));
    Ok(())
}

#[test]
fn archive_links_are_rejected_before_helm_execution() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "parent", "{}")?;
    fs::create_dir_all(root.path().join("charts"))?;
    let archive = fs::File::create(root.path().join("charts/unsafe.tgz"))?;
    let mut builder = tar::Builder::new(GzEncoder::new(archive, Compression::default()));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_mode(0o777);
    header.set_size(0);
    header.set_link_name("../../outside")?;
    header.set_cksum();
    builder.append_data(&mut header, "unsafe/link", std::io::empty())?;
    builder.into_inner()?.finish()?;
    let result = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path());
    let Err(error) = result else {
        eyre::bail!("archive symlink was accepted");
    };
    assert!(error.to_string().contains("unsupported archive member"));
    Ok(())
}

#[test]
fn schema_identity_must_match_the_exact_resource_group() -> eyre::Result<()> {
    let cache = ScratchDir::new("helm_adjudication")?;
    let schema = json!({"properties": {
        "apiVersion": {"enum": ["apps/v1"]}, "kind": {"enum": ["Deployment"]},
        "spec": {"properties": {"replicas": {"type": "integer"}}}
    }});
    write_cached_schema(cache.path(), "deployment-apps-v1.json", &schema)?;
    let validator =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    // The inference provider's short filename fallback must not establish schema ownership.
    let custom = json!({"apiVersion": "apps.example.test/v1", "kind": "Deployment", "metadata": {"name": "custom"}, "spec": {"replicas": "many"}});
    sim_assert_eq!(have: validator.validate(helm_invocation::HelmRunner::shared()?, &serde_json::to_vec(&custom)?)?, want: KubernetesVerdict::Uncertain(vec!["document 0: apps.example.test/v1/Deployment custom: original schema does not prove exact identity apps.example.test/v1/Deployment".to_string()]));
    let builtin = json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "builtin"}, "spec": {"replicas": "many"}});
    assert!(matches!(
        validator.validate(
            helm_invocation::HelmRunner::shared()?,
            &serde_json::to_vec(&builtin)?
        )?,
        KubernetesVerdict::Invalid(_)
    ));
    Ok(())
}

#[test]
fn schema_identity_metadata_must_be_complete_and_consistent() -> eyre::Result<()> {
    let resource = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": false}});
    let metadata = json!([{"group": "", "version": "v1", "kind": "ConfigMap"}]);
    for (schema, proves_identity) in [
        (
            json!({"x-kubernetes-group-version-kind": metadata, "properties": {"metadata": {"properties": {"name": {"type": "string"}}}}}),
            true,
        ),
        (
            json!({"x-kubernetes-group-version-kind": metadata, "properties": {"apiVersion": {"const": "other/v1"}}}),
            false,
        ),
        (
            json!({"x-kubernetes-group-version-kind": [{"version": "v1", "kind": "ConfigMap"}], "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"properties": {"metadata": {"properties": {"name": {"type": "string"}}}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "$ref": "#/definitions/shape", "definitions": {"shape": {}}, "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-04/schema#", "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "definitions": {"shape": {}}, "properties": {"apiVersion": {"$ref": "#/definitions/shape", "const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
    ] {
        let cache = ScratchDir::new("helm_adjudication")?;
        write_cached_schema(cache.path(), "configmap-v1.json", &schema)?;
        let validator =
            OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
        let verdict = validator.validate(
            helm_invocation::HelmRunner::shared()?,
            &serde_json::to_vec(&resource)?,
        )?;
        if proves_identity {
            assert!(
                matches!(verdict, KubernetesVerdict::Invalid(_)),
                "{verdict:?}"
            );
        } else {
            assert!(
                matches!(verdict, KubernetesVerdict::Uncertain(_)),
                "{verdict:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn packed_dependencies_preserve_the_parent_ignore_boundary() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "parent", "{}")?;
    fs::write(root.path().join(".helmignore"), "payload.txt")?;
    let child = ScratchDir::new("helm_adjudication")?;
    write_chart(child.path(), "child", "{}")?;
    fs::write(child.path().join("payload.txt"), "kept")?;
    fs::write(
        child.path().join("templates/configmap.yaml"),
        indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: child
        data:
          payload: {{ required "child file disappeared" (.Files.Get "payload.txt") | quote }}
    "#},
    )?;
    pack_chart(child.path(), &root.path().join("charts/child.tgz"), "child")?;
    let original = Command::new("helm")
        .envs(test_util::scratch::temp_env()?)
        .args(["template", "adjudication"])
        .arg(root.path())
        .args(["--kube-version", "1.29.0"])
        .output()?;
    eyre::ensure!(
        original.status.success(),
        "{}",
        String::from_utf8_lossy(&original.stderr)
    );
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "child"}, "data": {"payload": "kept"}});
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&original.stdout)?, want: expected);
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&probe.rendered.stdout)?, want: expected);
    Ok(())
}

#[test]
fn template_exclusions_only_apply_to_actual_chart_roots() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "files", "{}")?;
    fs::create_dir_all(root.path().join("files/templates/tests"))?;
    fs::write(
        root.path().join("files/templates/tests/payload.txt"),
        "kept",
    )?;
    fs::write(root.path().join("files/values.schema.json"), "asset")?;
    fs::write(
        root.path().join("templates/configmap.yaml"),
        indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: files
        data:
          payload: {{ required "data file disappeared" (.Files.Get "files/templates/tests/payload.txt") | quote }}
          schema: {{ required "schema asset disappeared" (.Files.Get "files/values.schema.json") | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&probe.rendered.stdout)?, want: json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "files"}, "data": {"payload": "kept", "schema": "asset"}}));
    Ok(())
}

fn write_cached_schema(
    cache: &Path,
    filename: &str,
    schema: &serde_json::Value,
) -> eyre::Result<()> {
    fs::write(
        cache.join(LAYOUT_MARKER_FILENAME),
        CACHE_LAYOUT_VERSION.to_string(),
    )?;
    let path = k8s_cache_path(
        cache,
        helm_schema_k8s::default_source_id(),
        "v1.29.0-standalone-strict",
        filename,
    );
    fs::create_dir_all(path.parent().ok_or_eyre("schema has no parent")?)?;
    fs::write(path, serde_json::to_vec(schema)?)?;
    Ok(())
}

/// The violations `validate` proves for a chart rendering `template` with
/// its defaults, judged against the pinned bundle.
fn pinned_violations(name: &str, template: &str) -> eyre::Result<KubernetesVerdict> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), name, "{}\n")?;
    fs::write(root.path().join("templates/resource.yaml"), template)?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?.validate(
        helm_invocation::HelmRunner::shared()?,
        &probe.rendered.stdout,
    )
}

/// A built-in API group version the pinned v1.29 server no longer serves is a
/// proven rejection: the bundle records the kind as absent upstream.
#[test]
fn an_api_the_pinned_server_does_not_serve_is_a_violation() -> eyre::Result<()> {
    for (api_version, kind) in [
        ("extensions/v1beta1", "Ingress"),
        ("networking.k8s.io/v1beta1", "Ingress"),
        ("policy/v1beta1", "PodSecurityPolicy"),
    ] {
        let verdict = pinned_violations(
            "removed",
            &formatdoc! {"
                apiVersion: {api_version}
                kind: {kind}
                metadata:
                  name: removed
            "},
        )?;
        let KubernetesVerdict::Invalid(violations) = verdict else {
            eyre::bail!("{api_version}/{kind} is not a proven violation: {verdict:?}");
        };
        let messages: Vec<String> = violations.iter().map(ToString::to_string).collect();
        sim_assert_eq!(have: messages, want: vec![format!(
            "document 0: {api_version}/{kind} removed: /apiVersion: \
             {api_version}/{kind} is not served by the pinned Kubernetes version"
        )]);
    }
    Ok(())
}

/// A rendered document without an apiVersion or kind is no applicable
/// resource; an empty `{}` item is the common case.
#[test]
fn a_document_without_api_version_or_kind_is_a_violation() -> eyre::Result<()> {
    for template in ["{}\n", "kind: ConfigMap\n", "apiVersion: v1\n"] {
        let verdict = pinned_violations("unidentified", template)?;
        assert!(
            matches!(verdict, KubernetesVerdict::Invalid(_)),
            "{template:?}: {verdict:?}"
        );
    }
    // A document of comments alone is no resource at all: Kubernetes'
    // decoder reads it as null and skips it.
    let verdict = pinned_violations("comments", "# nothing rendered here\n")?;
    sim_assert_eq!(have: verdict, want: KubernetesVerdict::Valid);
    // The defaults render's own `{}` pairs and adds nothing.
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "unidentified", "{}\n")?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        "value: scalar\n",
    )?;
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let cache = ScratchDir::new("helm_adjudication")?;
    let comparison =
        OfflineKubernetesValidator::new(cache.path(), helm_adjudication::KUBERNETES_RELEASE)?
            .compare_with_defaults(&chart, &probe)?;
    assert!(
        comparison.new_violations.is_empty() && comparison.uncertain.is_empty(),
        "{comparison:?}"
    );
    Ok(())
}

/// A CRD kind whose schema the pinned CRD catalog holds is decided like a
/// built-in kind; one the catalog lacks stays undecided.
#[test]
fn a_pinned_crd_schema_decides_a_crd_resource() -> eyre::Result<()> {
    let judge = |template: &str| -> eyre::Result<KubernetesVerdict> {
        let root = ScratchDir::new("helm_adjudication")?;
        write_chart(root.path(), "crd", "{}\n")?;
        fs::write(root.path().join("templates/resource.yaml"), template)?;
        let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?;
        let probe = chart.adjudicate(&json!({}))?;
        let bundle = test_util::workspace_testdata().join("provider-bundle");
        OfflineKubernetesValidator::with_crd_catalog(
            &bundle.join("kubernetes-json-schema-cache"),
            &bundle.join("crds-catalog-cache"),
            helm_adjudication::KUBERNETES_RELEASE,
        )?
        .validate(
            helm_invocation::HelmRunner::shared()?,
            &probe.rendered.stdout,
        )
    };
    let issuer = |spec: &str| {
        formatdoc! {"
            apiVersion: cert-manager.io/v1
            kind: Issuer
            metadata:
              name: issuer
            spec: {spec}
        "}
    };
    sim_assert_eq!(
        have: judge(&issuer("{selfSigned: {}}"))?,
        want: KubernetesVerdict::Valid,
    );
    assert!(matches!(
        judge(&issuer("{selfSigned: 3}"))?,
        KubernetesVerdict::Invalid(_)
    ));
    let unpinned = indoc! {"
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: unpinned
    "};
    assert!(matches!(judge(unpinned)?, KubernetesVerdict::Uncertain(_)));
    Ok(())
}

/// The Kubernetes version a chart whose manifest declares `kube_version`
/// renders its defaults under, as the templates observe it.
fn rendered_kubernetes_version(kube_version: Option<&str>) -> eyre::Result<String> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "versioned", "{}\n")?;
    if let Some(constraint) = kube_version {
        fs::write(
            root.path().join("Chart.yaml"),
            formatdoc! {r#"
                apiVersion: v2
                name: versioned
                version: 1.0.0
                kubeVersion: "{constraint}"
            "#},
        )?;
    }
    fs::write(
        root.path().join("templates/version.yaml"),
        indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: version
        data:
          kubernetes: {{ .Capabilities.KubeVersion.Version | quote }}
    "},
    )?;
    let probe = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path())?
        .adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    let rendered: serde_json::Value = serde_yaml::from_slice(&probe.rendered.stdout)?;
    Ok(rendered
        .pointer("/data/kubernetes")
        .and_then(serde_json::Value::as_str)
        .ok_or_eyre("rendered no Kubernetes version")?
        .to_string())
}

#[test]
fn a_chart_without_a_kube_version_constraint_renders_under_the_policy_version() -> eyre::Result<()>
{
    sim_assert_eq!(have: rendered_kubernetes_version(None)?, want: "v1.29.0");
    Ok(())
}

#[test]
fn a_constraint_the_policy_version_satisfies_keeps_it() -> eyre::Result<()> {
    sim_assert_eq!(have: rendered_kubernetes_version(Some(">=1.25.0-0"))?, want: "v1.29.0");
    Ok(())
}

#[test]
fn a_constraint_above_the_policy_version_selects_the_first_pinned_version_it_admits()
-> eyre::Result<()> {
    sim_assert_eq!(have: rendered_kubernetes_version(Some(">=1.30.0-0"))?, want: "v1.33.0");
    Ok(())
}

/// The shared evaluator abstains on Masterminds' `x` wildcard; jira's
/// spelling is decided by its Helm-verified verdict.
#[test]
fn a_wildcard_constraint_takes_its_helm_verified_version() -> eyre::Result<()> {
    sim_assert_eq!(have: rendered_kubernetes_version(Some(">=1.21.x-0"))?, want: "v1.29.0");
    Ok(())
}

/// A spelling neither the evaluator nor a Helm verdict decides abstains.
#[test]
fn an_undecided_constraint_is_refused() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "undecided", "{}\n")?;
    fs::write(
        root.path().join("Chart.yaml"),
        indoc! {r#"
        apiVersion: v2
        name: undecided
        version: 1.0.0
        kubeVersion: ">=1.22.x-0"
    "#},
    )?;
    let error = chart_kubernetes_version(root.path())
        .err()
        .ok_or_eyre("decided a constraint no evaluator decides")?;
    sim_assert_eq!(
        have: error.to_string(),
        want: format!(
            "{}: kubeVersion \">=1.22.x-0\" is outside the supported constraint syntax",
            root.path().join("Chart.yaml").display()
        ),
    );
    Ok(())
}

/// Helm would refuse the chart under every pinned version, so there is no
/// render to adjudicate against.
#[test]
fn a_constraint_no_pinned_version_satisfies_is_refused() -> eyre::Result<()> {
    let root = ScratchDir::new("helm_adjudication")?;
    write_chart(root.path(), "unsatisfiable", "{}\n")?;
    fs::write(
        root.path().join("Chart.yaml"),
        indoc! {r#"
        apiVersion: v2
        name: unsatisfiable
        version: 1.0.0
        kubeVersion: "<1.20.0-0"
    "#},
    )?;
    eyre::ensure!(
        PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, root.path()).is_err(),
        "prepared a chart Helm refuses"
    );
    let error = chart_kubernetes_version(root.path())
        .err()
        .ok_or_eyre("chose a version the manifest rejects")?;
    sim_assert_eq!(
        have: error.to_string(),
        want: format!(
            "{}: no pinned Kubernetes version satisfies kubeVersion \"<1.20.0-0\"",
            root.path().join("Chart.yaml").display()
        ),
    );
    Ok(())
}

/// okteto's manifest requires `kubeVersion: >=1.33.0-0`, so Helm refuses the
/// chart under the corpus policy version before any values are read.
#[test]
fn okteto_defaults_render_under_the_version_its_manifest_admits() -> eyre::Result<()> {
    let chart = PinnedHelmChart::prepare(
        helm_invocation::HelmRunner::shared()?,
        &test_util::workspace_testdata().join("charts/okteto"),
    )?;
    let probe = chart.adjudicate(&json!({}))?;
    sim_assert_eq!(
        have: (probe.values.is_some(), String::from_utf8_lossy(&probe.rendered.stderr).into_owned()),
        want: (true, String::new()),
    );
    eyre::ensure!(probe.rendered.success(), "{}", probe.evidence_dir.display());
    Ok(())
}

/// Every corpus manifest's constraint is decided, and only okteto and
/// jupyterhub need a version above the policy version.
#[test]
fn corpus_charts_render_under_the_version_their_manifest_admits() -> eyre::Result<()> {
    let mut charts = fs::read_dir(test_util::workspace_testdata().join("charts"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    charts.sort();
    let mut raised = BTreeMap::new();
    for chart in charts {
        if !chart.is_dir() {
            continue;
        }
        let version = chart_kubernetes_version(&chart)?;
        if version != "1.29.0" {
            let name = chart.file_name().ok_or_eyre("chart without a name")?;
            raised.insert(name.to_string_lossy().into_owned(), version);
        }
    }
    sim_assert_eq!(
        have: raised,
        want: BTreeMap::from([
            ("jupyterhub".to_string(), "1.33.0"),
            ("okteto".to_string(), "1.33.0"),
        ]),
    );
    Ok(())
}

/// A decoder run that Helm fails keeps a complete bundle: the invocation
/// record, the decoder chart with the documents, the values and the YAML the
/// documents were split from, all by bundle-relative paths.
#[cfg(unix)]
#[test]
fn a_failed_yaml_decoding_preserves_a_complete_bundle() -> eyre::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let root = ScratchDir::new("helm_adjudication")?;
    let programs = ScratchDir::new("helm_adjudication")?;
    let program = programs.path().join("helm");
    fs::write(
        &program,
        indoc! {r#"
            #!/bin/sh
            if [ "$1" = version ]; then printf v4.2.3; exit 0; fi
            echo 'injected decoder failure' >&2
            exit 1
        "#},
    )?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    let runner = helm_invocation::HelmRunner::with_program(root.path(), false, program)?;
    let case = ScratchDir::new("helm_adjudication")?;
    let rendered = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: decoded
    "};
    let decoded = helm_adjudication::decode(&runner, rendered.as_bytes(), case.path())?;
    let Err(failure) = decoded.documents else {
        eyre::bail!("an injected decoder failure decoded");
    };

    let relative = case.path().join("decode").canonicalize()?;
    let relative = relative.strip_prefix(test_util::scratch::root().canonicalize()?)?;
    let bundle = test_util::scratch::evidence_root().join(relative);
    eyre::ensure!(
        failure.contains(&format!("evidence={}", bundle.display()))
            && failure.contains("injected decoder failure"),
        "the failure does not report the bundle {}: {failure}",
        bundle.display()
    );
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(bundle.join("decode.invocation.json"))?)?;
    sim_assert_eq!(
        have: (
            record["chart"].as_str(),
            record["values"].as_str(),
            record["kubernetes_version"].is_string(),
            fs::read_to_string(bundle.join("charts/decode/documents/0.yaml"))?,
            fs::read(bundle.join("inputs/decode.values.json"))?,
            fs::read_to_string(bundle.join("rendered.yaml"))?,
        ),
        want: (
            Some("charts/decode"),
            Some("inputs/decode.values.json"),
            true,
            rendered.to_string(),
            fs::read(case.path().join("decode/values.json"))?,
            rendered.to_string(),
        )
    );
    Ok(())
}
