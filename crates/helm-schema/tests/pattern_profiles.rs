//! Pattern contracts across schema emission profiles.

use color_eyre::eyre;
use helm_schema::{AnalysisSession, generation::SchemaProfile};
use helm_schema_test_support::{
    acceptance::discovery::{ProbeInstance, ProfileSchemas},
    generate,
    registry::ChartRecipe,
};
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[test]
fn both_profiles_abstain_on_dot_without_guaranteed_unicode_mode() -> eyre::Result<()> {
    let chart = pattern_chart(
        "version: va\n",
        indoc! {r#"
            {{- if not (regexMatch "^v.$" .Values.version) }}
            {{- fail "version mismatch" }}
            {{- end }}
        "#},
    )?;
    let (full, lean) = profile_schemas(&chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, json!({"version": "va"}))?;
    for value in ["v\r", "v😀", "v\n"] {
        let probe = ProbeInstance::SparseOverride(json!({"version": value}));
        sim_assert_eq!(have: profiles.verdicts(&probe), want: (true, true), "{value:?}");
    }
    sim_assert_eq!(have: full, want: expected_pattern_schema(None, true));
    sim_assert_eq!(have: lean, want: expected_pattern_schema(None, false));
    Ok(())
}

#[test]
fn both_profiles_keep_go_repeats_with_leading_zero_as_literals() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^a{01}[bc]$""#, r"^a\{01\}[bc]$", "a{01}b", "ab")
}

#[test]
fn both_profiles_keep_go_spaced_repeat_as_literals() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^a{ 2 }[bc]$""#, r"^a\{ 2 \}[bc]$", "a{ 2 }b", "aab")
}

#[test]
fn both_profiles_match_go_ascii_whitespace() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^\\s+$""#, r"^[\t\n\f\r ]+$", " ", "\u{00A0}")
}

#[test]
fn both_profiles_allow_astral_nondigits() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^[\\D]+$""#, r"^[^0-9]+$", "😀", "3")
}

#[test]
fn both_profiles_allow_astral_values_in_negated_brackets() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^[^a]+$""#, r"^[^a]+$", "😀", "a")
}

#[test]
fn both_profiles_abstain_on_negated_surrogate_ranges() -> eyre::Result<()> {
    assert_pattern_abstains(
        r#"not (regexMatch "^[^\\x{D7FF}-\\x{E000}]+$" .Values.version)"#,
        "😀",
    )
}

#[test]
fn both_profiles_abstain_on_surrogate_ranges_under_schema_negation() -> eyre::Result<()> {
    assert_pattern_abstains(
        r#"regexMatch "^[\\x{D7FF}-\\x{E000}]+$" .Values.version"#,
        "😀",
    )
}

#[test]
fn both_profiles_abstain_on_ambiguous_leading_hyphens() -> eyre::Result<()> {
    assert_pattern_abstains(r#"not (regexMatch "^[--a]+$" .Values.version)"#, "5")
}

#[test]
fn constant_regex_whitespace_uses_go_ascii_membership() -> eyre::Result<()> {
    let chart = pattern_chart(
        "version: va\n",
        indoc! {r#"
            {{- if or (regexMatch "^\\s+$" "\u00a0") (not (regexMatch "^\\s+$" " ")) }}
            {{- fail "Go regexp whitespace mismatch" }}
            {{- end }}
        "#},
    )?;
    let (full, lean) = profile_schemas(&chart)?;
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": { "global": {}, "version": {} },
        "type": "object",
    });
    sim_assert_eq!(have: full, want: expected);
    sim_assert_eq!(have: lean, want: expected);
    Ok(())
}

#[test]
fn both_profiles_keep_go_braced_hex_escapes() -> eyre::Result<()> {
    assert_mandatory_pattern(r#""^v\\x{61}+$""#, r"^v\x61+$", "vaa", "vb")
}

#[test]
fn foreign_guard_and_default_leave_their_successful_alternatives_open() -> eyre::Result<()> {
    let chart = pattern_chart(
        indoc! {"
            enabled: false
            version: v1.0
        "},
        indoc! {r#"
            {{- if .Values.enabled }}
            {{- if not (regexMatch "^v[0-9]+\\.[0-9]+$" (default "v1.0" .Values.version)) }}
            {{- fail "version mismatch" }}
            {{- end }}
            {{- end }}
        "#},
    )?;
    let (full, lean) = profile_schemas(&chart)?;
    let expected_lean = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": { "enabled": {}, "global": {}, "version": {} },
        "type": "object",
    });
    let version_truthy = json!({"$ref": "#/definitions/constrains~1version:t"});
    let enabled = json!({"$ref": "#/definitions/constrains~1enabled:t"});
    let matches = json!({"$ref": "#/definitions/constrains~1version"});
    let type_check = required_version(&json!({"type": "string"}));
    let truthy_ref = json!({"$ref": "#/definitions/t"});
    let expected_full = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {"allOf": [&version_truthy, &enabled]},
                "then": version_requirement(&json!({"type": "string"})),
            },
            {
                "if": {"anyOf": [
                    {"allOf": [{"not": type_check}, &version_truthy, &matches, &enabled]},
                    {"allOf": [{"not": &matches}, &version_truthy, &enabled]},
                ]},
                "then": false,
            },
        ],
        "definitions": {
            "constrains/enabled:t": {
                "properties": {"enabled": &truthy_ref},
                "required": ["enabled"],
                "type": "object",
            },
            "constrains/version": required_version(&json!({"pattern": r"^v[0-9]+\.[0-9]+$", "type": "string"})),
            "constrains/version:t": required_version(&truthy_ref),
            "t": {"anyOf": [
                {"minProperties": 1, "type": "object"},
                {"not": {"const": 0}, "type": "number"},
                {"minLength": 1, "type": "string"},
                {"minItems": 1, "type": "array"},
                {"const": true},
            ]},
        },
        "properties": { "enabled": {}, "global": {}, "version": {} },
        "type": "object",
    });
    let defaults = test_util::helm_values::coalesce_chart_values(chart.path(), json!({}))?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    for (overrides, verdicts) in [
        (json!({"enabled": false, "version": "v1"}), (true, true)),
        (json!({"enabled": true, "version": "v1"}), (false, true)),
        (json!({"enabled": true, "version": "v1.0"}), (true, true)),
        (json!({"enabled": true, "version": ""}), (true, true)),
        (json!({"enabled": true, "version": null}), (true, true)),
        (json!({"enabled": false, "version": null}), (true, true)),
    ] {
        let probe = ProbeInstance::SparseOverride(overrides);
        sim_assert_eq!(have: profiles.verdicts(&probe), want: verdicts, "{probe:?}");
    }
    sim_assert_eq!(have: full, want: expected_full);
    sim_assert_eq!(have: lean, want: expected_lean);
    Ok(())
}

fn assert_pattern_abstains(helm_condition: &str, value: &str) -> eyre::Result<()> {
    let check = formatdoc! {r#"
        {{{{- if {helm_condition} }}}}
        {{{{- fail "version mismatch" }}}}
        {{{{- end }}}}
    "#};
    let defaults = json!({"version": "v"});
    let chart = pattern_chart(&defaults.to_string(), &check)?;
    let (full, lean) = profile_schemas(&chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    let probe = ProbeInstance::SparseOverride(json!({"version": value}));
    sim_assert_eq!(have: profiles.verdicts(&probe), want: (true, true));
    sim_assert_eq!(have: full, want: expected_pattern_schema(None, true));
    sim_assert_eq!(have: lean, want: expected_pattern_schema(None, false));
    Ok(())
}

fn assert_mandatory_pattern(
    helm_pattern: &str,
    ecma_pattern: &str,
    accepted: &str,
    rejected: &str,
) -> eyre::Result<()> {
    let check = formatdoc! {r#"
        {{{{- if not (regexMatch {helm_pattern} .Values.version) }}}}
        {{{{- fail "version mismatch" }}}}
        {{{{- end }}}}
    "#};
    let defaults = json!({"version": accepted});
    let chart = pattern_chart(&defaults.to_string(), &check)?;
    let (full, lean) = profile_schemas(&chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    for (value, accepts) in [(accepted, true), (rejected, false)] {
        let probe = ProbeInstance::SparseOverride(json!({"version": value}));
        sim_assert_eq!(have: profiles.verdicts(&probe), want: (accepts, accepts), "{value:?}");
    }
    for (mut actual, full_profile) in [(full, true), (lean, false)] {
        let mut expected = expected_pattern_schema(Some(ecma_pattern), full_profile);
        // The order of root conjuncts has no effect on validation.
        for schema in [&mut actual, &mut expected] {
            if let Some(conjuncts) = schema.get_mut("allOf").and_then(Value::as_array_mut) {
                conjuncts.sort_by_cached_key(Value::to_string);
            }
        }
        sim_assert_eq!(have: actual, want: expected);
    }
    Ok(())
}

fn expected_pattern_schema(pattern: Option<&str>, full: bool) -> Value {
    let string = json!({"type": "string"});
    let mut requirements = vec![version_requirement(&string)];
    let mut terminal = Vec::new();
    if let Some(pattern) = pattern {
        let matches = json!({"pattern": pattern, "type": "string"});
        requirements.push(version_requirement(&matches));
        terminal.push(json!({
            "anyOf": [
                {"not": required_version(&matches)},
                {"not": required_version(&string)},
            ],
        }));
    }
    if full {
        if pattern.is_none() {
            terminal.push(json!({"not": required_version(&string)}));
        }
        terminal.extend([
            json!({"not": required_version(&json!({}))}),
            required_version(&json!({"enum": [null]})),
        ]);
        requirements.push(json!({"if": {"anyOf": terminal}, "then": false}));
    }
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": requirements,
        "properties": { "global": {}, "version": {} },
        "type": "object",
    })
}

fn version_requirement(schema: &Value) -> Value {
    json!({"additionalProperties": {}, "properties": {"version": schema}})
}

fn required_version(schema: &Value) -> Value {
    json!({"properties": {"version": schema}, "required": ["version"], "type": "object"})
}

fn pattern_chart(values: &str, check: &str) -> eyre::Result<ScratchDir> {
    let chart = ScratchDir::new("pattern-profiles")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: pattern-profiles
            version: 0.1.0
        "},
    )?;
    std::fs::write(chart.path().join("values.yaml"), values)?;
    std::fs::write(
        chart.path().join("templates/configmap.yaml"),
        formatdoc! {"
            {check}apiVersion: v1
            kind: ConfigMap
            metadata:
              name: pattern-profiles
        "},
    )?;
    Ok(chart)
}

fn profile_schemas(chart: &ScratchDir) -> eyre::Result<(Value, Value)> {
    let schema = |profile| {
        let recipe = ChartRecipe::corpus("pattern-profiles", profile);
        let options = generate::generate_options_at(chart.path(), &recipe);
        generate::session_schema(&AnalysisSession::new(options), &recipe)
    };
    Ok((schema(SchemaProfile::Full)?, schema(SchemaProfile::Lean)?))
}
