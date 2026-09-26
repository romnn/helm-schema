//! Helm v4.2.3's value flags and loader checks. Every expected map or
//! refusal is what the pinned Helm printed for the same flags
//! (`helm template p <chart with empty values> FLAGS` over a
//! `{{ .Values | toJson }}` template); the commands are in the round-8
//! coalesce handoff.

use std::fs;
use std::path::PathBuf;

use color_eyre::eyre;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::helm_values::{AcceptanceDocument, ValuesError, ValuesOptions, acceptance_values};
use test_util::prelude::sim_assert_eq;

fn set(values: &[&str]) -> ValuesOptions {
    ValuesOptions {
        values: values.iter().map(ToString::to_string).collect(),
        ..ValuesOptions::default()
    }
}

/// Helm applies `--set-json`, then `--set`, then `--set-string`, whatever
/// their order on the command line.
#[test]
fn flag_groups_apply_in_helms_fixed_order() -> eyre::Result<()> {
    let json_then_set = ValuesOptions {
        json_values: vec!["x=2".to_string()],
        values: vec!["x=1".to_string()],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(have: json_then_set.merge_values()?, want: json!({"x": 1}));
    let set_then_string = ValuesOptions {
        values: vec!["x=4".to_string()],
        string_values: vec!["x=3".to_string()],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(have: set_then_string.merge_values()?, want: json!({"x": "3"}));
    Ok(())
}

#[test]
fn set_coerces_booleans_null_and_integers_only() -> eyre::Result<()> {
    sim_assert_eq!(
        have: set(&[r"a\.b=1,c=TRUE,d=-5,e=+7,f=0,g=007"]).merge_values()?,
        want: json!({"a.b": 1, "c": true, "d": -5, "e": 7, "f": 0, "g": "007"})
    );
    sim_assert_eq!(
        have: set(&["list={a,b,0123,true,null}"]).merge_values()?,
        want: json!({"list": ["a", "b", "0123", true, null]})
    );
    let null_forms = ValuesOptions {
        values: vec!["x=null".to_string()],
        string_values: vec!["s=null".to_string()],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(have: null_forms.merge_values()?, want: json!({"s": "null", "x": null}));
    Ok(())
}

#[test]
fn set_indices_and_empty_values() -> eyre::Result<()> {
    sim_assert_eq!(
        have: set(&["a[1]=x", "a[0].b=2"]).merge_values()?,
        want: json!({"a": [{"b": 2}, "x"]})
    );
    sim_assert_eq!(have: set(&["n.m="]).merge_values()?, want: json!({"n": {"m": ""}}));
    let empty_json = ValuesOptions {
        json_values: vec!["x=".to_string()],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(have: empty_json.merge_values()?, want: json!({"x": null}));
    let Err(ValuesError::NotValidated(reason)) = set(&["x"]).merge_values() else {
        eyre::bail!("Helm rejects a key without a value");
    };
    sim_assert_eq!(have: reason, want: r#"failed parsing --set data: key "x" has no value"#);
    Ok(())
}

/// `-f` files merge in order, each document by document, nulls kept.
#[test]
fn values_files_merge_every_document_in_order() -> eyre::Result<()> {
    let dir = tempfile::tempdir()?;
    let one = dir.path().join("one.yaml");
    let two = dir.path().join("two.yaml");
    let null = dir.path().join("null.yaml");
    fs::write(&one, "x: 1\n")?;
    fs::write(
        &two,
        indoc! {"
            x: 2
            ---
            z: 3
        "},
    )?;
    fs::write(&null, "x: null\n")?;
    let options = ValuesOptions {
        value_files: vec![one, two],
        json_values: vec![r#"y=[1,{"z":null}]"#.to_string()],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(
        have: options.merge_values()?,
        want: json!({"x": 2, "y": [1, {"z": null}], "z": 3})
    );
    let null_file = ValuesOptions {
        value_files: vec![null],
        ..ValuesOptions::default()
    };
    sim_assert_eq!(have: null_file.merge_values()?, want: json!({"x": null}));
    Ok(())
}

/// Helm reads `y: 3` as `"true": 3`; the port refuses rather than keep `y`.
#[test]
fn yaml_1_1_scalars_are_refused() -> eyre::Result<()> {
    let dir = tempfile::tempdir()?;
    for (index, source) in [
        "y: 3\n",
        "enabled: yes\n",
        "mode: 0644\n",
        "count: 1_000\n",
        "big: 9007199254740993\n",
        "1: one\n",
        indoc! {"
            base: &b {m: 1}
            merged:
              <<: *b
        "},
    ]
    .into_iter()
    .enumerate()
    {
        let file = dir.path().join(format!("{index}.yaml"));
        fs::write(&file, source)?;
        let options = ValuesOptions {
            value_files: vec![file],
            ..ValuesOptions::default()
        };
        assert!(
            matches!(options.merge_values(), Err(ValuesError::Unmodelled(_))),
            "{source}"
        );
    }
    Ok(())
}

fn chart_with(files: &[(&str, &str)]) -> eyre::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    for (name, contents) in files {
        let path: PathBuf = dir.path().join(name);
        fs::write(path, contents)?;
    }
    Ok(dir)
}

/// The loader refuses what Helm's loader refuses.
#[test]
fn charts_helm_cannot_load_are_not_validated() -> eyre::Result<()> {
    let manifest = indoc! {"
        apiVersion: v2
        name: p
        version: 0.1.0
    "};
    let bad_type = chart_with(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: p
                version: 0.1.0
                type: service
            "},
        ),
        ("values.yaml", "x: 1\n"),
    ])?;
    let bad_lock = chart_with(&[
        ("Chart.yaml", manifest),
        ("values.yaml", "x: 1\n"),
        ("Chart.lock", "generated: 5\n"),
    ])?;
    let no_manifest = chart_with(&[("values.yaml", "x: 1\n")])?;
    for (chart, reason) in [
        (
            &bad_type,
            "chart p: chart.metadata.type must be application or library",
        ),
        (
            &bad_lock,
            "cannot load Chart.lock: generated 5 is not a time",
        ),
        (&no_manifest, "Chart.yaml file is missing"),
    ] {
        let Err(ValuesError::NotValidated(have)) =
            acceptance_values(chart.path(), Value::Null, AcceptanceDocument::Template)
        else {
            eyre::bail!("Helm refuses to load the chart: {reason}");
        };
        sim_assert_eq!(have: have, want: reason);
    }
    Ok(())
}
