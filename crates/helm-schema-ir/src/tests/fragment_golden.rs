//! Golden fragment dumps for the Stage-B interpreter micro-cases: branchy
//! mappings, ranges over values, partial scalars, helper splices, block
//! scalars, and opaque taint. Each expected dump is a reviewed semantic
//! statement, not a regenerated snapshot; update it only with a reasoned
//! change.

use crate::SymbolicIrContext;
use crate::fragment_eval::dump_document;
use helm_schema_ast::DefineIndex;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn assert_fragment_dump(source: &str, helpers: &str, expected: &str) {
    let mut idx = DefineIndex::new();
    if !helpers.is_empty() {
        idx.add_file_source("_helpers.tpl", helpers);
    }
    let document = SymbolicIrContext::new(&idx).eval_document_fragment(source);
    sim_assert_eq!(have: dump_document(&document), want: expected);
}

/// Branch alternatives merge under one mapping key; the second arm carries
/// the negation of the first arm's condition, and condition reads keep the
/// per-guard prefix rule.
#[test]
fn branchy_mapping_merges_guarded_entry_arms() {
    let source = indoc! {r#"
        metadata:
          name: static
          {{- if .Values.commonAnnotations }}
          annotations:
            checksum: fixed
          {{- else if .Values.legacyAnnotations }}
          annotations:
            legacy: "true"
          {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      scalar [text{"static"}]
                  key "annotations":
                    when truthy(commonAnnotations):
                      mapping:
                        key "checksum":
                          when always:
                            scalar [text{"fixed"}]
                    when (truthy(legacyAnnotations) && !(truthy(commonAnnotations))):
                      mapping:
                        key "legacy":
                          when always:
                            scalar [text{"\"true\""}]
        reads:
          commonAnnotations [truthy(commonAnnotations)]
          legacyAnnotations [truthy(legacyAnnotations), not(commonAnnotations)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// An embedded control scopes only the entries inside its source window.
#[test]
fn embedded_control_preserves_before_inside_after_order() {
    let source = indoc! {r#"
        item:
          before: {{ .Values.before }}
          {{- if .Values.outer }}
          branch:
            before: {{ .Values.branchBefore }}
            {{- if .Values.inner }}
            inside: {{ .Values.inside }}
            {{- end }}
            after: {{ .Values.branchAfter }}
          {{- end }}
          after: {{ .Values.after }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "item":
              when always:
                mapping:
                  key "before":
                    when always:
                      splice before scalar
                  key "branch":
                    when truthy(outer):
                      mapping:
                        key "before":
                          when always:
                            splice branchBefore scalar
                        key "inside":
                          when truthy(inner):
                            splice inside scalar
                        key "after":
                          when always:
                            splice branchAfter scalar
                  key "after":
                    when always:
                      splice after scalar
        reads:
          outer [truthy(outer)]
          inner [truthy(inner), truthy(outer)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Nested controls jointly scope a container that escapes both regions.
#[test]
fn nested_embedded_controls_conjoin_their_conditions() {
    let source = indoc! {r#"
        {{- if .Values.outer }}
        {{- if .Values.inner }}
        - {{ .Values.value }}
        {{- end }}
        {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          sequence:
            item:
              when (truthy(inner) && truthy(outer)):
                splice value scalar
        reads:
          outer [truthy(outer)]
          inner [truthy(inner), truthy(outer)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Nested rotations keep every guard on the shared escaped item.
#[test]
fn double_rotation_adopts_the_shared_escaped_item_once() {
    let source = indoc! {r#"
        env:
        {{- if .Values.env }}
        {{- if kindIs "map" .Values.env }}
        - {{ merge (dict "name" "MERGED") .Values.env | toYaml | nindent 2 }}
        {{- else }}
        {{- toYaml .Values.env | nindent 0 }}
        {{- end }}
        {{- else }}
        {{- toYaml (list) | nindent 0 }}
        {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "env":
              when (truthy(env) && !(typeIs(env: object))):
                splice env fragment
              when !(truthy(env)):
                sequence:
              when always:
                sequence:
                  item:
                    when (truthy(env) && typeIs(env: object)):
                      mapping:
                        key "name":
                          when always:
                            scalar [text{"MERGED"}]
                    when (truthy(env) && typeIs(env: object)):
                      splice env fragment
        reads:
          env [truthy(env)]
          env [truthy(env), typeIs(env: object)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A shared escaped container advances through a wide control chain once per control.
#[test]
fn shared_container_advances_through_wide_control_chain() {
    let mut source = String::new();
    for index in 0..64 {
        source.push_str(&format!("{{{{- if .Values.g{index} }}}}\n"));
    }
    source.push_str("- name: fixed\n");
    for index in 0..128 {
        source.push_str(&format!("  child{index}: fixed\n"));
    }
    for _ in 0..64 {
        source.push_str("{{- else }}\n{{- end }}\n");
    }

    let mut guards = (0..64)
        .map(|index| format!("truthy(g{index})"))
        .collect::<Vec<_>>();
    guards.sort();
    let mut expected = format!(
        "when always:\n  sequence:\n    item:\n      when ({}):\n        mapping:\n          key \"name\":\n            when always:\n              scalar [text{{\"fixed\"}}]\n",
        guards.join(" && "),
    );
    for index in 0..128 {
        expected.push_str(&format!(
            "          key \"child{index}\":\n            when always:\n              scalar [text{{\"fixed\"}}]\n"
        ));
    }
    expected.push_str("reads:\n");
    for index in 0..64 {
        let mut read_guards = (0..=index)
            .map(|guard| format!("truthy(g{guard})"))
            .collect::<Vec<_>>();
        read_guards.sort();
        expected.push_str(&format!("  g{index} [{}]\n", read_guards.join(", ")));
    }

    assert_fragment_dump(&source, "", &expected);
}

/// Sibling adoption does not re-enter the control that performed it.
#[test]
fn sibling_adoption_advances_the_owned_control_boundary() {
    let source = indoc! {r#"
        data:
        {{- if .Values.outer }}
        {{- if .Values.inner }}
          pre: fixed
          key:
            leaf: {{ .Values.leaf }}
        {{- end }}
        {{- else }}
        {{- include "chart.defaults" . | nindent 2 }}
        {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "pre":
                    when (truthy(inner) && truthy(outer)):
                      scalar [text{"fixed"}]
                  key "key":
                    when (truthy(inner) && truthy(outer)):
                      mapping:
                        key "leaf":
                          when always:
                            splice leaf scalar
        reads:
          outer [truthy(outer)]
          inner [truthy(inner), truthy(outer)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Deferred branch content does not re-enter an already-owned outer control.
#[test]
fn deferred_branch_carries_the_owned_control_boundary() {
    let source = indoc! {r#"
        data:
        {{- if .Values.a }}
          key:
            leaf: fixed
        {{- else if .Values.b }}
            extra:
              inner: {{ .Values.inner }}
        {{- else }}
              x: {{ .Values.x }}
        {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "key":
                    when truthy(a):
                      mapping:
                        key "leaf":
                          when always:
                            scalar [text{"fixed"}]
                  key "extra":
                    when (truthy(b) && !(truthy(a))):
                      mapping:
                        key "inner":
                          when always:
                            splice inner scalar
                  key "x":
                    when (!(truthy(a)) && !(truthy(b))):
                      splice x scalar
        reads:
          a [truthy(a)]
          b [truthy(b), not(a)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A post-region child follows a conditional parent only on paths that render its header.
#[test]
fn deferred_child_bypasses_an_absent_parent_shell() {
    let source = indoc! {r#"
        data:
        {{- if .Values.enabled }}
          parent:
            inside: fixed
        {{- else }}
        {{- end }}
            after: {{ .Values.after }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "parent":
                    when truthy(enabled):
                      mapping:
                        key "inside":
                          when always:
                            scalar [text{"fixed"}]
                    when truthy(enabled):
                      mapping:
                        key "after":
                          when always:
                            splice after scalar
                  key "after":
                    when !(truthy(enabled)):
                      splice after scalar
        reads:
          enabled [truthy(enabled)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Deferred content falls back through adjacent conditional mapping shells in source order.
#[test]
fn deferred_child_uses_the_latest_rendered_adjacent_mapping_shell() {
    let source = indoc! {r#"
        data:
        {{ if .Values.a }}
          first:
        {{ end }}
        {{ if .Values.b }}
          second:
        {{ end }}
            after: fixed
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "first":
                    when (truthy(a) && !(truthy(b))):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
                  key "second":
                    when truthy(b):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
                  key "after":
                    when !((truthy(a) || truthy(b))):
                      scalar [text{"fixed"}]
        reads:
          a [truthy(a)]
          b [truthy(b)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Deferred content retains the earlier arm's trailing open mapping chain.
#[test]
fn deferred_child_uses_each_arm_trailing_open_mapping_chain() {
    let source = indoc! {r#"
        data:
        {{ if .Values.x }}
          first:
            sub:
        {{ else }}
          second:
        {{ end }}
              after: fixed
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "first":
                    when truthy(x):
                      mapping:
                        key "sub":
                    when truthy(x):
                      mapping:
                        key "sub":
                          when always:
                            mapping:
                              key "after":
                                when always:
                                  scalar [text{"fixed"}]
                  key "second":
                    when !(truthy(x)):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
        reads:
          x [truthy(x)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Alternative trailing chains retain their own indentation and containment metadata.
#[test]
fn deferred_child_preserves_complete_unequal_trailing_chains() {
    let source = indoc! {r"
        data:
        {{ if .Values.x }}
          first:
            sub:
        {{ else }}
          second:
              deep:
        {{ end }}
              after: fixed
    "};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "first":
                    when truthy(x):
                      mapping:
                        key "sub":
                    when truthy(x):
                      mapping:
                        key "sub":
                          when always:
                            mapping:
                              key "after":
                                when always:
                                  scalar [text{"fixed"}]
                  key "second":
                    when !(truthy(x)):
                      mapping:
                        key "deep":
                    when !(truthy(x)):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
        reads:
          x [truthy(x)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A later arm falls back to the open sibling that rendered before its control.
#[test]
fn later_arm_child_uses_an_earlier_open_sibling_shell() {
    let source = indoc! {r"
        data:
          zero:
        {{ if .Values.a }}
          first:
        {{ else }}
            extra: fixed
        {{ end }}
    "};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "zero":
                    when !(truthy(a)):
                      mapping:
                        key "extra":
                          when always:
                            scalar [text{"fixed"}]
                  key "first":
        reads:
          a [truthy(a)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Equal-indent items remain inside each arm's trailing open mapping shell.
#[test]
fn deferred_item_uses_each_arm_trailing_open_mapping_chain() {
    let source = indoc! {r#"
        containers:
        {{ if .Values.x }}
        - name: a
          env:
        {{ else }}
        - name: b
          envFrom:
        {{ end }}
          - name: {{ .Values.entryName }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "containers":
              when always:
                sequence:
                  item:
                    when truthy(x):
                      mapping:
                        key "name":
                          when always:
                            scalar [text{"a"}]
                        key "env":
                  item:
                    when !(truthy(x)):
                      mapping:
                        key "name":
                          when always:
                            scalar [text{"b"}]
                        key "envFrom":
                  item:
                    when !(truthy(x)):
                      mapping:
                        key "envFrom":
                          when always:
                            sequence:
                              item:
                                when always:
                                  mapping:
                                    key "name":
                                      when always:
                                        splice entryName scalar
                  item:
                    when truthy(x):
                      mapping:
                        key "env":
                          when always:
                            sequence:
                              item:
                                when always:
                                  mapping:
                                    key "name":
                                      when always:
                                        splice entryName scalar
        reads:
          x [truthy(x)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A same-indent deferred item is a sibling of an item parent in the open chain.
#[test]
fn deferred_equal_indent_item_is_not_nested_in_a_prior_item() {
    let source = indoc! {r"
        containers:
        {{ if .Values.x }}
        - name: first
          env:
            - name: old
        {{ end }}
            - name: {{ .Values.next }}
    "};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "containers":
              when always:
                sequence:
                  item:
                    when truthy(x):
                      mapping:
                        key "name":
                          when always:
                            scalar [text{"first"}]
                        key "env":
                          when always:
                            sequence:
                              item:
                                when always:
                                  mapping:
                                    key "name":
                                      when always:
                                        scalar [text{"old"}]
                  item:
                    when truthy(x):
                      mapping:
                        key "env":
                          when always:
                            sequence:
                              item:
                                when always:
                                  mapping:
                                    key "name":
                                      when always:
                                        splice next scalar
                  item:
                    when !(truthy(x)):
                      mapping:
                        key "name":
                          when always:
                            splice next scalar
        reads:
          x [truthy(x)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A node following a deferred nested control remains owned by that control's arm.
#[test]
fn deferred_nested_control_keeps_later_node_in_its_branch() {
    let source = indoc! {r#"
        data:
        {{- if .Values.direct }}
          direct: true
        {{- else }}
          env:
          - envVar:
              key: {{ required "key is required" .Values.key }}
          {{- if .Values.url }}
              value: {{ tpl .Values.url . }}
          {{- else }}
              value: fallback
          {{- end }}
              after: fixed
        {{- end }}
    "#};

    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "direct":
                    when truthy(direct):
                      scalar [text{"true"}]
                  key "env":
                    when !(truthy(direct)):
                      sequence:
                        item:
                          when always:
                            mapping:
                              key "envVar":
                                when always:
                                  mapping:
                                    key "key":
                                      when always:
                                        splice key scalar
                                    key "value":
                                      when truthy(url):
                                        splice url scalar
                                      when !(truthy(url)):
                                        scalar [text{"fallback"}]
                                    key "after":
                                      when always:
                                        scalar [text{"fixed"}]
        reads:
          direct [truthy(direct)]
          url [truthy(url), not(direct)]
    "#};

    assert_fragment_dump(source, "", expected);
}

/// An output-empty control remains beside the rendered nodes that supply its arms.
#[test]
fn empty_nested_control_keeps_escaped_arm_nodes_in_source_order() {
    let source = indoc! {r#"
        data:
          env:
          - envVar:
          {{- if .Values.direct }}
              key: DIRECT
          {{- else }}
              key: {{ required "key is required" .Values.key }}
          {{- if .Values.url }}
              value: {{ tpl .Values.url . }}
          {{- else }}
              value: fallback
          {{- end }}
          {{- end }}
              after: fixed
    "#};

    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "env":
                    when always:
                      sequence:
                        item:
                          when always:
                            mapping:
                              key "envVar":
                                when always:
                                  mapping:
                                    key "key":
                                      when truthy(direct):
                                        scalar [text{"DIRECT"}]
                                      when !(truthy(direct)):
                                        splice key scalar
                                    key "value":
                                      when (truthy(url) && !(truthy(direct))):
                                        splice url scalar
                                      when (!(truthy(direct)) && !(truthy(url))):
                                        scalar [text{"fallback"}]
                                    key "after":
                                      when always:
                                        scalar [text{"fixed"}]
        reads:
          direct [truthy(direct)]
          url [truthy(url), not(direct)]
    "#};

    assert_fragment_dump(source, "", expected);
}

/// A deeper output-empty control remains inside its open mapping container.
#[test]
fn deeper_empty_control_keeps_escaped_arm_nodes_in_parent() {
    let source = indoc! {r#"
        {{- if .Values.enabled }}
        data:
          before: fixed
          {{- if .Values.force }}
          password: {{ required "password is required" .Values.password }}
          {{- else }}
          password: generated
          {{- end }}
          after: fixed
        {{- end }}
    "#};

    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when truthy(enabled):
                mapping:
                  key "before":
                    when always:
                      scalar [text{"fixed"}]
                  key "password":
                    when truthy(force):
                      splice password scalar
                    when !(truthy(force)):
                      scalar [text{"generated"}]
                  key "after":
                    when always:
                      scalar [text{"fixed"}]
        reads:
          enabled [truthy(enabled)]
          force [truthy(enabled), truthy(force)]
    "#};

    assert_fragment_dump(source, "", expected);
}

/// An action-line key is a typed open mapping parent for deeper descendants.
#[test]
fn dynamic_action_line_key_opens_a_mapping_parent() {
    let source = indoc! {r#"
        data:
        {{ if .Values.enabled }}
          {{ "chosen" }}:
            inside: fixed
        {{ else }}
        {{ end }}
            after: fixed
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key dynamic [text{"chosen"}]:
                    when truthy(enabled):
                      mapping:
                        key "inside":
                          when always:
                            scalar [text{"fixed"}]
                  key dynamic [text{"chosen"}]:
                    when truthy(enabled):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
                  key "after":
                    when !(truthy(enabled)):
                      scalar [text{"fixed"}]
        reads:
          enabled [truthy(enabled)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Every output token before the structural colon contributes to one open dynamic key.
#[test]
fn multi_output_action_line_key_opens_one_mapping_parent() {
    let source = indoc! {r#"
        data:
        {{ if .Values.enabled }}
          {{ "pre" }}-{{ "fix" }}:
            inside: fixed
        {{ else }}
        {{ end }}
            after: fixed
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key dynamic [text{"pre"} text{"-"} text{"fix"}]:
                    when truthy(enabled):
                      mapping:
                        key "inside":
                          when always:
                            scalar [text{"fixed"}]
                  key dynamic [text{"pre"} text{"-"} text{"fix"}]:
                    when truthy(enabled):
                      mapping:
                        key "after":
                          when always:
                            scalar [text{"fixed"}]
                  key "after":
                    when !(truthy(enabled)):
                      scalar [text{"fixed"}]
        reads:
          enabled [truthy(enabled)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A dynamic header's terminal indent width determines its structural parent.
#[test]
fn dynamic_action_line_key_uses_its_rendered_indent() {
    let source = indoc! {r#"
        data:
        {{ "chosen" | indent 2 }}:
            inside: fixed
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key dynamic [text{"chosen"}]:
                    when always:
                      mapping:
                        key "inside":
                          when always:
                            scalar [text{"fixed"}]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Adjacent embedded controls retain independent source windows at scale.
#[test]
fn adjacent_embedded_controls_use_independent_crossing_sets() {
    let mut source = "data:\n".to_string();
    let mut expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
    "#}
    .to_string();
    let mut reads = "reads:\n".to_string();
    for index in 0..64 {
        source.push_str(&format!(
            "{{{{- if .Values.g{index} }}}}\n  item{index}:\n    leaf{index}: fixed\n{{{{- else }}}}\n{{{{- end }}}}\n"
        ));
        expected.push_str(&format!(
            "          key \"item{index}\":\n            when truthy(g{index}):\n              mapping:\n                key \"leaf{index}\":\n                  when always:\n                    scalar [text{{\"fixed\"}}]\n"
        ));
        reads.push_str(&format!("  g{index} [truthy(g{index})]\n"));
    }
    expected.push_str(&reads);

    assert_fragment_dump(&source, "", &expected);
}

/// A range rendering scalar items splices the iterated list at the container
/// (plus per-item dot splices); a destructured range rendering templated
/// entries splices the source as a fragment, with the header read recorded.
/// The VALUE variable carries member identity (`$value` is `env.*`), so the
/// dynamic entry also splices the member scalar.
#[test]
fn range_over_values_splices_list_and_mapping_sources() {
    let source = indoc! {r"
        spec:
          args:
            {{- range .Values.extraArgs }}
            - {{ . | quote }}
            {{- end }}
          env:
            {{- range $key, $value := .Values.env }}
            {{ $key }}: {{ $value }}
            {{- end }}
    "};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "spec":
              when always:
                mapping:
                  key "args":
                    when always:
                      sequence:
                        item:
                          when range(extraArgs):
                            splice extraArgs.* scalar
                    when range(extraArgs):
                      splice extraArgs scalar
                  key "env":
                    when range(env):
                      splice env fragment
                    when always:
                      mapping:
                        key dynamic [splice env partial range-key]:
                          when range(env):
                            splice env.* scalar
        reads:
          env [range(env)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Partial scalars keep ordered parts: splices interleaved with literal
/// text, and a `default` fallback marks the primary path defaulted while its
/// fallback text stays a contribution part.
#[test]
fn partial_scalar_keeps_ordered_parts() {
    let source = indoc! {r#"
        metadata:
          name: {{ .Values.prefix }}-{{ .Values.suffix }}
          host: {{ .Values.host | default "localhost" }}:8080
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      scalar [splice prefix partial text{"-"} splice suffix partial]
                  key "host":
                    when always:
                      scalar [text{"localhost"} text{":8080"}]
                    when truthy(host):
                      scalar [splice host partial defaulted text{":8080"}]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Helper calls splice their memoized summary fragments: a scalar helper's
/// internal branch condition becomes the splice's arm condition, a
/// structured helper stays mapping entries, and helper-internal guard reads
/// surface pathlessly carrying their own decoded condition — the same
/// convention document-level condition reads always had.
#[test]
fn helper_splice_lowers_summary_branches_into_arms() {
    let helpers = indoc! {r#"
        {{- define "chart.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride | trunc 63 -}}
        {{- else -}}
        {{- .Release.Name -}}
        {{- end -}}
        {{- end -}}
        {{- define "chart.labels" -}}
        app: {{ .Values.appName }}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        metadata:
          name: {{ include "chart.fullname" . }}
          labels: {{- include "chart.labels" . | nindent 4 }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when truthy(fullnameOverride):
                      splice fullnameOverride scalar
                  key "labels":
                    when always:
                      mapping:
                        key "app":
                          when always:
                            scalar [splice appName scalar]
        reads:
          fullnameOverride [truthy(fullnameOverride)]
    "#};
    assert_fragment_dump(source, helpers, expected);
}

/// Block-scalar bodies are render-suppressed blobs: contained splices keep
/// influencing the text without sink-typing the entry's document position.
#[test]
fn block_scalar_body_is_render_suppressed() {
    let source = indoc! {r"
        data:
          config.yaml: |
            port={{ .Values.port }}
            host=localhost
    "};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "config.yaml":
                    when always:
                      scalar suppressed [text{"    port="} splice port partial text{"\n    host=localhost"}]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Unknown calls carry their influence as opaque taint, and an inline
/// `{{ if }}…{{ end }}` region inside a scalar becomes guarded text arms.
#[test]
fn opaque_taint_and_inline_region_arms() {
    let source = indoc! {r#"
        metadata:
          name: {{ required "name is required" .Values.nameOverride }}
          kind: {{ if .Values.experimental }}Alpha{{ else }}Stable{{ end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      splice nameOverride scalar
                  key "kind":
                    when truthy(experimental):
                      scalar [text{"Alpha"}]
                    when !(truthy(experimental)):
                      scalar [text{"Stable"}]
        reads:
          experimental [truthy(experimental)]
    "#};
    assert_fragment_dump(source, "", expected);
}
