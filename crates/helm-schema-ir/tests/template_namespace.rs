//! Exact file-template execution names and caller-owned template context.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{SymbolicIrContext, SymbolicPolicy};
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

fn used_paths(index: &DefineIndex, base_path: Option<&str>, source: &str) -> BTreeSet<String> {
    let mut policy = SymbolicPolicy::default();
    if let Some(base_path) = base_path {
        policy.static_root_strings = BTreeMap::from([(
            vec!["Template".to_string(), "BasePath".to_string()],
            base_path.to_string(),
        )]);
    }
    SymbolicIrContext::with_policy(index, policy)
        .generate_contract_ir(source)
        .finalize()
        .uses()
        .iter()
        .map(|use_| use_.source_expr.encode())
        .filter(|path| !path.is_empty())
        .collect()
}

fn collision_index() -> DefineIndex {
    let mut index = DefineIndex::new();
    index.add_file_source(
        "root/templates/configmap.yaml",
        r#"{{ .Values.parentToken | quote }}"#,
    );
    index.add_file_source(
        "root/charts/kid/templates/configmap.yaml",
        r#"{{ .Values.childToken | quote }}"#,
    );
    index
}

#[test]
fn computed_names_select_the_callers_exact_template_namespace() {
    let index = collision_index();
    for call in [
        r#"include (print $.Template.BasePath "/configmap.yaml") ."#,
        r#"include (printf "%s/configmap.yaml" .Template.BasePath) ."#,
        r#"$base := $.Template.BasePath }}{{ include (print $base "/configmap.yaml") ."#,
        r#"$root := $ }}{{ include (print $root.Template.BasePath "/configmap.yaml") $root"#,
        r#"$root := . }}{{ include (print $root.Template.BasePath "/configmap.yaml") $root"#,
        r#"include (print (index $ "Template" "BasePath") "/configmap.yaml") ."#,
        r#"include (print (get (get $ "Template") "BasePath") "/configmap.yaml") ."#,
    ] {
        let source = formatdoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            data:
              token: {{{{ {call} }}}}
        "#};
        for (base, expected) in [
            ("root/templates", "parentToken"),
            ("root/charts/kid/templates", "childToken"),
        ] {
            sim_assert_eq!(have: used_paths(&index, Some(base), &source), want: BTreeSet::from([expected.to_string()]), "{call} in {base}");
        }
    }
}

#[test]
fn literal_execution_names_do_not_need_source_path_inference() {
    let index = collision_index();
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "root/charts/kid/templates/configmap.yaml" . }}
    "#};
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::from(["childToken".to_string()]));
}

#[test]
fn missing_template_context_does_not_guess_a_suffix_match() {
    let index = collision_index();
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include (print $.Template.BasePath "/configmap.yaml") . }}
    "#};
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::new());
}

#[test]
fn helper_defining_chart_does_not_replace_caller_template_data() {
    let mut index = collision_index();
    index.add_file_source(
        "root/charts/kid/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "kid.dispatch" -}}
        {{ include (print .Template.BasePath "/configmap.yaml") . }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "kid.dispatch" . }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["parentToken".to_string()]));
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::new());
}

#[test]
fn explicit_template_context_controls_the_called_body() {
    let mut index = collision_index();
    index.add_file_source(
        "root/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "dispatch" -}}
        {{ include (print .Template.BasePath "/configmap.yaml") . }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "dispatch" (dict "Template" (dict "BasePath" "root/charts/kid/templates") "Values" .Values) }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["childToken".to_string()]));
}

#[test]
fn nested_original_root_context_does_not_borrow_a_helpers_replacement_template() {
    let mut index = collision_index();
    index.add_file_source(
        "root/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "dispatch" -}}
        {{- $original := .original -}}
        {{ include (print $original.Template.BasePath "/configmap.yaml") $original }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "dispatch" (dict "original" $ "Template" (dict "BasePath" "root/charts/kid/templates")) }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["parentToken".to_string()]));
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::new());
}

#[test]
fn list_transported_root_keeps_its_template_namespace() {
    let mut index = collision_index();
    index.add_file_source(
        "root/charts/kid/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "list.dispatch" -}}
        {{- $original := index . 0 -}}
        {{ include (print $original.Template.BasePath "/configmap.yaml") $original }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "list.dispatch" (list $) }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["parentToken".to_string()]));
}

#[test]
fn caller_metadata_changes_before_the_call_reach_stored_root_arguments() {
    let mut index = collision_index();
    index.add_file_source(
        "root/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "dispatch" -}}
        {{- $original := .original -}}
        {{ include (print $original.Template.BasePath "/configmap.yaml") $original }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        {{- $arguments := dict "original" $ -}}
        {{- $_ := set . "Template" (dict "BasePath" "root/charts/kid/templates") -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "dispatch" $arguments }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["childToken".to_string()]));
}

#[test]
fn nested_original_root_context_survives_tpl_program_entry() {
    let mut index = collision_index();
    index.add_files_get_source(
        "files/dispatch.tpl",
        r#"{{ $original := .original }}{{ include (print $original.Template.BasePath "/configmap.yaml") $original }}"#,
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ tpl (.Files.Get "files/dispatch.tpl") (dict "original" $ "Template" (dict "BasePath" "root/charts/kid/templates")) | quote }}
    "#};
    // Replacing the program's root does not replace the root carried inside its argument.
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["parentToken".to_string()]));
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::new());
}

#[test]
fn tpl_arguments_resolve_caller_root_fields() {
    let mut index = collision_index();
    index.add_files_get_source(
        "files/dispatch.tpl",
        r#"{{ include (print $.Template.BasePath "/configmap.yaml") . }}"#,
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ tpl (.Files.Get "files/dispatch.tpl") (dict "Template" .Template "Values" .Values) | quote }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["parentToken".to_string()]));
}

#[test]
fn tpl_carried_roots_preserve_other_static_fields() {
    let mut index = DefineIndex::new();
    index.add_file_source("parent.config", "{{ .Values.parentToken }}");
    index.add_file_source("child.config", "{{ .Values.childToken }}");
    index.add_files_get_source(
        "files/dispatch.tpl",
        indoc! {r#"
            {{- $original := .original -}}
            {{ include (print $original.Chart.Name ".config") $original }}
        "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ tpl (.Files.Get "files/dispatch.tpl") (dict "original" $ "Chart" (dict "Name" "child")) | quote }}
    "#};
    let mut policy = SymbolicPolicy::default();
    policy.static_root_strings = BTreeMap::from([(
        vec!["Chart".to_string(), "Name".to_string()],
        "parent".to_string(),
    )]);
    let paths: BTreeSet<String> = SymbolicIrContext::with_policy(&index, policy)
        .generate_contract_ir(source)
        .finalize()
        .uses()
        .iter()
        .map(|use_| use_.source_expr.encode())
        .filter(|path| !path.is_empty())
        .collect();
    sim_assert_eq!(have: paths, want: BTreeSet::from(["parentToken".to_string()]));
}

fn assert_tpl_helper_guard_matches_inline_program(guard: &str) {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "guarded.program" . | quote }}
    "#};
    let mut outputs = Vec::new();
    for program in [
        r#"tpl (.Files.Get "files/program.tpl") (dict "Template" .Template "Values" .Values)"#,
        r#"required "token required" .Values.token"#,
    ] {
        let mut index = DefineIndex::new();
        index.add_files_get_source(
            "files/program.tpl",
            r#"{{ required "token required" .Values.token }}"#,
        );
        index.add_file_source(
            "root/templates/_helpers.tpl",
            &formatdoc! {r#"
                {{{{- define "guarded.program" -}}}}
                {{{{- if {guard} -}}}}
                {{{{ {program} }}}}
                {{{{- end -}}}}
                {{{{- end -}}}}
            "#},
        );
        let mut policy = SymbolicPolicy::default();
        policy.static_root_strings = BTreeMap::from([(
            vec!["Template".to_string(), "BasePath".to_string()],
            "root/templates".to_string(),
        )]);
        outputs.push(
            SymbolicIrContext::with_policy(&index, policy)
                .generate_contract_ir(source)
                .finalize()
                .into_schema_signals(),
        );
    }
    // The same program has the same guarded requirements whether inlined or loaded by tpl.
    sim_assert_eq!(have: &outputs[0], want: &outputs[1]);
}

#[test]
fn false_helper_branch_does_not_execute_tpl_program() {
    assert_tpl_helper_guard_matches_inline_program("false");
}

#[test]
fn true_helper_branch_preserves_tpl_program_requirements() {
    assert_tpl_helper_guard_matches_inline_program("true");
}

#[test]
fn unknown_helper_branch_guards_tpl_program_requirements() {
    assert_tpl_helper_guard_matches_inline_program(".Values.enabled");
}

#[test]
fn lazy_tpl_operands_match_inline_execution_requirements() {
    for operation in ["and", "or"] {
        for guard in ["false", "true", ".Values.enabled"] {
            let mut signals = Vec::new();
            for in_template in [true, false] {
                let mut index = DefineIndex::new();
                index.add_files_get_source(
                    "files/program.tpl",
                    r#"{{ required "token required" .Values.token }}"#,
                );
                let source = if in_template {
                    formatdoc! {r#"
                        apiVersion: v1
                        kind: ConfigMap
                        data:
                          token: {{{{ {operation} {guard} (tpl (.Files.Get "files/program.tpl") (dict "Template" .Template "Values" .Values)) | quote }}}}
                    "#}
                } else {
                    let condition = if operation == "and" {
                        guard.to_string()
                    } else {
                        format!("not ({guard})")
                    };
                    formatdoc! {r#"
                        {{{{ if {condition} }}}}
                        {{{{ required "token required" .Values.token | quote }}}}
                        {{{{ end }}}}
                    "#}
                };
                let mut policy = SymbolicPolicy::default();
                policy.static_root_strings = BTreeMap::from([(
                    vec!["Template".to_string(), "BasePath".to_string()],
                    "root/templates".to_string(),
                )]);
                signals.push(
                    SymbolicIrContext::with_policy(&index, policy)
                        .generate_contract_ir(&source)
                        .finalize()
                        .into_schema_signals(),
                );
            }
            sim_assert_eq!(have: signals[0].terminal_clauses(), want: signals[1].terminal_clauses(), "{operation} {guard}");
        }
    }
}

#[test]
fn tpl_context_preserves_execution_failures_without_output_identity() {
    let index = DefineIndex::new();
    let context = SymbolicIrContext::new(&index);
    let actual = context
        .generate_contract_ir(
            r#"{{ tpl "constant" (required "context required" .Values.context) | quote }}"#,
        )
        .finalize()
        .into_schema_signals();
    let expected = context
        .generate_contract_ir(
            r#"{{ $_ := required "context required" .Values.context }}{{ "constant" | quote }}"#,
        )
        .finalize()
        .into_schema_signals();
    sim_assert_eq!(have: actual, want: helm_schema_core::ContractSchemaSignals::new(BTreeMap::new(), expected.terminal_clauses().to_vec()));
}

#[test]
fn lazy_tpl_plain_yaml_captures_follow_the_selected_output() {
    for guard in ["false", "true", ".Values.enabled"] {
        let mut signals = Vec::new();
        for in_template in [true, false] {
            let mut index = DefineIndex::new();
            index.add_files_get_source(
                "files/program.tpl",
                r#"value: {{ printf "%s" .Values.token }}"#,
            );
            let body = if in_template {
                formatdoc! {r#"
                    {{{{- define "yaml.program" -}}}}
                    {{{{ and {guard} (tpl (.Files.Get "files/program.tpl") .) }}}}
                    {{{{- end -}}}}
                "#}
            } else {
                formatdoc! {r#"
                    {{{{- define "yaml.program" -}}}}
                    {{{{- if {guard} -}}}}
                    value: {{{{ printf "%s" .Values.token }}}}
                    {{{{- end -}}}}
                    {{{{- end -}}}}
                "#}
            };
            index.add_file_source("root/templates/_helpers.tpl", &body);
            let source = indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                data:
                  payload.yaml: |-
                    {{ include "yaml.program" . | nindent 4 }}
            "#};
            signals.push(
                SymbolicIrContext::new(&index)
                    .generate_contract_ir(source)
                    .finalize()
                    .into_schema_signals(),
            );
        }
        if guard == "true" {
            assert!(
                !signals[1].terminal_clauses().is_empty(),
                "the YAML sink control must carry a lexical failure"
            );
        }
        sim_assert_eq!(have: signals[0].terminal_clauses(), want: signals[1].terminal_clauses(), "{guard}");
    }
}

#[test]
fn selected_literal_tpl_programs_keep_their_own_requirements() {
    let index = DefineIndex::new();
    let context = SymbolicIrContext::new(&index);
    let actual = context
        .generate_contract_ir(indoc! {r#"
        {{- $program := "" -}}
        {{- if .Values.enabled -}}
        {{- $program = "{{ required \"a required\" .Values.a }}" -}}
        {{- else -}}
        {{- $program = "{{ required \"b required\" .Values.b }}" -}}
        {{- end -}}
        {{ tpl $program . | quote }}
    "#})
        .finalize()
        .into_schema_signals();
    let expected = context
        .generate_contract_ir(indoc! {r#"
        {{- if .Values.enabled -}}
        {{ required "a required" .Values.a | quote }}
        {{- else -}}
        {{ required "b required" .Values.b | quote }}
        {{- end -}}
    "#})
        .finalize()
        .into_schema_signals();
    assert!(!expected.terminal_clauses().is_empty());
    sim_assert_eq!(have: actual.terminal_clauses(), want: expected.terminal_clauses());
}

#[test]
fn transformed_tpl_output_does_not_keep_plain_yaml_captures() {
    for transform in ["quote", "b64enc"] {
        let mut index = DefineIndex::new();
        index.add_files_get_source(
            "files/program.tpl",
            r#"value: {{ printf "%s" .Values.token }}"#,
        );
        index.add_file_source(
            "root/templates/_helpers.tpl",
            &formatdoc! {r#"
                {{{{- define "yaml.program" -}}}}
                {{{{ tpl (.Files.Get "files/program.tpl") . | {transform} }}}}
                {{{{- end -}}}}
            "#},
        );
        let signals = SymbolicIrContext::new(&index)
            .generate_contract_ir(indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                data:
                  payload.yaml: |-
                    {{ include "yaml.program" . | nindent 4 }}
            "#})
            .finalize()
            .into_schema_signals();
        sim_assert_eq!(have: signals.terminal_clauses(), want: Vec::<Vec<helm_schema_core::ConditionalGuard>>::new().as_slice(), "{transform}");
    }
}

#[test]
fn selected_files_keep_their_program_selection_conditions() {
    let mut index = DefineIndex::new();
    index.add_files_get_source("files/a.tpl", r#"{{ required "a required" .Values.a }}"#);
    index.add_files_get_source("files/b.tpl", r#"{{ required "b required" .Values.b }}"#);
    let context = SymbolicIrContext::new(&index);
    let actual = context
        .generate_contract_ir(indoc! {r#"
        {{ tpl (.Files.Get (ternary "files/a.tpl" "files/b.tpl" .Values.choose)) . | quote }}
    "#})
        .finalize()
        .into_schema_signals();
    let expected = context.generate_contract_ir(indoc! {r#"
        {{ tpl (ternary "{{ required \"a required\" .Values.a }}" "{{ required \"b required\" .Values.b }}" .Values.choose) . | quote }}
    "#}).finalize().into_schema_signals();
    assert!(!expected.terminal_clauses().is_empty());
    sim_assert_eq!(have: actual.terminal_clauses(), want: expected.terminal_clauses());
}

#[test]
fn coalesced_default_programs_keep_source_selection_conditions() {
    let index = DefineIndex::new();
    let a = r#"{{ required "x required" .Values.x }}"#;
    let b = r#"{{ required "y required" .Values.y }}"#;
    let context = SymbolicIrContext::with_chart_default_strings(
        &index,
        BTreeMap::from([
            ("a".to_string(), a.to_string()),
            ("b".to_string(), b.to_string()),
        ]),
    );
    let actual = context
        .generate_contract_ir(r#"{{ tpl (coalesce .Values.a .Values.b) . | quote }}"#)
        .finalize()
        .into_schema_signals();
    let expected = context
        .generate_contract_ir(indoc! {r#"
        {{- if .Values.a -}}
        {{ tpl .Values.a . | quote }}
        {{- else if .Values.b -}}
        {{ tpl .Values.b . | quote }}
        {{- end -}}
    "#})
        .finalize()
        .into_schema_signals();
    let mut expected = expected.terminal_clauses().to_vec();
    sim_assert_eq!(have: expected.len(), want: 2);
    // Coalesce retains its first source's selection even when the exact program implies it.
    // The equivalent if statement expresses that selection as an ambient, removable guard.
    expected[0].insert(
        0,
        helm_schema_core::ConditionalGuard::Truthy {
            path: helm_schema_core::ValuesPath::parse("a"),
        },
    );
    sim_assert_eq!(have: actual.terminal_clauses(), want: expected.as_slice());
}

#[test]
fn template_name_is_entry_data_in_shared_helper_caches() {
    let mut index = DefineIndex::new();
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ include "name.dispatch" . }}
    "#};
    index.add_file_source("root/templates/first.yaml", source);
    index.add_file_source("root/templates/second.yaml", source);
    index.add_file_source(
        "root/charts/kid/templates/_helpers.tpl",
        indoc! {r#"
        {{- define "name.dispatch" -}}
        {{- if eq .Template.Name "root/templates/first.yaml" -}}
        {{ .Values.first | quote }}
        {{- else -}}
        {{ .Values.second | quote }}
        {{- end -}}
        {{- end -}}
    "#},
    );
    let context = SymbolicIrContext::new(&index);
    for (entry, expected) in [
        ("root/templates/first.yaml", "first"),
        ("root/templates/second.yaml", "second"),
        ("root/templates/first.yaml", "first"),
    ] {
        let paths: BTreeSet<String> = context
            .generate_contract_ir_for_source(source, entry)
            .finalize()
            .uses()
            .iter()
            .map(|use_| use_.source_expr.encode())
            .filter(|path| !path.is_empty())
            .collect();
        sim_assert_eq!(have: paths, want: BTreeSet::from([expected.to_string()]), "{entry}");
    }
}

#[test]
fn file_payloads_cannot_supply_executable_template_definitions() {
    let mut index = DefineIndex::new();
    index.add_files_get_source(
        "files/templates/configmap.yaml",
        indoc! {r#"
        {{- define "not.executable" -}}{{ .Values.fabricated }}{{- end -}}
        {{ .Values.alsoFabricated }}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          one: {{ include "not.executable" . }}
          two: {{ include "files/templates/configmap.yaml" . }}
    "#};
    sim_assert_eq!(have: used_paths(&index, None, source), want: BTreeSet::new());
}

#[test]
fn payload_paths_do_not_replace_equal_template_execution_names() {
    let mut index = collision_index();
    index.add_files_get_source(
        "root/templates/configmap.yaml",
        indoc! {r#"
        {{- if eq $.Template.Name "root/templates/caller.yaml" -}}
        {{ .Values.payload }}
        {{- else -}}
        {{ .Values.wrongCaller }}
        {{- end -}}
    "#},
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          executed: {{ include "root/templates/configmap.yaml" . }}
          payload: {{ tpl (.Files.Get "root/templates/configmap.yaml") . | quote }}
    "#};
    index.add_file_source("root/templates/caller.yaml", source);
    let paths: BTreeSet<String> = SymbolicIrContext::new(&index)
        .generate_contract_ir_for_source(source, "root/templates/caller.yaml")
        .finalize()
        .uses()
        .iter()
        .map(|use_| use_.source_expr.encode())
        .filter(|path| !path.is_empty())
        .collect();
    sim_assert_eq!(have: paths, want: BTreeSet::from(["parentToken".to_string(), "payload".to_string()]));
}

#[test]
fn tpl_programs_receive_the_explicit_template_context() {
    let mut index = collision_index();
    index.add_files_get_source(
        "files/dispatch.tpl",
        r#"{{ include (print $.Template.BasePath "/configmap.yaml") . }}"#,
    );
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          token: {{ tpl (.Files.Get "files/dispatch.tpl") (dict "Template" (dict "BasePath" "root/charts/kid/templates") "Values" .Values) | quote }}
    "#};
    sim_assert_eq!(have: used_paths(&index, Some("root/templates"), source), want: BTreeSet::from(["childToken".to_string()]));
}
