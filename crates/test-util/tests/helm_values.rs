//! Helm v4.2.3 values documents for the charts under `testdata/helm-values`.
//!
//! Every expected document was produced by the real Helm v4.2.3:
//! `helm template` printing `{{ .Values | toJson }}` for the template
//! document, and `helm lint` accepting a root schema `{"const": <document>}`
//! in the values rule (lint raw) or in the template rule (lint twice). The
//! commands are in the round-8 coalesce handoff.

use std::collections::BTreeSet;
use std::path::PathBuf;

use color_eyre::eyre;
use serde_json::{Value, json};
use test_util::helm_values::{
    AcceptanceDocument, DependencyValues, ValuesError, ValuesOptions, Warning, acceptance_values,
    coalesce_chart_values, coalesce_tables,
};
use test_util::prelude::sim_assert_eq;

fn chart(name: &str) -> PathBuf {
    test_util::workspace_testdata()
        .join("helm-values")
        .join(name)
}

fn document(name: &str, overrides: &Value, kind: AcceptanceDocument) -> eyre::Result<Value> {
    Ok(acceptance_values(&chart(name), overrides.clone(), kind)?.root)
}

/// Why Helm validates no document for `overrides`.
fn not_validated(name: &str, overrides: &Value, kind: AcceptanceDocument) -> eyre::Result<String> {
    match acceptance_values(&chart(name), overrides.clone(), kind) {
        Err(ValuesError::NotValidated(reason)) => Ok(reason),
        Err(error) => Err(error.into()),
        Ok(values) => eyre::bail!("{name}: composed {values:?} where Helm validates nothing"),
    }
}

fn null_overrides() -> Value {
    json!({
        "owned": null, "absent": null, "defaultNull": null,
        "nested": {"owned": null, "absent": null, "defaultNull": null},
    })
}

#[test]
fn a_null_override_deletes_only_a_default_it_meets() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("nulls"), null_overrides())?,
        want: json!({
            "absent": null,
            "nested": {"absent": null, "defaultNull": null, "kept": 1},
            "scalar": 5,
            "table": {"inner": {"x": 1}},
        })
    );
    Ok(())
}

#[test]
fn lint_raw_keeps_the_values_file_nulls_under_the_overrides() -> eyre::Result<()> {
    sim_assert_eq!(
        have: document("nulls", &null_overrides(), AcceptanceDocument::LintRaw)?,
        want: json!({
            "absent": null,
            "defaultNull": null,
            "nested": {"kept": 1, "absent": null, "defaultNull": null},
            "table": {"inner": {"x": 1}},
            "scalar": 5,
        })
    );
    Ok(())
}

/// Lint's second `CoalesceValues` refills the defaults the first deleted.
#[test]
fn lint_coalesces_twice_and_refills_deleted_defaults() -> eyre::Result<()> {
    sim_assert_eq!(
        have: document("nulls", &null_overrides(), AcceptanceDocument::LintCoalescedTwice)?,
        want: json!({
            "owned": 1,
            "absent": null,
            "nested": {"kept": 1, "owned": 2, "absent": null, "defaultNull": null},
            "table": {"inner": {"x": 1}},
            "scalar": 5,
        })
    );
    Ok(())
}

/// Lint's values rule writes the values file into the override tables, so
/// the template rule sees a null default the template command never does.
#[test]
fn lint_template_rule_reads_overrides_the_values_rule_rewrote() -> eyre::Result<()> {
    let overrides = json!({"table": 5, "scalar": {"x": 1}, "nested": {"kept": {"y": 1}}});
    sim_assert_eq!(
        have: document("nulls", &overrides, AcceptanceDocument::LintCoalescedTwice)?,
        want: json!({
            "owned": 1, "table": 5, "scalar": {"x": 1},
            "nested": {"kept": {"y": 1}, "owned": 2, "defaultNull": null},
        })
    );
    sim_assert_eq!(
        have: document("nulls", &overrides, AcceptanceDocument::LintRaw)?,
        want: json!({
            "owned": 1, "defaultNull": null, "table": 5, "scalar": {"x": 1},
            "nested": {"kept": {"y": 1}, "owned": 2, "defaultNull": null},
        })
    );
    Ok(())
}

#[test]
fn map_and_scalar_conflicts_keep_the_override() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(
            &chart("nulls"),
            json!({"table": 5, "scalar": {"x": 1}, "nested": {"kept": {"y": 1}}}),
        )?,
        want: json!({"nested": {"kept": {"y": 1}, "owned": 2}, "owned": 1, "scalar": {"x": 1}, "table": 5})
    );
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("nulls"), json!({"table": {"inner": 7}}))?,
        want: json!({"nested": {"kept": 1, "owned": 2}, "owned": 1, "scalar": 5, "table": {"inner": 7}})
    );
    Ok(())
}

/// The parent's pass merges a dependency scope, keeping its nulls; only the
/// dependency's own default deletes one.
#[test]
fn a_dependency_enabled_null_survives_without_a_child_default() -> eyre::Result<()> {
    let overrides = json!({"plain": {"enabled": null}, "flagged": {"enabled": null}});
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("toggle"), overrides.clone())?,
        want: json!({
            "flagged": {"global": {}, "replicas": 2},
            "plain": {"enabled": null, "global": {}, "replicas": 1},
        })
    );
    // Lint's values rule deleted both nulls from the override tables first.
    sim_assert_eq!(
        have: document("toggle", &overrides, AcceptanceDocument::LintCoalescedTwice)?,
        want: json!({
            "flagged": {"enabled": true, "global": {}, "replicas": 2},
            "plain": {"enabled": true, "global": {}, "replicas": 1},
        })
    );
    sim_assert_eq!(
        have: document("toggle", &overrides, AcceptanceDocument::LintRaw)?,
        want: json!({"plain": {}, "flagged": {}})
    );
    Ok(())
}

#[test]
fn a_false_condition_removes_the_dependency_and_keeps_its_scope() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("toggle"), json!({"flagged": {"enabled": false}}))?,
        want: json!({
            "flagged": {"enabled": false},
            "plain": {"enabled": true, "global": {}, "replicas": 1},
        })
    );
    Ok(())
}

#[test]
fn a_non_map_dependency_scope_aborts_helm() -> eyre::Result<()> {
    for (name, overrides, dependency) in [
        ("toggle", json!({"plain": 5}), "plain"),
        ("toggle", json!({"flagged": false}), "flagged"),
        ("aliases", json!({"extra": null}), "extra"),
    ] {
        for kind in [
            AcceptanceDocument::Template,
            AcceptanceDocument::LintCoalescedTwice,
        ] {
            sim_assert_eq!(
                have: not_validated(name, &overrides, kind)?,
                want: format!("Helm aborts: type mismatch on {dependency}")
            );
        }
    }
    // A null scope meeting the parent's default is deleted, then refilled.
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("toggle"), json!({"plain": null}))?,
        want: json!({
            "flagged": {"enabled": true, "global": {}, "replicas": 2},
            "plain": {"global": {}, "replicas": 1},
        })
    );
    Ok(())
}

/// A dependency's nested global tables are merged into tables its parent
/// still holds, so they surface in the parent's `global` too.
#[test]
fn globals_flow_down_and_nested_dependency_globals_leak_up() -> eyre::Result<()> {
    let global = json!({"g": 1, "h": 3, "t": {"a": 1, "b": 2, "deep": {"p": 1, "q": 2}}});
    let grandchild = json!({"global": global, "leaf": 1});
    let child = json!({"global": global, "grandchild": grandchild, "own": 1});
    let values = acceptance_values(&chart("globals"), json!({}), AcceptanceDocument::Template)?;
    sim_assert_eq!(
        have: values.root,
        want: json!({"child": child, "global": {"g": 1, "t": {"a": 1, "deep": {"p": 1, "q": 2}}}})
    );
    sim_assert_eq!(
        have: values.dependencies,
        want: vec![
            DependencyValues { scope: vec!["child".to_string()], values: child },
            DependencyValues {
                scope: vec!["child".to_string(), "grandchild".to_string()],
                values: grandchild,
            },
        ]
    );

    let global =
        json!({"g": 1, "h": 3, "k": 1, "t": {"a": 1, "b": 2, "c": 3, "deep": {"p": 1, "q": 2}}});
    sim_assert_eq!(
        have: coalesce_chart_values(
            &chart("globals"),
            json!({"global": {"t": {"c": 3}}, "child": {"global": {"g": 9, "k": 1}}}),
        )?,
        want: json!({
            "child": {"global": global, "grandchild": {"global": global, "leaf": 1}, "own": 1},
            "global": {"g": 1, "t": {"a": 1, "c": 3, "deep": {"p": 1, "q": 2}}},
        })
    );
    Ok(())
}

/// Aliases instantiate a chart per requirement, tags and conditions remove
/// instances by name, and a chart whose version misses its requirement's
/// constraint stays loaded under its own name.
#[test]
fn aliases_tags_and_version_matching_follow_helm() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("aliases"), json!({}))?,
        want: json!({
            "extra": {"from": "extra", "global": {}},
            "first": {"from": "dep", "global": {}},
            "second": {"enabled": false},
            "tags": {"optional": false},
            "versioned": {"enabled": false},
        })
    );
    sim_assert_eq!(
        have: coalesce_chart_values(
            &chart("aliases"),
            json!({"second": {"enabled": true}, "tags": {"optional": true}, "versioned": {"enabled": true}}),
        )?,
        want: json!({
            "extra": {"from": "extra", "global": {}},
            "first": {"from": "dep", "global": {}},
            "second": {"enabled": true, "from": "dep", "global": {}},
            "tagged": {"from": "tagged", "global": {}},
            "tags": {"optional": true},
            "versioned": {"enabled": true, "from": "versioned", "global": {}},
        })
    );
    Ok(())
}

#[test]
fn import_values_place_child_tables_under_the_parent() -> eyre::Result<()> {
    let exporter = json!({
        "exports": {"data": {"exported": 1, "shared": "child"}},
        "global": {},
        "service": {"name": "child", "port": 80},
    });
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("imports"), json!({}))?,
        want: json!({
            "exported": 1,
            "exporter": exporter,
            "imported": {"service": {"name": "parent", "port": 80}},
            "shared": "parent",
        })
    );
    sim_assert_eq!(
        have: coalesce_chart_values(
            &chart("imports"),
            json!({"exported": null, "imported": {"service": {"port": null}}}),
        )?,
        want: json!({
            "exporter": exporter,
            "imported": {"service": {"name": "parent"}},
            "shared": "parent",
        })
    );
    Ok(())
}

#[test]
fn packaged_dependencies_load_from_their_archive() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("packed"), json!({}))?,
        want: json!({"own": 1, "packed": {"global": {}, "packed": true}})
    );
    Ok(())
}

/// The chart loader merges every YAML document of `values.yaml`; lint's
/// values rule reads the first one only.
#[test]
fn multi_document_values_merge_except_in_lint_raw() -> eyre::Result<()> {
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("multidoc"), json!({}))?,
        want: json!({"a": 1, "b": 2, "m": {"x": 1, "z": 2}})
    );
    sim_assert_eq!(
        have: document("multidoc", &json!({}), AcceptanceDocument::LintCoalescedTwice)?,
        want: json!({"a": 1, "b": 2, "m": {"x": 1, "z": 2}})
    );
    sim_assert_eq!(
        have: document("multidoc", &json!({}), AcceptanceDocument::LintRaw)?,
        want: json!({"a": 1, "m": {"x": 1}})
    );
    Ok(())
}

#[test]
fn coalesce_tables_deletes_a_null_only_over_a_value() {
    sim_assert_eq!(
        have: coalesce_tables(
            &json!({"set": null, "unset": null, "nested": {"set": null, "new": {"inner": null}}}),
            &json!({"set": 1, "kept": 2, "nested": {"set": 3}}),
        ),
        want: json!({"unset": null, "kept": 2, "nested": {"new": {"inner": null}}})
    );
}

/// The corpus wrapper's packaged dependency composes to the document Helm
/// v4.2.3 printed when the wrapper was pinned.
#[test]
fn the_packaged_corpus_wrapper_matches_its_pinned_helm_document() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata()
        .join("charts")
        .join("schema-emission-temporal-wrapper");
    let pinned: Value =
        serde_json::from_slice(&std::fs::read(chart.join("coalesced-defaults.json"))?)?;
    sim_assert_eq!(have: coalesce_chart_values(&chart, json!({}))?, want: pinned);
    Ok(())
}

/// The distinct warnings `helm template` logs on stderr for each input.
#[test]
fn type_conflicts_are_the_warnings_helm_logs() -> eyre::Result<()> {
    let warnings = |name: &str, overrides: Value| -> eyre::Result<BTreeSet<Warning>> {
        Ok(acceptance_values(&chart(name), overrides, AcceptanceDocument::Template)?.warnings)
    };
    sim_assert_eq!(
        have: warnings(
            "nulls",
            json!({"table": 5, "scalar": {"x": 1}, "nested": {"kept": {"y": 1}}}),
        )?,
        want: BTreeSet::from([
            Warning::DefaultNotATable { path: "nulls.scalar".to_string() },
            Warning::NonTableUnderTable { path: "nulls.nested.kept".to_string() },
        ])
    );
    sim_assert_eq!(
        have: warnings("nulls", json!({"table": {"inner": 7}}))?,
        want: BTreeSet::from([Warning::TableOverNonTable { path: "nulls.table.inner".to_string() }])
    );
    sim_assert_eq!(
        have: warnings("toggle", json!({"plain": {"enabled": "yes"}}))?,
        want: BTreeSet::from([Warning::NonBoolCondition {
            chart: "plain".to_string(),
            path: "plain.enabled".to_string(),
        }])
    );
    let overrides = ValuesOptions {
        values: vec!["child.global.t=5".to_string()],
        ..ValuesOptions::default()
    }
    .merge_values()?;
    sim_assert_eq!(
        have: warnings("globals", overrides)?,
        want: BTreeSet::from([
            Warning::TableOverNonTable { path: "globals.child.global.t".to_string() },
            Warning::GlobalMapOntoNonMap { key: "t".to_string() },
            Warning::GlobalNonMapOntoMap { key: "t".to_string() },
        ])
    );
    Ok(())
}

/// Helm loads undeclared dependencies in Go map order. When one writes into
/// the shared `global` tables, the siblings after it see its value, so
/// `helm template` printed `q: from-a` 29 times and `q: from-b` once in
/// 30 runs: no single document is Helm's, and the port refuses. Declared
/// dependencies keep their requirement order and compose exactly.
#[test]
fn global_writes_among_unordered_dependencies_are_refused() -> eyre::Result<()> {
    for kind in [
        AcceptanceDocument::Template,
        AcceptanceDocument::LintCoalescedTwice,
    ] {
        assert!(
            matches!(
                acceptance_values(&chart("unordered-globals"), json!({}), kind),
                Err(ValuesError::Unmodelled(_))
            ),
            "{kind:?}"
        );
    }
    let global = json!({"t": {"deep": {"p": 1, "q": "from-b"}}});
    sim_assert_eq!(
        have: coalesce_chart_values(&chart("ordered-globals"), json!({}))?,
        want: json!({"a": {"global": global}, "b": {"global": global}, "global": global})
    );
    Ok(())
}
