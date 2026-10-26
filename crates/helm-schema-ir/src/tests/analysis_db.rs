use color_eyre::eyre;
use std::collections::BTreeSet;

use super::{IrAnalysisDb, ParsedDefines, extract_define_blocks};
use crate::abstract_value::AbstractValue;
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn helper_db(source: &str) -> IrAnalysisDb {
    let mut defines = DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", source);
    IrAnalysisDb::new(&defines)
}

fn helper_outputs(source: &str, invocation: &str) -> Vec<Option<String>> {
    let db = helper_db(source);
    let execution = crate::value::execute_document(invocation, None, &db);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    execution
        .text()
        .map(|(_, output)| output.exact_text())
        .collect()
}

fn assert_unknown_helper_output(source: &str, invocation: &str) {
    let outputs = helper_outputs(source, invocation);
    assert!(
        !outputs.is_empty(),
        "unknown execution must retain a normal alternative"
    );
    assert!(
        outputs.iter().all(Option::is_none),
        "unknown execution cannot become exact empty output: {outputs:?}"
    );
}

#[test]
fn recursive_helpers_preserve_unknown_output_instead_of_empty_cached_summaries() {
    for (source, invocation) in [
        (
            indoc! {r#"
            {{- define "direct" -}}
            {{- include "direct" . -}}
            {{- end -}}
        "#},
            r#"{{ include "direct" . }}"#,
        ),
        (
            indoc! {r#"
            {{- define "conditional" -}}
            {{- if .Values.enabled -}}
            {{- include "conditional" . -}}
            {{- end -}}
            {{- end -}}
        "#},
            r#"{{ include "conditional" . }}"#,
        ),
        (
            indoc! {r#"
            {{- define "a" -}}
            {{- if .Values.enabled -}}{{ include "b" . }}{{- end -}}
            {{- end -}}
            {{- define "b" -}}
            {{- include "a" . -}}
            {{- end -}}
        "#},
            r#"{{ include "a" . }}"#,
        ),
    ] {
        let outputs = helper_outputs(source, invocation);
        // The zero-iteration/untaken arm may be exact empty, while the recursive arm stays unknown.
        assert!(
            outputs.iter().any(Option::is_none),
            "recursive execution lost its unknown alternative: {outputs:?}"
        );
    }
}

#[test]
fn relevant_cycle_cuts_do_not_reuse_a_result_from_another_call_chain() {
    let source = indoc! {r#"
        {{- define "a" -}}
        {{- .Values.from_a -}}
        {{- include "target" . -}}
        {{- end -}}
        {{- define "target" -}}
        {{- include "a" . -}}
        {{- end -}}
    "#};
    for invocation in [r#"{{ include "target" . }}"#, r#"{{ include "a" . }}"#] {
        let db = helper_db(source);
        let execution = crate::value::execute_document(invocation, None, &db);
        assert!(
            execution
                .reads()
                .iter()
                .any(|read| read.values_path == ValuesPath::parse("from_a"))
        );
        assert!(
            execution
                .text()
                .any(|(_, output)| output.exact_text().is_none())
        );
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    }
}

#[test]
fn dynamic_helper_names_preserve_unknown_output() {
    let source = indoc! {r#"
        {{- define "dynamic" -}}
        {{- include .Values.helper . -}}
        {{- include "dynamic" . -}}
        {{- end -}}
    "#};
    let db = helper_db(source);
    let execution = crate::value::execute_document(r#"{{ include "dynamic" . }}"#, None, &db);
    let string_name = Predicate::from(helm_schema_core::Guard::TypeIs {
        path: ValuesPath::parse("helper"),
        schema_type: "string".to_owned(),
    });
    sim_assert_eq!(have: execution.abort_predicate(), want: string_name.negated());
    let outputs = execution.text().collect::<Vec<_>>();
    assert!(
        !outputs.is_empty(),
        "dynamic execution lost its unknown normal alternative"
    );
    for (condition, output) in outputs {
        sim_assert_eq!(have: condition.normalize_boolean(), want: string_name.clone());
        sim_assert_eq!(have: output.exact_text(), want: None);
    }
}

#[test]
fn unknown_expressions_preserve_unknown_output() {
    let source = indoc! {r#"
        {{- define "unknown" -}}
        {{- 0x1p10000 -}}
        {{- end -}}
    "#};
    assert_unknown_helper_output(source, r#"{{ include "unknown" . }}"#);
}

#[test]
fn tpl_programs_preserve_unknown_output_without_a_static_helper_walk() {
    let source = indoc! {r#"
        {{- define "render-file" -}}
        {{- tpl (.Files.Get "files/program.tpl") . -}}
        {{- end -}}
    "#};
    assert_unknown_helper_output(source, r#"{{ include "render-file" . }}"#);
}

#[test]
fn separate_invocations_do_not_replay_a_cached_writer() {
    let source = indoc! {r#"
        {{- define "append-once" -}}
        {{- $_ := set . "items" (append .items "x") -}}
        {{- end -}}
    "#};
    let invocation = indoc! {r#"
        {{- $d := dict "items" (list) -}}
        {{- $_ := include "append-once" $d -}}
        {{- $first := len $d.items -}}
        {{- $_ := include "append-once" $d -}}
        {{- $first }}{{ len $d.items -}}
    "#};
    sim_assert_eq!(have: helper_outputs(source, invocation), want: vec![Some("12".to_owned())]);
}

#[test]
fn repeated_leaf_calls_observe_current_inputs() {
    let source = indoc! {r#"
        {{- define "leaf" -}}
        {{- .Values.value -}}
        {{- end -}}
    "#};
    let invocation = indoc! {r#"
        {{- $d := dict "Values" (dict "value" "first") -}}
        {{- include "leaf" $d -}}
        {{- $_ := set $d.Values "value" "second" -}}
        {{- include "leaf" $d -}}
    "#};
    sim_assert_eq!(have: helper_outputs(source, invocation), want: vec![Some("firstsecond".to_owned())]);
}

#[test]
fn extracts_define_blocks_with_exact_body_spans() {
    let src = indoc::indoc! {r#"
        {{- define "outer" -}}
        before
        {{- define "inner" -}}
        inside
        {{- end -}}
        after
        {{- end -}}
    "#};

    let blocks = extract_define_blocks(src);
    sim_assert_eq!(have: blocks.len(), want: 2);
    sim_assert_eq!(have: blocks[0].name, want: "outer");
    sim_assert_eq!(have: blocks[1].name, want: "inner");
    sim_assert_eq!(
        have: &src[blocks[0].body_offset..blocks[0].body_offset + blocks[0].body.len()],
        want: blocks[0].body
    );
    sim_assert_eq!(
        have: &src[blocks[1].body_offset..blocks[1].body_offset + blocks[1].body.len()],
        want: blocks[1].body
    );
    assert!(blocks[0].body.contains("before"));
    assert!(blocks[0].body.contains("after"));
    assert!(blocks[0].body.contains(r#"{{- define "inner" -}}"#));
    sim_assert_eq!(have: blocks[1].body.trim(), want: "inside");
}

#[test]
fn extracts_define_blocks_without_comment_masking_heuristics() {
    let src = indoc::indoc! {r#"
        {{- define "x" -}}
        {{/* {{ end }} should not terminate the define */}}
        value
        {{- end -}}
    "#};

    let blocks = extract_define_blocks(src);
    sim_assert_eq!(have: blocks.len(), want: 1);
    sim_assert_eq!(have: blocks[0].name, want: "x");
    assert!(blocks[0].body.contains("should not terminate"));
    sim_assert_eq!(
        have: &src[blocks[0].body_offset..blocks[0].body_offset + blocks[0].body.len()],
        want: blocks[0].body
    );
}

#[test]
fn extracts_single_line_define_body_between_actions() {
    let src = r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#;

    let blocks = extract_define_blocks(src);
    sim_assert_eq!(have: blocks.len(), want: 1);
    sim_assert_eq!(have: blocks[0].name.as_str(), want: "common.name");
    sim_assert_eq!(have: blocks[0].body.as_str(), want: "{{ .Values.nameOverride }}");
    sim_assert_eq!(have: blocks[0].body_offset, want: 28);
}

const WORKERS_MERGE_DEFINE: &str = indoc! {r#"
    {{- define "workersMergeValues" -}}
      {{- $inputMap := index . 0 -}}
      {{- $overwriteMap := index . 1 -}}
      {{- $sectionName := index . 2 -}}
      {{- $orBoolean := index . 3 -}}
      {{- $outputMap := dict -}}

      {{- $fullOverwrite := list "annotations" "labels" "resources" -}}

      {{- range $key, $val := $inputMap -}}
        {{- if and (hasKey $overwriteMap $key) (has $key $fullOverwrite) -}}
          {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
        {{- else if and (hasKey $overwriteMap $key) (kindIs "map" $val) -}}
          {{- $nested := include "workersMergeValues" (list $val (get $overwriteMap $key) $key $orBoolean) | fromYaml -}}
          {{- if gt (len $nested) 0 -}}
            {{- $_ := set $outputMap $key $nested -}}
          {{- end -}}
        {{- else if and (hasKey $overwriteMap $key) (not (and (kindIs "slice" (get $overwriteMap $key)) (eq (len (get $overwriteMap $key)) 0))) -}}
          {{- if and (kindIs "bool" $val) (has $sectionName $orBoolean) -}}
            {{- $_ := set $outputMap $key (or $val (get $overwriteMap $key)) -}}
          {{- else -}}
            {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
          {{- end -}}
        {{- else -}}
          {{- $_ := set $outputMap $key $val -}}
        {{- end -}}
      {{- end -}}
      {{- range $key, $val := $overwriteMap -}}
        {{- if not (hasKey $inputMap $key) -}}
          {{- $_ := set $outputMap $key $val -}}
        {{- end -}}
      {{- end -}}
      {{- toYaml $outputMap -}}
    {{- end -}}
"#};

const PARSED_MAP_MERGE_DEFINES: &str = indoc! {r#"
    {{- define "render" -}}
    {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
    {{- if contains "{{" (toJson .value) }}
      {{- if .scope }}
        {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
      {{- else }}
        {{- tpl $value .context }}
      {{- end }}
    {{- else }}
      {{- $value }}
    {{- end }}
    {{- end -}}

    {{- define "merge" -}}
    {{- $dst := dict -}}
    {{- range .values -}}
    {{- $dst = include "render" (dict "value" . "context" $.context "scope" $.scope) | fromYaml | merge $dst -}}
    {{- end -}}
    {{ $dst | toYaml }}
    {{- end -}}
"#};

#[test]
fn recursive_pair_merge_body_executes_without_a_shape_shortcut() {
    let invocation = r#"{{ include "workersMergeValues" (list .Values.input .Values.overwrite "section" (list)) }}"#;
    let outputs = helper_outputs(WORKERS_MERGE_DEFINE, invocation);
    assert!(
        outputs.iter().any(Option::is_none),
        "unknown iterations must retain unknown output: {outputs:?}"
    );
}

#[test]
fn pair_merge_near_miss_executes_the_authored_body() {
    let source = WORKERS_MERGE_DEFINE.replace(
        "set $outputMap $key $val",
        "set $outputMap $key .Values.injected",
    );
    assert!(source.contains(".Values.injected"));
    let invocation = r#"{{ include "workersMergeValues" (list .Values.input .Values.overwrite "section" (list)) }}"#;
    let outputs = helper_outputs(&source, invocation);
    assert!(
        outputs.iter().any(Option::is_none),
        "unknown body execution must retain unknown output: {outputs:?}"
    );
}

#[test]
fn parsed_map_list_body_executes_without_a_recognizer_result() {
    let invocation = r#"{{ include "merge" (dict "values" .Values.layers "context" .) }}"#;
    let outputs = helper_outputs(PARSED_MAP_MERGE_DEFINES, invocation);
    assert!(
        outputs.iter().any(Option::is_none),
        "unknown layers must retain unknown output: {outputs:?}"
    );
}

#[test]
fn array_decoder_near_miss_executes_the_authored_program() {
    let source = PARSED_MAP_MERGE_DEFINES.replace("fromYaml", "fromYamlArray");
    let invocation = r#"{{ include "merge" (dict "values" .Values.layers "context" .) }}"#;
    let outputs = helper_outputs(&source, invocation);
    assert!(
        outputs.iter().any(Option::is_none),
        "unknown body execution must retain unknown output: {outputs:?}"
    );
}

#[test]
fn parsed_defines_share_parses_and_preserve_file_roles() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "leaf" }}value{{ end }}"#,
    );
    let parsed = ParsedDefines::new(&defines);
    let db = IrAnalysisDb::with_parsed_policy(&parsed, crate::symbolic::SymbolicPolicy::default());
    sim_assert_eq!(have: parsed.define_body("leaf"), want: Some("value"));
    assert!(db.has_helper("leaf"));
    assert!(db.file_source("templates/_helpers.tpl").is_some());
    sim_assert_eq!(have: db.file_source_paths(), want: vec!["templates/_helpers.tpl"]);
}

/// Caps bound helper values before recursive structure causes combinatorial expansion.
///
/// Corpus measurement placed the nearest precise value at width 31 and the first widened value at
/// width 58.
/// A synthetic 256-leaf value reproduced the preflight blowup this bound prevents.
pub(crate) const BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT: usize = 32;

pub(crate) fn widen_large_bound_value_ref(
    binding: &AbstractValue,
    helper_name: &str,
    widened_paths: &mut BTreeSet<String>,
) -> Option<AbstractValue> {
    let width = binding.structural_width();
    if width <= BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT {
        return None;
    }

    tracing::debug!(
        helper = helper_name,
        structural_width = width,
        structural_width_limit = BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT,
        "widening bound helper value"
    );
    widened_paths.extend(binding.paths().iter().map(ValuesPath::encode));
    Some(AbstractValue::Top)
}

#[test]
fn repeated_helper_lookups_share_one_parsed_program() -> eyre::Result<()> {
    use color_eyre::eyre::OptionExt as _;
    let db = helper_db(r#"{{ define "leaf" }}{{ .Values.x }}{{ end }}"#);
    let first = db
        .parsed_helper_body("leaf")
        .ok_or_eyre("first helper missing")?;
    let second = db
        .parsed_helper_body("leaf")
        .ok_or_eyre("second helper missing")?;
    assert!(std::ptr::eq(first.syntax, second.syntax));
    assert!(std::ptr::eq(
        &raw const first.syntax.tree,
        &raw const second.syntax.tree
    ));
    assert!(std::ptr::eq(
        &raw const first.syntax.expressions,
        &raw const second.syntax.expressions
    ));
    Ok(())
}
