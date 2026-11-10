//! Discriminating laws for the R3b target architecture (ADR 0072, ADR 0073 step 1).
//!
//! Each law is a chart and a set of coalesced values documents with the
//! verdict real Helm 4.2.3 gives them (`helm template --kube-version 1.29.0`,
//! confirmed by the pinned helmsweep driver). The receipts (chart, user values
//! file, Helm output) are kept per law under the step-1 lane's
//! `transfer/receipts/<law>/`. Documents are the values Helm validates: the
//! user file coalesced over the chart defaults, with null deletion of declared
//! keys and globals copied into each dependency instance.
//!
//! A law split into `accepts` and `rejects` separates the false-rejection
//! guard from the false-acceptance closure. A law the current line fails is
//! ignored with the ADR 0073 step and ADR 0072 invariant that must turn it
//! green.

use color_eyre::eyre;
use indoc::indoc;
use serde_json::{Value, json};
use vfs::VfsPath;

use test_util::prelude::sim_assert_eq;

const CHART: &str = indoc! {"
    apiVersion: v2
    name: law
    version: 0.1.0
"};

const CONFIG_MAP: &str = indoc! {"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: law
"};

/// The final schema for a chart whose files are given relative to its root.
///
/// Declared defaults only annotate, so every verdict comes from template and
/// Helm semantics rather than authoring policy (ADR 0043).
fn law_schema(files: &[(&str, &str)]) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    for (path, source) in files {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring: crate::generation::AuthoringPolicy {
            declared_types: crate::generation::DeclaredTypes::Annotate,
            ..Default::default()
        },
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            ..Default::default()
        },
    });
    Ok(session.emit(crate::output::EmitRequest {
        reference_policy: crate::output::ReferencePolicy::SelfContained,
        output: crate::output::OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: crate::output::DefinitionNames::Source,
        },
    })?)
}

/// A single-template chart: its defaults, the template's parts in order, and
/// an optional helper file.
fn template_law_schema(values: &str, template: &[&str], helpers: &str) -> eyre::Result<Value> {
    let template = template.concat();
    let mut files = vec![
        ("Chart.yaml", CHART),
        ("values.yaml", values),
        ("templates/cm.yaml", template.as_str()),
    ];
    if !helpers.is_empty() {
        files.push(("templates/_helpers.tpl", helpers));
    }
    law_schema(&files)
}

/// Compares the schema's verdict on each coalesced document with Helm's.
fn assert_helm_verdicts(
    schema: &Value,
    cells: &[(&str, Value)],
    helm_renders: bool,
) -> eyre::Result<()> {
    let validator = jsonschema::validator_for(schema)?;
    let have = cells
        .iter()
        .map(|(label, document)| (*label, validator.is_valid(document)))
        .collect::<Vec<_>>();
    let want = cells
        .iter()
        .map(|(label, _)| (*label, helm_renders))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: want, "{schema:#}");
    Ok(())
}

fn assumption_join_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            enabled: false
            program: ""
            after: "x"
        "#},
        &[
            "{{- if .Values.enabled }}{{ tpl .Values.program $ }}{{ end }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  after: {{ upper .Values.after | quote }}
            "},
        ],
        "",
    )
}

/// (a) A `tpl` program under a branch may write a value a later typed use
/// reads: the assumption "the program writes nothing" survives the join only
/// on the arm that ran it (Sol finding 1).
#[test]
fn assumption_join_accepts_documents_a_conditional_program_repairs() -> eyre::Result<()> {
    assert_helm_verdicts(
        &assumption_join_schema()?,
        &[
            (
                "a1: enabled, the program sets after to a string",
                json!({
                    "enabled": true,
                    "program": r#"{{ $_ := set .Values "after" "ok" }}"#,
                    "after": 7
                }),
            ),
            (
                "a4: disabled, string after",
                json!({ "enabled": false, "program": "", "after": "y" }),
            ),
        ],
        true,
    )
}

/// (a) With the branch not taken no program runs, so the typed use reads the
/// input: validity joins as `(enabled ∧ write_free) ∨ ¬enabled`.
#[test]
#[ignore = "target: ADR 0073 step 3 (ADR 0072 §2: analysis validity joins as (branch ∧ valid) ∨ ¬branch)"]
fn assumption_join_rejects_the_untaken_branch_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &assumption_join_schema()?,
        &[(
            "a2: disabled, integer after",
            json!({ "enabled": false, "program": "", "after": 7 }),
        )],
        false,
    )
}

fn live_alias_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            payload:
              x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $alias := .Values.payload }}
                {{- $_ := set $alias "x" "ok" }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper .Values.payload.x | quote }}
            "},
        ],
        "",
    )
}

/// (b) A live alias written by `set` is what a later read sees.
#[test]
fn live_alias_write_accepts_an_overwritten_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &live_alias_schema()?,
        &[(
            "b1: integer x is overwritten",
            json!({ "payload": { "x": 7 } }),
        )],
        true,
    )
}

/// (b) `set` demands a map receiver.
#[test]
fn live_alias_write_rejects_a_scalar_receiver() -> eyre::Result<()> {
    assert_helm_verdicts(
        &live_alias_schema()?,
        &[("b2: set on a string aborts", json!({ "payload": "s" }))],
        false,
    )
}

const PAYLOAD_DEFAULTS: &str = indoc! {r#"
    payload:
      x: "a"
"#};

fn snapshot_before_write_schema() -> eyre::Result<Value> {
    template_law_schema(
        PAYLOAD_DEFAULTS,
        &[
            indoc! {r#"
                {{- $alias := .Values.payload }}
                {{- $saved := toYaml $alias }}
                {{- $_ := set $alias "x" "ok" }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  live: {{ upper .Values.payload.x | quote }}
                  snap: {{ upper (fromYaml $saved).x | quote }}
            "},
        ],
        "",
    )
}

/// (b) A serialization taken before a write keeps the input it read.
#[test]
fn serialization_before_a_write_accepts_a_string_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &snapshot_before_write_schema()?,
        &[("b3: string x", json!({ "payload": { "x": "b" } }))],
        true,
    )
}

/// (b) The snapshot, decoded later, still demands a string of the input.
#[test]
#[ignore = "target: ADR 0073 step 4 (ADR 0072 §1: a deferred serialization keeps its heap version)"]
fn serialization_before_a_write_rejects_the_snapshotted_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &snapshot_before_write_schema()?,
        &[(
            "b4: integer x reaches the snapshot",
            json!({ "payload": { "x": 7 } }),
        )],
        false,
    )
}

/// (b) A serialization taken after the write sees the written string.
#[test]
fn serialization_after_a_write_accepts_an_overwritten_input() -> eyre::Result<()> {
    let schema = template_law_schema(
        PAYLOAD_DEFAULTS,
        &[
            indoc! {r#"
                {{- $alias := .Values.payload }}
                {{- $_ := set $alias "x" "ok" }}
                {{- $saved := toYaml $alias }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  snap: {{ upper (fromYaml $saved).x | quote }}
            "},
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "b5: integer x is overwritten before serialization",
            json!({ "payload": { "x": 7 } }),
        )],
        true,
    )
}

fn merge_destination_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            dst: {}
            src:
              x: "a"
        "#},
        &[
            "{{- $_ := merge .Values.dst .Values.src }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper .Values.dst.x | quote }}
            "},
        ],
        "",
    )
}

/// (b) `merge` mutates its destination; the destination's own members win.
#[test]
fn merge_destination_accepts_documents_whose_merged_member_is_a_string() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_destination_schema()?,
        &[
            (
                "b6: source supplies a string",
                json!({ "dst": {}, "src": { "x": "a" } }),
            ),
            (
                "b7: destination string wins over an integer source",
                json!({ "dst": { "x": "b" }, "src": { "x": 7 } }),
            ),
        ],
        true,
    )
}

/// (b) Which operand the merged member comes from decides the demand.
#[test]
#[ignore = "target: ADR 0073 step 4 (ADR 0072 §1/§4: merge effect with a relational pullback)"]
fn merge_destination_rejects_an_integer_merged_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_destination_schema()?,
        &[
            (
                "b8: source supplies an integer",
                json!({ "dst": {}, "src": { "x": 7 } }),
            ),
            (
                "b9: destination integer wins",
                json!({ "dst": { "x": 7 }, "src": { "x": "a" } }),
            ),
        ],
        false,
    )
}

fn nested_copy_schema(copy: &str) -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            a:
              inner:
                x: "a"
        "#},
        &[
            "{{- $m := ",
            copy,
            " }}\n",
            "{{- $_ := set $m.inner \"x\" \"ok\" }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper .Values.a.inner.x | quote }}
            "},
        ],
        "",
    )
}

/// (b) `merge` into a fresh map keeps nested maps aliased to the input.
#[test]
fn merge_keeps_nested_aliases_live() -> eyre::Result<()> {
    assert_helm_verdicts(
        &nested_copy_schema("merge (dict) .Values.a")?,
        &[(
            "b10: the write through the merged copy reaches the input",
            json!({ "a": { "inner": { "x": 7 } } }),
        )],
        true,
    )
}

/// (b) `deepCopy` separates the copy; the input's own string is accepted.
#[test]
fn deep_copy_accepts_a_string_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &nested_copy_schema("deepCopy .Values.a")?,
        &[("b11: string x", json!({ "a": { "inner": { "x": "b" } } }))],
        true,
    )
}

/// (b) The write to the deep copy does not repair the input.
#[test]
#[ignore = "target: ADR 0073 step 4 (ADR 0072 §1: deepCopy separates nested maps)"]
fn deep_copy_rejects_an_integer_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &nested_copy_schema("deepCopy .Values.a")?,
        &[(
            "b12: the copy's write does not reach the input",
            json!({ "a": { "inner": { "x": 7 } } }),
        )],
        false,
    )
}

fn break_limited_schema() -> eyre::Result<Value> {
    template_law_schema(
        "items: []\n",
        &[
            "{{- range .Values.items }}{{ $_ := required \"name\" .name }}{{ break }}{{ end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (c) A consumer before `break` runs for the first member only.
#[test]
fn break_limited_consumption_accepts_later_members_without_the_field() -> eyre::Result<()> {
    assert_helm_verdicts(
        &break_limited_schema()?,
        &[
            (
                "c1: first named, second empty",
                json!({ "items": [{ "name": "ok" }, {}] }),
            ),
            ("c3: no members", json!({ "items": [] })),
        ],
        true,
    )
}

/// (c) The first member is still consumed.
#[test]
#[ignore = "target: ADR 0073 step 6 (ADR 0072 §6: exact bounded execution with break states)"]
fn break_limited_consumption_rejects_an_unnamed_first_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &break_limited_schema()?,
        &[("c2: first member empty", json!({ "items": [{}] }))],
        false,
    )
}

fn range_else_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            items: []
            fallback: "f"
        "#},
        &[
            "{{- range .Values.items }}{{ $_ := upper . }}{{ else }}{{ $_ := upper $.Values.fallback }}{{ end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (d) The `else` arm of a range runs only on zero iterations.
#[test]
fn zero_iteration_arm_accepts_any_fallback_when_members_exist() -> eyre::Result<()> {
    assert_helm_verdicts(
        &range_else_schema()?,
        &[(
            "d1: members exist, integer fallback",
            json!({ "items": ["a"], "fallback": 7 }),
        )],
        true,
    )
}

/// (d) Zero iterations consume the fallback; members are consumed as strings.
#[test]
#[ignore = "target: ADR 0073 step 6 (ADR 0072 §6: zero-iteration and member transfer states)"]
fn zero_iteration_arm_rejects_its_input_and_typed_members() -> eyre::Result<()> {
    assert_helm_verdicts(
        &range_else_schema()?,
        &[
            (
                "d2: no members, integer fallback",
                json!({ "items": [], "fallback": 7 }),
            ),
            (
                "d3: integer member",
                json!({ "items": [7], "fallback": "f" }),
            ),
        ],
        false,
    )
}

/// (d) Map ranges iterate in sorted key order, so `break` after the first
/// consumes only the smallest key's value.
#[test]
fn map_range_break_consumes_only_the_first_sorted_key() -> eyre::Result<()> {
    let schema = template_law_schema(
        "m: {}\n",
        &[
            "{{- range $k, $v := .Values.m }}{{ $_ := upper $v }}{{ break }}{{ end }}\n",
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "d4: first key string, later key integer",
            json!({ "m": { "a": "x", "b": 7 } }),
        )],
        true,
    )
}

fn continue_schema() -> eyre::Result<Value> {
    template_law_schema(
        "items: []\n",
        &[
            "{{- range $i, $v := .Values.items }}{{ if eq $i 0 }}{{ continue }}{{ end }}{{ $_ := upper $v }}{{ end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (d) `continue` skips the consumer for the member it fires on.
#[test]
fn continue_skips_the_consumer_for_the_first_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &continue_schema()?,
        &[(
            "d6: integer first member, string rest",
            json!({ "items": [7, "a"] }),
        )],
        true,
    )
}

/// (d) Members after the skipped one are consumed.
#[test]
#[ignore = "target: ADR 0073 step 6 (ADR 0072 §6: index binder and continue states)"]
fn continue_rejects_a_later_integer_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &continue_schema()?,
        &[("d7: integer second member", json!({ "items": ["a", 7] }))],
        false,
    )
}

fn child_global_schema() -> eyre::Result<Value> {
    let parent_template = [
        CONFIG_MAP,
        indoc! {"
            data:
              k: v
        "},
    ]
    .concat();
    law_schema(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: law
                version: 0.1.0
                dependencies:
                  - name: child
                    version: 0.1.0
            "},
        ),
        (
            "values.yaml",
            indoc! {r#"
                global:
                  shared: "p"
            "#},
        ),
        ("templates/cm.yaml", &parent_template),
        (
            "charts/child/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: child
                version: 0.1.0
            "},
        ),
        (
            "charts/child/values.yaml",
            indoc! {r#"
                global:
                  childOnly: "c"
            "#},
        ),
        (
            "charts/child/templates/cm.yaml",
            indoc! {"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: child
                data:
                  shared: {{ upper .Values.global.shared | quote }}
                  childOnly: {{ upper .Values.global.childOnly | quote }}
            "},
        ),
    ])
}

/// (e) Child globals are the parent's globals merged over the child's own:
/// a child-only member survives when the parent does not set it.
#[test]
fn child_global_fallback_member_survives_parent_globals() -> eyre::Result<()> {
    assert_helm_verdicts(
        &child_global_schema()?,
        &[(
            "e1: defaults; childOnly comes from the child",
            json!({
                "global": { "shared": "p" },
                "child": { "global": { "shared": "p", "childOnly": "c" } }
            }),
        )],
        true,
    )
}

/// (e) A parent global overrides the child's member; a child-local override
/// reaches only the child.
#[test]
fn child_global_rejects_integer_members_from_either_owner() -> eyre::Result<()> {
    assert_helm_verdicts(
        &child_global_schema()?,
        &[
            (
                "e2: root global childOnly integer",
                json!({
                    "global": { "shared": "p", "childOnly": 7 },
                    "child": { "global": { "shared": "p", "childOnly": 7 } }
                }),
            ),
            (
                "e3: child-local childOnly integer",
                json!({
                    "global": { "shared": "p" },
                    "child": { "global": { "shared": "p", "childOnly": 7 } }
                }),
            ),
            (
                "e4: root global shared integer",
                json!({
                    "global": { "shared": 7 },
                    "child": { "global": { "shared": 7, "childOnly": "c" } }
                }),
            ),
        ],
        false,
    )
}

const PERMISSIVE_LOCAL: &str = r#"{{- define "local" -}}{{ toString .Values.x }}{{- end -}}"#;

fn tpl_define_schema(program: &str) -> eyre::Result<Value> {
    template_law_schema(
        "x: \"a\"\n",
        &[
            CONFIG_MAP,
            "data:\n",
            "  a: {{ tpl `",
            program,
            "` $ | quote }}\n",
            "  b: {{ include \"local\" . | quote }}\n",
        ],
        PERMISSIVE_LOCAL,
    )
}

const STRICT_LOCAL: &str = r#"{{- define "local" -}}{{ upper .Values.x }}{{- end -}}"#;

/// (f) A define introduced inside a `tpl` string is resolved by `include`
/// within that invocation's cloned namespace.
#[test]
fn tpl_define_accepts_a_string_input() -> eyre::Result<()> {
    let program = [STRICT_LOCAL, r#"{{ include "local" . }}"#].concat();
    assert_helm_verdicts(
        &tpl_define_schema(&program)?,
        &[("f1: string x", json!({ "x": "b" }))],
        true,
    )
}

/// (f) The `include` inside the program runs the program's own define.
#[test]
fn tpl_define_is_resolved_by_include_in_its_invocation() -> eyre::Result<()> {
    let program = [STRICT_LOCAL, r#"{{ include "local" . }}"#].concat();
    assert_helm_verdicts(
        &tpl_define_schema(&program)?,
        &[("f2: the tpl-local define consumes x", json!({ "x": 7 }))],
        false,
    )
}

/// (f) The clone is discarded: the chart's define stays in force afterwards.
#[test]
fn tpl_define_does_not_leak_out_of_its_invocation() -> eyre::Result<()> {
    assert_helm_verdicts(
        &tpl_define_schema(STRICT_LOCAL)?,
        &[("f3: the outer include stays permissive", json!({ "x": 7 }))],
        true,
    )
}

fn from_yaml_schema() -> eyre::Result<Value> {
    template_law_schema(
        "raw: \"a: 1\"\n",
        &[
            "{{- $decoded := fromYaml .Values.raw }}\n",
            CONFIG_MAP,
            "data:\n",
            "  decoded: {{ $decoded | toJson | quote }}\n",
        ],
        "",
    )
}

/// (g) `fromYaml` on malformed text returns a map holding `Error`; it does not
/// abort like manifest decoding.
#[test]
fn from_yaml_error_map_does_not_abort() -> eyre::Result<()> {
    assert_helm_verdicts(
        &from_yaml_schema()?,
        &[
            ("g1: malformed YAML", json!({ "raw": "a: [" })),
            ("g2: a sequence, not a map", json!({ "raw": "- a" })),
        ],
        true,
    )
}

/// (g) `fromYaml` takes a string.
#[test]
fn from_yaml_rejects_a_non_string_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &from_yaml_schema()?,
        &[("g3: integer raw", json!({ "raw": 7 }))],
        false,
    )
}

fn missing_versus_null_schema() -> eyre::Result<Value> {
    template_law_schema(
        "x: \"d\"\n",
        &[
            indoc! {r#"
                {{- if hasKey .Values "opt" }}{{ $_ := required "opt" .Values.opt }}{{ end }}
                {{- $_ := upper .Values.x }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )
}

/// (h) An undeclared key is either missing or present; only a present key is
/// consumed.
#[test]
fn missing_key_is_not_consumed() -> eyre::Result<()> {
    assert_helm_verdicts(
        &missing_versus_null_schema()?,
        &[
            ("h1: defaults", json!({ "x": "d" })),
            ("h4: present string", json!({ "x": "d", "opt": "v" })),
        ],
        true,
    )
}

/// (h) An undeclared null survives coalescing and is present.
#[test]
fn undeclared_null_is_present_and_consumed() -> eyre::Result<()> {
    assert_helm_verdicts(
        &missing_versus_null_schema()?,
        &[
            (
                "h2: undeclared null is present",
                json!({ "x": "d", "opt": null }),
            ),
            ("h5: present empty string", json!({ "x": "d", "opt": "" })),
        ],
        false,
    )
}

/// (h) A declared key set to null is deleted, so its consumer sees a missing
/// value and aborts.
#[test]
fn deleted_declared_default_is_missing() -> eyre::Result<()> {
    assert_helm_verdicts(
        &missing_versus_null_schema()?,
        &[("h3: declared null is deleted", json!({}))],
        false,
    )
}

/// (i) Two calls of a helper that reads randomness are distinct executions.
#[test]
fn helper_calls_with_randomness_are_not_merged() -> eyre::Result<()> {
    let schema = template_law_schema(
        "z: \"s\"\n",
        &[
            "{{- if eq (include \"r\" .) (include \"r\" .) }}{{ $_ := upper .Values.z }}{{ end }}\n",
            CONFIG_MAP,
        ],
        r#"{{- define "r" -}}{{ randAlphaNum 16 }}{{- end -}}"#,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("i1: the equal-results branch never runs", json!({ "z": 7 }))],
        true,
    )
}

fn namespace_memo_schema() -> eyre::Result<Value> {
    template_law_schema(
        "w: \"s\"\n",
        &[
            CONFIG_MAP,
            "data:\n",
            "  a: {{ include \"h\" . | quote }}\n",
            "  b: {{ tpl `{{- define \"h\" -}}{{ upper .Values.w }}{{- end -}}{{ include \"h\" . }}` $ | quote }}\n",
        ],
        r#"{{- define "h" -}}{{ toString .Values.w }}{{- end -}}"#,
    )
}

/// (i) The same helper name and argument resolve differently inside a `tpl`
/// clone that redefines it.
#[test]
fn helper_calls_in_different_namespaces_accept_a_string_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &namespace_memo_schema()?,
        &[("i2: string w", json!({ "w": "t" }))],
        true,
    )
}

/// (i) A memo keyed without the namespace would reuse the permissive outer
/// result for the strict inner call.
#[test]
fn helper_calls_in_different_namespaces_are_not_merged() -> eyre::Result<()> {
    assert_helm_verdicts(
        &namespace_memo_schema()?,
        &[("i3: the tpl-local helper consumes w", json!({ "w": 7 }))],
        false,
    )
}
