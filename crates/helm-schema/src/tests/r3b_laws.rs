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
use indoc::{formatdoc, indoc};
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
    law_schema_with_archives(files, &[])
}

/// [`law_schema`] with packaged dependencies: each archive is a chart directory's files, written
/// as a gzipped tarball at its path (for example `charts/lib-0.1.0.tgz`).
fn law_schema_with_archives(
    files: &[(&str, &str)],
    archives: &[(&str, &[(&str, &str)])],
) -> eyre::Result<Value> {
    let archives: Vec<(&str, ArchiveEntries<'_>)> = archives
        .iter()
        .map(|(archive, entries)| {
            let entries = entries
                .iter()
                .map(|(name, source)| (name.as_bytes(), *source))
                .collect();
            (*archive, entries)
        })
        .collect();
    law_schema_with_byte_archives(files, &archives)
}

/// A packaged chart's entries: raw tar names and their sources.
type ArchiveEntries<'a> = Vec<(&'a [u8], &'a str)>;

/// [`law_schema_with_archives`] whose entry names are raw bytes, as a tarball may hold them.
fn law_schema_with_byte_archives(
    files: &[(&str, &str)],
    archives: &[(&str, ArchiveEntries<'_>)],
) -> eyre::Result<Value> {
    use std::io::Write as _;

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    for (path, source) in files {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    for (archive, entries) in archives {
        let mut bytes = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (name, source) in entries {
                let mut header = tar::Header::new_gnu();
                let field = &mut header.as_old_mut().name;
                field
                    .get_mut(..name.len())
                    .ok_or_eyre("an entry name fits the tar header")?
                    .copy_from_slice(name);
                header.set_size(u64::try_from(source.len())?);
                header.set_mode(0o644);
                header.set_entry_type(tar::EntryType::Regular);
                header.set_cksum();
                builder.append(&header, std::io::Cursor::new(source.as_bytes()))?;
            }
            builder.into_inner()?.finish()?;
        }
        let path = chart_dir.join(archive)?;
        path.parent().create_dir_all()?;
        path.create_file()?.write_all(&bytes)?;
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
            indoc! {"
                data:
                  x: {{ upper $d.inner.x | quote }}
            "},
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
            indoc! {"
                data:
                  x: {{ upper $d.inner.x | quote }}
            "},
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
            indoc! {r#"
                {{- $u := dict "x" .Values.bad }}
                {{- $m := dict }}
            "#},
            &merges,
            CONFIG_MAP,
            indoc! {"
                data:
                  x: {{ upper $u.x | quote }}
            "},
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
            indoc! {"
                data:
                  x: {{ upper $d.inner.x | quote }}
            "},
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
            indoc! {"
                data:
                  x: {{ upper $alias.x | quote }}
            "},
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

// Step 6a laws (ADR 0072 §6). Receipts: `transfer/receipts-6a/<law>` (Helm 4.2.3, `make.py`,
// `run.sh`, `verdicts.txt`); integer counts are also rendered through `--set` (ADR 0044).

fn include_repair_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            items: []
            after: "x"
        "#},
        &[
            "{{- range .Values.items }}{{- include \"repair\" $ }}{{- end }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  after: {{ upper .Values.after | quote }}
            "},
        ],
        "{{- define \"repair\" -}}{{- $_ := set .Values \"after\" \"ok\" -}}{{- end -}}\n",
    )
}

/// (6a-A) A helper included in the body writes the input; the write leaves the loop
/// (receipts `a_include_repair`: a1, a4 render).
#[test]
fn an_included_write_in_a_range_body_repairs_a_later_input() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_repair_schema()?,
        &[
            (
                "a1: a member runs the repair",
                json!({ "items": ["a"], "after": 7 }),
            ),
            (
                "a4: no member, string",
                json!({ "items": [], "after": "y" }),
            ),
        ],
        true,
    )
}

/// (6a-A) Without a member nothing repairs the input (receipt a2 aborts at `upper`).
#[test]
#[ignore = "target: ADR 0073 step 4c (a join keeps the arm an input write did not reach); the loop part is step 6a1"]
fn a_range_without_members_leaves_a_later_input_unrepaired() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_repair_schema()?,
        &[("a2: no member, integer", json!({ "items": [], "after": 7 }))],
        false,
    )
}

fn include_noop_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            items: []
            after: "x"
        "#},
        &[
            "{{- range .Values.items }}{{- include \"noop\" $ }}{{- end }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  after: {{ upper .Values.after | quote }}
            "},
        ],
        "{{- define \"noop\" -}}{{- end -}}\n",
    )
}

/// (6a1-M) A string renders whether or not members run (receipt `m_include_noop` m3).
#[test]
fn an_include_that_writes_nothing_accepts_a_later_string() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_noop_schema()?,
        &[(
            "m3: a member, string",
            json!({ "items": ["a"], "after": "y" }),
        )],
        true,
    )
}

/// (6a1-M) An included helper that writes nothing leaves the later abort in place: the body's
/// footprint is what it executed, not its text (receipts m1, m2 abort at `upper`).
#[test]
fn an_include_that_writes_nothing_keeps_a_later_abort() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_noop_schema()?,
        &[
            (
                "m1: a member, integer",
                json!({ "items": ["a"], "after": 7 }),
            ),
            ("m2: no member, integer", json!({ "items": [], "after": 7 })),
        ],
        false,
    )
}

fn include_flag_schema() -> eyre::Result<Value> {
    template_law_schema(
        "items: []\n",
        &[
            indoc! {r#"
                {{- $state := dict "first" true }}
                {{- range .Values.items }}
                {{- if not $state.first }}{{ $_ := upper . }}{{ end }}
                {{- include "mark" $state }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "{{- define \"mark\" -}}{{- $_ := set . \"first\" false -}}{{- end -}}\n",
    )
}

/// (6a-B) A flag an included helper clears skips only the first member
/// (receipts `b_include_flag`: b1, b2, b4 render).
#[test]
fn an_included_flag_skips_only_the_first_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_flag_schema()?,
        &[
            ("b1: integer, then string", json!({ "items": [7, "a"] })),
            ("b2: one integer", json!({ "items": [7] })),
            ("b4: strings", json!({ "items": ["a", "b"] })),
        ],
        true,
    )
}

/// (6a-B) The write carries into the next iteration, so later members are consumed
/// (receipt b3 aborts at `upper`).
#[test]
#[ignore = "target: ADR 0073 step 6b (ADR 0072 §6: carried state and positional members)"]
fn an_included_flag_rejects_a_later_integer_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &include_flag_schema()?,
        &[("b3: string, then integer", json!({ "items": ["a", 7] }))],
        false,
    )
}

/// (6a-C) A flag set by a member's value guards later consumers; no universal member
/// demand may follow from it (receipts `c_carried_flag`: c1, c2, c4 render; c3 aborts and
/// is not claimed).
#[test]
fn a_carried_flag_accepts_members_before_the_flag() -> eyre::Result<()> {
    let schema = template_law_schema(
        "items: []\n",
        &[
            indoc! {r#"
                {{- $found := false }}
                {{- range .Values.items }}
                {{- if $found }}{{ $_ := upper . }}{{ else if eq (toString .) "x" }}{{ $found = true }}{{ end }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("c1: integer, then x", json!({ "items": [7, "x"] })),
            ("c2: integers", json!({ "items": [7, 7] })),
            ("c4: x, then string", json!({ "items": ["x", "a"] })),
        ],
        true,
    )
}

fn member_field_schema() -> eyre::Result<Value> {
    template_law_schema(
        "count: []\n",
        &[
            "{{- range .Values.count }}{{ $_ := .name }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-D) Member structure read over a map member, no member, or a zero count, which
/// `--set count=0` renders (receipts `d_member_field`: d1, d5, d4.set render).
#[test]
fn a_member_field_read_accepts_map_members_and_a_zero_count() -> eyre::Result<()> {
    assert_helm_verdicts(
        &member_field_schema()?,
        &[
            ("d1: map member", json!({ "count": [{ "name": "a" }] })),
            ("d4: zero count", json!({ "count": 0 })),
            ("d5: no member", json!({ "count": [] })),
        ],
        true,
    )
}

/// (6a-D) A string member and a positive count abort in every transport: `.name` on a
/// string, and on the integer members `--set count=3` yields (receipts d2, d3, d3.set).
#[test]
#[ignore = "target: ADR 0073 step 6a (ADR 0072 §6: kind split, members and integer arms)"]
fn a_member_field_read_rejects_scalar_members_and_positive_counts() -> eyre::Result<()> {
    assert_helm_verdicts(
        &member_field_schema()?,
        &[
            ("d2: string member", json!({ "count": ["a"] })),
            ("d3: count three", json!({ "count": 3 })),
        ],
        false,
    )
}

fn printing_count_schema() -> eyre::Result<Value> {
    template_law_schema(
        "count: []\n",
        &[
            "{{- range .Values.count }}{{ $_ := print . }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-E) A body that only prints iterates integer counts under `--set` (ADR 0044)
/// (receipts `e_printing_count`: e1.set, e2.set, e3 render).
#[test]
fn a_printing_range_body_accepts_integer_counts_and_lists() -> eyre::Result<()> {
    assert_helm_verdicts(
        &printing_count_schema()?,
        &[
            ("e1: count three", json!({ "count": 3 })),
            ("e2: count zero", json!({ "count": 0 })),
            ("e3: list", json!({ "count": ["a", 7] })),
        ],
        true,
    )
}

/// (6a-E) A string and a fractional number abort in every transport (receipts e4, e5).
#[test]
fn a_printing_range_body_rejects_strings_and_fractions() -> eyre::Result<()> {
    assert_helm_verdicts(
        &printing_count_schema()?,
        &[
            ("e4: string", json!({ "count": "abc" })),
            ("e5: fraction", json!({ "count": 2.5 })),
        ],
        false,
    )
}

fn member_tpl_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            items: []
            after: "x"
        "#},
        &[
            "{{- range .Values.items }}{{ $_ := tpl .template $ }}{{- end }}\n",
            CONFIG_MAP,
            indoc! {"
                data:
                  after: {{ upper .Values.after | quote }}
            "},
        ],
        "",
    )
}

/// (6a-F) A member program that writes repairs the later input
/// (receipts `f_member_tpl`: f2, f3, f4 render).
#[test]
fn a_member_program_in_a_range_body_accepts_a_repairing_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &member_tpl_schema()?,
        &[
            (
                "f2: repairing member",
                json!({ "items": [{ "template": "{{ $_ := set .Values \"after\" \"ok\" }}" }], "after": 7 }),
            ),
            (
                "f3: plain, then repairing member",
                json!({ "items": [{ "template": "plain" }, { "template": "{{ $_ := set .Values \"after\" \"ok\" }}" }], "after": 7 }),
            ),
            (
                "f4: plain member, string",
                json!({ "items": [{ "template": "plain" }], "after": "y" }),
            ),
        ],
        true,
    )
}

/// (6a-F) When every member program is action-free the later abort stands; the atom is
/// closed over the members at the loop exit (receipt f1 aborts at `upper`).
#[test]
#[ignore = "target: ADR 0073 step 6a (ADR 0072 §5-§6: a member atom closed over the loop)"]
fn a_plain_member_program_keeps_a_later_abort() -> eyre::Result<()> {
    assert_helm_verdicts(
        &member_tpl_schema()?,
        &[(
            "f1: plain member, integer",
            json!({ "items": [{ "template": "plain" }], "after": 7 }),
        )],
        false,
    )
}

// Step 6 laws from the Sol design review (agentmux 20261117T164516-0544b181), receipts in
// `transfer/receipts-6a/<law>`.

fn two_binder_count_schema() -> eyre::Result<Value> {
    template_law_schema(
        "count: []\n",
        &[
            "{{- range $k, $v := .Values.count }}{{ $_ := print $v }}{{- else }}{{ $_ := print \"empty\" }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-G) Two binders over a list run the body or `else` (receipt `g_two_binder_count` g3).
#[test]
fn two_binders_accept_an_empty_list() -> eyre::Result<()> {
    assert_helm_verdicts(
        &two_binder_count_schema()?,
        &[("g3: empty list", json!({ "count": [] }))],
        true,
    )
}

/// (6a-G, green on the line; 6a keeps it) Two binders over an integer abort before emptiness
/// is tested, so `else` never runs, in every transport (receipts g1, g2, g1.set, g2.set).
#[test]
fn two_binders_reject_every_integer_count() -> eyre::Result<()> {
    assert_helm_verdicts(
        &two_binder_count_schema()?,
        &[
            ("g1: count zero", json!({ "count": 0 })),
            ("g2: count two", json!({ "count": 2 })),
        ],
        false,
    )
}

fn shallow_snapshot_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            a: "x"
            b: "y"
        "#},
        &[
            indoc! {r#"
                {{- $m := dict "a" .Values.a "b" .Values.b }}
                {{- range $k, $v := $m }}
                {{- if eq $k "a" }}{{ $_ := set $m "b" "new" }}{{ end }}
                {{- $_ := upper $v }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-H) String entries render (receipt `h_shallow_snapshot` h2).
#[test]
fn a_range_snapshot_accepts_string_entries() -> eyre::Result<()> {
    assert_helm_verdicts(
        &shallow_snapshot_schema()?,
        &[("h2: strings", json!({ "a": "x", "b": "y" }))],
        true,
    )
}

/// (6a-H, green on the line; 6a keeps it) The entry value is captured when the range starts: a later `set` of the key does
/// not change the value the iteration sees (receipt h1 aborts at `upper`).
#[test]
fn a_range_snapshot_rejects_an_entry_overwritten_after_it_began() -> eyre::Result<()> {
    assert_helm_verdicts(
        &shallow_snapshot_schema()?,
        &[("h1: integer b", json!({ "a": "x", "b": 7 }))],
        false,
    )
}

/// (6a-H) Nested maps inside entries stay live: an earlier iteration's write reaches the later
/// member (receipt `h_nested_live` h3 renders).
#[test]
fn a_range_snapshot_keeps_nested_members_live() -> eyre::Result<()> {
    let schema = template_law_schema(
        "b: \"y\"\n",
        &[
            indoc! {r#"
                {{- $m := dict "a" (dict "v" "x") "b" (dict "v" .Values.b) }}
                {{- range $k, $v := $m }}
                {{- if eq $k "a" }}{{ $_ := set (get $m "b") "v" "ok" }}{{ end }}
                {{- $_ := upper $v.v }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[("h3: integer b repaired", json!({ "b": 7 }))],
        true,
    )
}

/// (6a-I) A fact relative to the first member is not a fact about every member: no claim may
/// follow from relabelling the first iteration (receipts `i_first_relative` i1, i2 render; i3
/// aborts and is relational, so it is not claimed).
#[test]
fn a_first_member_comparison_accepts_equal_members() -> eyre::Result<()> {
    let schema = template_law_schema(
        "xs: []\n",
        &[
            "{{- range .Values.xs }}{{ if ne . (index $.Values.xs 0) }}{{ fail \"later\" }}{{ end }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("i1: equal members", json!({ "xs": ["a", "a"] })),
            ("i2: one member", json!({ "xs": ["a"] })),
        ],
        true,
    )
}

fn cross_member_repair_schema() -> eyre::Result<Value> {
    template_law_schema(
        "xs: []\n",
        &[
            "{{- range .Values.xs }}{{ $_ := tpl .p $ }}{{ $_ := upper .v }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-J) One member's program repairs a later member before its consumer runs
/// (receipt `j_cross_member_repair` j1 renders).
#[test]
fn an_earlier_member_program_accepts_a_later_repaired_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &cross_member_repair_schema()?,
        &[(
            "j1: the first program repairs the second value",
            json!({ "xs": [
                { "p": "{{ $_ := set (index .Values.xs 1) \"v\" \"ok\" }}", "v": "ok" },
                { "p": "", "v": 7 }
            ] }),
        )],
        true,
    )
}

/// (6a-J) When every member program is action-free, every member value is consumed
/// (receipt j2 aborts at `upper`).
#[test]
#[ignore = "target: ADR 0073 step 6a (ADR 0072 §5-§6: preceding validity guards each iteration's reads)"]
fn plain_member_programs_reject_an_integer_member() -> eyre::Result<()> {
    assert_helm_verdicts(
        &cross_member_repair_schema()?,
        &[(
            "j2: plain programs, integer second value",
            json!({ "xs": [{ "p": "", "v": "ok" }, { "p": "", "v": 7 }] }),
        )],
        false,
    )
}

fn guarded_member_schema() -> eyre::Result<Value> {
    template_law_schema(
        "xs: []\n",
        &[
            "{{- range .Values.xs }}{{ if .enabled }}{{ $_ := upper .name }}{{ end }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )
}

/// (6a-K) A guarded consumer demands nothing of the members its guard skips
/// (receipts `k_guarded_member` k1, k2 render).
#[test]
fn a_guarded_member_consumer_accepts_skipped_members() -> eyre::Result<()> {
    assert_helm_verdicts(
        &guarded_member_schema()?,
        &[
            (
                "k1: disabled, integer name",
                json!({ "xs": [{ "enabled": false, "name": 7 }] }),
            ),
            (
                "k2: enabled string, then disabled integer",
                json!({ "xs": [{ "enabled": true, "name": "a" }, { "enabled": false, "name": 7 }] }),
            ),
        ],
        true,
    )
}

/// (6a-K) The guarded consumer is a universal implication over every member
/// (receipts k3, k4 abort at `upper`).
#[test]
#[ignore = "target: ADR 0073 step 6a (ADR 0072 §6: universal guarded implications)"]
fn a_guarded_member_consumer_rejects_any_enabled_integer_name() -> eyre::Result<()> {
    assert_helm_verdicts(
        &guarded_member_schema()?,
        &[
            (
                "k3: enabled integer second",
                json!({ "xs": [{ "enabled": true, "name": "a" }, { "enabled": true, "name": 7 }] }),
            ),
            (
                "k4: enabled integer first",
                json!({ "xs": [{ "enabled": true, "name": 7 }] }),
            ),
        ],
        false,
    )
}

fn recursive_walk_schema() -> eyre::Result<Value> {
    template_law_schema(
        "root: {}\n",
        &["{{- $_ := include \"walk\" .Values.root }}\n", CONFIG_MAP],
        "{{- define \"walk\" -}}{{- if .next }}{{ include \"walk\" .next }}{{- else }}{{ upper .v }}{{- end }}{{- end -}}\n",
    )
}

/// (6e-L) A recursive walk renders when its leaf is a string (receipt `l_recursive_walk` l2).
#[test]
fn a_recursive_walk_accepts_a_string_leaf() -> eyre::Result<()> {
    assert_helm_verdicts(
        &recursive_walk_schema()?,
        &[(
            "l2: deep string leaf",
            json!({ "root": { "next": { "next": { "v": "a" } } } }),
        )],
        true,
    )
}

/// (6e-L) Without recursion the leaf is consumed directly (receipt l3 aborts).
#[test]
fn a_recursive_walk_rejects_an_integer_leaf_at_the_root() -> eyre::Result<()> {
    assert_helm_verdicts(
        &recursive_walk_schema()?,
        &[("l3: shallow integer leaf", json!({ "root": { "v": 7 } }))],
        false,
    )
}

/// (6e-L) The recursion descends its argument and aborts at the leaf (receipt l1).
#[test]
#[ignore = "target: ADR 0073 step 6e (a recursive call transfer over arguments and outcomes)"]
fn a_recursive_walk_rejects_a_deep_integer_leaf() -> eyre::Result<()> {
    assert_helm_verdicts(
        &recursive_walk_schema()?,
        &[(
            "l1: deep integer leaf",
            json!({ "root": { "next": { "next": { "v": 7 } } } }),
        )],
        false,
    )
}

// Step 6 laws from Sol's re-review of design v2 (agentmux 20261117T164516-0544b181, turn 2),
// receipts in `transfer/receipts-6a/<law>`.

/// (6a2-N) `--set count=3` yields int64 elements, so a body requiring int64 renders under the
/// transport union (ADR 0044; receipts `n_int64_elements` n1.set renders, n2).
#[test]
fn an_integer_count_accepts_a_body_requiring_int64_elements() -> eyre::Result<()> {
    let schema = template_law_schema(
        "count: []\n",
        &[
            "{{- range .Values.count }}{{ if not (kindIs \"int64\" .) }}{{ fail \"not int64\" }}{{ end }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("n1: count three", json!({ "count": 3 })),
            ("n2: empty list", json!({ "count": [] })),
        ],
        true,
    )
}

/// (6a2-N) The elements are never `int`, so a body requiring `int` aborts in every transport
/// (receipts `n_int_elements` n3, n3.set).
#[test]
#[ignore = "target: ADR 0073 step 6a2 (ADR 0072 §6: int64 integer-arm elements)"]
fn an_integer_count_rejects_a_body_requiring_int_elements() -> eyre::Result<()> {
    let schema = template_law_schema(
        "count: []\n",
        &[
            "{{- range .Values.count }}{{ if not (kindIs \"int\" .) }}{{ fail \"not int\" }}{{ end }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[("n3: count three", json!({ "count": 3 }))],
        false,
    )
}

/// (6a1-O) A slice range reads each slot when it visits it: sorting the backing in an earlier
/// iteration changes what the later one appends (receipts `o_slice_slots_live`: o1 renders with
/// `$o = [b b]`).
#[test]
fn a_slice_range_reads_each_slot_when_visited() -> eyre::Result<()> {
    let schema = template_law_schema(
        "xs: []\n",
        &[
            indoc! {r#"
                {{- $s := toStrings .Values.xs }}{{- $o := list }}
                {{- range $s }}{{ $o = append $o . }}{{ $_ := sortAlpha $s }}{{- end }}
                {{- if has "a" $o }}{{ fail "a appended" }}{{ end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[("o1: b, then a", json!({ "xs": ["b", "a"] }))],
        true,
    )
}

/// (6a1-P) A write under a computed key may hit any field, so it updates them weakly
/// (receipts `p_computed_key`: p1 renders).
#[test]
fn a_computed_key_write_in_a_loop_accepts_a_repaired_field() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            before: "x"
            key: "k"
            xs: []
        "#},
        &[
            indoc! {r#"
                {{- $d := dict "v" .Values.before }}
                {{- range .Values.xs }}{{ $_ := set $d $.Values.key "ok" }}{{- end }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  v: {{ upper $d.v | quote }}
            "},
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "p1: the key hits v",
            json!({ "before": 7, "key": "v", "xs": ["a"] }),
        )],
        true,
    )
}

/// (6a2-Q) A member compared with the first member: only members equal to it are consumed
/// (receipts `q_mixed_members`: q1 renders; q2 aborts and is relational, so it is not claimed).
#[test]
fn a_first_member_equality_accepts_mixed_later_members() -> eyre::Result<()> {
    let schema = template_law_schema(
        "xs: []\n",
        &[
            "{{- range .Values.xs }}{{ if eq (toString .) (toString (index $.Values.xs 0)) }}{{ $_ := upper . }}{{ end }}{{- end }}\n",
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[("q1: string, then integer", json!({ "xs": ["a", 7] }))],
        true,
    )
}

fn bound_locality_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            first: "f"
            name: "n"
            xs: []
        "#},
        &[
            indoc! {r#"
                {{- $_ := upper .Values.first }}
                {{- $saved := .Values.name }}
                {{- $a := "" }}{{- $b := "" }}{{- $c := "" }}{{- $d := "" }}{{- $e := "" }}
                {{- range .Values.xs }}{{ $e = $d }}{{ $d = $c }}{{ $c = $b }}{{ $b = $a }}{{ $a = . }}{{- end }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  saved: {{ upper $saved | quote }}
                  e: {{ $e | quote }}
            "},
        ],
        "",
    )
}

/// (6a1-R) A loop whose carried bindings need more passes than the bound renders strings
/// (receipt `r_bound_locality` r3).
#[test]
fn a_loop_past_its_pass_bound_accepts_strings() -> eyre::Result<()> {
    assert_helm_verdicts(
        &bound_locality_schema()?,
        &[(
            "r3: strings",
            json!({ "first": "f", "name": "n", "xs": ["a", "b", "c", "d", "e", "f"] }),
        )],
        true,
    )
}

/// (6a1-R, Sol S7) The bound widens only what the loop carries: a scalar captured before it and a
/// fact recorded before it keep their demands (receipts r1, r2 abort at `upper`).
#[test]
fn a_loop_past_its_pass_bound_keeps_unrelated_facts() -> eyre::Result<()> {
    assert_helm_verdicts(
        &bound_locality_schema()?,
        &[
            (
                "r1: integer captured name",
                json!({ "first": "f", "name": 7, "xs": ["a", "b"] }),
            ),
            (
                "r2: integer first",
                json!({ "first": 7, "name": "n", "xs": ["a", "b"] }),
            ),
        ],
        false,
    )
}

// Step 6a1 laws from Sol's pre-landing review of 5863e6d9 (agentmux 20261117T215117-7f241ebd),
// receipts in `transfer/receipts-6a/<law>`.

/// Counts the `name` counter spans an analysis opens on this thread.
struct CounterLayer {
    name: &'static str,
    count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CounterLayer {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = attributes.metadata();
        if metadata.target() == "helm_schema_ir::counters" && metadata.name() == self.name {
            self.count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Runs `analysis` and counts the `name` counter spans it opens.
fn counted<T>(name: &'static str, analysis: impl FnOnce() -> T) -> (T, usize) {
    use tracing_subscriber::layer::SubscriberExt as _;
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(CounterLayer {
        name,
        count: std::sync::Arc::clone(&count),
    });
    let result = tracing::subscriber::with_default(subscriber, analysis);
    (result, count.load(std::sync::atomic::Ordering::Relaxed))
}

/// (6a1-T) In-place sorts of shared slots reach a third-iteration state the second does not
/// cover: three members sort the backing to `abcd`, which skips the consumer (receipts
/// `t_slice_sort_third`: DCBA, CBAD, BACD, ABCD for zero to three members; t3 and t4 render, t0 to
/// t2 abort at `upper`).
#[test]
fn a_slice_sorted_in_place_reaches_its_third_iteration_state() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            xs: []
            label: "s"
        "#},
        &[
            indoc! {r#"
                {{- $s := splitList "," "d,c,b,a" }}
                {{- range .Values.xs }}{{ $_ := sortAlpha (slice $s 0 2) }}{{ $_ := sortAlpha (slice $s 1 3) }}{{ $_ := sortAlpha (slice $s 2 4) }}{{- end }}
                {{- if ne (join "" $s) "abcd" }}{{ $_ := upper .Values.label }}{{ end }}
            "#},
            CONFIG_MAP,
            indoc! {r#"
                data:
                  s: {{ join "" $s | upper | quote }}
            "#},
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "t3: three members sort the backing",
            json!({ "xs": [1, 2, 3], "label": 7 }),
        )],
        true,
    )
}

fn escaping_dicts_schema() -> eyre::Result<Value> {
    template_law_schema(
        indoc! {r#"
            first: "f"
            name: "n"
            v: "s"
            xs: []
        "#},
        &[
            indoc! {r#"
                {{- $_ := upper .Values.first }}
                {{- $saved := .Values.name }}
                {{- $a := dict "v" "init" }}{{- $b := $a }}{{- $c := $a }}{{- $d := $a }}{{- $e := $a }}
                {{- range .Values.xs }}{{ $e = $d }}{{ $d = $c }}{{ $c = $b }}{{ $b = $a }}{{ $a = dict "v" $.Values.v }}{{- end }}
            "#},
            CONFIG_MAP,
            indoc! {"
                data:
                  saved: {{ upper $saved | quote }}
                  e: {{ $e.v | quote }}
            "},
        ],
        "",
    )
}

/// (6a1-U) Bindings that carry a fresh dictionary per member reach the pass bound, terminate,
/// and widen only themselves (receipts `u_escaping_dicts`: u1 and u4 render, u2 and u3 abort at
/// `upper`).
#[test]
fn escaping_dictionaries_past_the_pass_bound_terminate_locally() -> eyre::Result<()> {
    let (schema, bounds) = counted("loop_fixpoint_bound", escaping_dicts_schema);
    let schema = schema?;
    assert!(bounds > 0, "the chain must reach the pass bound");
    let document = |first: Value, name: Value, v: Value| json!({ "first": first, "name": name, "v": v, "xs": [1, 2, 3, 4, 5, 6] });
    assert_helm_verdicts(
        &schema,
        &[
            ("u1: strings", document(json!("f"), json!("n"), json!("s"))),
            ("u4: integer v", document(json!("f"), json!("n"), json!(7))),
        ],
        true,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            (
                "u2: integer name",
                document(json!("f"), json!(7), json!("s")),
            ),
            (
                "u3: integer first",
                document(json!(7), json!("n"), json!("s")),
            ),
        ],
        false,
    )
}

/// (6a1-W) A write to an input map first read inside the body changes the state the next
/// iteration starts from (receipts `w_materialized_input`: w1 renders, w2 and w3 abort at
/// `upper`).
#[test]
fn a_write_to_an_input_map_first_read_in_the_body_is_carried() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {r#"
            cfg:
              flag: false
            label: "s"
            xs: []
        "#},
        &[
            indoc! {r#"
                {{- range .Values.xs }}
                {{- if $.Values.cfg.flag }}{{ $_ := set $.Values.cfg "ready" true }}{{ end }}
                {{- $_ := set $.Values.cfg "flag" true }}
                {{- end }}
                {{- if not .Values.cfg.ready }}{{ $_ := upper .Values.label }}{{ end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "w1: the second member sets ready",
            json!({ "cfg": { "flag": false }, "label": 7, "xs": [1, 2] }),
        )],
        true,
    )
}

/// The final schema of a chart whose files are given relative to its root, with the committed
/// Kubernetes schema bundle, so verdicts include the API server's demands on rendered fields.
fn provider_law_schema(files: &[(&str, &str)]) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    for (path, source) in files {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    let bundle = test_util::workspace_testdata().join("provider-bundle");
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
            k8s_versions: vec!["v1.29.0-standalone-strict".to_owned()],
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            allow_net: false,
            crd_catalog_cache_dir: Some(bundle.join("crds-catalog-cache")),
            disable_k8s_schemas: false,
            crd_override_dir: Some(bundle.join("crds-catalog-cache")),
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

/// (6a1-X, external-dns) A range whose body runs an unknown `tpl` needs the Rest passes, and
/// their exits must keep the document's placement facts: `serviceAccount.name` lands in a
/// `ServiceAccount`'s `metadata.name`, which the API server requires to be a string. Each selected
/// layout of the joined output completes on an allowance of its own, so the costly loop
/// alternatives no longer starve the others. The chart is corpus external-dns reduced to its
/// service account (receipt `x_edns_service_account/battery-probe`: Helm renders `name: true`,
/// Kubernetes rejects it; the line and main reject the document).
#[test]
fn a_loop_needing_rest_passes_keeps_its_placement_facts() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("charts/external-dns");
    let read = |path: &str| std::fs::read_to_string(chart.join(path));
    let (manifest, values, helpers, account) = (
        read("Chart.yaml")?,
        read("values.yaml")?,
        read("templates/_helpers.tpl")?,
        read("templates/serviceaccount.yaml")?,
    );
    let schema = provider_law_schema(&[
        ("Chart.yaml", &manifest),
        ("values.yaml", &values),
        ("templates/_helpers.tpl", &helpers),
        ("templates/serviceaccount.yaml", &account),
    ])?;
    // With annotations, the unknown `tpl` output may restructure the document, so neither the
    // line nor this transfer claims the field there.
    assert_helm_verdicts(
        &schema,
        &[(
            "x1: no annotations, boolean name",
            test_util::helm_values::coalesce_chart_values(
                &chart,
                json!({"serviceAccount": {"name": true}}),
            )?,
        )],
        false,
    )
}

/// The final schema of corpus chart `chart` with the committed Kubernetes schema bundle.
fn provider_corpus_schema(chart: &str) -> eyre::Result<Value> {
    let chart_dir = test_util::workspace_testdata().join("charts").join(chart);
    let bundle = test_util::workspace_testdata().join("provider-bundle");
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_source: VfsPath::new(vfs::PhysicalFS::new(chart_dir)).into(),
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
            k8s_versions: vec!["v1.29.0-standalone-strict".to_owned()],
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            allow_net: false,
            crd_catalog_cache_dir: Some(bundle.join("crds-catalog-cache")),
            disable_k8s_schemas: false,
            crd_override_dir: Some(bundle.join("crds-catalog-cache")),
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

/// (6a1-Y, longhorn) A render use is self-guarded when its condition proves its source truthy,
/// also when normalization factors that test through every arm of a disjunction: here the
/// `if and … (not (eq .Values.t "")) … else if eq .Values.t "k3s"` chain splits on `t`'s type.
/// Helm renders `namespaceOverride: false` (and `0`) through `.Release.Namespace`
/// (receipt `y_longhorn_namespace_falsy`); main accepts both, the transfer before this law
/// rejected both.
#[test]
fn a_disjunctive_self_guard_keeps_the_falsy_escape() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "release_namespace" -}}
        {{- if .Values.namespaceOverride -}}
        {{- .Values.namespaceOverride -}}
        {{- else -}}
        {{- .Release.Namespace -}}
        {{- end -}}
        {{- end -}}
    "#};
    let account = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: a
          namespace: {{ include "release_namespace" . }}
    "#};
    let policy = indoc! {r#"
        {{- if and .Values.a .Values.c (not (eq .Values.t "")) }}
        kind: NetworkPolicy
        metadata:
          namespace: {{ include "release_namespace" . }}
        {{- else if eq .Values.t "k3s" }}
        {{- end }}
    "#};
    let schema = provider_law_schema(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: longhorn
                version: 1.0.0
            "},
        ),
        (
            "values.yaml",
            indoc! {r#"
                namespaceOverride: ""
                a: false
                c: false
                t: ""
            "#},
        ),
        ("templates/_helpers.tpl", helpers),
        ("templates/sa.yaml", account),
        ("templates/np.yaml", policy),
    ])?;
    let document = |namespace: Value| json!({ "namespaceOverride": namespace, "a": false, "c": false, "t": "" });
    assert_helm_verdicts(
        &schema,
        &[
            ("y1: false", document(json!(false))),
            ("y2: zero", document(json!(0))),
        ],
        true,
    )
}

/// (6a1-Z, vpa) An arm guarded by its target's own truthiness never fires on a Helm-falsy input,
/// even when its member-host domain is incomplete, so it cannot keep the values.yaml base
/// (`type: object`) for those inputs. Helm renders `updater.podMonitor: false` (the whole
/// template is under `if .Values.updater.podMonitor`; receipt `z_vpa_pod_monitor_falsy`, the
/// a11 pair's probe); main and the line accept it, a11 rejected it. The values.yaml base now
/// holds under the incomplete arm's own guards, whose projection is inexact here, so a truthy
/// non-map is accepted as on the line.
#[test]
fn a_self_truthy_arm_leaves_falsy_inputs_open() -> eyre::Result<()> {
    let schema = provider_corpus_schema("vpa")?;
    assert_helm_verdicts(
        &schema,
        &[(
            "z1: updater.podMonitor false",
            r3b_values("vpa", json!({"updater": {"podMonitor": false}}))?,
        )],
        true,
    )
}

/// The reduced kube-prometheus-stack chart of the requirement laws: its values, helpers and the
/// alertmanager rule file, without dependencies.
fn kps_rule_files() -> eyre::Result<Vec<(&'static str, String)>> {
    let chart = test_util::workspace_testdata().join("charts/kube-prometheus-stack");
    let read = |path: &str| std::fs::read_to_string(chart.join(path));
    Ok(vec![
        (
            "Chart.yaml",
            "apiVersion: v2\nname: kube-prometheus-stack\nversion: 1.0.0\n".to_owned(),
        ),
        ("values.yaml", read("values.yaml")?),
        ("templates/_helpers.tpl", read("templates/_helpers.tpl")?),
        (
            "templates/prometheus/_rules.tpl",
            read("templates/prometheus/_rules.tpl")?,
        ),
        (
            "templates/prometheus/rules-1.14/alertmanager.rules.yaml",
            read("templates/prometheus/rules-1.14/alertmanager.rules.yaml")?,
        ),
    ])
}

/// (6a1-K, kube-prometheus-stack) The occurrences of one requirement group share almost all of
/// their guard graphs: every alert's `dig … .Values.customRules` runs below the same rule-file
/// prefix. One fold walks and imports each shared guard node once, and a group whose union
/// exhausts its allowance splits into parts that fold, so no occurrence's requirement widens
/// (before the shared walk, single occurrences fell back here).
#[test]
fn a_fold_charges_each_shared_guard_node_once() -> eyre::Result<()> {
    let files = kps_rule_files()?;
    let files = files
        .iter()
        .map(|(path, source)| (*path, source.as_str()))
        .collect::<Vec<_>>();
    let (schema, bounds) = counted("requirement_occurrence_bound", || {
        provider_law_schema(&files)
    });
    schema?;
    sim_assert_eq!(have: bounds, want: 0);
    Ok(())
}

/// (6a1-K) A requirement group is emitted in its smaller exact form, its union or the
/// conjunction of its parts, and a file's abort needs no earlier survival: the verdicts stay
/// Helm's. A string `customRules` aborts every enabled alert's `dig`; with the rule group off
/// nothing reads it and Helm renders (receipt `k_kps_rule_requirements`). The values.yaml
/// `type: object` base holds only under the incomplete member-host arms' own guards; the line
/// kept it beside them and rejected the second document (main accepts it).
#[test]
fn split_requirements_and_absorbing_failures_keep_helm_verdicts() -> eyre::Result<()> {
    let files = kps_rule_files()?;
    let files = files
        .iter()
        .map(|(path, source)| (*path, source.as_str()))
        .collect::<Vec<_>>();
    let schema = provider_law_schema(&files)?;
    let mut defaults = serde_yaml::from_str::<Value>(files[1].1)?;
    let validator = jsonschema::validator_for(&schema)?;
    let mut verdicts = Vec::new();
    verdicts.push(("defaults", validator.is_valid(&defaults)));
    defaults["customRules"] = json!("x");
    verdicts.push(("string customRules", validator.is_valid(&defaults)));
    defaults["defaultRules"]["rules"]["alertmanager"] = json!(false);
    verdicts.push((
        "group off, string customRules",
        validator.is_valid(&defaults),
    ));
    sim_assert_eq!(
        have: verdicts,
        want: vec![
            ("defaults", true),
            ("string customRules", false),
            ("group off, string customRules", true),
        ]
    );
    Ok(())
}

/// (6a1 exception, longhorn) A coalesced registry rendered as `{{ . }}/` into an image's plain
/// scalar: a list prints as `[map[]]/…`, which Helm's YAML parse rejects (receipt
/// `l_longhorn_image_registry_list`, Helm 4.2.3: the list aborts in YAML parsing, a string
/// renders).
#[test]
fn a_coalesced_list_rendered_into_a_plain_scalar_is_rejected() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        (
            "values.yaml",
            indoc! {r#"
                global:
                  imageRegistry: ""
                image:
                  registry: ""
                  repository: longhornio/manager
                  tag: v1
            "#},
        ),
        (
            "templates/d.yaml",
            indoc! {r"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
                data:
                  image: {{ with (coalesce .Values.global.imageRegistry .Values.image.registry) }}{{ . }}/{{ end }}{{ .Values.image.repository }}:{{ .Values.image.tag }}
            "},
        ),
    ])?;
    let document = |registry: Value| {
        json!({
            "global": { "imageRegistry": registry },
            "image": { "registry": "", "repository": "longhornio/manager", "tag": "v1" },
        })
    };
    assert_helm_verdicts(
        &schema,
        &[("l1: a string registry", document(json!("reg.example")))],
        true,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("l2: a list registry", document(json!([{}])))],
        false,
    )
}

/// The final schema of corpus chart `chart` under the default (asserting) declared-type
/// policy, with Kubernetes schemas off so verdicts come from template and authoring policy.
fn corpus_assert_schema(chart: &str) -> eyre::Result<Value> {
    let chart_dir = test_util::workspace_testdata().join("charts").join(chart);
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(chart_dir));
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_source: chart_dir.into(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring: crate::generation::AuthoringPolicy::default(),
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

fn r3b_values(chart: &str, overrides: Value) -> eyre::Result<Value> {
    let path = test_util::workspace_testdata().join("charts").join(chart);
    Ok(test_util::helm_values::coalesce_chart_values(
        &path, overrides,
    )?)
}

/// datadog `commonLabels` is serialized with `toYaml` into a labels mapping's entries. A
/// non-empty scalar or list there is not a mapping entry, so Helm 4.2.3 rejects
/// `commonLabels: 1.5` (`agent-services.yaml`: could not find expected ':'). Receipts
/// `transfer/receipts-6a/s_datadog_common_labels` (5 hashes in `hashes.txt`).
#[test]
#[ignore = "lane C: datadog is multi-instance (Release Unknown until step 5's shared heap), its fullname arms read nameOverride, and a helper-text choice is one layout hole"]
fn datadog_common_labels_keep_their_declared_type_after_a_lowered_serialization() -> eyre::Result<()>
{
    assert_helm_verdicts(
        &corpus_assert_schema("datadog")?,
        &[(
            "c1: number",
            r3b_values("datadog", json!({"commonLabels": 1.5}))?,
        )],
        false,
    )
}

/// (6a1 exception control) A labels map renders (receipt c2).
#[test]
fn datadog_common_labels_accept_a_labels_map() -> eyre::Result<()> {
    assert_helm_verdicts(
        &corpus_assert_schema("datadog")?,
        &[(
            "c2: map",
            r3b_values("datadog", json!({"commonLabels": {"team": "dev"}}))?,
        )],
        true,
    )
}

/// The datadog shape without the chart: a helper's entries and a guarded `toYaml` of the labels,
/// captured and indented into `metadata.labels`. Only a mapping keeps the peers' mapping; any other
/// truthy value is a bare node among its entries, which Helm's YAML parse rejects (receipt
/// `transfer/receipts-lanec3/labels-splice`). `entries` are the helper's lines before the splice.
fn spliced_labels_schema(values: &str, entries: &str) -> eyre::Result<Value> {
    let helpers = formatdoc! {r#"
        {{{{- define "law.labels" -}}}}
        {entries}
        {{{{- if .Values.commonLabels }}}}
        {{{{ toYaml .Values.commonLabels }}}}
        {{{{- end }}}}
        {{{{- end -}}}}
    "#};
    law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", values),
        ("templates/_helpers.tpl", &helpers),
        (
            "templates/cm.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
                  labels:
                {{ include "law.labels" . | indent 4 }}
            "#},
        ),
    ])
}

/// The splice verdicts of `commonLabels` once every entry before it is known to stay in its
/// mapping: Helm rejects each truthy non-mapping and renders mappings and falsy values.
fn assert_labels_splice_verdicts(schema: &Value, document: &Value) -> eyre::Result<()> {
    let with = |labels: Value| labels_document(document, labels);
    assert_helm_verdicts(
        schema,
        &[
            ("number", with(json!(1.5))?),
            ("string", with(json!("3"))?),
            ("boolean", with(json!(true))?),
            ("list", with(json!([{}]))?),
        ],
        false,
    )?;
    assert_labels_splice_acceptances(schema, document)
}

/// The acceptance half of [`assert_labels_splice_verdicts`]: mappings and falsy values render.
fn assert_labels_splice_acceptances(schema: &Value, document: &Value) -> eyre::Result<()> {
    let with = |labels: Value| labels_document(document, labels);
    assert_helm_verdicts(
        schema,
        &[
            ("labels", with(json!({"team": "dev"}))?),
            ("empty", with(json!({}))?),
            ("false", with(json!(false))?),
        ],
        true,
    )
}

fn labels_document(document: &Value, labels: Value) -> eyre::Result<Value> {
    let mut document = document.clone();
    document
        .as_object_mut()
        .ok_or_eyre("a values document is a mapping")?
        .insert("commonLabels".to_owned(), labels);
    Ok(document)
}

#[test]
#[ignore = "a helper's text choice (`if` around `toYaml`) is one layout hole; laying it out arm by arm must keep partial guards first (8 helm-schema-gen regressions otherwise)"]
fn spliced_labels_reject_every_truthy_non_mapping() -> eyre::Result<()> {
    let schema = spliced_labels_schema("commonLabels: {}\n", "helm.sh/chart: law")?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

/// A peer entry printing a string drawn from a finite-state language (`typeOf` of a number is
/// `float64` or `int64`) stays a plain scalar on every spelling, raw or after edits that keep every
/// spelling (`trunc 7`, `trimSuffix "-"`) and a case mapping, so the image verdicts follow
/// (receipt `transfer/receipts-lanec3/domain-image`, Helm 4.2.3).
#[test]
fn a_string_domain_peer_keeps_the_image_verdicts() -> eyre::Result<()> {
    let schema = image_schema(
        "",
        &[
            "  kind: {{ typeOf .Values.count }}\n",
            r#"  short: {{ typeOf .Values.count | trunc 7 | trimSuffix "-" | upper }}"#,
            "\n",
        ]
        .concat(),
        "",
    )?;
    let document = |registry: Value| image_document(registry, &json!({"count": 1}));
    assert_helm_verdicts(&schema, &[("list", document(json!([{}]))?)], false)?;
    assert_helm_verdicts(
        &schema,
        &[("string", document(json!("reg.example"))?)],
        true,
    )
}

/// A `ConfigMap` after `prefix`, whose `data` holds `before`, an image whose registry is a coalesced
/// value printed into a plain scalar, and `after` (`before` and `after` are whole lines indented
/// for `data`). A list registry prints as `[map[]]/`, which Helm's YAML parse rejects.
fn image_schema(prefix: &str, before: &str, after: &str) -> eyre::Result<Value> {
    let image = r"  image: {{ with (coalesce .Values.global.imageRegistry .Values.image.registry) }}{{ . }}/{{ end }}{{ .Values.image.repository }}:{{ .Values.image.tag }}";
    let template = [
        prefix,
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: x\ndata:\n",
        before,
        image,
        "\n",
        after,
    ]
    .concat();
    law_schema(&[
        ("Chart.yaml", CHART),
        (
            "values.yaml",
            indoc! {r#"
                count: 1
                x: ""
                global:
                  imageRegistry: ""
                image:
                  registry: ""
                  repository: longhornio/manager
                  tag: v1
            "#},
        ),
        ("templates/d.yaml", &template),
    ])
}

fn image_document(registry: Value, extra: &Value) -> eyre::Result<Value> {
    let mut document = json!({
        "count": 1,
        "x": "",
        "global": {"imageRegistry": null},
        "image": {"registry": "", "repository": "longhornio/manager", "tag": "v1"},
    });
    document["global"]["imageRegistry"] = registry;
    let fields = document
        .as_object_mut()
        .ok_or_eyre("a values document is a mapping")?;
    for (key, value) in extra.as_object().ok_or_eyre("extra fields are a mapping")? {
        fields.insert(key.clone(), value.clone());
    }
    Ok(document)
}

/// Fresh nondeterministic branch choices around unknown programs reach a finite validity Top.
#[test]
fn fresh_loop_validity_assumptions_terminate_with_a_counter() -> eyre::Result<()> {
    let (schema, bounds) = counted("loop_assumption_top", || {
        template_law_schema(
            indoc! {"
                program: plain
                name: ok
            "},
            &[
                indoc! {r#"
                {{- $_ := upper .Values.name }}
                {{- range 64 }}
                  {{- if eq (randAlpha 1) "a" }}{{ $_ := tpl $.Values.program $ }}{{ end }}
                {{- end }}
            "#},
                CONFIG_MAP,
            ],
            "",
        )
    });
    let schema = schema?;
    sim_assert_eq!(have: bounds, want: 1);
    assert_helm_verdicts(
        &schema,
        &[("plain program", json!({"program": "plain", "name": "ok"}))],
        true,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "earlier consumption",
            json!({"program": "plain", "name": 7}),
        )],
        false,
    )
}

/// (6a1 rework item 1, ADR 0072 §10) The assumption Top widens only the loop: facts after the
/// loop, in the template and in a helper's caller, keep the validity assumption each pass states.
/// Helm aborts at `upper` on `after: 7` for every choice `randAlpha` makes.
#[test]
#[ignore = "target: ADR 0073 step 2 effects (ADR 0072 §5: a demand after a randAlpha-guarded tpl survives its partial assumption; the loss reproduces without any loop)"]
fn a_loop_assumption_top_keeps_the_facts_after_the_loop() -> eyre::Result<()> {
    const LOOP: &str = indoc! {r#"
        {{- range 64 }}
          {{- if eq (randAlpha 1) "a" }}{{ $_ := tpl $.Values.program $ }}{{ end }}
        {{- end }}
    "#};
    let direct = format!("{LOOP}{{{{- $_ := upper .Values.after }}}}\n");
    let helper = format!("{{{{- define \"law.loop\" }}}}{LOOP}{{{{- end }}}}\n");
    for (template, helpers) in [
        (direct, String::new()),
        (
            "{{- include \"law.loop\" . }}{{- $_ := upper .Values.after }}\n".to_owned(),
            helper,
        ),
    ] {
        let (schema, bounds) = counted("loop_assumption_top", || {
            template_law_schema(
                indoc! {"
                    program: plain
                    after: ok
                "},
                &[template.as_str(), CONFIG_MAP],
                &helpers,
            )
        });
        let schema = schema?;
        sim_assert_eq!(have: bounds, want: 1, "{template}");
        assert_helm_verdicts(
            &schema,
            &[("plain program", json!({"program": "plain", "after": "ok"}))],
            true,
        )?;
        assert_helm_verdicts(
            &schema,
            &[(
                "consumption after the loop",
                json!({"program": "plain", "after": 7}),
            )],
            false,
        )?;
    }
    Ok(())
}

/// The exact second iteration consumes bad before the carried binding loses its identity.
#[test]
fn rest_keeps_consumptions_proved_before_widening() -> eyre::Result<()> {
    let schema = template_law_schema(
        "bad: ok\n",
        &[
            indoc! {r#"
                {{- $x := "ok" }}{{ $next := .Values.bad }}
                {{- range 64 }}
                  {{- $_ := upper $x }}{{ $x = $next }}{{ $next = "ok" }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[("second iteration aborts", json!({"bad": 7}))],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("all iterations render", json!({"bad": "ok"}))],
        true,
    )
}

/// An open-source merge changes a late input map even when neither map has explicit fields.
#[test]
fn a_late_layered_remainder_reaches_the_next_iteration() -> eyre::Result<()> {
    let schema = template_law_schema(
        indoc! {"
            cfg: {}
            source: {}
            label: ok
        "},
        &[
            indoc! {r#"
                {{- range 64 }}
                  {{- if hasKey $.Values.cfg "carry" }}{{ $_ := upper $.Values.label }}{{ end }}
                  {{- $_ := mergeOverwrite $.Values.cfg $.Values.source }}
                {{- end }}
            "#},
            CONFIG_MAP,
        ],
        "",
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "the second iteration consumes label",
            json!({"cfg": {}, "source": {"carry": true}, "label": 7}),
        )],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[(
            "an empty source never adds carry",
            json!({"cfg": {}, "source": {}, "label": 7}),
        )],
        true,
    )
}

/// Lane C step 3 law set (Astra item 3): the release name before a labels splice. Helm validates
/// the name (1 to 53 bytes of its DNS grammar) before rendering, so a quoted name closes its
/// scalar, `trunc 63 | trimSuffix "-"` leaves it unchanged, and the splice verdicts follow
/// (receipts `transfer/receipts-lanec3/release-{quoted,raw}`, Helm 4.2.3).
#[test]
#[ignore = "waiting for step 5b: needs the initial Release fields once per render"]
fn a_quoted_release_name_peer_keeps_the_splice_verdicts() -> eyre::Result<()> {
    let schema = spliced_labels_schema(
        "commonLabels: {}\n",
        indoc! {r"
            helm.sh/chart: law
            instance: {{ .Release.Name | quote }}"},
    )?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

#[test]
#[ignore = "waiting for step 5b: needs the initial Release fields once per render"]
fn a_truncated_release_name_peer_keeps_the_splice_verdicts() -> eyre::Result<()> {
    let schema = spliced_labels_schema(
        "commonLabels: {}\n",
        indoc! {r#"
            helm.sh/chart: law
            name: {{ .Release.Name | trunc 63 | trimSuffix "-" }}"#},
    )?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

/// The datadog fullname arm: the name joined with the chart name, truncated and trimmed as a
/// whole, inside double quotes (receipt `transfer/receipts-lanec3/release-printf`).
#[test]
#[ignore = "waiting for step 5b: needs the initial Release fields once per render"]
fn a_composed_release_name_peer_keeps_the_splice_verdicts() -> eyre::Result<()> {
    let schema = spliced_labels_schema(
        "commonLabels: {}\n",
        indoc! {r#"
            helm.sh/chart: law
            name: "{{ printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" }}""#},
    )?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

/// A parent chart `graph` with one dependency `child`: each body precedes a `ConfigMap`. Helm runs
/// the child's file first (deeper paths first), and both see one shared Release map.
fn release_graph_schema(
    parent_body: &str,
    child_body: &str,
    child_values: &str,
) -> eyre::Result<Value> {
    let parent = formatdoc! {"
        {parent_body}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: graph
    "};
    let child = formatdoc! {"
        {child_body}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: child
    "};
    law_schema(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: graph
                version: 0.1.0
                dependencies:
                  - name: child
                    version: 0.1.0
            "},
        ),
        ("values.yaml", "{}\n"),
        ("templates/root.yaml", &parent),
        (
            "charts/child/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: child
                version: 0.1.0
            "},
        ),
        ("charts/child/values.yaml", child_values),
        ("charts/child/templates/child.yaml", &child),
    ])
}

fn child_raw(raw: Value) -> Value {
    let mut document = json!({"child": {"raw": null, "global": {}}});
    document["child"]["raw"] = raw;
    document
}

/// A child's write to `.Release`, directly or through an alias, reaches the parent's later read:
/// `upper` of the written number aborts (receipts
/// `transfer/receipts-lanec3/release-write-{direct,shallow,pick}`).
#[test]
#[ignore = "lane C step 3: chart instances share one Release object in Helm file order (step 5 shared heap)"]
fn a_child_write_to_release_reaches_the_parent_read() -> eyre::Result<()> {
    for child in [
        r#"{{ $_ := set .Release "Service" .Values.raw }}"#,
        r#"{{ $r := .Release }}{{ $_ := set $r "Service" .Values.raw }}"#,
        r#"{{ $r := (pick $ "Release").Release }}{{ $_ := set $r "Service" .Values.raw }}"#,
    ] {
        let schema =
            release_graph_schema("{{ $_ := upper .Release.Service }}", child, "raw: ok\n")?;
        assert_helm_verdicts(&schema, &[("integer", child_raw(json!(7)))], false)?;
        assert_helm_verdicts(&schema, &[("string", child_raw(json!("text")))], true)?;
    }
    Ok(())
}

/// A write to a deep copy, or a rebinding of the child's own root slot, leaves the shared map
/// alone (receipts `transfer/receipts-lanec3/release-write-{deep,rebind}`).
#[test]
fn a_child_write_to_a_release_copy_stays_in_the_child() -> eyre::Result<()> {
    for child in [
        r#"{{ $r := deepCopy .Release }}{{ $_ := set $r "Service" .Values.raw }}"#,
        r#"{{ $_ := set $ "Release" (dict "Service" .Values.raw) }}"#,
    ] {
        let schema =
            release_graph_schema("{{ $_ := upper .Release.Service }}", child, "raw: ok\n")?;
        assert_helm_verdicts(
            &schema,
            &[
                ("integer", child_raw(json!(7))),
                ("string", child_raw(json!("text"))),
            ],
            true,
        )?;
    }
    Ok(())
}

fn release_tpl_schema() -> eyre::Result<Value> {
    release_graph_schema(
        r#"{{ if kindIs "string" .Release.Name }}{{ fail "name is still text" }}{{ end }}"#,
        "{{ tpl .Values.program $ }}",
        "program: \"\"\n",
    )
}

fn child_program(program: &str) -> Value {
    json!({"child": {"program": program, "global": {}}})
}

/// An earlier child runs a values-supplied `tpl` program against `$`: a program that renames the
/// release reaches the parent's read, so the name is unknown there (receipt
/// `transfer/receipts-lanec3/release-tpl-opaque`, `renames` exit 0).
#[test]
fn an_opaque_child_program_leaves_the_release_name_unknown() -> eyre::Result<()> {
    assert_helm_verdicts(
        &release_tpl_schema()?,
        &[(
            "renames",
            child_program(r#"{{ $_ := set .Release "Name" 7 }}"#),
        )],
        true,
    )
}

/// An action-free program writes nothing, so the parent still sees the validated name and fails
/// (same receipt, `action-free` exit 1).
#[test]
#[ignore = "lane C step 3: chart instances share one Release object in Helm file order (step 5 shared heap)"]
fn an_action_free_child_program_keeps_the_release_name() -> eyre::Result<()> {
    assert_helm_verdicts(
        &release_tpl_schema()?,
        &[("action-free", child_program(""))],
        false,
    )
}

/// The longhorn shape: a raw `.Release.Namespace` line before an image whose registry is a
/// coalesced value printed into a plain scalar. The initial namespace is an installable RFC 1123
/// label (ADR 0072), so it keeps the mapping and the image verdicts follow. `prefix` precedes
/// the `ConfigMap` (receipts `transfer/receipts-lanec3/namespace-{raw,override,write}`, Helm 4.2.3
/// under 1- and 63-byte namespaces).
fn namespace_image_schema(prefix: &str, namespace: &str) -> eyre::Result<Value> {
    let template = formatdoc! {r"
        {prefix}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: x
          namespace: {namespace}
        data:
          image: {{{{ with (coalesce .Values.global.imageRegistry .Values.image.registry) }}}}{{{{ . }}}}/{{{{ end }}}}{{{{ .Values.image.repository }}}}:{{{{ .Values.image.tag }}}}
    "};
    law_schema(&[
        ("Chart.yaml", CHART),
        (
            "values.yaml",
            indoc! {r#"
                global:
                  imageRegistry: ""
                namespaceOverride: ""
                ns: ""
                image:
                  registry: ""
                  repository: longhornio/manager
                  tag: v1
            "#},
        ),
        ("templates/d.yaml", &template),
    ])
}

fn namespace_document(registry: Value, extra: &Value) -> eyre::Result<Value> {
    let mut document = json!({
        "global": {"imageRegistry": null},
        "namespaceOverride": "",
        "ns": "",
        "image": {"registry": "", "repository": "longhornio/manager", "tag": "v1"},
    });
    document["global"]["imageRegistry"] = registry;
    let fields = document
        .as_object_mut()
        .ok_or_eyre("a values document is a mapping")?;
    for (key, value) in extra.as_object().ok_or_eyre("extra fields are a mapping")? {
        fields.insert(key.clone(), value.clone());
    }
    Ok(document)
}

#[test]
fn a_raw_release_namespace_keeps_the_image_verdicts() -> eyre::Result<()> {
    let schema = namespace_image_schema("", "{{ .Release.Namespace }}")?;
    let none = json!({});
    assert_helm_verdicts(
        &schema,
        &[("string", namespace_document(json!("reg.example"), &none)?)],
        true,
    )
}

/// The installable namespace never opens a flow token, so a list registry after it still fails.
#[test]
#[ignore = "waiting for step 5b: needs the initial Release fields once per render"]
fn an_initial_release_namespace_keeps_the_image_rejections() -> eyre::Result<()> {
    let none = json!({});
    for namespace in [
        "{{ .Release.Namespace }}",
        "{{ default .Release.Namespace .Values.namespaceOverride }}",
    ] {
        let schema = namespace_image_schema("", namespace)?;
        assert_helm_verdicts(
            &schema,
            &[("list", namespace_document(json!([{}]), &none)?)],
            false,
        )?;
    }
    Ok(())
}

/// A truthy override prints values text instead, which keeps its own input semantics.
#[test]
fn a_namespace_override_keeps_the_image_verdicts() -> eyre::Result<()> {
    let schema = namespace_image_schema(
        "",
        "{{ default .Release.Namespace .Values.namespaceOverride }}",
    )?;
    let none = json!({});
    let team = json!({"namespaceOverride": "team-a"});
    // A truthy override may change the YAML layout (receipt `namespace-override-layout`).
    let layout = json!({"namespaceOverride": "x\n  extra: y"});
    assert_helm_verdicts(
        &schema,
        &[("list override", namespace_document(json!([{}]), &team)?)],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("string", namespace_document(json!("reg.example"), &none)?),
            (
                "string override",
                namespace_document(json!("reg.example"), &team)?,
            ),
            (
                "string layout",
                namespace_document(json!("reg.example"), &layout)?,
            ),
        ],
        true,
    )
}

/// A write before the read replaces the initial namespace with values text.
#[test]
fn a_written_release_namespace_keeps_the_image_verdicts() -> eyre::Result<()> {
    let schema = namespace_image_schema(
        r#"{{- $_ := set .Release "Namespace" .Values.ns }}"#,
        "{{ .Release.Namespace }}",
    )?;
    let team = json!({"ns": "team-a"});
    let colon = json!({"ns": "a: b"});
    assert_helm_verdicts(
        &schema,
        &[
            ("list", namespace_document(json!([{}]), &team)?),
            (
                "string colon",
                namespace_document(json!("reg.example"), &colon)?,
            ),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[("string", namespace_document(json!("reg.example"), &team)?)],
        true,
    )
}

/// A chart writes a member of the one Release map; the name it set before is still read after it
/// (receipt `transfer/receipts-lanec3/release-own-write-literal-seeded`).
#[test]
#[ignore = "waiting for step 5b: a helper's writes to the entry Release map (unknown fields) do not yet keep a seeded name precise"]
fn a_chart_write_to_another_release_member_keeps_the_name() -> eyre::Result<()> {
    let schema = spliced_labels_schema(
        "commonLabels: {}\n",
        indoc! {r#"
            {{- $_ := set .Release "Name" "law" }}
            {{- $_ := set .Release "temp_vars" dict }}
            {{- if not (index .Release "temp_vars") }}{{ $_ := set .Release "temp_vars" dict }}{{ end }}
            {{- if not (index .Release.temp_vars "password") }}{{ $_ := set .Release.temp_vars "password" "generated" }}{{ end }}
            secret: {{ index .Release.temp_vars "password" }}
            instance: {{ .Release.Name | quote }}"#},
    )?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

/// jira's exact shape keys the member by a text built from the name (receipt
/// `transfer/receipts-lanec3/release-own-write`).
#[test]
#[ignore = "a read through a computed map key after a write through the same key is unknown (map key identity, not Release)"]
fn a_chart_write_through_a_computed_release_key_keeps_the_name() -> eyre::Result<()> {
    let schema = spliced_labels_schema(
        "commonLabels: {}\n",
        indoc! {r#"
            {{- if not (index .Release "temp_vars") }}{{ $_ := set .Release "temp_vars" dict }}{{ end }}
            {{- $key := printf "%s_%s" .Release.Name "password" }}
            {{- if not (index .Release.temp_vars $key) }}{{ $_ := set .Release.temp_vars $key "generated" }}{{ end }}
            secret: {{ index .Release.temp_vars $key }}
            instance: {{ .Release.Name | quote }}"#},
    )?;
    assert_labels_splice_verdicts(&schema, &json!({}))
}

/// A chart's own write to the name reaches its later read (receipt
/// `transfer/receipts-lanec3/release-name-write`).
#[test]
fn a_chart_write_to_the_release_name_reaches_its_read() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "name: x\n"),
        (
            "templates/cm.yaml",
            &[
                r#"{{- $_ := set .Release "Name" .Values.name }}{{ $_ := upper .Release.Name }}"#,
                "\n",
                CONFIG_MAP,
            ]
            .concat(),
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("integer", json!({"name": 7}))], false)?;
    assert_helm_verdicts(&schema, &[("string", json!({"name": "text"}))], true)
}

/// The datadog fullname helper: `printf "%s-%s"` of the name and the chart name (or an override),
/// truncated and trimmed as a whole, in double quotes and plain. Without an override the text is at
/// most 53 + 1 + 3 bytes and ends in the chart name's last letter, so both edits are identities
/// (receipt `transfer/receipts-lanec3/release-fullname`).
fn release_fullname_schema() -> eyre::Result<Value> {
    law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "commonLabels: {}\nnameOverride: \"\"\n"),
        (
            "templates/_helpers.tpl",
            indoc! {r#"
                {{- define "law.fullname" -}}
                {{- $name := default .Chart.Name .Values.nameOverride -}}
                {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
                {{- end -}}
                {{- define "law.labels" -}}
                helm.sh/chart: law
                app: "{{ include "law.fullname" . }}"
                name: {{ include "law.fullname" . }}
                {{- if .Values.commonLabels }}
                {{ toYaml .Values.commonLabels }}
                {{- end }}
                {{- end -}}
            "#},
        ),
        (
            "templates/cm.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
                  labels:
                {{ include "law.labels" . | indent 4 }}
            "#},
        ),
    ])
}

#[test]
#[ignore = "waiting for step 5b: needs the initial Release fields once per render"]
fn a_composed_fullname_without_override_keeps_the_splice_verdicts() -> eyre::Result<()> {
    assert_labels_splice_verdicts(&release_fullname_schema()?, &json!({"nameOverride": ""}))
}

/// An override is values text of any length: the edits are not identities there, and a labels
/// map still renders.
#[test]
fn a_composed_fullname_with_override_accepts_a_labels_map() -> eyre::Result<()> {
    assert_helm_verdicts(
        &release_fullname_schema()?,
        &[(
            "labels override",
            json!({"nameOverride": "custom", "commonLabels": {"team": "dev"}}),
        )],
        true,
    )
}

/// `trimSuffix` applies to the whole printed text, never to one part: `a-` followed by `b` is
/// `a-b`, so the comparison with `ab` never fails (receipt `transfer/receipts-lanec3/cross-part-trim`).
#[test]
fn a_whole_text_trim_does_not_apply_to_one_part() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "b: \"\"\n"),
        (
            "templates/cm.yaml",
            &[
                r#"{{- if eq (printf "a-%s" .Values.b | trimSuffix "-") "ab" }}{{ fail "joined across parts" }}{{ end }}"#,
                "\n",
                CONFIG_MAP,
            ]
            .concat(),
        ),
    ])?;
    assert_helm_verdicts(
        &schema,
        &[
            ("b", json!({"b": "b"})),
            ("empty", json!({"b": ""})),
            ("dash", json!({"b": "-"})),
            ("b dash", json!({"b": "b-"})),
        ],
        true,
    )
}

/// An earlier `tpl` of values text in the same chart may rename the release the chart named first;
/// the image line that renders only for a text name is rejected exactly when the program is
/// action-free (receipt `transfer/receipts-lanec3/tpl-same-chart-image-seeded`).
#[test]
#[ignore = "waiting for read-identity-preserving effects (ADR 0072 §5)"]
fn a_same_chart_program_keeps_the_image_only_when_action_free() -> eyre::Result<()> {
    let schema = image_schema(
        "{{- $_ := set .Release \"Name\" \"law\" }}\n{{- $_ := tpl (toYaml .Values.x) $ }}\n",
        "  {{- if kindIs \"string\" .Release.Name }}\n",
        "  {{- end }}\n",
    )?;
    let renames = r#"{{ $_ := set .Release "Name" 7 }}"#;
    let document = |registry: Value, x: Value| image_document(registry, &json!({"x": x}));
    assert_helm_verdicts(
        &schema,
        &[
            ("action-free", document(json!([{}]), json!("plain"))?),
            (
                "action-free map",
                document(json!([{}]), json!({"a": ["plain"], "b": {}}))?,
            ),
            (
                "brace space",
                document(json!([{}]), json!("{ {not an action}"))?,
            ),
        ],
        false,
    )?;
    assert_helm_verdicts(
        &schema,
        &[
            ("renames", document(json!([{}]), json!(renames))?),
            (
                "renames nested",
                document(json!([{}]), json!({"a": [renames]}))?,
            ),
            ("string", document(json!("reg.example"), json!("plain"))?),
        ],
        true,
    )
}

/// A choice whose only text arm lies beyond the layout's search bound still lays out as a choice
/// (or widens), never as one scalar token: `kz: v` is a mapping entry (receipt
/// `transfer/receipts-lanec3/deep-text-choice`, 70 nested arms).
#[test]
#[ignore = "a frozen choice whose arm is YAML text is one layout hole, so `kz: v` reads as one plain scalar (also on the line)"]
fn a_deep_text_arm_is_not_read_as_a_scalar() -> eyre::Result<()> {
    let arms = 70;
    let mut values = String::from("k: \"\"\n");
    let mut template = String::from(indoc! {r#"
        {{- define "law.text" -}}
        kz: v
        {{- end -}}
        {{- $v := include "law.text" . }}
    "#});
    for arm in 1..=arms {
        let keyword = if arm == 1 { "if" } else { "else if" };
        writeln!(values, "s{arm}: \"\"")?;
        writeln!(
            template,
            r#"{{{{- {keyword} eq .Values.k "{arm}" }}}}{{{{ $v = .Values.s{arm} }}}}"#
        )?;
    }
    template.push_str(indoc! {"
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: x
        data:
          {{ $v }}
    "});
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", &values),
        ("templates/cm.yaml", &template),
    ])?;
    assert_helm_verdicts(&schema, &[("text arm", json!({"k": "none"}))], true)
}

fn release_alias_schema() -> eyre::Result<Value> {
    release_graph_schema(
        r#"{{ $_ := set .Values.child.r "Service" "changed" }}{{ if eq .Release.Service "Helm" }}{{ fail "release not shared" }}{{ end }}"#,
        r#"{{ if .Values.export }}{{ $_ := set .Values "r" .Release }}{{ end }}"#,
        "r: {}\nexport: true\n",
    )
}

fn child_export(export: bool) -> Value {
    json!({"child": {"r": {}, "export": export, "global": {}}})
}

/// An earlier child exports an alias of the one Release map into its values without writing
/// the map; the parent writes through the alias and reads the change (Astra's counterexample to a
/// per-instance hand-off; receipt `transfer/receipts-lanec3/release-alias-export`, exit 0).
#[test]
fn a_release_alias_exported_by_a_child_reaches_the_parent() -> eyre::Result<()> {
    assert_helm_verdicts(
        &release_alias_schema()?,
        &[("export", child_export(true))],
        true,
    )
}

/// Without the alias the parent still reads Helm's service and fails (same receipt, exit 1).
#[test]
#[ignore = "lane C step 3: chart instances share one Release object in Helm file order (step 5 shared heap)"]
fn without_an_exported_alias_the_parent_reads_the_initial_service() -> eyre::Result<()> {
    assert_helm_verdicts(
        &release_alias_schema()?,
        &[("no export", child_export(false))],
        false,
    )
}

/// The longhorn image line: the registry is the first truthy of the values registry, a helper's
/// text and a component registry, printed under `with`. A list registry is chosen and prints as
/// `[map[]]/`, which Helm's YAML parse rejects (receipt
/// `transfer/receipts-lanec3/coalesce-helper-text`); printed without `with` the line is rejected.
#[test]
#[ignore = "the `with` test over a coalesce with a helper-text operand is partial, and layout choices drop partial guards"]
fn a_coalesced_list_behind_a_helper_text_operand_is_rejected() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        (
            "values.yaml",
            "global:\n  imageRegistry: \"\"\nprivate: \"\"\nimage:\n  registry: \"\"\n",
        ),
        (
            "templates/_helpers.tpl",
            indoc! {r#"
                {{- define "law.url" -}}
                {{- if .Values.private -}}
                {{- .Values.private -}}
                {{- end -}}
                {{- end -}}
            "#},
        ),
        (
            "templates/d.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
                data:
                  image: {{ with (coalesce .Values.global.imageRegistry (include "law.url" .) .Values.image.registry) }}{{ . }}/{{ end }}x
            "#},
        ),
    ])?;
    let document = |registry: Value| json!({"global": {"imageRegistry": registry}, "private": "", "image": {"registry": ""}});
    assert_helm_verdicts(&schema, &[("list", document(json!([{}])))], false)?;
    assert_helm_verdicts(&schema, &[("string", document(json!("reg.example")))], true)
}

const INITIAL_SERVICE_FAILS: &str =
    "{{ if eq .Release.Service \"Helm\" }}{{ fail \"initial service\" }}{{ end }}\n";

/// Helm executes every non-partial template, `templates/z.tpl` included, even when it emits
/// nothing; it runs before `templates/a.yaml`, so its write reaches the read (receipt
/// `transfer/receipts-lanec3/unscheduled-tpl`, exit 0).
#[test]
fn an_unscheduled_template_write_reaches_a_later_read() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "x: 1\n"),
        (
            "templates/z.tpl",
            "{{- $_ := set .Release \"Service\" \"changed\" -}}\n",
        ),
        (
            "templates/a.yaml",
            &[INITIAL_SERVICE_FAILS, CONFIG_MAP].concat(),
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("defaults", json!({"x": 1}))], true)
}

/// A test template runs first (deeper path) and its output is filtered only after rendering, so
/// its write reaches the read even when tests are excluded (receipt
/// `transfer/receipts-lanec3/unscheduled-test`, exit 0 with and without `--skip-tests`).
#[test]
fn an_excluded_test_template_write_reaches_a_later_read() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "x: 1\n"),
        (
            "templates/tests/t.yaml",
            indoc! {r#"
                {{- $_ := set .Release "Service" "changed" -}}
                apiVersion: v1
                kind: Pod
                metadata:
                  name: t
                  annotations:
                    helm.sh/hook: test
                spec:
                  containers:
                  - name: t
                    image: busybox
            "#},
        ),
        (
            "templates/a.yaml",
            &[INITIAL_SERVICE_FAILS, CONFIG_MAP].concat(),
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("defaults", json!({"x": 1}))], true)
}

/// Helm parses only a library chart's partials: the packaged library's `_helpers.tpl` defines the
/// helper that runs, not its `0.tpl`, so the write reaches the read through `$` and through
/// `.Subcharts.lib` (receipt `transfer/receipts-lanec3/library-shadow`, both exit 0).
#[test]
fn a_library_helper_is_the_partial_helm_parses() -> eyre::Result<()> {
    for packaged in [true, false] {
        let schema = library_shadow_schema(packaged)?;
        assert_helm_verdicts(
            &schema,
            &[
                ("root", json!({"via": "root"})),
                ("subcharts", json!({"via": "subcharts"})),
            ],
            true,
        )?;
    }
    Ok(())
}

fn library_shadow_schema(packaged: bool) -> eyre::Result<Value> {
    let library: &[(&str, &str)] = &[
        (
            "Chart.yaml",
            "apiVersion: v2\nname: lib\nversion: 0.1.0\ntype: library\n",
        ),
        (
            "templates/_helpers.tpl",
            "{{- define \"lib.touch\" -}}{{- $_ := set .Release \"Service\" \"changed\" -}}{{- end -}}\n",
        ),
        (
            "templates/0.tpl",
            "{{- define \"lib.touch\" -}}{{- $_ := \"unchanged\" -}}{{- end -}}\n",
        ),
    ];
    let template = [
        r#"{{- if eq .Values.via "subcharts" }}{{ include "lib.touch" .Subcharts.lib }}{{ else }}{{ include "lib.touch" $ }}{{ end }}"#,
        "\n",
        INITIAL_SERVICE_FAILS,
        CONFIG_MAP,
    ]
    .concat();
    let mut files = vec![
        (
            "Chart.yaml".to_owned(),
            indoc! {"
                apiVersion: v2
                name: law
                version: 0.1.0
                dependencies:
                  - name: lib
                    version: 0.1.0
            "}
            .to_owned(),
        ),
        ("values.yaml".to_owned(), "via: root\n".to_owned()),
        ("templates/a.yaml".to_owned(), template),
    ];
    let archived: Vec<_> = library
        .iter()
        .map(|(name, source)| (format!("lib/{name}"), *source))
        .collect();
    if !packaged {
        files.extend(
            library
                .iter()
                .map(|(name, source)| (format!("charts/lib/{name}"), (*source).to_owned())),
        );
    }
    let files: Vec<_> = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    let archived: Vec<_> = archived
        .iter()
        .map(|(path, source)| (path.as_str(), *source))
        .collect();
    if packaged {
        law_schema_with_archives(&files, &[("charts/lib-0.1.0.tgz", &archived)])
    } else {
        law_schema(&files)
    }
}

fn disabled_writer_schema() -> eyre::Result<Value> {
    let child: &[(&str, &str)] = &[
        (
            "child/Chart.yaml",
            "apiVersion: v2\nname: child\nversion: 0.1.0\n",
        ),
        ("child/values.yaml", "{}\n"),
        (
            "child/templates/c.yaml",
            "{{- $_ := set .Release \"Service\" \"changed\" -}}\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: child\n",
        ),
    ];
    law_schema_with_archives(
        &[
            (
                "Chart.yaml",
                indoc! {"
                    apiVersion: v2
                    name: law
                    version: 0.1.0
                    dependencies:
                      - name: child
                        version: 0.1.0
                        condition: child.enabled
                "},
            ),
            ("values.yaml", "child:\n  enabled: false\n"),
            (
                "templates/a.yaml",
                &[INITIAL_SERVICE_FAILS, CONFIG_MAP].concat(),
            ),
        ],
        &[("charts/child-0.1.0.tgz", child)],
    )
}

/// A conditionally disabled packaged dependency counts as a chart instance: when enabled, its
/// earlier write reaches the parent's read (receipt `transfer/receipts-lanec3/disabled-tgz-writer`,
/// enabled exit 0).
#[test]
fn an_enabled_packaged_writer_reaches_the_parent_read() -> eyre::Result<()> {
    assert_helm_verdicts(
        &disabled_writer_schema()?,
        &[("enabled", json!({"child": {"enabled": true, "global": {}}}))],
        true,
    )
}

/// Disabled, the parent alone renders and reads the initial service (same receipt, exit 1).
#[test]
#[ignore = "lane C: every discovered instance counts, so a disabled dependency keeps Release shared (sound, imprecise)"]
fn a_disabled_packaged_writer_leaves_the_initial_service() -> eyre::Result<()> {
    assert_helm_verdicts(
        &disabled_writer_schema()?,
        &[(
            "disabled",
            json!({"child": {"enabled": false, "global": {}}}),
        )],
        false,
    )
}

/// `trimPrefix` edits the whole text: an affix crossing the selected first part `%!` removes what
/// would otherwise be an invalid plain prefix (receipt `transfer/receipts-lanec3/trim-prefix-cross`,
/// both exit 0).
#[test]
fn a_whole_text_trim_prefix_crosses_the_first_part() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "c: false\n"),
        (
            "templates/d.yaml",
            &[
                CONFIG_MAP,
                "data:\n",
                r#"  v: {{ printf "%s%s" (ternary "%!" "a" .Values.c) "x-rest" | trimPrefix "%!x" }}"#,
                "\n",
            ]
            .concat(),
        ),
    ])?;
    assert_helm_verdicts(
        &schema,
        &[
            ("selected", json!({"c": true})),
            ("other", json!({"c": false})),
        ],
        true,
    )
}

/// Sprig's `trunc` slices bytes: `trunc -2` keeps the last two bytes of `abé`, and `trunc 0`
/// empties a nonempty text (receipt `transfer/receipts-lanec3/trunc-bytes`, exit 0).
#[test]
fn truncation_follows_byte_widths() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "x: 1\n"),
        (
            "templates/d.yaml",
            &[
                r#"{{ if ne ("abé" | trunc -2) "é" }}{{ fail "negative trunc keeps the last two bytes" }}{{ end }}"#,
                r#"{{ if ne (printf "%s-" .Release.Name | trunc 0) "" }}{{ fail "trunc 0 empties a nonempty text" }}{{ end }}"#,
                "\n",
                CONFIG_MAP,
                "data:\n",
                r#"  a: "{{ "abé" | trunc -2 }}""#,
                "\n",
                r#"  b: "{{ printf "%s-" .Release.Name | trunc 0 }}""#,
                "\n",
            ]
            .concat(),
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("defaults", json!({"x": 1}))], true)
}

/// Helm parses every template, test hooks included, and the shallower `templates/tests/_touch.tpl`
/// redefines the helper after `templates/deep/nested/_touch.tpl`; its write reaches the read even
/// when tests are excluded (receipt `transfer/receipts-lanec3/test-partial-shadow`, exit 0 with and
/// without `--skip-tests`).
#[test]
fn an_excluded_test_partial_defines_the_helper_helm_runs() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "x: 1\n"),
        (
            "templates/deep/nested/_touch.tpl",
            "{{- define \"touch\" -}}{{- end -}}\n",
        ),
        (
            "templates/tests/_touch.tpl",
            "{{- define \"touch\" -}}{{- $_ := set .Release \"Service\" \"changed\" -}}{{- end -}}\n",
        ),
        (
            "templates/a.yaml",
            &[
                "{{- include \"touch\" . }}\n",
                INITIAL_SERVICE_FAILS,
                CONFIG_MAP,
            ]
            .concat(),
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("defaults", json!({"x": 1}))], true)
}

/// A library partial whose name is not UTF-8 still defines the helper Helm runs: Helm parses
/// `templates/_touch-\xff.tpl` after the deeper no-op, so its write reaches the read (receipt
/// `transfer/receipts-lanec3/library-bytes-partial`, exit 0).
#[test]
fn a_library_partial_with_a_byte_name_keeps_release_shared() -> eyre::Result<()> {
    let library: ArchiveEntries<'_> = vec![
        (
            b"lib/Chart.yaml",
            "apiVersion: v2\nname: lib\nversion: 0.1.0\ntype: library\n",
        ),
        (
            b"lib/templates/deep/nested/_touch.tpl",
            "{{- define \"lib.touch\" -}}{{- end -}}\n",
        ),
        (
            b"lib/templates/_touch-\xff.tpl",
            "{{- define \"lib.touch\" -}}{{- $_ := set .Release \"Service\" \"changed\" -}}{{- end -}}\n",
        ),
    ];
    let schema = law_schema_with_byte_archives(
        &[
            (
                "Chart.yaml",
                indoc! {"
                    apiVersion: v2
                    name: law
                    version: 0.1.0
                    dependencies:
                      - name: lib
                        version: 0.1.0
                "},
            ),
            ("values.yaml", "x: 1\n"),
            (
                "templates/a.yaml",
                &[
                    "{{- include \"lib.touch\" . }}\n",
                    INITIAL_SERVICE_FAILS,
                    CONFIG_MAP,
                ]
                .concat(),
            ),
        ],
        &[("charts/lib-0.1.0.tgz", library)],
    )?;
    assert_helm_verdicts(&schema, &[("defaults", json!({"x": 1}))], true)
}

/// The acceptance controls of the release-name peer laws hold without the initial Release: a
/// labels map and falsy values render whatever the name is.
#[test]
fn release_name_peers_keep_the_splice_acceptances() -> eyre::Result<()> {
    for entries in [
        "helm.sh/chart: law\ninstance: {{ .Release.Name | quote }}",
        "helm.sh/chart: law\nname: {{ .Release.Name | trunc 63 | trimSuffix \"-\" }}",
        "helm.sh/chart: law\nname: \"{{ printf \"%s-%s\" .Release.Name .Chart.Name | trunc 63 | trimSuffix \"-\" }}\"",
    ] {
        let schema = spliced_labels_schema("commonLabels: {}\n", entries)?;
        assert_labels_splice_acceptances(&schema, &json!({}))?;
    }
    assert_labels_splice_acceptances(&release_fullname_schema()?, &json!({"nameOverride": ""}))
}

/// `trunc 3` splits `é` in `{{é`: `toYamlPretty` spells the invalid string `!!binary e3vD`, which
/// erases the opener, while `toYaml` encodes through JSON and keeps it (receipt
/// `transfer/receipts-lanec3/split-rune-yaml`: `pretty-split` and `plain-none` exit 0,
/// `plain-split` exit 1).
#[test]
#[ignore = "waiting for read-identity-preserving effects (ADR 0072 §5)"]
fn a_split_rune_keeps_an_opener_only_through_to_yaml() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "code: abc\nencoder: pretty\n"),
        (
            "templates/c.yaml",
            indoc! {r#"
                {{- if eq .Values.encoder "pretty" }}
                {{- if contains "{{" (toYamlPretty (trunc 3 .Values.code)) }}{{ fail "pretty opener" }}{{ end }}
                {{- else }}
                {{- if contains "{{" (toYaml (trunc 3 .Values.code)) }}{{ fail "plain opener" }}{{ end }}
                {{- end }}
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
            "#},
        ),
    ])?;
    assert_helm_verdicts(
        &schema,
        &[
            ("pretty-split", json!({"code": "{{é", "encoder": "pretty"})),
            ("plain-none", json!({"code": "abc", "encoder": "plain"})),
        ],
        true,
    )
}

/// The `toYaml` control of [`a_split_rune_keeps_an_opener_only_through_to_yaml`]: the opener a
/// truncated input keeps through JSON encoding fails the render (same receipt, `plain-split`).
#[test]
#[ignore = "waiting for read-identity-preserving effects (ADR 0072 §5)"]
fn a_split_rune_opener_through_to_yaml_fails_the_render() -> eyre::Result<()> {
    let schema = law_schema(&[
        ("Chart.yaml", CHART),
        ("values.yaml", "code: abc\n"),
        (
            "templates/c.yaml",
            indoc! {r#"
                {{- if contains "{{" (toYaml (trunc 3 .Values.code)) }}{{ fail "plain opener" }}{{ end }}
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: x
            "#},
        ),
    ])?;
    assert_helm_verdicts(&schema, &[("plain-split", json!({"code": "{{é"}))], false)
}
