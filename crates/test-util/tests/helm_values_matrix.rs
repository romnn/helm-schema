//! The independent Helm v4.2.3 verification matrix (round 8, "astra"),
//! pinned to the output the pinned Helm printed for each row:
//! `helm template p <chart> <flags>` with a `{{ .Values | toJson }}`
//! template. Each row passes its literal flags through `ValuesOptions`;
//! the runner is `round8-coalesce-evidence/astra/matrix.py`.

use std::fs;
use std::path::Path;

use color_eyre::eyre;
use flate2::Compression;
use flate2::write::GzEncoder;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::helm_values::{
    AcceptanceDocument, DependencyValues, ValuesError, ValuesOptions, acceptance_values,
};
use test_util::prelude::sim_assert_eq;

/// How the matrix chart `p` carries its dependency `sub`.
enum Child<'a> {
    None,
    Vendored { values: &'a str, fields: &'a str },
    Packaged { values: &'a str },
}

fn chart(root_values: &str, child: &Child<'_>) -> eyre::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let fields = match child {
        Child::None => None,
        Child::Vendored { fields, .. } => Some(*fields),
        Child::Packaged { .. } => Some(""),
    };
    let mut manifest = indoc! {"
        apiVersion: v2
        name: p
        version: 0.1.0
    "}
    .to_string();
    if let Some(fields) = fields {
        manifest.push_str(indoc! {"
            dependencies:
              - name: sub
                version: 0.1.0
        "});
        for line in fields.lines() {
            manifest.push_str("    ");
            manifest.push_str(line);
            manifest.push('\n');
        }
    }
    fs::write(root.join("Chart.yaml"), manifest)?;
    fs::write(root.join("values.yaml"), root_values)?;
    let sub_manifest = indoc! {"
        apiVersion: v2
        name: sub
        version: 0.1.0
    "};
    match child {
        Child::None => {}
        Child::Vendored { values, .. } => {
            fs::create_dir_all(root.join("charts/sub"))?;
            fs::write(root.join("charts/sub/Chart.yaml"), sub_manifest)?;
            fs::write(root.join("charts/sub/values.yaml"), values)?;
        }
        Child::Packaged { values } => {
            fs::create_dir_all(root.join("charts"))?;
            let archive = fs::File::create(root.join("charts/sub-0.1.0.tgz"))?;
            let mut tar = tar::Builder::new(GzEncoder::new(archive, Compression::default()));
            append(&mut tar, "sub/Chart.yaml", sub_manifest)?;
            append(&mut tar, "sub/values.yaml", values)?;
            tar.into_inner()?.finish()?;
        }
    }
    Ok(dir)
}

fn append(
    tar: &mut tar::Builder<GzEncoder<fs::File>>,
    path: &str,
    contents: &str,
) -> eyre::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(contents.len())?);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, contents.as_bytes())?;
    Ok(())
}

/// The override map of a Helm command line's `--set-json`, `--set` and
/// `--set-string` flags.
fn flags(json: &[&str], set: &[&str], string: &[&str]) -> eyre::Result<Value> {
    let owned = |flags: &[&str]| flags.iter().map(ToString::to_string).collect();
    Ok(ValuesOptions {
        json_values: owned(json),
        values: owned(set),
        string_values: owned(string),
        ..ValuesOptions::default()
    }
    .merge_values()?)
}

fn template(dir: &Path, overrides: &Value) -> eyre::Result<Value> {
    Ok(acceptance_values(dir, overrides.clone(), AcceptanceDocument::Template)?.root)
}

#[test]
fn null_overrides_meet_or_miss_their_defaults() -> eyre::Result<()> {
    let meets = chart("{x: 1}", &Child::None)?;
    let dir = tempfile::tempdir()?;
    let null_file = dir.path().join("null.yaml");
    fs::write(&null_file, "{x: null}\n")?;
    let file = ValuesOptions {
        value_files: vec![null_file],
        ..ValuesOptions::default()
    }
    .merge_values()?;
    let nulls = [
        flags(&["x=null"], &[], &[])?,
        flags(&[], &["x=null"], &[])?,
        file,
    ];
    for null in &nulls {
        sim_assert_eq!(have: template(meets.path(), null)?, want: json!({}));
    }
    sim_assert_eq!(
        have: template(meets.path(), &flags(&[], &[], &["x=null"])?)?,
        want: json!({"x": "null"})
    );
    let unmatched = chart("{}", &Child::None)?;
    for null in &nulls {
        sim_assert_eq!(have: template(unmatched.path(), null)?, want: json!({"x": null}));
    }
    let default_nulls = chart(
        "{x: null, m: {x: null}, seq: [null, {x: null}]}",
        &Child::None,
    )?;
    sim_assert_eq!(
        have: template(default_nulls.path(), &flags(&["x=null", "m.x=null"], &[], &[])?)?,
        want: json!({"m": {"x": null}, "seq": [null, {"x": null}]})
    );
    Ok(())
}

#[test]
fn a_dependency_null_is_deleted_only_by_a_child_default() -> eyre::Result<()> {
    let overrides = flags(&["sub.enabled=null"], &[], &[])?;
    let parent_only = chart(
        "{sub: {enabled: true}}",
        &Child::Vendored {
            values: "{}",
            fields: "",
        },
    )?;
    sim_assert_eq!(
        have: template(parent_only.path(), &overrides)?,
        want: json!({"sub": {"enabled": null, "global": {}}})
    );
    let child_default = chart(
        "{sub: {enabled: true}}",
        &Child::Vendored {
            values: "{enabled: true}",
            fields: "",
        },
    )?;
    sim_assert_eq!(
        have: template(child_default.path(), &overrides)?,
        want: json!({"sub": {"global": {}}})
    );
    // Lint's template rule restores the default the template command drops.
    sim_assert_eq!(
        have: acceptance_values(child_default.path(), overrides, AcceptanceDocument::LintCoalescedTwice)?.dependencies,
        want: vec![DependencyValues {
            scope: vec!["sub".to_string()],
            values: json!({"enabled": true, "global": {}}),
        }]
    );
    Ok(())
}

#[test]
fn a_non_map_dependency_scope_is_a_type_mismatch() -> eyre::Result<()> {
    let scope = chart(
        "{}",
        &Child::Vendored {
            values: "{}",
            fields: "",
        },
    )?;
    let Err(ValuesError::NotValidated(reason)) = acceptance_values(
        scope.path(),
        flags(&["sub=false"], &[], &[])?,
        AcceptanceDocument::Template,
    ) else {
        eyre::bail!("Helm aborts on a non-map dependency scope");
    };
    sim_assert_eq!(have: reason, want: "Helm aborts: type mismatch on sub");
    Ok(())
}

#[test]
fn scalars_replace_tables_and_tables_replace_scalars() -> eyre::Result<()> {
    let conflict = chart("{a: {x: 1}, b: 1}", &Child::None)?;
    sim_assert_eq!(
        have: template(conflict.path(), &flags(&[], &["a=2", "b.x=3"], &[])?)?,
        want: json!({"a": 2, "b": {"x": 3}})
    );
    Ok(())
}

/// A child schema validates its scope with the injected `global`; the root
/// schema validates the root with the dependency's scope.
#[test]
fn globals_reach_the_dependency_scope_schemas_validate() -> eyre::Result<()> {
    let globals = chart(
        "{global: {x: 1}}",
        &Child::Vendored {
            values: "{}",
            fields: "",
        },
    )?;
    let values = acceptance_values(globals.path(), json!({}), AcceptanceDocument::Template)?;
    sim_assert_eq!(
        have: values.root,
        want: json!({"global": {"x": 1}, "sub": {"global": {"x": 1}}})
    );
    sim_assert_eq!(
        have: values.dependencies,
        want: vec![DependencyValues {
            scope: vec!["sub".to_string()],
            values: json!({"global": {"x": 1}}),
        }]
    );
    let global_null = chart(
        "{}",
        &Child::Vendored {
            values: "{}",
            fields: "",
        },
    )?;
    sim_assert_eq!(
        have: template(global_null.path(), &flags(&[r#"global={"x":null}"#], &[], &[])?)?,
        want: json!({"global": {"x": null}, "sub": {"global": {"x": null}}})
    );
    Ok(())
}

#[test]
fn an_alias_renames_the_scope_and_a_condition_removes_it() -> eyre::Result<()> {
    let alias = chart(
        "{}",
        &Child::Vendored {
            values: "{x: 1}",
            fields: "alias: a",
        },
    )?;
    sim_assert_eq!(
        have: template(alias.path(), &flags(&[], &["a.x=2"], &[])?)?,
        want: json!({"a": {"global": {}, "x": 2}})
    );
    let disabled = chart(
        "{sub: {enabled: false}, tags: {group: true}}",
        &Child::Vendored {
            values: "{x: 1}",
            fields: indoc! {"
                condition: sub.enabled
                tags: [group]
            "},
        },
    )?;
    sim_assert_eq!(
        have: template(disabled.path(), &json!({}))?,
        want: json!({"sub": {"enabled": false}, "tags": {"group": true}})
    );
    Ok(())
}

/// Imports read the chart defaults, never the overrides.
#[test]
fn imports_copy_child_defaults_into_the_parent() -> eyre::Result<()> {
    let exports = chart(
        "{}",
        &Child::Vendored {
            values: "{exports: {cfg: {x: 1}}}",
            fields: "import-values: [cfg]",
        },
    )?;
    sim_assert_eq!(
        have: template(exports.path(), &json!({}))?,
        want: json!({"sub": {"exports": {"cfg": {"x": 1}}, "global": {}}, "x": 1})
    );
    sim_assert_eq!(
        have: template(exports.path(), &flags(&[], &["sub.exports.cfg.x=2"], &[])?)?,
        want: json!({"sub": {"exports": {"cfg": {"x": 2}}, "global": {}}, "x": 1})
    );
    let child_parent = chart(
        "{dest: {x: 9}}",
        &Child::Vendored {
            values: "{cfg: {x: 1, z: 2}}",
            fields: "import-values: [{child: cfg, parent: dest}]",
        },
    )?;
    sim_assert_eq!(
        have: template(child_parent.path(), &json!({}))?,
        want: json!({"dest": {"x": 9, "z": 2}, "sub": {"cfg": {"x": 1, "z": 2}, "global": {}}})
    );
    Ok(())
}

#[test]
fn a_packaged_dependency_loads_from_its_archive() -> eyre::Result<()> {
    let tgz = chart("{}", &Child::Packaged { values: "{x: 1}" })?;
    sim_assert_eq!(
        have: template(tgz.path(), &json!({}))?,
        want: json!({"sub": {"global": {}, "x": 1}})
    );
    Ok(())
}

#[test]
fn later_values_documents_win() -> eyre::Result<()> {
    let multidoc = chart(
        indoc! {"
            a: 1
            ---
            a: 2
            b: 3
        "},
        &Child::None,
    )?;
    sim_assert_eq!(have: template(multidoc.path(), &json!({}))?, want: json!({"a": 2, "b": 3}));
    Ok(())
}

/// The chart loader honours `.helmignore`; lint's values rule reads
/// `values.yaml` from disk regardless.
#[test]
fn an_ignored_values_file_is_no_default_except_to_lint_raw() -> eyre::Result<()> {
    let ignored = chart("{x: 1}", &Child::None)?;
    fs::write(ignored.path().join(".helmignore"), "values.yaml\n")?;
    sim_assert_eq!(have: template(ignored.path(), &json!({}))?, want: json!({}));
    sim_assert_eq!(
        have: acceptance_values(ignored.path(), json!({}), AcceptanceDocument::LintRaw)?.root,
        want: json!({"x": 1})
    );
    Ok(())
}

/// Helm's lint-versus-template witness: `{"not": {"required": ["x"]}}`
/// passes `helm template --set-json x=null` and fails `helm lint`, whose
/// second pass restores `x`.
#[test]
fn lint_validates_the_default_template_deletes() -> eyre::Result<()> {
    let meets = chart("{x: 1}", &Child::None)?;
    let overrides = flags(&["x=null"], &[], &[])?;
    sim_assert_eq!(have: template(meets.path(), &overrides)?, want: json!({}));
    sim_assert_eq!(
        have: acceptance_values(meets.path(), overrides, AcceptanceDocument::LintCoalescedTwice)?.root,
        want: json!({"x": 1})
    );
    Ok(())
}
