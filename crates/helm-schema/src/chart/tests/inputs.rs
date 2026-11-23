use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

use super::*;

fn snapshot(entries: &[(&str, &str)]) -> eyre::Result<Vec<crate::chart::ChartContext>> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, body) in entries {
        test_util::write(&root.join(name)?, body)?;
    }
    Ok(crate::chart::discover_chart_contexts(&root.into(), true)?)
}

const ROOT: &str = indoc! {"
    apiVersion: v2
    name: root
    version: 0.1.0
"};
const CHILD: &str = indoc! {"
    apiVersion: v1
    name: leaf
    version: 0.1.0
"};

#[test]
fn slice3_whole_metadata_overlay_precedes_validation() -> eyre::Result<()> {
    let charts = snapshot(&[
        (
            "Chart.yaml",
            indoc! {r"
            apiVersion: v2
            name: root
            version: 0.1.0
            annotations: {original: retained, override: before}
            dependencies:
              - {name: old, alias: duplicate, condition: keep}
              - {name: second, alias: duplicate}
        "},
        ),
        (
            "requirements.yaml",
            indoc! {r#"
            name: renamed
            apiVersion: v1
            annotations: {override: after}
            dependencies:
              - {name: leaf, alias: ""}
        "#},
        ),
        ("charts/child/requirements.yaml", CHILD),
    ])?;
    let root = charts.first().ok_or_eyre("root")?;
    let leaf = charts.get(1).ok_or_eyre("child")?;
    sim_assert_eq!(have: &root.template_namespace, want: "renamed");
    sim_assert_eq!(have: root.files.metadata()["apiVersion"].as_str(), want: Some("v1"));
    sim_assert_eq!(have: root.files.metadata()["annotations"], want: serde_yaml::from_str::<Value>("original: retained\noverride: after")?);
    sim_assert_eq!(have: &leaf.template_namespace, want: "renamed/charts/leaf");
    sim_assert_eq!(have: leaf.dependency_activation_chain.first().map(|edge| edge.condition_paths.as_slice()), want: Some(["keep".to_string()].as_slice()));
    Ok(())
}

#[test]
fn slice3_resolved_controls_do_not_supply_authoring_hints() -> eyre::Result<()> {
    for (control, path, legacy) in [
        ("Condition: 077", "63", false),
        (r#"condition: "077""#, "077", true),
        ("condition: on", "true", false),
        (r#"condition: "on""#, "on", true),
        ("<<: {condition: flag}", "flag", false),
        ("Condition: shadow, condition: actual", "actual", true),
    ] {
        let root = format!("{ROOT}dependencies: [{{name: leaf, {control}}}]\n");
        let charts = snapshot(&[("Chart.yaml", &root), ("charts/leaf/Chart.yaml", CHILD)])?;
        let child = charts.get(1).ok_or_eyre("leaf")?;
        let edge = child
            .dependency_activation_chain
            .first()
            .ok_or_eyre("activation")?;
        sim_assert_eq!(have: edge.condition_paths.first().map_or("", String::as_str), want: path);
        sim_assert_eq!(have: edge.legacy, want: legacy, "{control}");
    }
    Ok(())
}

#[test]
fn slice3_values_merge_without_resolving_an_ambiguous_key() -> eyre::Result<()> {
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        (
            "values.yaml",
            indoc! {r#"
            # This preamble has no lint values document.
            ---
            cfg: {left: 077, both: old}
            array: [1, 2]
            replace: {old: true}
            paths: {077: octal, "63": quoted, safe: retained}
            ---
            cfg: {both: new, right: 0x2a}
            array: [3]
            replace: on
        "#},
        ),
    ])?;
    let root = charts.first().ok_or_eyre("root")?;
    let defaults = root.files.defaults()?;
    sim_assert_eq!(have: defaults["cfg"], want: serde_yaml::from_str::<Value>("{left: 63, both: new, right: 42}")?);
    sim_assert_eq!(have: defaults["array"], want: serde_yaml::from_str::<Value>("[3]")?);
    sim_assert_eq!(have: defaults["replace"].as_bool(), want: Some(true));
    assert!(matches!(defaults["paths"]["63"], Value::Tagged(_)));
    sim_assert_eq!(have: defaults["paths"]["safe"].as_str(), want: Some("retained"));
    sim_assert_eq!(have: root.files.lint_values().as_ref().and_then(|doc| doc["cfg"]["both"].as_str()), want: Some("old"));
    Ok(())
}

#[test]
fn review_typed_loader_rejects_json_conversion_errors() {
    let mut accepted = Vec::new();
    for (file, body) in [
        ("values.yaml", "scalar"),
        ("values.yaml", "x: .inf"),
        ("values.yaml", "null: 1"),
        (
            "values.yaml",
            indoc! {"
            scalar
            ---
            x: 1
        "},
        ),
        ("requirements.yaml", "description: []"),
        ("requirements.yaml", "extra: {null: 1}"),
    ] {
        let result = snapshot(&[("Chart.yaml", ROOT), (file, body)])
            .and_then(|charts| Ok(crate::chart::build_composed_values_document(&charts, true)?));
        match result {
            Ok(_) => accepted.push((file, body)),
            Err(error) => assert!(
                matches!(error.downcast_ref::<CliError>(), Some(CliError::ChartDecode { path, .. }) if path.ends_with(file)),
                "{error:?}"
            ),
        }
    }
    sim_assert_eq!(have: accepted, want: Vec::<(&str, &str)>::new());
}

#[test]
fn review_folded_child_fields_apply_in_json_order() -> eyre::Result<()> {
    let child = format!("{CHILD}dependencies: []\nDependencies: [{{name: unused}}]\n");
    let charts = snapshot(&[("Chart.yaml", ROOT), ("charts/leaf/Chart.yaml", &child)])?;
    sim_assert_eq!(have: charts.iter().map(|chart| chart.template_namespace.as_str()).collect::<Vec<_>>(), want: vec!["root", "root/charts/leaf"]);
    let metadata = charts.get(1).ok_or_eyre("child")?.files.metadata();
    sim_assert_eq!(have: &metadata["dependencies"], want: &Value::Sequence(Vec::new()));
    Ok(())
}

#[test]
fn review_child_metadata_error_precedes_root_defaults_error() -> eyre::Result<()> {
    let error = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", "bad: ["),
        ("charts/leaf/Chart.yaml", "bad: ["),
    ])
    .err()
    .ok_or_eyre("expected child metadata error")?;
    assert!(
        matches!(error.downcast_ref::<CliError>(), Some(CliError::ChartDecode { path, .. }) if path.ends_with("charts/leaf/Chart.yaml")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn review_physical_lint_values_belong_only_to_root() -> eyre::Result<()> {
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", "\u{feff}text: |\n  x"),
        ("charts/leaf/Chart.yaml", CHILD),
        ("charts/leaf/values.yaml", "childOnly: true"),
    ])?;
    let root = charts.first().ok_or_eyre("root")?;
    let child = charts.get(1).ok_or_eyre("child")?;
    sim_assert_eq!(have: (root.files.lint_values(), child.files.lint_values()), want: (Some(serde_yaml::from_str("text: x")?), None));
    sim_assert_eq!(have: root.files.defaults()?["text"].as_str(), want: Some("x\n"));
    Ok(())
}

#[test]
fn review_annotation_alias_keeps_dependency_control_provenance() -> eyre::Result<()> {
    let root = indoc! {"
        apiVersion: v2
        name: root
        version: 0.1.0
        annotations:
          first: &label retained
          second: *label
        dependencies:
          - name: leaf
            condition: gates.leaf
    "};
    let charts = snapshot(&[("Chart.yaml", root), ("charts/leaf/Chart.yaml", CHILD)])?;
    let child = charts.get(1).ok_or_eyre("child")?;
    let edge = child
        .dependency_activation_chain
        .first()
        .ok_or_eyre("edge")?;
    assert!(edge.legacy);
    Ok(())
}

#[test]
fn review_collision_can_discard_a_nonfinite_value_before_json() -> eyre::Result<()> {
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", r#"paths: {1: .inf, "1": ok, keep: true}"#),
    ])?;
    let values = charts.first().ok_or_eyre("root chart")?.files.defaults()?;
    sim_assert_eq!(have: &values["paths"], want: &serde_yaml::from_str::<Value>(r#"{"1": ok, keep: true}"#)?);
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", r#"paths: {1: {null: 1}, "1": ok}"#),
    ])?;
    let error = charts
        .first()
        .ok_or_eyre("root chart")?
        .files
        .defaults()
        .expect_err("invalid key before collision resolution");
    assert!(
        matches!(error, CliError::ChartDecode { message, .. } if message.contains("unsupported map key"))
    );
    Ok(())
}

#[test]
fn review_quoted_merge_spelling_does_not_hide_authored_controls() -> eyre::Result<()> {
    let root = indoc! {r#"
        apiVersion: v2
        name: root
        version: 0.1.0
        "<<": note
        dependencies:
          - name: leaf
            condition: gates.leaf
    "#};
    let charts = snapshot(&[
        ("Chart.yaml", root),
        (
            "requirements.yaml",
            "dependencies: [{name: leaf, version: 0.1.0}]",
        ),
        ("charts/leaf/Chart.yaml", CHILD),
    ])?;
    let child = charts.get(1).ok_or_eyre("child")?;
    let edge = child
        .dependency_activation_chain
        .first()
        .ok_or_eyre("edge")?;
    sim_assert_eq!(have: (edge.condition_paths.as_slice(), edge.legacy), want: (&["gates.leaf".to_owned()][..], true));
    Ok(())
}

fn annotation(charts: &[crate::chart::ChartContext], key: &str) -> Option<String> {
    let path = ["Chart", "Annotations", key].map(str::to_owned).to_vec();
    charts.first()?.static_root_strings.get(&path).cloned()
}

/// Each possible winner of a converted-key collision is decoded as Helm would decode it alone.
/// The load fails only when every winner fails. Successful winners that agree stay precise.
#[test]
fn review_metadata_collisions_keep_only_successful_winners() {
    for (metadata, expected) in [
        (r#"annotations: {1: [], "1": ok}"#, Some(Some("ok"))),
        (r#"annotations: {1: ok, "1": ok}"#, Some(Some("ok"))),
        (r#"annotations: {1: one, "1": two}"#, Some(None)),
        (r#"annotations: {1: [], "1": {}}"#, None),
        (r#"annotations: {1: {null: 1}, "1": ok}"#, None),
        ("Description: []\ndescription: ok", None),
    ] {
        let manifest = format!("{ROOT}{metadata}\n");
        let have = match snapshot(&[("Chart.yaml", &manifest)]) {
            Ok(charts) => Some(annotation(&charts, "1")),
            Err(error) => {
                assert!(
                    matches!(
                        error.downcast_ref::<CliError>(),
                        Some(CliError::ChartDecode { .. })
                    ),
                    "{metadata}: {error:?}"
                );
                None
            }
        };
        sim_assert_eq!(have: have.as_ref().map(Option::as_deref), want: expected, "{metadata}");
    }
}

#[test]
fn review_values_collisions_keep_only_successful_winners() -> eyre::Result<()> {
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", r#"paths: {1: same, "1": same, keep: true}"#),
    ])?;
    let defaults = charts.first().ok_or_eyre("root chart")?.files.defaults()?;
    sim_assert_eq!(have: &defaults["paths"], want: &serde_yaml::from_str::<Value>(r#"{"1": same, keep: true}"#)?);
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", r#"paths: {1: .inf, "1": -.inf}"#),
    ])?;
    let error = charts
        .first()
        .ok_or_eyre("root chart")?
        .files
        .defaults()
        .expect_err("every winner fails JSON marshaling");
    assert!(
        matches!(&error, CliError::ChartDecode { message, .. } if message.contains("unsupported value")),
        "{error:?}"
    );
    Ok(())
}

/// Helm's render framer consumes `---#raw` as a separator; lint's direct reader keeps the key.
#[test]
fn review_raw_separator_keeps_the_physical_lint_document() -> eyre::Result<()> {
    let charts = snapshot(&[
        ("Chart.yaml", ROOT),
        ("values.yaml", "root: retained\n---#raw: lint\n"),
    ])?;
    let root = charts.first().ok_or_eyre("root chart")?;
    sim_assert_eq!(have: root.files.defaults()?, want: &serde_yaml::from_str::<Value>("root: retained")?);
    sim_assert_eq!(have: root.files.lint_values(), want: Some(serde_yaml::from_str("{root: retained, '---#raw': lint}")?));
    Ok(())
}

/// Folded identities bind and activate as in Helm, but the legacy reader never saw them.
#[test]
fn review_folded_dependency_identity_activates_without_a_legacy_hint() -> eyre::Result<()> {
    for (dependency, values_key, gate, hinted) in [
        (
            "{name: leaf, condition: gates.leaf}",
            "leaf",
            "gates.leaf",
            true,
        ),
        (
            "{Name: leaf, condition: gates.leaf}",
            "leaf",
            "gates.leaf",
            false,
        ),
        (
            "{name: leaf, alias: other, condition: gates.other}",
            "other",
            "gates.other",
            true,
        ),
        (
            "{name: leaf, Alias: other, condition: gates.other}",
            "other",
            "gates.other",
            false,
        ),
    ] {
        let root = format!("{ROOT}dependencies: [{dependency}]\n");
        let charts = snapshot(&[("Chart.yaml", &root), ("charts/leaf/Chart.yaml", CHILD)])?;
        let child = charts.get(1).ok_or_eyre("child")?;
        let edge = child
            .dependency_activation_chain
            .first()
            .ok_or_eyre("activation")?;
        sim_assert_eq!(have: (child.values_prefix.as_slice(), edge.condition_paths.as_slice()), want: (&[values_key.to_owned()][..], &[gate.to_owned()][..]), "{dependency}");
        let defines = crate::chart::build_define_index(&charts)?;
        let analysis = crate::analysis::analyze_charts(
            &charts,
            &defines,
            &crate::values_roots::ValuesRoots::from_values_document(&Value::Null),
            None,
        )?;
        let signals = analysis.contract.finalize().into_schema_signals();
        let boolean = signals
            .evidence_for(&helm_schema_core::ValuesPath::parse(gate))
            .is_some_and(|evidence| evidence.type_hints.contains("boolean"));
        sim_assert_eq!(have: boolean, want: hinted, "{dependency}");
    }
    Ok(())
}

/// Helm 4.2.3 renders either collision winner, so the schema must accept both.
#[test]
fn review_array_collision_schema_accepts_every_winner() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, ROOT)?;
    test_util::write(
        &root.join("values.yaml")?,
        r#"paths: {true: [fixed, text], "true": [fixed, 7]}"#,
    )?;
    // A sibling read makes the declared defaults type `paths.true`.
    test_util::write(
        &root.join("templates/config.yaml")?,
        "value: {{ .Values.paths.other | quote }}\n",
    )?;
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_source: root.into(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring: crate::generation::AuthoringPolicy::default(),
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..Default::default()
        },
    });
    let schema = session.generated_schema()?.schema;
    let validator = jsonschema::validator_for(&schema)?;
    for winner in [
        serde_json::json!({"paths": {"true": ["fixed", "text"]}}),
        serde_json::json!({"paths": {"true": ["fixed", 7]}}),
    ] {
        sim_assert_eq!(have: validator.is_valid(&winner), want: true, "{winner}");
    }
    Ok(())
}
