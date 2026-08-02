use super::extract_define_blocks;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

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

#[test]
fn recognizes_recursive_custom_merge_helper() {
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", WORKERS_MERGE_DEFINE);
    let db = super::IrAnalysisDb::new(&defines);
    assert!(db.custom_merge_helper("workersMergeValues").is_some());
}

#[test]
fn merge_recognition_requires_accumulator_discipline() {
    // A `set` writing something OTHER than the two maps' members breaks
    // the merge contract, so recognition must abstain.
    let source = WORKERS_MERGE_DEFINE.replace(
        "set $outputMap $key $val",
        "set $outputMap $key .Values.injected",
    );
    assert!(source.contains(".Values.injected"));
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", &source);
    let db = super::IrAnalysisDb::new(&defines);
    assert!(db.custom_merge_helper("workersMergeValues").is_none());
}

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
fn recognizes_parsed_map_list_merge_helper() {
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", PARSED_MAP_MERGE_DEFINES);
    let db = super::IrAnalysisDb::new(&defines);

    assert!(matches!(
        db.custom_merge_helper("merge"),
        Some(super::CustomMergeHelper::ParsedMapList)
    ));
}

#[test]
fn parsed_map_merge_recognition_requires_map_only_decode() {
    let source = PARSED_MAP_MERGE_DEFINES.replace("fromYaml", "fromYamlArray");
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", &source);
    let db = super::IrAnalysisDb::new(&defines);

    assert!(db.custom_merge_helper("merge").is_none());
}
