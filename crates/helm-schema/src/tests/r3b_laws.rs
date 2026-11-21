//! Discriminating laws for the R3b target architecture (ADR 0072, ADR 0073 step 1).
//!
//! Each law is a chart and a set of coalesced values documents with the
//! verdict real Helm 4.2.3 gives them (`helm template --kube-version 1.29.0`,
//! confirmed by the pinned helmsweep driver). The receipts (chart, user values
//! file, Helm output) are kept per law under the step-1 lane's
//! `transfer/receipts/<law>/`, and for the laws marked step 4a under the step-4
//! lane's `transfer/receipts/<law>/`. Documents are the values Helm validates: the
//! user file coalesced over the chart defaults, with null deletion of declared
//! keys and globals copied into each dependency instance.
//!
//! A law split into `accepts` and `rejects` separates the false-rejection
//! guard from the false-acceptance closure. A law the current line fails is
//! ignored with the ADR 0073 step and ADR 0072 invariant that must turn it
//! green.

use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
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
        chart_source: chart_dir.into(),
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

fn merge_nested_alias_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            dst:
              inner: {}
            src:
              inner:
                x: "a"
        "#},
        &[
            indoc! {r"
                {{- $alias := .Values.dst.inner }}
                {{- $_ := merge .Values.dst .Values.src }}
            "},
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper $alias.x | quote }}
            "},
        ],
        "",
    )
}

/// (b, step 4a) `merge` recurses into the destination's own nested map, so an alias taken
/// before the merge sees the merged member (receipts `m_nested_alias`).
#[test]
fn merge_writes_reach_an_earlier_nested_alias() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_nested_alias_schema()?,
        &[(
            "m1: the source string reaches the alias",
            json!({ "dst": { "inner": {} }, "src": { "inner": { "x": "b" } } }),
        )],
        true,
    )
}

/// (b, step 4a) The alias reads the merged integer, or the destination's own integer.
#[test]
fn merge_writes_reject_an_integer_through_an_earlier_nested_alias() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_nested_alias_schema()?,
        &[
            (
                "m2: the source integer reaches the alias",
                json!({ "dst": { "inner": {} }, "src": { "inner": { "x": 7 } } }),
            ),
            (
                "m3: the destination integer is kept",
                json!({ "dst": { "inner": { "x": 7 } }, "src": { "inner": { "x": "a" } } }),
            ),
        ],
        false,
    )
}

fn merge_source_alias_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            dst: {}
            src:
              inner:
                x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $_ := merge .Values.dst .Values.src }}
                {{- $_ := set .Values.src.inner "x" "ok" }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper .Values.dst.inner.x | quote }}
            "},
        ],
        "",
    )
}

/// (b, step 4a) A source map the destination lacks is assigned by reference: a later write
/// through the source reaches the destination (receipts `m_source_alias`).
#[test]
fn merge_assigns_a_missing_member_map_by_reference() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_source_alias_schema()?,
        &[(
            "m4: the source integer is overwritten through the alias",
            json!({ "dst": {}, "src": { "inner": { "x": 7 } } }),
        )],
        true,
    )
}

/// (b, step 4a) A destination map of its own is merged into, not replaced by the source.
#[test]
fn merge_keeps_a_destination_map_of_its_own() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_source_alias_schema()?,
        &[(
            "m5: the destination map keeps its integer",
            json!({ "dst": { "inner": { "x": 7 } }, "src": { "inner": { "x": "a" } } }),
        )],
        false,
    )
}

/// (b, step 4a) `merge` replaces an empty destination member and skips a deleted source
/// member (receipts `m_empty_and_nil`).
#[test]
fn merge_replaces_only_empty_destination_members() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_destination_schema()?,
        &[
            (
                "m6: an integer zero is empty and takes the source string",
                json!({ "dst": { "x": 0 }, "src": { "x": "a" } }),
            ),
            (
                "m9: a deleted source member keeps the destination string",
                json!({ "dst": { "x": "b" }, "src": {} }),
            ),
        ],
        true,
    )
}

/// (b, step 4a) The member `merge` leaves in the destination is the one consumed.
#[test]
fn merge_rejects_the_integer_it_leaves_in_the_destination() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_destination_schema()?,
        &[
            (
                "m7: an empty string takes the source integer",
                json!({ "dst": { "x": "" }, "src": { "x": 7 } }),
            ),
            (
                "m8: a deleted source member keeps the destination integer",
                json!({ "dst": { "x": 7 }, "src": {} }),
            ),
        ],
        false,
    )
}

fn merge_overwrite_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            dst: {}
            src:
              x: "a"
        "#},
        &[
            "{{- $_ := mergeOverwrite .Values.dst .Values.src }}\n",
            CONFIG_MAP,
            indoc! {r#"
                data:
                  x: {{ upper (default "d" .Values.dst.x) | quote }}
            "#},
        ],
        "",
    )
}

/// (b, step 4a) `mergeOverwrite` writes every present source member, empty ones too
/// (receipts `m_overwrite`).
#[test]
fn merge_overwrite_accepts_documents_whose_source_replaces_an_integer() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_overwrite_schema()?,
        &[
            (
                "o2: the source string replaces the integer",
                json!({ "dst": { "x": 7 }, "src": { "x": "a" } }),
            ),
            (
                "o3: the empty source string replaces the integer",
                json!({ "dst": { "x": 7 }, "src": { "x": "" } }),
            ),
        ],
        true,
    )
}

/// (b, step 4a) An overwriting source integer, or a deleted source member, leaves an integer.
#[test]
fn merge_overwrite_rejects_the_integer_it_leaves() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merge_overwrite_schema()?,
        &[
            (
                "o1: the source integer overwrites the string",
                json!({ "dst": { "x": "b" }, "src": { "x": 7 } }),
            ),
            (
                "o4: a deleted source member keeps the integer",
                json!({ "dst": { "x": 7 }, "src": {} }),
            ),
        ],
        false,
    )
}

fn copy_occurrences_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            a:
              inner:
                x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $one := deepCopy .Values.a }}
                {{- $two := deepCopy .Values.a }}
                {{- $_ := set $one.inner "x" "ok" }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper $two.inner.x | quote }}
            "},
        ],
        "",
    )
}

/// (b, step 4a) Each `deepCopy` is separate: one copy's write does not reach another
/// (receipts `c_copy_occurrences`).
#[test]
fn deep_copies_are_separate_per_occurrence() -> eyre::Result<()> {
    assert_helm_verdicts(
        &copy_occurrences_schema()?,
        &[("c1: string x", json!({ "a": { "inner": { "x": "b" } } }))],
        true,
    )
}

/// (b, step 4a) The second copy still reads the input's integer.
#[test]
fn deep_copies_reject_an_integer_another_copy_overwrote() -> eyre::Result<()> {
    assert_helm_verdicts(
        &copy_occurrences_schema()?,
        &[(
            "c2: the other copy's write does not repair the input",
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

const AFTER_STRING: &str = indoc! {"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: law
    data:
      after: {{ upper .Values.after | quote }}
"};

/// Step 3 laws: abort survival never guards later facts, analysis validity does (ADR 0072
/// §2–3). Receipts: `transfer/receipts/<chart>/` of the step-3 lane (`helm template
/// --kube-version 1.29.0`, Helm v4.2.3).
fn abort_only_helper_schema() -> eyre::Result<Value> {
    template_law_schema(
        r#"after: "x""#,
        &[r#"{{- include "law.check" . }}"#, "\n", AFTER_STRING],
        indoc! {r#"
            {{- define "law.check" -}}
            {{- if .Capabilities.APIVersions.Has "law.example/v1/Gate" -}}
            {{- fail "law.example/v1 Gate is not supported" -}}
            {{- end -}}
            {{- end -}}
        "#},
    )
}

/// A helper branch whose every exit aborts leaves its other outcome as a survived check: a
/// later typed use applies whatever the opaque capability test answers (receipt helper-branch).
#[test]
fn helper_abort_only_branch_rejects_a_later_typed_input() -> eyre::Result<()> {
    let schema = abort_only_helper_schema()?;
    assert_helm_verdicts(
        &schema,
        &[
            ("r1: integer after", json!({ "after": 7 })),
            ("r2: boolean after", json!({ "after": true })),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("a1: string after", json!({ "after": "y" }))],
        true,
    )
}

/// An abort-only arm of an `if` in the template itself (receipt abort-only-arm).
#[test]
fn abort_only_arm_rejects_a_later_typed_input() -> eyre::Result<()> {
    let schema = template_law_schema(
        r#"after: "x""#,
        &[
            r#"{{- if .Capabilities.APIVersions.Has "law.example/v1/Gate" }}{{ fail "unsupported" }}{{ end }}"#,
            "\n",
            AFTER_STRING,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("r1: integer after", json!({ "after": 7 })),
            ("r2: boolean after", json!({ "after": true })),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("a1: string after", json!({ "after": "y" }))],
        true,
    )
}

/// A conditional `tpl` of a program without template actions writes nothing, so the later
/// typed use applies on both arms (receipt conditional-tpl; RECV2 R-a's `{program: "plain",
/// after: 7}`).
#[test]
fn conditional_action_free_program_keeps_the_later_typed_use() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            enabled: true
            program: "plain"
            after: "x"
        "#},
        &[
            "{{- if .Values.enabled }}# {{ tpl .Values.program $ }}{{ end }}\n",
            AFTER_STRING,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            (
                "r1: enabled, plain program, integer after",
                json!({ "enabled": true, "program": "plain", "after": 7 }),
            ),
            (
                "r2: disabled, integer after",
                json!({ "enabled": false, "program": "", "after": 7 }),
            ),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            (
                "a1: the program sets after to a string",
                json!({
                    "enabled": true,
                    "program": r#"{{ $_ := set .Values "after" "ok" }}"#,
                    "after": 7
                }),
            ),
            (
                "a2: plain program, string after",
                json!({ "enabled": true, "program": "plain", "after": "y" }),
            ),
        ],
        true,
    )
}

const EXCLUSIVE_PAIRS: usize = 20;

fn exclusive_pairs(overrides: &[(&str, bool)], after: Value) -> Value {
    let mut document = serde_json::Map::new();
    for index in 1..=EXCLUSIVE_PAIRS {
        for side in ["a", "b"] {
            let key = format!("{side}{index}");
            let value = overrides
                .iter()
                .find(|(name, _)| *name == key)
                .is_some_and(|(_, value)| *value);
            document.insert(key, Value::Bool(value));
        }
    }
    document.insert("after".to_owned(), after);
    Value::Object(document)
}

/// Twenty earlier files each abort on one exclusive pair. Every later file runs only on
/// documents that survived all of them, so its typed use carries none of those checks
/// (receipt cross-file).
#[test]
fn cross_file_survival_does_not_guard_a_later_typed_use() -> eyre::Result<()> {
    let mut values = String::from("after: \"x\"\n");
    let mut checks = Vec::new();
    for index in 1..=EXCLUSIVE_PAIRS {
        writeln!(values, "a{index}: false\nb{index}: false")?;
        checks.push((
            format!("templates/check-{index:02}.yaml"),
            format!(
                "{{{{- if and .Values.a{index} .Values.b{index} }}}}{{{{ fail \"a{index} and b{index} are exclusive\" }}}}{{{{ end }}}}\n"
            ),
        ));
    }
    let mut files = vec![
        ("Chart.yaml", CHART),
        ("values.yaml", values.as_str()),
        ("templates/z.yaml", AFTER_STRING),
    ];
    files.extend(
        checks
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str())),
    );
    let schema = law_schema(&files)?;
    assert_helm_verdicts(
        &schema,
        &[
            ("r1: integer after", exclusive_pairs(&[], json!(7))),
            (
                "r2: boolean after",
                exclusive_pairs(&[("a3", true)], json!(true)),
            ),
            (
                "r3: an exclusive pair",
                exclusive_pairs(&[("a3", true), ("b3", true)], json!("y")),
            ),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("a1: string after", exclusive_pairs(&[], json!("y"))),
            (
                "a2: one side of a pair",
                exclusive_pairs(&[("b7", true)], json!("y")),
            ),
        ],
        true,
    )
}

/// Twenty guarded member reads of one host: their union's diagram is exponential in the pair
/// order, and exhausting it never removes a rejection (ADR 0072 §10; receipt budget-union).
#[test]
fn member_reads_under_exclusive_pairs_keep_every_rejection() -> eyre::Result<()> {
    let mut values = String::from(indoc! {r#"
        x:
          port: "v"
    "#});
    let mut template = String::new();
    for index in 1..=EXCLUSIVE_PAIRS {
        writeln!(values, "a{index}: false\nb{index}: false")?;
        writeln!(
            template,
            "{{{{- if and .Values.a{index} .Values.b{index} }}}}{{{{ $_ := .Values.x.port }}}}{{{{ end }}}}"
        )?;
    }
    template.push_str(CONFIG_MAP);
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", values.as_str()),
        ("templates/cm.yaml", template.as_str()),
    ])?;
    let pairs = |overrides: &[(&str, bool)], x: Value| {
        let mut document = exclusive_pairs(overrides, Value::Null);
        if let Value::Object(map) = &mut document {
            map.remove("after");
            map.insert("x".to_owned(), x);
        }
        document
    };
    assert_helm_verdicts(
        &schema,
        &[
            (
                "r1: first pair, string host",
                pairs(&[("a1", true), ("b1", true)], json!("s")),
            ),
            (
                "r2: last pair, integer host",
                pairs(&[("a20", true), ("b20", true)], json!(7)),
            ),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("a1: no pair, string host", pairs(&[], json!("s"))),
            (
                "a2: first pair, map host",
                pairs(&[("a1", true), ("b1", true)], json!({ "port": "v" })),
            ),
        ],
        true,
    )
}

fn merged_guard_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            base:
              gate: {}
              live: false
              x: "a"
            override:
              gate: {}
        "#},
        &[
            "{{- $gate := mergeOverwrite .Values.base.gate .Values.override.gate }}\n",
            CONFIG_MAP,
            indoc! {r"
                data:
                {{- if and (not $gate.enabled) .Values.base.live }}
                  x: {{ upper .Values.base.x | quote }}
                {{- end }}
            "},
        ],
        "",
    )
}

/// (b, step 4a) A guard over a merged member keeps the payload it guards
/// (receipts `g_merged_guard`).
#[test]
fn merged_guard_rejects_the_live_integer_payload() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merged_guard_schema()?,
        &[(
            "g1: a disabled merged gate renders the integer payload",
            json!({
                "base": { "gate": { "enabled": false }, "live": true, "x": 7 },
                "override": { "gate": { "enabled": false } },
            }),
        )],
        false,
    )
}

/// (b, step 4a) The merged gate decides the payload in both directions.
#[test]
fn merged_guard_accepts_dormant_or_valid_payloads() -> eyre::Result<()> {
    assert_helm_verdicts(
        &merged_guard_schema()?,
        &[
            (
                "g2: a string payload renders",
                json!({
                    "base": { "gate": { "enabled": false }, "live": true, "x": "b" },
                    "override": { "gate": { "enabled": false } },
                }),
            ),
            (
                "g3: the override enables the gate, so the payload is dormant",
                json!({
                    "base": { "gate": { "enabled": false }, "live": true, "x": 7 },
                    "override": { "gate": { "enabled": true } },
                }),
            ),
        ],
        true,
    )
}

/// (b, step 4a, Sol 1) A merge recurses into each alternative of a conditional nested map
/// (receipts `s1_select_nested`).
#[test]
fn merge_recurses_into_a_conditionally_set_nested_map() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            choose: false
            x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $d := dict }}
                {{- if .Values.choose }}{{- $_ := set $d "inner" (dict "x" "ok") }}{{- end }}
                {{- $_ := mergeOverwrite $d (dict "inner" (dict "x" .Values.x)) }}
            "#},
            CONFIG_MAP,
            "data:\n  x: {{ upper $d.inner.x | quote }}\n",
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s1a: the chosen nested map takes the integer",
            json!({ "choose": true, "x": 7 }),
        )],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s1b: the chosen nested map takes the string",
            json!({ "choose": true, "x": "b" }),
        )],
        true,
    )
}

/// (b, step 4a, Sol 2) A map member read from a copy is the copy's own map, which a later write
/// to the source does not reach (receipts `s2_copy_identity`).
#[test]
fn merged_copy_members_do_not_follow_source_writes() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            a:
              inner:
                x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $c := deepCopy .Values.a }}
                {{- $d := merge (dict) $c }}
                {{- $_ := set .Values.a.inner "x" "ok" }}
            "#},
            CONFIG_MAP,
            "data:\n  x: {{ upper $d.inner.x | quote }}\n",
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s2a: the copy keeps the integer",
            json!({ "a": { "inner": { "x": 7 } } }),
        )],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s2b: the copy keeps the string",
            json!({ "a": { "inner": { "x": "b" } } }),
        )],
        true,
    )
}

/// (b, step 4a, Sol 3) An `unset` may empty a merged map, so its truthiness is not the
/// sources' (receipts `s3_tombstone_truth`).
#[test]
fn unset_merged_map_is_not_assumed_non_empty() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            src:
              x: true
            bad: "s"
        "#},
        &[
            indoc! {r#"
                {{- $m := mergeOverwrite (dict) .Values.src }}
                {{- $_ := unset $m "x" }}
            "#},
            CONFIG_MAP,
            indoc! {r"
                data:
                {{- if $m }}
                  x: {{ upper .Values.bad | quote }}
                {{- else }}
                  x: empty
                {{- end }}
            "},
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s3a: removing the only key renders the else branch",
            json!({ "src": { "x": true }, "bad": 7 }),
        )],
        true,
    )
}

/// (b, step 4a, Sol 4) Past the layer bound only the destination widens; an unrelated map keeps
/// its obligation (receipts `s4_layer_bound`).
#[test]
fn layer_bound_widens_only_the_destination() -> eyre::Result<()> {
    let merges = "{{- $_ := mergeOverwrite $m $.Values.src }}\n".repeat(72);
    let schema = template_law_schema(
        indoc! {r#"
            src: {}
            bad: "s"
        "#},
        &[
            "{{- $u := dict \"x\" .Values.bad }}\n{{- $m := dict }}\n",
            &merges,
            CONFIG_MAP,
            "data:\n  x: {{ upper $u.x | quote }}\n",
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s4a: the unrelated integer still aborts",
            json!({ "src": {}, "bad": 7 }),
        )],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s4b: a string renders",
            json!({ "src": { "k": "v" }, "bad": "b" }),
        )],
        true,
    )
}

/// (b, step 4a, Sol r2 1) A copy of a copied map keeps each alternative member as its own copy
/// (receipts `s5_copy_of_copy`).
#[test]
fn copies_of_copies_keep_their_members() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            a:
              inner:
                x: "a"
        "#},
        &[
            indoc! {r#"
                {{- $one := deepCopy .Values.a }}
                {{- $copies := deepCopy (list $one) }}
                {{- $_ := set $one.inner "x" "ok" }}
                {{- $d := merge (dict) (first $copies) }}
            "#},
            CONFIG_MAP,
            "data:\n  x: {{ upper $d.inner.x | quote }}\n",
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s5a: the second copy keeps the integer",
            json!({ "a": { "inner": { "x": 7 } } }),
        )],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "s5b: the second copy keeps the string",
            json!({ "a": { "inner": { "x": "b" } } }),
        )],
        true,
    )
}

/// (b, step 4a, Sol r2 2) A destination widened past the layer bound still reaches the maps it
/// shares with its sources, so a later write through it is not lost (receipts
/// `s6_bound_alias`).
#[test]
fn bounded_destination_keeps_its_source_aliases() -> eyre::Result<()> {
    let merges = "{{- $_ := mergeOverwrite $d.nested .Values.src }}\n".repeat(72);
    let schema = template_law_schema(
        indoc! {r#"
            src:
              inner: {}
            bad: "s"
        "#},
        &[
            indoc! {r#"
                {{- $alias := .Values.src.inner }}
                {{- $_ := set $alias "x" .Values.bad }}
                {{- $d := dict "nested" (dict) }}
            "#},
            &merges,
            "{{- $_ := mergeOverwrite $d.nested (dict \"inner\" (dict \"x\" \"ok\")) }}\n",
            CONFIG_MAP,
            "data:\n  x: {{ upper $alias.x | quote }}\n",
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            (
                "s6a: the shared map is repaired before the integer is read",
                json!({ "src": { "inner": {} }, "bad": 7 }),
            ),
            (
                "s6b: a string renders",
                json!({ "src": { "inner": {} }, "bad": "b" }),
            ),
        ],
        true,
    )
}

/// Eight cross-path nil-receiver aborts whose union is exponential under the path order, then a
/// guarded `fail` that only the executed fold carries: the inexact `has` decision before it keeps
/// its capture approximate. A demand's coverage only marks it redundant, so when the union of the
/// earlier clauses exhausts the group's cone the abort is still folded (ADR 0072 §10; receipts
/// `transfer/frfix-receipts/law-coverage`, Helm 4.2.3 through helmsweep).
fn exhausted_coverage_schema() -> eyre::Result<Value> {
    let mut hosts = String::new();
    for index in 1..=8 {
        writeln!(
            hosts,
            "{{{{- if .Values.a{index:02} }}}}{{{{ $_ := .Values.h{index:02}.field }}}}{{{{ end }}}}"
        )?;
    }
    law_schema(&[
        ("Chart.yaml", CHART),
        (
            "values.yaml",
            indoc! {"
                flag: true
                soft: false
                namespaces: []
                hook:
                  enabled: false
            "},
        ),
        ("templates/hosts.yaml", &hosts),
        (
            "templates/validate.yaml",
            indoc! {r#"
                {{- if .Values.soft }}
                {{- if has .Release.Namespace .Values.namespaces }}
                {{- fail "release namespace is managed" }}
                {{- end }}
                {{- end }}
                {{- if not .Values.flag }}
                {{- if .Values.hook.enabled }}
                {{- fail "hook needs flag" }}
                {{- end }}
                {{- end }}
            "#},
        ),
    ])
}

#[test]
fn exhausted_failure_coverage_accepts_the_documents_helm_renders() -> eyre::Result<()> {
    assert_helm_verdicts(
        &exhausted_coverage_schema()?,
        &[
            (
                "defaults",
                json!({ "flag": true, "soft": false, "namespaces": [], "hook": { "enabled": false } }),
            ),
            (
                "hook with flag",
                json!({ "flag": true, "soft": false, "namespaces": [], "hook": { "enabled": true } }),
            ),
            (
                "no hook without flag",
                json!({ "flag": false, "soft": false, "namespaces": [], "hook": { "enabled": false } }),
            ),
        ],
        true,
    )
}

#[test]
fn exhausted_failure_coverage_still_rejects_the_abort() -> eyre::Result<()> {
    assert_helm_verdicts(
        &exhausted_coverage_schema()?,
        &[(
            "hook without flag",
            json!({ "flag": false, "soft": false, "namespaces": [], "hook": { "enabled": true } }),
        )],
        false,
    )
}

#[test]
#[ignore = "target: ADR 0073 step 4 effects (call reached => write-free); see designs/reviews/activation2-kyverno-direction-astra-2026-11-20.md"]
fn kyverno_readiness_requires_a_string_image_tag() -> eyre::Result<()> {
    let testdata = test_util::workspace_testdata();
    let chart_dir = testdata.join("charts/kyverno");
    let bundle = testdata.join("provider-bundle");
    let crds = bundle.join("crds-catalog-cache");
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_source: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)).into(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring: crate::generation::AuthoringPolicy::default(),
        provider: crate::provider::ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_owned()],
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            crd_catalog_cache_dir: Some(crds.clone()),
            crd_override_dir: Some(crds),
            allow_net: false,
            ..Default::default()
        },
    });
    let mut schema = session.emit(crate::output::EmitRequest {
        reference_policy: crate::output::ReferencePolicy::SelfContained,
        output: crate::output::OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: crate::output::DefinitionNames::Source,
        },
    })?;
    // Isolate the render demand from the compatibility hint, which rejects the controls too.
    let enabled = schema
        .pointer_mut("/properties/reportsServer/properties/enabled")
        .and_then(Value::as_object_mut)
        .ok_or_eyre("missing reportsServer.enabled schema")?;
    if enabled.get("type") == Some(&json!("boolean")) {
        enabled.remove("type");
    }

    let validator = jsonschema::validator_for(&schema)?;
    let cases = [
        ("number", json!(1.5)),
        ("coercible string", json!("3")),
        ("other string", json!("not-a-value")),
        ("nonempty list", json!([{}])),
        ("nonempty map", json!({"unknown": true})),
    ];
    let mut have = Vec::new();
    let mut want = Vec::new();
    for (label, enabled) in cases {
        let missing_tag = test_util::helm_values::coalesce_chart_values(
            &chart_dir,
            json!({"reportsServer": {"enabled": enabled}}),
        )?;
        let valid_tag = test_util::helm_values::coalesce_chart_values(
            &chart_dir,
            json!({"reportsServer": {"enabled": enabled}, "test": {"image": {"tag": "ok"}}}),
        )?;
        have.push((
            label,
            validator.is_valid(&missing_tag),
            validator.is_valid(&valid_tag),
        ));
        want.push((label, false, true));
    }
    sim_assert_eq!(have: have, want: want);
    let skip_readiness = test_util::helm_values::coalesce_chart_values(
        &chart_dir,
        json!({"reportsServer": {"enabled": "3", "waitForReady": false}}),
    )?;
    sim_assert_eq!(have: validator.is_valid(&skip_readiness), want: true);
    Ok(())
}
