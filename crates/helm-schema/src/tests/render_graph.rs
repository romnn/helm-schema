//! Laws for one Helm render across chart instances (ADR 0072 §8, ADR 0073 step 5).
//!
//! Helm 4.2.3 (`pkg/engine/engine.go:544-590`) gives every chart instance its own root map, copies
//! the one `Release` and `Capabilities` reference into each, binds a dependency's `.Values` to the
//! parent's values subtree, and executes the files of all instances in one schedule: descending
//! slash count, then reverse lexical order (`engine.go:497-523`). Each law is a parent chart
//! `graph` with one dependency `child` and the verdict real Helm gives each coalesced document
//! (`helm template release-name <chart> -f <case> --kube-version 1.29.0`). The receipts (chart,
//! user values, output or first error, exit code) are kept under the step-5b lane's
//! `transfer/receipts-s5b/<law>/`.

use color_eyre::eyre;
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use std::fmt::Write as _;

use test_util::prelude::sim_assert_eq;

use super::r3b_laws::law_schema;

/// The final schema for `graph` with the dependency `child`, gated on `child.enabled` when
/// `condition` holds. Each file body is followed by a `ConfigMap` named after the file.
fn graph_schema(
    parent_values: &str,
    child_values: &str,
    files: &[(&str, &str)],
    condition: bool,
) -> eyre::Result<Value> {
    let condition = condition.then_some("child.enabled");
    siblings_schema(parent_values, &[("child", child_values, condition)], files)
}

/// The final schema for `graph` with the listed dependencies `(name, values, condition)`.
/// Each file body is followed by a `ConfigMap` named after the file.
fn siblings_schema(
    parent_values: &str,
    children: &[(&str, &str, Option<&str>)],
    files: &[(&str, &str)],
) -> eyre::Result<Value> {
    let mut dependencies = String::new();
    for (name, _, condition) in children {
        let condition = condition
            .map(|condition| format!("\n    condition: {condition}"))
            .unwrap_or_default();
        writeln!(
            dependencies,
            "  - name: {name}\n    version: 0.1.0{condition}"
        )?;
    }
    let metadata =
        format!("apiVersion: v2\nname: graph\nversion: 0.1.0\ndependencies:\n{dependencies}");
    let child_files = children
        .iter()
        .map(|(name, values, _)| {
            (
                format!("charts/{name}/Chart.yaml"),
                format!("apiVersion: v2\nname: {name}\nversion: 0.1.0\n"),
                format!("charts/{name}/values.yaml"),
                (*values).to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let templates = files
        .iter()
        .map(|(path, body)| {
            let name = path
                .rsplit('/')
                .next()
                .and_then(|file| file.split('.').next())
                .unwrap_or("law");
            (
                *path,
                formatdoc! {"
                    {body}
                    apiVersion: v1
                    kind: ConfigMap
                    metadata:
                      name: {name}
                "},
            )
        })
        .collect::<Vec<_>>();
    let mut chart = vec![
        ("Chart.yaml", metadata.as_str()),
        ("values.yaml", parent_values),
    ];
    for (metadata_path, metadata, values_path, values) in &child_files {
        chart.push((metadata_path.as_str(), metadata.as_str()));
        chart.push((values_path.as_str(), values.as_str()));
    }
    chart.extend(templates.iter().map(|(path, body)| (*path, body.as_str())));
    law_schema(&chart)
}

/// Asserts the schema's verdict on each coalesced document equals Helm's.
fn assert_verdicts(schema: &Value, cells: &[(&str, Value, bool)]) -> eyre::Result<()> {
    let validator = jsonschema::validator_for(schema)?;
    let have = cells
        .iter()
        .map(|(label, document, _)| (*label, validator.is_valid(document)))
        .collect::<Vec<_>>();
    let want = cells
        .iter()
        .map(|(label, _, renders)| (*label, *renders))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: want, "{schema:#}");
    Ok(())
}

/// A child's write lands between two parent files: the deep parent file runs first, the child
/// second and the shallow parent file last. Helm renders with the child enabled and fails on
/// `early` with it disabled (receipts `order/`).
#[test]
fn a_child_write_between_parent_files_follows_the_helm_schedule() -> eyre::Result<()> {
    let schema = graph_schema(
        "{}\n",
        "{}\n",
        &[
            (
                "templates/a/b/c/deep.yaml",
                r#"{{ $_ := set .Release "Service" "early" }}"#,
            ),
            (
                "charts/child/templates/child.yaml",
                r#"{{ $_ := set .Release "Service" "middle" }}"#,
            ),
            (
                "templates/root.yaml",
                r#"{{ if eq .Release.Service "early" }}{{ fail "early" }}{{ end }}"#,
            ),
        ],
        true,
    )?;
    assert_verdicts(
        &schema,
        &[
            ("enabled", json!({"child": {"global": {}}}), true),
            ("disabled", json!({"child": {"enabled": false}}), false),
        ],
    )
}

/// A child stores the shared Release map in its values; the parent writes through
/// `.Values.child.r` and observes the write in `.Release`. Without the export the parent's
/// check fails (receipts `alias-export/`).
#[test]
fn a_release_alias_exported_through_child_values_reaches_the_parent() -> eyre::Result<()> {
    let schema = graph_schema(
        "{}\n",
        indoc! {"
            export: true
            r: {}
        "},
        &[
            (
                "charts/child/templates/child.yaml",
                r#"{{ if .Values.export }}{{ $_ := set .Values "r" .Release }}{{ end }}"#,
            ),
            (
                "templates/root.yaml",
                r#"{{ $_ := set .Values.child.r "Service" "changed" }}{{ if eq .Release.Service "Helm" }}{{ fail "not shared" }}{{ end }}"#,
            ),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "export",
                json!({"child": {"export": true, "r": {}, "global": {}}}),
                true,
            ),
            (
                "no export",
                json!({"child": {"export": false, "r": {}, "global": {}}}),
                false,
            ),
        ],
    )
}

/// The child writes `.Values.raw` into Release through `write`; the parent's `upper
/// .Release.Service` demands a string at `child.raw`, never at the parent's own `raw`. A write
/// that reaches the shared map rejects a numeric `child.raw`; a deep copy or a rebinding of the
/// child's root slot leaves the shared map untouched (receipts `identity-<write>/`).
fn assert_release_write_identity(write: &str, shared: bool) -> eyre::Result<()> {
    let schema = graph_schema(
        "raw: ok\n",
        "raw: ok\n",
        &[
            ("charts/child/templates/child.yaml", write),
            ("templates/root.yaml", "{{ $_ := upper .Release.Service }}"),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "child integer",
                json!({"raw": "ok", "child": {"raw": 7, "global": {}}}),
                !shared,
            ),
            (
                "child string",
                json!({"raw": "ok", "child": {"raw": "s", "global": {}}}),
                true,
            ),
            (
                "parent integer",
                json!({"raw": 7, "child": {"raw": "s", "global": {}}}),
                true,
            ),
        ],
    )
}

#[test]
fn a_direct_child_release_write_places_the_parent_demand_at_the_child() -> eyre::Result<()> {
    assert_release_write_identity(r#"{{ $_ := set .Release "Service" .Values.raw }}"#, true)
}

#[test]
fn an_aliased_child_release_write_places_the_parent_demand_at_the_child() -> eyre::Result<()> {
    assert_release_write_identity(
        r#"{{ $r := .Release }}{{ $_ := set $r "Service" .Values.raw }}"#,
        true,
    )
}

#[test]
fn a_shallow_copied_root_keeps_the_shared_release() -> eyre::Result<()> {
    assert_release_write_identity(
        r#"{{ $ctx := merge (dict) $ }}{{ $_ := set $ctx.Release "Service" .Values.raw }}"#,
        true,
    )
}

#[test]
fn a_picked_root_keeps_the_shared_release() -> eyre::Result<()> {
    assert_release_write_identity(
        r#"{{ $_ := set (pick $ "Release").Release "Service" .Values.raw }}"#,
        true,
    )
}

#[test]
fn a_deep_copied_release_write_leaves_the_parent_unconstrained() -> eyre::Result<()> {
    assert_release_write_identity(
        r#"{{ $_ := set (deepCopy .Release) "Service" .Values.raw }}"#,
        false,
    )
}

#[test]
fn a_rebound_child_release_leaves_the_parent_unconstrained() -> eyre::Result<()> {
    assert_release_write_identity(
        r#"{{ $_ := set $ "Release" (dict "Service" .Values.raw) }}"#,
        false,
    )
}

/// An opaque child program receiving `$` may rename the release, so the parent's typed Name
/// holds only where the program is action-free (receipts `opaque-tpl/`).
///
/// Precision: the child's assumption is discharged when the parent instance begins, and the joined
/// Release name is a selection whose `if` test the branch leaves partial, so the parent's abort is
/// not claimed. Needs Select-first branch forking or guarded effects (ADR 0072 §5).
#[test]
#[ignore = "precision: needs Select-first branch forking / guarded effects, ADR 0072 §5"]
fn an_opaque_child_program_may_rename_the_release_the_parent_reads() -> eyre::Result<()> {
    let schema = graph_schema(
        "v: ok\n",
        "program: ''\n",
        &[
            (
                "charts/child/templates/child.yaml",
                "{{ $_ := tpl .Values.program $ }}",
            ),
            (
                "templates/root.yaml",
                "{{ if .Release.Name }}{{ $_ := upper .Values.v }}{{ end }}",
            ),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "renames",
                json!({"v": 7, "child": {"program": "{{ $_ := set .Release \"Name\" \"\" }}", "global": {}}}),
                true,
            ),
            (
                "action-free",
                json!({"v": 7, "child": {"program": "plain", "global": {}}}),
                false,
            ),
            (
                "string",
                json!({"v": "s", "child": {"program": "plain", "global": {}}}),
                true,
            ),
        ],
    )
}

/// The parent's own demand holds whether the child is enabled or not, and the child's abort
/// holds only while it is enabled (receipts `parent-demand/`).
#[test]
fn a_parent_demand_is_independent_of_the_child_activation() -> eyre::Result<()> {
    let schema = graph_schema(
        "v: ok\n",
        "strict: false\n",
        &[
            (
                "charts/child/templates/child.yaml",
                r#"{{ $_ := set .Release "Service" "child" }}{{ if .Values.strict }}{{ fail "strict" }}{{ end }}"#,
            ),
            ("templates/root.yaml", "{{ $_ := upper .Values.v }}"),
        ],
        true,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "integer enabled",
                json!({"v": 7, "child": {"strict": false, "global": {}}}),
                false,
            ),
            (
                "integer disabled",
                json!({"v": 7, "child": {"enabled": false}}),
                false,
            ),
            (
                "string enabled",
                json!({"v": "s", "child": {"strict": false, "global": {}}}),
                true,
            ),
            (
                "string disabled",
                json!({"v": "s", "child": {"enabled": false}}),
                true,
            ),
            (
                "strict enabled",
                json!({"v": "s", "child": {"strict": true, "global": {}}}),
                false,
            ),
            (
                "strict disabled",
                json!({"v": "s", "child": {"strict": true, "enabled": false}}),
                true,
            ),
        ],
    )
}

/// An unknown child program receives only the child's root, so it cannot reach the parent's own
/// values: the parent's demand holds whatever the program does (receipts
/// `child-writer-scope/`).
#[test]
fn an_unknown_child_writer_does_not_reach_the_parent_values() -> eyre::Result<()> {
    let writer = r#"{{ $_ := set .Release "Service" "w" }}{{ $_ := set $.Values "v" "s" }}"#;
    let schema = graph_schema(
        "v: ok\n",
        "x: {}\n",
        &[
            (
                "charts/child/templates/child.yaml",
                "{{ $_ := tpl (toYaml .Values.x) $ }}",
            ),
            ("templates/root.yaml", "{{ $_ := upper .Values.v }}"),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "integer under a writer",
                json!({"v": 7, "child": {"x": {"a": writer}, "global": {}}}),
                false,
            ),
            (
                "integer under plain text",
                json!({"v": 7, "child": {"x": {"a": "b"}, "global": {}}}),
                false,
            ),
            (
                "string under a writer",
                json!({"v": "s", "child": {"x": {"a": writer}, "global": {}}}),
                true,
            ),
        ],
    )
}

/// Helm's schedule runs the parent's deep `early` file, then the child, then the parent's `root`.
/// The parent's own program may rewrite `v` before `root` reads it, so the demand holds exactly
/// where that program is plain text; the child running in between changes nothing (receipts
/// `interleaved-assumption/`).
#[test]
fn a_parent_program_keeps_its_own_demand_across_an_interleaved_child() -> eyre::Result<()> {
    let schema = graph_schema(
        "v: ok\nprogram: \"\"\n",
        "x: \"\"\n",
        &[
            (
                "templates/a/b/early.yaml",
                "{{ $_ := tpl .Values.program $ }}",
            ),
            (
                "charts/child/templates/child.yaml",
                "{{ $_ := upper .Values.x }}",
            ),
            ("templates/root.yaml", "{{ $_ := upper .Values.v }}"),
        ],
        false,
    )?;
    let document = |v: Value, program: &str| json!({"v": v, "program": program, "child": {"x": "", "global": {}}});
    assert_verdicts(
        &schema,
        &[
            (
                "integer under plain text",
                document(json!(7), "plain"),
                false,
            ),
            (
                "integer under a writer",
                document(json!(7), r#"{{ $_ := set .Values "v" "s" }}"#),
                true,
            ),
            (
                "string under plain text",
                document(json!("s"), "plain"),
                true,
            ),
        ],
    )
}

/// A child's `tpl` of a values string, the shape of Bitnami's `common.tplvalues.render`, runs
/// against the child's root alone: the parent's member access and type demand survive it, with
/// plain text and with a writer alike (receipts `text-writer/`).
#[test]
fn an_unknown_child_text_program_does_not_reach_the_parent_values() -> eyre::Result<()> {
    let writer = r#"{{ $_ := set .Release "Service" "w" }}{{ $_ := set $.Values "v" "s" }}"#;
    let schema = graph_schema(
        "v: ok\na:\n  b:\n    enabled: false\n",
        "labels: \"\"\n",
        &[
            (
                "charts/child/templates/child.yaml",
                "{{ $_ := tpl .Values.labels $ }}",
            ),
            (
                "templates/root.yaml",
                "{{ $_ := upper .Values.v }}{{ $_ := .Values.a.b.enabled }}",
            ),
        ],
        false,
    )?;
    let document = |v: Value, b: Value, labels: &str| json!({"v": v, "a": {"b": b}, "child": {"labels": labels, "global": {}}});
    assert_verdicts(
        &schema,
        &[
            (
                "scalar member under a writer",
                document(json!("ok"), json!("x"), writer),
                false,
            ),
            (
                "scalar member under plain text",
                document(json!("ok"), json!("x"), "plain"),
                false,
            ),
            (
                "integer under a writer",
                document(json!(7), json!({"enabled": false}), writer),
                false,
            ),
            (
                "integer under plain text",
                document(json!(7), json!({"enabled": false}), "plain"),
                false,
            ),
            (
                "string under a writer",
                document(json!("s"), json!({"enabled": false}), writer),
                true,
            ),
        ],
    )
}

/// Every instance's Capabilities is one `*common.Capabilities` struct, which no template function
/// can write: Sprig's `set` on it fails with "wrong type for value; expected map[string]interface
/// {}". An unknown child program reaching it leaves the parent's non-nil `KubeVersion`, and the
/// demand under it, intact (receipts `capabilities-struct/` and `capabilities-struct-unbounded/`).
fn assert_capabilities_survive(program: &str) -> eyre::Result<()> {
    let writer = r#"{{ $_ := set .Release "Service" "w" }}"#;
    let schema = graph_schema(
        "v: ok\n",
        "x: {}\n",
        &[
            ("charts/child/templates/child.yaml", program),
            (
                "templates/root.yaml",
                "{{ if .Capabilities.KubeVersion }}{{ $_ := upper .Values.v }}{{ end }}",
            ),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "integer under a writer",
                json!({"v": 7, "child": {"x": {"a": writer}, "global": {}}}),
                false,
            ),
            (
                "integer under plain text",
                json!({"v": 7, "child": {"x": {"a": "b"}, "global": {}}}),
                false,
            ),
            (
                "string under a writer",
                json!({"v": "s", "child": {"x": {"a": writer}, "global": {}}}),
                true,
            ),
        ],
    )
}

#[test]
fn an_unknown_child_writer_leaves_the_shared_capabilities_typed() -> eyre::Result<()> {
    assert_capabilities_survive("{{ $_ := tpl (toYaml .Values.x) $ }}")
}

/// Sorting names read from a cluster lookup widens values of unknown reach, so the write may
/// reach any object of the child's own render; the shared host struct is still not one of them.
#[test]
fn an_unbounded_child_write_leaves_the_shared_capabilities_typed() -> eyre::Result<()> {
    assert_capabilities_survive(
        r#"{{ $names := list }}{{ range $item := (lookup "v1" "ConfigMap" "" "").items }}{{ $names = append $names $item.metadata.name }}{{ end }}{{ $_ := sortAlpha $names }}{{ $_ := tpl (toYaml .Values.x) $ }}"#,
    )
}

/// Runs the sibling-reach chart: `logstash` stores a dictionary on its own root and exports it
/// through the shared Release only when asked; `kibana` then runs an opaque program on its own
/// root; the parent reads the dictionary through `.Subcharts` (receipts `sibling-reach/`). Each
/// cell is `(label, v, export, Helm renders)` with the writing program as kibana's text.
fn assert_sibling_reach(cells: &[(&str, Value, bool, bool)]) -> eyre::Result<()> {
    let writer = r#"{{ if .Release.export }}{{ $_ := set .Release.export "v" "s" }}{{ end }}"#;
    let schema = siblings_schema(
        "{}\n",
        &[
            (
                "logstash",
                indoc! {"
                v: ok
                export: false
            "},
                None,
            ),
            ("kibana", "x: {}\n", None),
        ],
        &[
            (
                "charts/logstash/templates/store.yaml",
                r#"{{ $_ := set $ "d" (dict "v" .Values.v) }}{{ if .Values.export }}{{ $_ := set .Release "export" $.d }}{{ end }}"#,
            ),
            (
                "charts/kibana/templates/writer.yaml",
                "{{ $_ := tpl (toYaml .Values.x) $ }}",
            ),
            (
                "templates/root.yaml",
                "{{ $_ := upper .Subcharts.logstash.d.v }}",
            ),
        ],
    )?;
    let cells = cells
        .iter()
        .map(|(label, v, export, renders)| {
            let document = json!({
                "logstash": {"v": v, "export": export, "global": {}},
                "kibana": {"x": {"a": writer}, "global": {}},
            });
            (*label, document, *renders)
        })
        .collect::<Vec<_>>();
    assert_verdicts(&schema, &cells)
}

/// A sibling's unknown program may rewrite another sibling's object it reaches through a stored
/// alias: Helm 4.2.3 lets the program rewrite an exported integer, and a private string passes
/// whatever the program does. The receipt's exported integer under plain text fails, but a
/// serialized map's text is not known to be free of actions, so the program stays a possible
/// writer there.
#[test]
fn a_sibling_writer_may_rewrite_what_the_shared_release_exports() -> eyre::Result<()> {
    assert_sibling_reach(&[
        ("private string under a writer", json!("ok"), false, true),
        ("exported integer under a writer", json!(7), true, true),
    ])
}

/// A sibling's unknown program reaches another sibling's private object only through a stored
/// alias, so Helm 4.2.3 fails the private integer whatever the program does. Holding that needs
/// the writer's effect guarded by the export condition when the object is read (ADR 0072 §5).
#[test]
#[ignore = "step 5b precision: a conditional opaque write widens the whole object"]
fn a_sibling_writer_leaves_a_private_object_alone() -> eyre::Result<()> {
    assert_sibling_reach(&[("private integer under a writer", json!(7), false, false)])
}

/// A parent stores its own values map into Release, then Release loses its fields to the field
/// bound. An unknown child program reaching Release can rewrite the parent's `v` through that
/// lost reference, so the parent's later demand holds only where the program writes nothing
/// (receipts `release-held-values/`).
#[test]
fn a_values_map_held_by_a_widened_release_stays_reachable() -> eyre::Result<()> {
    let schema = graph_schema(
        "v: ok\n",
        "x: {}\n",
        &[
            (
                "templates/a/b/c/deep.yaml",
                r#"{{ $_ := set .Release "pv" .Values }}{{ range $i := until 130 }}{{ $_ := set $.Release (printf "k%d" $i) $i }}{{ end }}"#,
            ),
            (
                "charts/child/templates/child.yaml",
                "{{ $_ := tpl (toYaml .Values.x) $ }}",
            ),
            ("templates/root.yaml", "{{ $_ := upper .Values.v }}"),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "integer rewritten through Release",
                json!({"v": 7, "child": {"x": {"a": "{{ $_ := set .Release.pv \"v\" \"s\" }}"}, "global": {}}}),
                true,
            ),
            (
                "string under plain text",
                json!({"v": "s", "child": {"x": {"a": "b"}, "global": {}}}),
                true,
            ),
        ],
    )
}

/// The reduced ECK chart instances: kibana fails on a missing reference, the registry on a
/// missing version.
const KIBANA_HELPERS: &str = indoc! {r#"
                    {{- define "kibana.name" -}}
                    {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
                    {{- end }}
                    {{- define "kibana.fullname" -}}
                    {{- if .Values.fullnameOverride }}
                    {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
                    {{- $name := default .Chart.Name .Values.nameOverride }}
                    {{- if contains $name .Release.Name }}
                    {{- end }}
                    {{- end }}
                    {{- end }}
                    {{- define "kibana.chart" -}}
                    {{- end }}
                    {{- define "kibana.labels" -}}
                    {{ include "kibana.selectorLabels" . }}
                    {{- end }}
                    {{- define "kibana.selectorLabels" -}}
                    app.kubernetes.io/name: {{ include "kibana.name" . }}
                    {{- end }}
                "#};
const KIBANA_MANIFEST: &str = indoc! {r#"
                    apiVersion: v1
                    kind: ConfigMap
                    metadata:
                      name: {{ include "kibana.fullname" . }}
                      labels:
                        {{- include "kibana.labels" . | nindent 4 }}
                    data:
                      version: {{ required "version" .Values.version | quote }}
                      {{- $esRef := or ((.Values.spec).elasticsearchRef) (.Values.elasticsearchRef) }}
                      {{- if not (or ($esRef).name ($esRef).secretName) }}
                      {{- fail "An elasticsearchRef name or secretName is required" }}
                      {{- end }}
                "#};
const REGISTRY_HELPERS: &str = indoc! {r#"
                    {{- define "epr.name" -}}
                    {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
                    {{- define "epr.fullname" -}}
                    {{- if .Values.fullnameOverride }}
                    {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
                    {{- end }}
                    {{- end }}
                    {{- end }}
                    {{- define "epr.labels" -}}
                    {{ include "epr.selectorLabels" . }}
                    {{- end }}
                    {{- define "epr.selectorLabels" -}}
                    app.kubernetes.io/name: {{ include "epr.name" . }}
                    {{- end }}
                "#};
const REGISTRY_MANIFEST: &str = indoc! {r#"
                    apiVersion: v1
                    kind: ConfigMap
                    metadata:
                      name: {{ include "epr.fullname" . }}
                      labels:
                        {{- include "epr.labels" . | nindent 4 }}
                    data:
                      version: {{ required "version" .Values.version | quote }}
                "#};

/// Each instance's failures stand on their own. With a disabled registry's `required`, kibana's
/// own `fail`s and a disabled agent's `fail` in one render, kibana's extraObjects check still
/// rejects a document that sets it to a scalar: Helm fails at kibana's file whatever the agent
/// values are (receipts `instance-aborts/`, reduced from ECK).
#[test]
fn an_instance_failure_does_not_depend_on_a_later_instance_failure() -> eyre::Result<()> {
    let extra_objects = |message: &str| {
        format!(
            r#"{{{{- if .Values.extraObjects }}}}{{{{ if not (kindIs "slice" .Values.extraObjects) }}}}{{{{ fail "{message}" }}}}{{{{ end }}}}{{{{ end }}}}"#
        )
    };
    let kibana_extra = extra_objects("kibana");
    let agent_extra = extra_objects("agent");
    let schema = siblings_schema(
        indoc! {"
            eck-package-registry:
              enabled: false
            eck-agent:
              enabled: false
        "},
        &[
            ("eck-kibana", "{}\n", Some("eck-kibana.enabled")),
            ("eck-agent", "{}\n", Some("eck-agent.enabled")),
            (
                "eck-package-registry",
                "{}\n",
                Some("eck-package-registry.enabled"),
            ),
        ],
        &[
            (
                "charts/eck-agent/templates/extra-objects.yaml",
                agent_extra.as_str(),
            ),
            ("charts/eck-kibana/templates/_helpers.tpl", KIBANA_HELPERS),
            (
                "charts/eck-kibana/templates/extra-objects.yaml",
                kibana_extra.as_str(),
            ),
            ("charts/eck-kibana/templates/kibana.yaml", KIBANA_MANIFEST),
            (
                "charts/eck-package-registry/templates/_helpers.tpl",
                REGISTRY_HELPERS,
            ),
            (
                "charts/eck-package-registry/templates/epr.yaml",
                REGISTRY_MANIFEST,
            ),
        ],
    )?;
    let document = |extra_objects: Value| {
        json!({
            "eck-kibana": {
                "enabled": true,
                "extraObjects": extra_objects,
                "elasticsearchRef": {"name": "elasticsearch"},
                "version": "9.5.0",
            },
        })
    };
    assert_verdicts(
        &schema,
        &[
            ("scalar extraObjects", document(json!(true)), false),
            ("list extraObjects", document(json!([])), true),
        ],
    )
}

/// A conditional opaque writer on the parent root rewrites the parent's values only where it
/// runs: where it is skipped, a later check of the same values still fails the render. Helm
/// fails `x: true` with the writer off and renders it with `x: []` (receipts
/// `guarded-writer/`, reduced from the openebs chart's `openebs.validate.localpv`).
#[test]
fn a_skipped_opaque_writer_leaves_a_later_check_intact() -> eyre::Result<()> {
    let schema = graph_schema(
        indoc! {"
            run: false
            p: plain
            x: null
        "},
        "{}\n",
        &[
            (
                "templates/a/b/writer.yaml",
                "{{ if .Values.run }}{{ $_ := tpl .Values.p . }}{{ end }}",
            ),
            (
                "templates/check.yaml",
                r#"{{ if not .Values.run }}{{ if .Values.x }}{{ if not (kindIs "slice" .Values.x) }}{{ fail "x" }}{{ end }}{{ end }}{{ end }}"#,
            ),
        ],
        false,
    )?;
    assert_verdicts(
        &schema,
        &[
            (
                "scalar with the writer off",
                json!({"run": false, "p": "plain", "x": true, "child": {"global": {}}}),
                false,
            ),
            (
                "list with the writer off",
                json!({"run": false, "p": "plain", "x": [], "child": {"global": {}}}),
                true,
            ),
        ],
    )
}

#[test]
fn all_release_name_branches_abort_when_the_library_is_disabled() -> eyre::Result<()> {
    let schema = law_schema(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: graph
                version: 0.1.0
                dependencies:
                  - name: library
                    version: 0.1.0
                    tags:
                      - library
            "},
        ),
        ("values.yaml", "{}\n"),
        (
            "charts/library/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: library
                version: 0.1.0
                type: library
            "},
        ),
        (
            "charts/library/templates/_value.tpl",
            r#"{{ define "library.value" }}text{{ end }}"#,
        ),
        (
            "templates/cm.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: graph
                data:
                  value: {{ if eq .Release.Name "one" }}{{ include "library.value" . | quote }}{{ else }}{{ include "library.value" (dict) | quote }}{{ end }}
            "#},
        ),
    ])?;
    assert_verdicts(
        &schema,
        &[
            ("enabled", json!({"tags": {"library": true}}), true),
            ("disabled", json!({"tags": {"library": false}}), false),
            ("absent", json!({"tags": {}}), true),
        ],
    )
}

fn missing_library_schema(architecture: &str) -> eyre::Result<Value> {
    let child_defaults = format!("architecture: {architecture}\n");
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
                    condition: child.enabled
                  - name: library
                    version: 0.1.0
                    tags: [library]
            "},
        ),
        (
            "values.yaml",
            indoc! {"
                child:
                  enabled: true
            "},
        ),
        (
            "charts/child/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: child
                version: 0.1.0
            "},
        ),
        ("charts/child/values.yaml", &child_defaults),
        (
            "charts/child/templates/cm.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: graph
                data:
                  value: {{ if ne .Values.architecture "replication" }}{{ include "library.value" . | quote }}{{ else }}"replicated"{{ end }}
            "#},
        ),
        (
            "charts/library/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: library
                version: 0.1.0
                type: library
            "},
        ),
        (
            "charts/library/templates/_value.tpl",
            r#"{{ define "library.value" }}text{{ end }}"#,
        ),
    ])
}

/// A dependency default and its null deletion both select the missing helper.
/// Helm 4.2.3 receipts are in `transfer/r3/raw-library`.
#[test]
fn a_dependency_default_and_its_absence_select_the_same_missing_helper() -> eyre::Result<()> {
    let schema = missing_library_schema("standalone")?;
    assert_verdicts(
        &schema,
        &[
            (
                "raw",
                json!({"child": {"enabled": true}, "tags": {"library": false}}),
                false,
            ),
            (
                "coalesced",
                json!({"child": {"enabled": true, "architecture": "standalone"}, "tags": {"library": false}}),
                false,
            ),
            (
                "explicit override",
                json!({"child": {"enabled": true, "architecture": "replication"}, "tags": {"library": false}}),
                true,
            ),
            (
                "disabled",
                json!({"child": {"enabled": false}, "tags": {"library": false}}),
                true,
            ),
        ],
    )?;

    let schema = missing_library_schema("replication")?;
    assert_verdicts(
        &schema,
        &[
            (
                "omitted with a working dependency default",
                json!({"child": {"enabled": true}, "tags": {"library": false}}),
                true,
            ),
            (
                "explicit failing architecture",
                json!({"child": {"enabled": true, "architecture": "standalone"}, "tags": {"library": false}}),
                false,
            ),
        ],
    )
}
