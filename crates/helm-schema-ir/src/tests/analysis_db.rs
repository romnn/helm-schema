use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::{IrAnalysisDb, extract_define_blocks};
use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::fragment_eval::summary::FragmentSummary;
use crate::fragment_expr_eval::FragmentEvalContext;
use helm_schema_core::ValuesPath;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[test]
fn entry_files_reuse_name_independent_helper_summaries() {
    for order in [
        ["chart/templates/a.yaml", "chart/templates/b.yaml"],
        ["chart/templates/b.yaml", "chart/templates/a.yaml"],
    ] {
        let mut defines = helm_schema_ast::DefineIndex::new();
        defines.add_file_source(
            "chart/templates/_helpers.tpl",
            r#"{{ define "entry" }}{{ .Values.token | quote }}{{ end }}"#,
        );
        let source = r#"{{ include "entry" . }}"#;
        for path in order {
            defines.add_file_source(path, source);
        }
        let db = IrAnalysisDb::new(&defines);
        for path in order {
            let _ = crate::fragment_eval::eval_document(source, Some(path), &db);
        }
        let keys = db.bound_helper_calls.borrow();
        let calls: Vec<_> = keys.keys().filter(|key| key.name == "entry").collect();
        sim_assert_eq!(have: calls.len(), want: 1, "keys: {calls:#?}");
    }
}

fn caller_bindings(name: &str, base_path: &str) -> HashMap<String, AbstractValue> {
    let string = |value: &str| AbstractValue::StringSet(BTreeSet::from([value.to_string()]));
    HashMap::from([
        ("Values".to_string(), AbstractValue::values_root()),
        (
            "Chart".to_string(),
            AbstractValue::Dict(BTreeMap::from([("Name".to_string(), string("chart"))])),
        ),
        (
            "Template".to_string(),
            AbstractValue::Dict(BTreeMap::from([
                ("Name".to_string(), string(name)),
                ("BasePath".to_string(), string(base_path)),
            ])),
        ),
    ])
}

fn summarize_with_bindings(
    db: &IrAnalysisDb,
    bindings: &HashMap<String, AbstractValue>,
    argument: &helm_schema_ast::TemplateExpr,
) -> Rc<FragmentSummary> {
    let env = EvalEnv {
        dot: Some(AbstractValue::RootContext),
        root_fields: bindings.clone(),
        ..EvalEnv::default()
    };
    db.summarize_bound_helper_call(
        "entry",
        Some(argument),
        Some(bindings),
        Some(&AbstractValue::RootContext),
        &env,
        FragmentEvalContext::new(db),
        &mut HashSet::new(),
    )
    .summary
}

fn assert_summary_semantics_equal(
    have: &FragmentSummary,
    have_db: &IrAnalysisDb,
    want: &FragmentSummary,
    want_db: &IrAnalysisDb,
) {
    sim_assert_eq!(have: &have.root, want: &want.root);
    sim_assert_eq!(have: have.root_render_indent, want: want.root_render_indent);
    sim_assert_eq!(have: &have.value, want: &want.value);
    sim_assert_eq!(have: &have.json_payload_truth, want: &want.json_payload_truth);
    sim_assert_eq!(have: &have.pre_rewrite_strict_paths, want: &want.pre_rewrite_strict_paths);
    sim_assert_eq!(have: have.expression_effects(), want: want.expression_effects());
    sim_assert_eq!(have: have.scalar_dispatch(have_db), want: want.scalar_dispatch(want_db));
}

#[test]
fn unobservable_caller_names_share_summaries_in_both_orders() {
    for body in [
        "literal",
        "{{ .Chart.Name }}",
        "{{ .Values.value }}",
        "{{ $name := .Chart.Name }}{{ $name | upper }}",
        "{{ include \"leaf\" . }}",
    ] {
        let source = format!(
            r#"{{{{ define "entry" }}}}{body}{{{{ end }}}}{{{{ define "leaf" }}}}{{{{ .Values.value }}}}{{{{ end }}}}"#
        );
        for order in [["a.yaml", "b.yaml"], ["b.yaml", "a.yaml"]] {
            let db = helper_db(&source);
            let argument = helm_schema_ast::TemplateExpr::Field(Vec::new());
            let first = summarize_with_bindings(
                &db,
                &caller_bindings(order[0], "chart/templates"),
                &argument,
            );
            let second = summarize_with_bindings(
                &db,
                &caller_bindings(order[1], "chart/templates"),
                &argument,
            );
            sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: true, "body: {body}");
            for (name, cached) in [(order[0], first), (order[1], second)] {
                let uncached_db = helper_db(&source);
                let uncached = summarize_with_bindings(
                    &uncached_db,
                    &caller_bindings(name, "chart/templates"),
                    &argument,
                );
                assert_summary_semantics_equal(&cached, &db, &uncached, &uncached_db);
            }
        }
    }
}

#[test]
fn observable_or_unproved_caller_names_keep_separate_summaries() {
    for body in [
        "{{ .Template.Name }}",
        "{{ with .Template }}{{ .Name }}{{ end }}",
        "{{ get (get . \"Template\") \"Name\" }}",
        "{{ index . \"Template\" \"Name\" }}",
        "{{ index . .Values.key }}",
        "{{ toYaml . }}",
        "{{ $root := . }}{{ $root.Template.Name }}",
        "{{ dict \"root\" . | toJson }}",
        "{{ $_ := set . \"saved\" .Template.Name }}",
        "{{ tpl \"{{ .Template.Name }}\" . }}",
        "{{ include .Values.helper . }}",
        "{{ include \"sensitive\" . }}",
        "{{ 0x1p10000 }}",
    ] {
        let source = format!(
            r#"{{{{ define "entry" }}}}{body}{{{{ end }}}}{{{{ define "sensitive" }}}}{{{{ .Template.Name }}}}{{{{ end }}}}"#
        );
        for order in [["a.yaml", "b.yaml"], ["b.yaml", "a.yaml"]] {
            let db = helper_db(&source);
            let argument = helm_schema_ast::TemplateExpr::Field(Vec::new());
            let first = summarize_with_bindings(
                &db,
                &caller_bindings(order[0], "chart/templates"),
                &argument,
            );
            let second = summarize_with_bindings(
                &db,
                &caller_bindings(order[1], "chart/templates"),
                &argument,
            );
            sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false, "body: {body}");
            for (name, cached) in [(order[0], first), (order[1], second)] {
                let uncached_db = helper_db(&source);
                let uncached = summarize_with_bindings(
                    &uncached_db,
                    &caller_bindings(name, "chart/templates"),
                    &argument,
                );
                assert_summary_semantics_equal(&cached, &db, &uncached, &uncached_db);
            }
        }
    }
}

#[test]
fn caller_name_projection_preserves_base_path_and_nested_root_carriers() {
    let source = r#"{{ define "entry" }}literal{{ end }}"#;
    let db = helper_db(source);
    let argument = helm_schema_ast::TemplateExpr::Field(Vec::new());
    let first = summarize_with_bindings(&db, &caller_bindings("a", "first/templates"), &argument);
    let second = summarize_with_bindings(&db, &caller_bindings("b", "second/templates"), &argument);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);

    let db = helper_db(r#"{{ define "entry" }}{{ toYaml .alias }}{{ end }}"#);
    let mut first_bindings = caller_bindings("a", "chart/templates");
    first_bindings.insert("alias".to_string(), AbstractValue::RootContext);
    let mut second_bindings = caller_bindings("b", "chart/templates");
    second_bindings.insert("alias".to_string(), AbstractValue::RootContext);
    let first = summarize_with_bindings(&db, &first_bindings, &argument);
    let second = summarize_with_bindings(&db, &second_bindings, &argument);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);

    let db = helper_db(source);
    let nested_argument = helm_schema_ast::TemplateExpr::Call {
        function: "dict".to_string(),
        args: vec![
            helm_schema_ast::TemplateExpr::Literal(helm_schema_ast::Literal::String(
                "context".to_string(),
            )),
            helm_schema_ast::TemplateExpr::Variable(String::new()),
        ],
    };
    let first = summarize_with_bindings(
        &db,
        &caller_bindings("a", "chart/templates"),
        &nested_argument,
    );
    let second = summarize_with_bindings(
        &db,
        &caller_bindings("b", "chart/templates"),
        &nested_argument,
    );
    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);

    let db = helper_db(source);
    first_bindings.insert("alias".to_string(), first_bindings["Template"].clone());
    second_bindings.insert("alias".to_string(), second_bindings["Template"].clone());
    let first = summarize_with_bindings(&db, &first_bindings, &argument);
    let second = summarize_with_bindings(&db, &second_bindings, &argument);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);
}

fn helper_db(source: &str) -> IrAnalysisDb {
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", source);
    IrAnalysisDb::new(&defines)
}

fn summarize_helper(
    db: &IrAnalysisDb,
    name: &str,
    initial_seen: impl IntoIterator<Item = &'static str>,
) -> Rc<FragmentSummary> {
    let mut seen = initial_seen
        .into_iter()
        .map(str::to_string)
        .collect::<HashSet<_>>();
    db.summarize_bound_helper_call(
        name,
        None,
        None,
        None,
        &EvalEnv::default(),
        FragmentEvalContext::new(db),
        &mut seen,
    )
    .summary
}

#[test]
fn direct_recursion_keeps_the_helper_in_its_cycle_cut_key() {
    let db = helper_db(indoc! {r#"
        {{- define "direct" -}}
        {{- include "direct" . -}}
        {{- end -}}
    "#});
    let summary = summarize_helper(&db, "direct", []);

    sim_assert_eq!(have: db.bound_helper_calls.borrow().len(), want: 1);
    sim_assert_eq!(have: Rc::strong_count(&summary), want: 2);
}

#[test]
fn conditional_recursion_is_in_the_conservative_cycle_cut_key() {
    let db = helper_db(indoc! {r#"
        {{- define "conditional" -}}
        {{- if .Values.enabled -}}
        {{- include "conditional" . -}}
        {{- end -}}
        {{- end -}}
    "#});
    let seen = HashSet::from(["conditional".to_string(), "unrelated".to_string()]);

    sim_assert_eq!(
        have: db.helper_seen_key("conditional", &seen),
        want: BTreeSet::from(["conditional".to_string()])
    );
}

#[test]
fn mutual_recursion_keeps_every_reachable_cycle_cut() {
    let db = helper_db(indoc! {r#"
        {{- define "a" -}}
        {{- if .Values.enabled -}}{{ include "b" . }}{{- end -}}
        {{- end -}}
        {{- define "b" -}}
        {{- include "a" . -}}
        {{- end -}}
    "#});
    let summary = summarize_helper(&db, "a", []);
    let seen = HashSet::from(["a".to_string(), "b".to_string(), "unrelated".to_string()]);

    sim_assert_eq!(have: db.bound_helper_calls.borrow().len(), want: 2);
    sim_assert_eq!(have: Rc::strong_count(&summary), want: 2);
    sim_assert_eq!(
        have: db.helper_seen_key("a", &seen),
        want: BTreeSet::from(["a".to_string(), "b".to_string()])
    );
}

#[test]
fn relevant_cycle_cuts_do_not_share_helper_summaries() {
    let db = helper_db(indoc! {r#"
        {{- define "a" -}}
        {{- .Values.from_a -}}
        {{- include "target" . -}}
        {{- end -}}
        {{- define "target" -}}
        {{- include "a" . -}}
        {{- end -}}
    "#});

    let cut = summarize_helper(&db, "target", ["a"]);
    let uncut = summarize_helper(&db, "target", ["unrelated"]);
    let cut_paths = cut
        .rendered
        .iter()
        .map(|row| row.path.clone())
        .collect::<Vec<_>>();
    let uncut_paths = uncut
        .rendered
        .iter()
        .map(|row| row.path.clone())
        .collect::<Vec<_>>();

    sim_assert_eq!(have: Rc::ptr_eq(&cut, &uncut), want: false);
    sim_assert_eq!(have: cut_paths, want: Vec::<ValuesPath>::new());
    sim_assert_eq!(have: uncut_paths, want: vec![ValuesPath::parse("from_a")]);
}

#[test]
fn dynamic_helper_names_retain_the_whole_active_chain() {
    let db = helper_db(indoc! {r#"
        {{- define "dynamic" -}}
        {{- include .Values.helper . -}}
        {{- include "dynamic" . -}}
        {{- end -}}
    "#});
    let seen = HashSet::from(["dynamic".to_string(), "caller".to_string()]);

    sim_assert_eq!(
        have: db.helper_seen_key("dynamic", &seen),
        want: BTreeSet::from(["caller".to_string(), "dynamic".to_string()])
    );
    let first = summarize_helper(&db, "dynamic", ["caller-one"]);
    let second = summarize_helper(&db, "dynamic", ["caller-two"]);

    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);
    sim_assert_eq!(have: db.bound_helper_calls.borrow().len(), want: 2);
}

#[test]
fn unknown_expressions_retain_the_whole_active_chain() {
    let db = helper_db(indoc! {r#"
        {{- define "unknown" -}}
        {{- 0x1p10000 -}}
        {{- end -}}
    "#});
    let seen = HashSet::from(["unknown".to_string(), "caller".to_string()]);

    sim_assert_eq!(
        have: db.helper_seen_key("unknown", &seen),
        want: BTreeSet::from(["caller".to_string(), "unknown".to_string()])
    );
}

#[test]
fn tpl_programs_retain_the_whole_active_chain() {
    let db = helper_db(indoc! {r#"
        {{- define "render-file" -}}
        {{- tpl (.Files.Get "files/program.tpl") . -}}
        {{- end -}}
    "#});
    let seen = HashSet::from(["render-file".to_string(), "caller".to_string()]);

    sim_assert_eq!(
        have: db.helper_seen_key("render-file", &seen),
        want: BTreeSet::from(["caller".to_string(), "render-file".to_string()])
    );
    let first = summarize_helper(&db, "render-file", ["caller-one"]);
    let second = summarize_helper(&db, "render-file", ["caller-two"]);

    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: false);
    sim_assert_eq!(have: db.bound_helper_calls.borrow().len(), want: 2);
}

#[test]
fn irrelevant_caller_chains_share_one_helper_summary() {
    let db = helper_db(indoc! {r#"
        {{- define "leaf" -}}
        {{- .Values.value -}}
        {{- end -}}
    "#});

    let first = summarize_helper(&db, "leaf", ["caller-one"]);
    let second = summarize_helper(&db, "leaf", ["caller-two"]);

    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: true);
    sim_assert_eq!(have: db.bound_helper_calls.borrow().len(), want: 1);
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
