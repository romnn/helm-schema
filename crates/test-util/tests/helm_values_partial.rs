//! Partial parent overrides retain dependency defaults only in coalesced documents.

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::{Value, json};
use test_util::helm_values::{
    AcceptanceDocument, AcceptanceInputs, DependencyProbeKind, ValuesError,
};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

fn write_chart(path: &std::path::Path, metadata: &Value, values: &Value) -> eyre::Result<()> {
    std::fs::create_dir_all(path.join("templates"))?;
    std::fs::write(path.join("Chart.yaml"), serde_json::to_vec(metadata)?)?;
    std::fs::write(path.join("values.yaml"), serde_json::to_vec(values)?)?;
    Ok(())
}

#[test]
fn partial_override_leaves_subchart_only_defaults_out_of_raw_lint() -> eyre::Result<()> {
    let scratch = ScratchDir::new("partial-subchart-defaults")?;
    write_chart(
        scratch.path(),
        &json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{"name": "child", "version": "1.0.0"}]
        }),
        &json!({}),
    )?;
    write_chart(
        &scratch.path().join("charts/child"),
        &json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0"
        }),
        &json!({"first": 1, "second": 2}),
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    let probes = inputs.dependency_probes()?;
    let row = probes
        .iter()
        .find(|probe| probe.overlay == json!({"child": {"first": 1}}))
        .ok_or_eyre("missing partial-subchart-override row")?;
    sim_assert_eq!(have: row.enabled, want: Some(true));
    sim_assert_eq!(have: inputs.compose(row.overlay.clone(), AcceptanceDocument::LintRaw)?.root,
        want: json!({"child": {"first": 1}}));
    for kind in [
        AcceptanceDocument::LintCoalescedTwice,
        AcceptanceDocument::Template,
    ] {
        sim_assert_eq!(have: inputs.compose(row.overlay.clone(), kind)?.root,
            want: json!({"child": {"first": 1, "second": 2, "global": {}}}));
    }
    sim_assert_eq!(have: inputs.dependency_probes()?, want: probes);
    Ok(())
}

#[test]
fn partial_overrides_cover_nested_aliases_library_charts_and_activation_fallbacks()
-> eyre::Result<()> {
    let scratch = ScratchDir::new("partial-subchart-activation")?;
    write_chart(
        scratch.path(),
        &json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{"name": "child", "alias": "renamed", "version": "1.0.0",
                "condition": "gate.primary,gate.fallback", "tags": ["optional"]}]
        }),
        &json!({"gate": {"primary": false, "fallback": false}, "tags": {"optional": false}}),
    )?;
    write_chart(
        &scratch.path().join("charts/child"),
        &json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0",
            "dependencies": [{"name": "grand", "version": "1.0.0", "condition": "grand.enabled"}]
        }),
        &json!({"aa": {"first": 7, "second": 8}, "grand": {"enabled": false}}),
    )?;
    write_chart(
        &scratch.path().join("charts/child/charts/grand"),
        &json!({
            "apiVersion": "v2", "name": "grand", "version": "1.0.0", "type": "library"
        }),
        &json!({"enabled": false, "first": 1, "second": 2}),
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    let probes = inputs.dependency_probes()?;
    let partial: Vec<_> = probes
        .iter()
        .filter(|probe| probe.kind == DependencyProbeKind::PartialOverride)
        .collect();
    sim_assert_eq!(have: partial.len(), want: 2);
    let parent = partial.first().ok_or_eyre("parent row absent")?;
    sim_assert_eq!(have: &parent.overlay, want: &json!({
        "gate": {"primary": true}, "tags": {"optional": true},
        "renamed": {"aa": {"first": 7}}
    }));
    let nested = partial.get(1).ok_or_eyre("nested row absent")?;
    sim_assert_eq!(have: &nested.overlay, want: &json!({
        "gate": {"primary": true}, "tags": {"optional": true},
        "renamed": {"grand": {"enabled": true}}
    }));
    for probe in partial {
        sim_assert_eq!(have: probe.enabled, want: Some(true));
        sim_assert_eq!(have: &probe.not_validated, want: &None);
        let template = inputs.compose(probe.overlay.clone(), AcceptanceDocument::Template)?;
        assert!(
            template
                .dependencies
                .iter()
                .any(|dependency| dependency.scope == probe.scope)
        );
    }
    sim_assert_eq!(have: inputs.dependency_probes()?, want: probes);
    Ok(())
}

#[test]
fn partial_overrides_select_one_ordered_non_null_default_branch() -> eyre::Result<()> {
    for (defaults, expected) in [
        (
            json!({"alpha": null, "beta": {"keep": 5, "omit": 7}, "omega": 3}),
            json!({"beta": {"keep": 5}}),
        ),
        (json!({}), json!({})),
        (json!({"only": null}), json!({})),
        (
            json!({"array": [1, null, {}], "omega": 4}),
            json!({"array": [1, null, {}]}),
        ),
        (json!({"empty": {}, "omega": 4}), json!({"empty": {}})),
    ] {
        let scratch = ScratchDir::new("partial-subchart-values")?;
        write_chart(
            scratch.path(),
            &json!({"apiVersion": "v2", "name": "root", "version": "1.0.0"}),
            &json!({}),
        )?;
        // Undeclared dependencies use their actual chart name and stay in the inventory.
        write_chart(
            &scratch.path().join("charts/child"),
            &json!({
                "apiVersion": "v2", "name": "child", "version": "1.0.0", "type": "library"
            }),
            &defaults,
        )?;
        let inputs = AcceptanceInputs::load(scratch.path())?;
        let partial: Vec<_> = inputs
            .dependency_probes()?
            .into_iter()
            .filter(|probe| probe.kind == DependencyProbeKind::PartialOverride)
            .collect();
        sim_assert_eq!(have: partial.len(), want: 1);
        sim_assert_eq!(have: &partial.first().ok_or_eyre("partial row absent")?.overlay,
            want: &json!({"child": expected}));
    }
    Ok(())
}

#[test]
fn partial_overrides_share_inventory_for_archives_aliases_and_replacement_requirements()
-> eyre::Result<()> {
    for name in ["aliases", "packed", "imports", "globals"] {
        let inputs = AcceptanceInputs::load(
            &test_util::workspace_testdata()
                .join("helm-values")
                .join(name),
        )?;
        let expected: Vec<_> = inputs
            .dependency_controls()?
            .into_iter()
            .map(|control| control.scope)
            .collect();
        let partial: Vec<_> = inputs
            .dependency_probes()?
            .into_iter()
            .filter(|probe| probe.kind == DependencyProbeKind::PartialOverride)
            .collect();
        sim_assert_eq!(have: partial.iter().map(|probe| probe.scope.clone()).collect::<Vec<_>>(), want: expected);
        assert!(partial.iter().all(|probe| probe.enabled == Some(true)));
    }
    Ok(())
}

#[test]
fn partial_override_rows_count_toward_the_dependency_work_cap() -> eyre::Result<()> {
    let scratch = ScratchDir::new("partial-subchart-row-cap")?;
    write_chart(
        scratch.path(),
        &json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{"name": "child", "version": "1.0.0",
                "condition": vec!["child.enabled"; 2_730].join(",")}]
        }),
        &json!({}),
    )?;
    write_chart(
        &scratch.path().join("charts/child"),
        &json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0"
        }),
        &json!({"enabled": true, "other": 2}),
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    assert!(
        matches!(inputs.dependency_probes(), Err(ValuesError::Unmodelled(reason)) if reason.contains("row cap"))
    );
    Ok(())
}

#[test]
fn partial_override_preserves_ancestor_activation_inside_the_target_scope() -> eyre::Result<()> {
    let scratch = ScratchDir::new("partial-subchart-ancestor-gate")?;
    write_chart(
        scratch.path(),
        &json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{"name": "child", "version": "1.0.0", "condition": "child.grand.parentEnabled"}]
        }),
        &json!({}),
    )?;
    write_chart(
        &scratch.path().join("charts/child"),
        &json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0",
            "dependencies": [{"name": "grand", "version": "1.0.0", "condition": "grand.enabled"}]
        }),
        &json!({"grand": {"enabled": false, "parentEnabled": false}}),
    )?;
    write_chart(
        &scratch.path().join("charts/child/charts/grand"),
        &json!({
            "apiVersion": "v2", "name": "grand", "version": "1.0.0"
        }),
        &json!({"aa": 1, "second": 2}),
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    let row = inputs
        .dependency_probes()?
        .into_iter()
        .find(|probe| {
            probe.kind == DependencyProbeKind::PartialOverride && probe.scope == ["child", "grand"]
        })
        .ok_or_eyre("nested partial control absent")?;
    sim_assert_eq!(have: row.overlay, want: json!({"child": {"grand": {
        "aa": 1, "enabled": true, "parentEnabled": true
    }}}));
    sim_assert_eq!(have: row.enabled, want: Some(true));
    Ok(())
}

#[test]
fn partial_override_preserves_parent_owned_siblings_and_subchart_defaults() -> eyre::Result<()> {
    let scratch = ScratchDir::new("partial-parent-siblings")?;
    write_chart(
        scratch.path(),
        &json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{"name": "child", "version": "1.0.0"}]
        }),
        &json!({"child": {"first": {"keep": 99, "omit": 98}, "second": 97}}),
    )?;
    write_chart(
        &scratch.path().join("charts/child"),
        &json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0"
        }),
        &json!({"first": {"keep": 1, "omit": 2}, "second": 3, "third": 4}),
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    let row = inputs
        .dependency_probes()?
        .into_iter()
        .find(|probe| probe.kind == DependencyProbeKind::PartialOverride)
        .ok_or_eyre("partial row absent")?;
    sim_assert_eq!(have: &row.overlay, want: &json!({"child": {
        "first": {"keep": 1}
    }}));
    sim_assert_eq!(have: inputs.compose(row.overlay.clone(), AcceptanceDocument::LintRaw)?.root,
        want: json!({"child": {"first": {"keep": 1, "omit": 98}, "second": 97}}));
    sim_assert_eq!(have: inputs.compose(row.overlay, AcceptanceDocument::Template)?.root,
        want: json!({"child": {"first": {"keep": 1, "omit": 98}, "second": 97, "third": 4, "global": {}}}));
    Ok(())
}
