//! Cached values composition and dependency control inventory regressions.

use color_eyre::eyre;
use indoc::indoc;
use serde_json::json;
use test_util::helm_values::{
    AcceptanceDocument, AcceptanceInputs, DependencyProbeKind, acceptance_values,
};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[test]
fn cached_composition_is_isolated_across_overlays_and_document_kinds() -> eyre::Result<()> {
    for name in [
        "nulls", "aliases", "imports", "globals", "multidoc", "packed",
    ] {
        let chart = test_util::workspace_testdata()
            .join("helm-values")
            .join(name);
        let inputs = AcceptanceInputs::load(&chart)?;
        for overlay in [
            json!({}),
            json!({"nested": {"owned": null}, "second": {"enabled": false}}),
            json!({}),
        ] {
            for document in AcceptanceDocument::ALL {
                sim_assert_eq!(have: inputs.compose(overlay.clone(), document)?, want: acceptance_values(&chart, overlay.clone(), document)?);
            }
        }
    }
    Ok(())
}

#[test]
fn inventory_preserves_aliases_tags_and_undeclared_charts() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("helm-values/aliases");
    let inputs = AcceptanceInputs::load(&chart)?;
    let controls = inputs.dependency_controls()?;
    assert!(controls.iter().any(|control| control.scope == ["second"]
        && control.conditions == vec![vec!["second", "enabled"]]));
    assert!(controls.iter().any(
        |control| control.scope == ["tagged"] && control.tags == vec![vec!["tags", "optional"]]
    ));
    assert!(
        controls
            .iter()
            .any(|control| control.scope == ["extra"] && !control.declared)
    );
    let probes = inputs.dependency_probes()?;
    assert!(probes.iter().any(|probe| probe.scope == ["second"]
        && probe.kind == DependencyProbeKind::Disable
        && probe.enabled == Some(false)));
    assert!(probes.iter().any(|probe| probe.scope == ["tagged"]
        && probe.kind == DependencyProbeKind::Enable
        && probe.enabled == Some(true)));
    sim_assert_eq!(have: inputs.dependency_probes()?, want: probes);
    Ok(())
}

#[test]
fn repeated_conditions_cannot_bypass_the_dependency_work_cap() -> eyre::Result<()> {
    let scratch = ScratchDir::new("dependency-row-cap")?;
    std::fs::create_dir_all(scratch.path().join("charts/child"))?;
    std::fs::write(
        scratch.path().join("Chart.yaml"),
        serde_json::to_vec(&json!({
            "apiVersion": "v2", "name": "root", "version": "1.0.0",
            "dependencies": [{
                "name": "child", "version": "1.0.0",
                "condition": vec!["child.enabled"; 3_000].join(",")
            }]
        }))?,
    )?;
    std::fs::write(
        scratch.path().join("charts/child/Chart.yaml"),
        serde_json::to_vec(&json!({
            "apiVersion": "v2", "name": "child", "version": "1.0.0"
        }))?,
    )?;
    let inputs = AcceptanceInputs::load(scratch.path())?;
    assert!(
        matches!(inputs.dependency_probes(), Err(test_util::helm_values::ValuesError::Unmodelled(reason)) if reason.contains("row cap"))
    );
    Ok(())
}

#[test]
fn replacement_requirements_and_nested_fallbacks_activate_ancestors() -> eyre::Result<()> {
    let scratch = ScratchDir::new("dependency-controls")?;
    let root = scratch.path();
    std::fs::create_dir_all(root.join("charts/child/charts/grand"))?;
    std::fs::write(
        root.join("Chart.yaml"),
        indoc! {"
            apiVersion: v1
            name: root
            version: 1.0.0
            dependencies:
              - name: ignored
                version: 1.0.0
        "},
    )?;
    std::fs::write(
        root.join("requirements.yaml"),
        indoc! {"
            dependencies:
              - name: child
                alias: renamed
                version: 1.0.0
                condition: renamed.enabled
        "},
    )?;
    std::fs::write(
        root.join("values.yaml"),
        indoc! {"
            renamed: {enabled: false}
        "},
    )?;
    std::fs::write(
        root.join("charts/child/Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: child
            version: 1.0.0
            dependencies:
              - name: grand
                version: 1.0.0
                condition: grand.enabled,grand.fallback
        "},
    )?;
    std::fs::write(
        root.join("charts/child/values.yaml"),
        indoc! {"
            grand: {enabled: false, fallback: false}
        "},
    )?;
    std::fs::write(
        root.join("charts/child/charts/grand/Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: grand
            version: 1.0.0
        "},
    )?;
    std::fs::write(
        root.join("charts/child/charts/grand/values.yaml"),
        indoc! {"
            {}
        "},
    )?;
    let inputs = AcceptanceInputs::load(root)?;
    let controls = inputs.dependency_controls()?;
    sim_assert_eq!(have: controls.len(), want: 2);
    let probes = inputs.dependency_probes()?;
    assert!(probes.iter().any(|probe| probe.overlay
        == json!({"renamed": {"enabled": true, "grand": {"enabled": null, "fallback": true}}})
        && probe.enabled == Some(true)));
    assert!(
        probes
            .iter()
            .any(|probe| probe.kind == DependencyProbeKind::ScopeNull
                && probe.scope == ["renamed", "grand"])
    );
    Ok(())
}

#[test]
#[ignore = "diagnostic inventory over every corpus chart"]
fn dependency_probe_corpus_inventory() -> eyre::Result<()> {
    let root = test_util::workspace_testdata().join("charts");
    let mut entries = std::fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut failures = Vec::new();
    let mut checked = 0;
    for entry in entries {
        let chart = entry.path();
        if !chart.join("Chart.yaml").is_file() {
            continue;
        }
        checked += 1;
        match AcceptanceInputs::load(&chart).and_then(|inputs| inputs.dependency_probes()) {
            Ok(probes) => eprintln!("{}: {} rows", chart.display(), probes.len()),
            Err(err) => failures.push(format!("{}: {err}", chart.display())),
        }
    }
    eprintln!("charts_checked={checked}; failures={}", failures.len());
    eyre::ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn packaged_dependencies_expose_the_same_typed_controls() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("helm-values/packed");
    let inputs = AcceptanceInputs::load(&chart)?;
    let controls = inputs.dependency_controls()?;
    sim_assert_eq!(have: controls.len(), want: 1);
    assert!(
        controls
            .iter()
            .any(|control| control.scope == ["packed"] && control.declared)
    );
    assert!(
        inputs
            .dependency_probes()?
            .iter()
            .any(|probe| probe.kind == DependencyProbeKind::ScopeNull)
    );
    Ok(())
}

#[test]
fn dependency_inventory_depth_cap_fails_instead_of_omitting_controls() -> eyre::Result<()> {
    let scratch = ScratchDir::new("dependency-depth-cap")?;
    let mut current = scratch.path().to_path_buf();
    for depth in 0..=65 {
        std::fs::create_dir_all(&current)?;
        let manifest = if depth < 65 {
            indoc! {"
                apiVersion: v2
                name: child
                version: 1.0.0
                dependencies:
                  - name: child
                    version: 1.0.0
            "}
        } else {
            indoc! {"
                apiVersion: v2
                name: child
                version: 1.0.0
            "}
        };
        std::fs::write(current.join("Chart.yaml"), manifest)?;
        std::fs::write(
            current.join("values.yaml"),
            indoc! {"
            {}
        "},
        )?;
        current = current.join("charts/child");
    }
    let inputs = AcceptanceInputs::load(scratch.path())?;
    assert!(
        matches!(inputs.dependency_controls(), Err(test_util::helm_values::ValuesError::Unmodelled(reason)) if reason.contains("depth cap"))
    );
    Ok(())
}

#[test]
fn lint_twice_is_not_validated_without_templates_for_gate_null_overlay() -> eyre::Result<()> {
    let scratch = ScratchDir::new("lint-no-templates-gate")?;
    let root = scratch.path();
    std::fs::write(
        root.join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: gate
        version: 1.0.0
    "},
    )?;
    std::fs::write(
        root.join("values.yaml"),
        indoc! {"
        gate: false
        nested:
          'n': 3
    "},
    )?;
    let overlay = json!({"gate": true, "nested": {"n": null}});
    let inputs = AcceptanceInputs::load(root)?;
    sim_assert_eq!(
        have: inputs.compose(overlay.clone(), AcceptanceDocument::LintRaw)?.root,
        want: json!({"gate": true, "nested": {}})
    );
    sim_assert_eq!(
        have: inputs.compose(overlay.clone(), AcceptanceDocument::Template)?.root,
        want: json!({"gate": true, "nested": {}})
    );
    // Helm skips the template lint rule before a second pass could restore n: 3.
    assert!(matches!(
        inputs.compose(overlay.clone(), AcceptanceDocument::LintCoalescedTwice),
        Err(test_util::helm_values::ValuesError::NotValidated(_))
    ));
    assert!(matches!(
        acceptance_values(
            root,
            overlay.clone(),
            AcceptanceDocument::LintCoalescedTwice
        ),
        Err(test_util::helm_values::ValuesError::NotValidated(_))
    ));
    std::fs::create_dir(root.join("templates"))?;
    assert!(matches!(
        inputs.compose(overlay.clone(), AcceptanceDocument::LintCoalescedTwice),
        Err(test_util::helm_values::ValuesError::NotValidated(_))
    ));
    sim_assert_eq!(
        have: AcceptanceInputs::load(root)?.compose(overlay, AcceptanceDocument::LintCoalescedTwice)?.root,
        want: json!({"gate": true, "nested": {"n": 3}})
    );
    Ok(())
}

#[test]
fn lint_twice_is_not_validated_for_prometheus_operator_crds() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("charts/prometheus-operator-crds");
    assert!(!chart.join("templates").exists());
    let inputs = AcceptanceInputs::load(&chart)?;
    inputs.compose(json!({}), AcceptanceDocument::LintRaw)?;
    assert!(matches!(
        inputs.compose(json!({}), AcceptanceDocument::LintCoalescedTwice),
        Err(test_util::helm_values::ValuesError::NotValidated(_))
    ));
    assert!(matches!(
        acceptance_values(&chart, json!({}), AcceptanceDocument::LintCoalescedTwice),
        Err(test_util::helm_values::ValuesError::NotValidated(_))
    ));
    Ok(())
}
