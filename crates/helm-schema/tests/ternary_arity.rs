//! Full-schema regressions for the operand count of Sprig's `ternary`.
//!
//! `ternary` takes exactly two values and a condition. Go checks a call's
//! operand count before evaluating it, so any other count aborts wherever the
//! call is reached: pinned Helm 4.2.3 fails with `wrong number of args for
//! ternary: want 3 got N` on every values document that reaches it, and
//! renders every document that does not.

use std::fs;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::AnalysisSession;
use helm_schema_gen::{AuthoringPolicy, DeclaredTypes, RootPolicy, SchemaProfile};
use helm_schema_test_support::generate::generate_options_at;
use helm_schema_test_support::registry::ChartRecipe;
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// The whole-chart schema of a probe chart whose only template is `call`
/// followed by a `ConfigMap`, under the witness authoring options
/// (`--declared-types annotate`, closed root), without the
/// self-identification annotations (`$schema`, the generated marker and the
/// policy fingerprint), which carry no acceptance semantics.
fn probe_schema(values: &str, call: &str) -> eyre::Result<Value> {
    let scratch = ScratchDir::new("ternary-arity")?;
    let root = scratch.path();
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: probe
            version: 0.1.0
        "},
    )?;
    fs::write(root.join("values.yaml"), values)?;
    fs::write(
        root.join("templates/probe.yaml"),
        formatdoc! {"
            {call}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: probe
        "},
    )?;
    let recipe = ChartRecipe {
        authoring: AuthoringPolicy {
            root: RootPolicy::default(),
            declared_types: DeclaredTypes::Annotate,
        },
        ..ChartRecipe::corpus("probe", SchemaProfile::Full)
    };
    let session = AnalysisSession::new(generate_options_at(root, &recipe));
    let mut schema = session.generated_schema()?.schema;
    let object = schema
        .as_object_mut()
        .ok_or_eyre("generated schema is not an object")?;
    for key in ["$schema", "x-helm-schema-generated", "x-helm-schema-policy"] {
        object.remove(key);
    }
    Ok(schema)
}

/// The schema that rejects exactly the documents with a Helm-truthy
/// `enabled`, the guard every guarded probe below sits under.
fn rejects_truthy_enabled() -> Value {
    json!({
        "$defs": {
            "t": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" }
                ]
            }
        },
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "properties": { "enabled": { "$ref": "#/$defs/t" } },
                    "required": ["enabled"],
                    "type": "object"
                },
                "then": false
            }
        ],
        "properties": {
            "enabled": {},
            "global": {}
        },
        "type": "object"
    })
}

/// Two operands: Helm aborts with `want 3 got 2` once `enabled` is truthy.
#[test]
fn ternary_with_two_operands_aborts_where_reached() -> eyre::Result<()> {
    let schema = probe_schema(
        "enabled: false\n",
        r#"{{ if .Values.enabled }}{{ $_ := ternary "a" "b" }}{{ end }}"#,
    )?;
    sim_assert_eq!(have: schema, want: rejects_truthy_enabled());
    Ok(())
}

/// A piped condition counts as an operand: one value plus the pipe is two.
#[test]
fn piped_ternary_with_one_value_aborts_where_reached() -> eyre::Result<()> {
    let schema = probe_schema(
        "enabled: false\n",
        r#"{{ if .Values.enabled }}{{ $_ := true | ternary "a" }}{{ end }}"#,
    )?;
    sim_assert_eq!(have: schema, want: rejects_truthy_enabled());
    Ok(())
}

/// Three values plus the pipe is four operands.
#[test]
fn piped_ternary_with_three_values_aborts_where_reached() -> eyre::Result<()> {
    let schema = probe_schema(
        "enabled: false\n",
        r#"{{ if .Values.enabled }}{{ $_ := true | ternary "a" "b" "c" }}{{ end }}"#,
    )?;
    sim_assert_eq!(have: schema, want: rejects_truthy_enabled());
    Ok(())
}

/// An unguarded call with four operands aborts on every values document.
#[test]
fn unguarded_ternary_with_four_operands_rejects_every_document() -> eyre::Result<()> {
    let schema = probe_schema("{}\n", r#"{{ $_ := ternary "a" "b" true "extra" }}"#)?;
    sim_assert_eq!(
        have: schema,
        want: json!({
            "additionalProperties": false,
            "allOf": [false],
            "properties": { "global": {} },
            "type": "object"
        })
    );
    Ok(())
}
