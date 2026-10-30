//! Helper and dependency boundaries must preserve the values Helm can render.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use helm_schema_gen::{PreparedValuesDocuments, ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::{ContractIr, DependencyValuesRoot, SymbolicIrContext};
use helm_schema_k8s::Chain;
use helm_schema_test_support::{generate, registry::ChartRecipe};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const SCRUB: &str = indoc! {r#"
    {{- define "scrub" -}}
      {{- $result := dict -}}
      {{- range $key, $value := . -}}
        {{- if kindIs "map" $value -}}
          {{- $nested := include "scrub" $value | fromYaml -}}
          {{- if gt (len $nested) 0 -}}
            {{- $_ := set $result $key $nested -}}
          {{- end -}}
        {{- else if not (kindIs "invalid" $value) -}}
          {{- $_ := set $result $key $value -}}
        {{- end -}}
      {{- end -}}
      {{- toYaml $result -}}
    {{- end -}}
"#};

fn template_schema(source: &str, values: &Value) -> eyre::Result<Value> {
    let chart = ScratchDir::new("helper-boundaries")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: repro
        version: 1.0.0
    "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        serde_json::to_vec(values)?,
    )?;
    std::fs::write(chart.path().join("templates/_helpers.tpl"), SCRUB)?;
    std::fs::write(chart.path().join("templates/cm.yaml"), source)?;
    let recipe = ChartRecipe::corpus("airflow", helm_schema_gen::SchemaProfile::Full);
    generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(chart.path(), &recipe)),
        &recipe,
    )
}

fn schema(
    contract: ContractIr,
    raw: &Value,
    composed: &Value,
    refill: &Value,
) -> eyre::Result<Value> {
    let signals = contract.finalize().into_schema_signals();
    let documents = PreparedValuesDocuments::new(
        serde_yaml::to_value(composed)?,
        serde_yaml::to_value(refill)?,
    )
    .with_uncoalesced_root(serde_yaml::to_value(raw)?);
    let provider = Chain::new(Vec::new());
    let mut input = ValuesSchemaInput::new(&signals, &provider);
    input.values_documents = Some(&documents);
    Ok(generate_values_schema(input))
}

/// An opaque recursive result must withdraw unsupported default field typing.
#[test]
fn recursive_helper_preserves_guarded_worker_fields() -> eyre::Result<()> {
    let values = json!({"workers": {"affinity": {}}});
    let have = template_schema(
        indoc! {r#"
            {{- $workers := include "scrub" .Values.workers | fromYaml -}}
            apiVersion: v1
            kind: Pod
            spec:
              {{- with $workers.affinity }}
              affinity: {{ toYaml . | nindent 4 }}
              {{- end }}
        "#},
        &values,
    )?;
    let want = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {"global": {}, "workers": {}},
        "type": "object"
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

/// Formatting after recursive decoding must not restore the input default type.
#[test]
fn recursive_helper_preserves_formatted_worker_fields() -> eyre::Result<()> {
    let values = json!({"workers": {"safeToEvict": true}});
    let have = template_schema(
        indoc! {r#"
            {{- $workers := include "scrub" .Values.workers | fromYaml -}}
            {{- $annotations := mergeOverwrite (dict) (dict "safe" (toString $workers.safeToEvict)) -}}
            apiVersion: v1
            kind: Pod
            metadata:
              annotations: {{ toYaml $annotations | nindent 4 }}
        "#},
        &values,
    )?;
    let want = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {"global": {}, "workers": {}},
        "type": "object"
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

fn dependency_contract(path: &str, failure: Predicate) -> ContractIr {
    let mut contract = ContractIr::default();
    contract.push_pathless_scalar(ValuesPath::parse(&format!("{path}.enabled")));
    contract.push_dependency_values_root(DependencyValuesRoot {
        path: ValuesPath::parse(path),
        active: vec![vec![Guard::Truthy {
            path: ValuesPath::parse(&format!("{path}.enabled")),
        }]],
        asserted_before_pruning: true,
        declared_by_parent: true,
    });
    contract.add_terminal_fail_condition(failure);
    contract
}

#[test]
fn dependency_default_failure_requires_the_supplied_member() -> eyre::Result<()> {
    let enabled = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("vpa.enabled"),
    });
    let failure = Predicate::Or(vec![
        Predicate::all(vec![
            enabled.clone(),
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("vpa.serviceAccount"),
                schema_type: "object".into(),
            })
            .negated(),
        ]),
        Predicate::all(vec![
            enabled,
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse("vpa.extra"),
            }),
        ]),
    ]);
    let raw = json!({"vpa": {"enabled": false}});
    let composed =
        json!({"vpa": {"enabled": false, "serviceAccount": {"create": true}, "extra": true}});
    let have = schema(
        dependency_contract("vpa", failure.clone()),
        &raw,
        &composed,
        &composed,
    )?;
    let want = schema(
        dependency_contract("vpa", failure),
        &json!({"vpa": {"enabled": true}}),
        &composed,
        &composed,
    )?;
    sim_assert_eq!(have: &have, want: &want);
    assert!(jsonschema::validator_for(&have)?.is_valid(&json!({"vpa": {"enabled": true}})));
    Ok(())
}

#[test]
fn raw_lint_does_not_inherit_a_deleted_dependency_abort() -> eyre::Result<()> {
    let failure = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("redis"),
        schema_type: "object".into(),
    })
    .negated();
    let values = json!({"redis": {"enabled": true}});
    let have = schema(
        dependency_contract("redis", failure.clone()),
        &values,
        &values,
        &values,
    )?;
    let mut want = schema(
        dependency_contract("redis", Predicate::False),
        &values,
        &values,
        &values,
    )?;
    want["$defs"]["helm-condition-0"] = json!({"not": {
        "properties": {"redis": {"type": "object"}},
        "required": ["redis"], "type": "object"
    }});
    want["allOf"]
        .as_array_mut()
        .ok_or_eyre("expected dependency table assertion")?
        .push(json!({
            "if": {"$ref": "#/$defs/helm-condition-0"},
            "then": {
                "if": {
                    "properties": {"redis": {"not": {"enum": [null]}}},
                    "required": ["redis"], "type": "object"
                },
                "then": false
            }
        }));
    sim_assert_eq!(have: &have, want: &want);
    let validator = jsonschema::validator_for(&have)?;
    let document = json!({"redis": null});
    let errors = validator
        .iter_errors(&document)
        .map(|error| format!("{error} at {}", error.schema_path()))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: errors, want: Vec::<String>::new());
    Ok(())
}

/// Helm skips a null dependency condition; metadata alone cannot require a boolean.
#[test]
fn dependency_activation_metadata_admits_null_flags() -> eyre::Result<()> {
    let chart = ScratchDir::new("dependency-flags")?;
    std::fs::create_dir_all(chart.path().join("charts/child"))?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: repro
        version: 1.0.0
        dependencies:
          - name: child
            version: 1.0.0
            condition: child.enabled
    "},
    )?;
    std::fs::write(
        chart.path().join("charts/child/Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: child
        version: 1.0.0
    "},
    )?;
    std::fs::write(
        chart.path().join("charts/child/values.yaml"),
        "enabled: true",
    )?;
    let raw = json!({"child": {"enabled": false}});
    std::fs::write(chart.path().join("values.yaml"), serde_json::to_vec(&raw)?)?;
    let source = indoc! {r"
        {{ if .Values.child.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        {{ end }}
    "};
    std::fs::write(chart.path().join("templates/cm.yaml"), source)?;
    let recipe = ChartRecipe {
        minimize: false,
        ..ChartRecipe::corpus("airflow", helm_schema_gen::SchemaProfile::Full)
    };
    let have = generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(chart.path(), &recipe)),
        &recipe,
    )?;
    let enabled = ValuesPath::parse("child.enabled");
    let mut contract = SymbolicIrContext::new(&DefineIndex::new()).generate_contract_ir(source);
    contract.push_pathless_scalar(ValuesPath::parse("child"));
    contract.push_dependency_values_root(DependencyValuesRoot {
        path: ValuesPath::parse("child"),
        active: vec![
            vec![Guard::Eq {
                path: enabled.clone(),
                value: GuardValue::Bool(true),
            }],
            vec![
                Guard::NotEq {
                    path: enabled.clone(),
                    value: GuardValue::Bool(true),
                },
                Guard::NotEq {
                    path: enabled,
                    value: GuardValue::Bool(false),
                },
            ],
        ],
        asserted_before_pruning: true,
        declared_by_parent: true,
    });
    let want = schema(contract, &raw, &raw, &json!({"child": {"enabled": true}}))?;
    sim_assert_eq!(have: &have, want: &want);
    assert!(jsonschema::validator_for(&have)?.is_valid(&json!({"child": {"enabled": null}})));
    Ok(())
}

/// An unresolved root merge cannot close the raw document to the pre-rewrite keys.
#[test]
fn opaque_root_rebinding_withdraws_only_key_closure() -> eyre::Result<()> {
    let values = json!({"_internal_defaults": {"pilot": {"env": {}}}});
    let body = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: opaque-root
        data:
          {{- with .Values.pilot.env }}
          {{- toYaml . | nindent 2 }}
          {{- end }}
    "};
    let source = indoc::formatdoc! {r#"
        {{{{ $defaults := .Values._internal_defaults }}}}
        {{{{ $_ := unset .Values "_internal_defaults" }}}}
        {{{{ $_ := set .Values "_original" (dict) }}}}
        {{{{ $_ := set $ "Values" (mustMergeOverwrite $defaults $.Values) }}}}
        {body}
    "#};
    let have = template_schema(&source, &values)?;
    let want = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": {},
        "properties": {"_internal_defaults": {}, "global": {}},
        "$defs": {
            "constrains/_internal_defaults+_internal_defaults=null": {
                "anyOf": [
                    {"not": {"type": "object", "properties": {"_internal_defaults": {}}, "required": ["_internal_defaults"]}},
                    {"type": "object", "properties": {"_internal_defaults": {"type": "object"}}, "required": ["_internal_defaults"]},
                    {"type": "object", "properties": {"_internal_defaults": {"enum": [null]}}, "required": ["_internal_defaults"]}
                ]
            },
            "constrains/_internal_defaults+_internal_defaults=null@2": {
                "not": {"$ref": "#/$defs/constrains~1_internal_defaults+_internal_defaults=null"}
            }
        },
        "allOf": [{
            "if": {"$ref": "#/$defs/constrains~1_internal_defaults+_internal_defaults=null@2"},
            "then": false
        }]
    });
    sim_assert_eq!(have: &have, want: &want);
    let helper_source = indoc::formatdoc! {r#"
        {{{{ define "replace-original-root" }}}}
        {source}
        {{{{ end }}}}
        {{{{ include "replace-original-root" $ }}}}
    "#};
    let helper_have = template_schema(&helper_source, &values)?;
    sim_assert_eq!(have: &helper_have, want: &want);
    let validator = jsonschema::validator_for(&have)?;
    assert!(validator.is_valid(&json!({"pilot": {"env": {"EXTRA": "ok"}}})));
    assert!(!validator.is_valid(&json!({"_internal_defaults": 7})));

    // A known replacement and a transparent alias retain the caller's closed key policy.
    let constant_body = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: known-root
    "};
    let unchanged = template_schema(constant_body, &values)?;
    for prefix in [
        r#"{{ $_ := set $ "Values" (dict) }}"#,
        r#"{{ $_ := set $ "Values" .Values }}"#,
        indoc! {r#"
            {{ $opaque := dict }}
            {{ $_ := set $opaque (randAlphaNum 1) true }}
            {{ $_ := set $ "Values" $opaque }}
        "#},
        indoc! {r#"
            {{ define "replace-local-root" }}
            {{ $opaque := dict }}
            {{ $_ := set $opaque (randAlphaNum 1) true }}
            {{ $_ := set $ "Values" $opaque }}
            {{ end }}
            {{ include "replace-local-root" (dict) }}
        "#},
        indoc! {r#"
            {{ $ := dict }}
            {{ $opaque := dict }}
            {{ $_ := set $opaque (randAlphaNum 1) true }}
            {{ $_ := set $ "Values" $opaque }}
        "#},
    ] {
        let source = indoc::formatdoc!("{prefix}\n{constant_body}");
        let have = template_schema(&source, &values)?;
        sim_assert_eq!(have: &have, want: &unchanged);
    }
    Ok(())
}

/// An opaque replacement sourced from one member cannot consume unknown raw root keys.
#[test]
fn scalar_input_region_keeps_raw_root_key_closure() -> eyre::Result<()> {
    let values = json!({"source": "value"});
    let prefix = indoc! {"
        {{ $value := b64enc .Values.source }}
        {{ $opaque := dict }}
        {{ $_ := set $opaque (randAlphaNum 1) $value }}
    "};
    let body = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: scoped-root
    "};
    let control = indoc::formatdoc!("{prefix}\n{body}");
    let source = indoc::formatdoc! {r#"
        {prefix}
        {{{{ $_ := set $ "Values" $opaque }}}}
        {body}
    "#};
    let want = template_schema(&control, &values)?;
    let have = template_schema(&source, &values)?;
    sim_assert_eq!(have: &have, want: &want);
    let validator = jsonschema::validator_for(&have)?;
    assert!(validator.is_valid(&json!({"source": "value"})));
    assert!(!validator.is_valid(&json!({"source": "value", "extra": true})));
    assert!(!validator.is_valid(&json!({"source": 7})));
    Ok(())
}
