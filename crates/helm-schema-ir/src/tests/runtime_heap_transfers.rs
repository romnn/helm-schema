use std::fmt::Write as _;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use test_util::prelude::sim_assert_eq;

fn texts(source: &str) -> Vec<Option<String>> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    execute_document(source, Some("templates/test.yaml"), &db)
        .text()
        .map(|(_, text)| text.exact_text())
        .collect()
}

#[test]
fn merge_installs_source_references_but_retains_an_existing_destination_map() {
    sim_assert_eq!(
        have: texts(r#"{{ $source := dict "m" (dict "x" 0) }}{{ $destination := dict }}{{ $_ := merge $destination $source }}{{ $_ := set $destination.m "x" 4 }}{{ $source.m.x }}"#),
        want: vec![Some("4".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ $destination := dict "m" (dict "x" 1) }}{{ $source := dict "m" (dict "y" 2) }}{{ $_ := merge $destination $source }}{{ $_ := set $destination.m "y" 4 }}{{ $source.m.y }}/{{ $destination.m.x }}"#),
        want: vec![Some("2/1".to_owned())]
    );
}

#[test]
fn merge_precedence_and_empty_fields_follow_consumed_values() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict "x" 0 "s" "" }}{{ $_ := merge $d (dict "x" 2 "s" "filled") (dict "x" 3) }}{{ $d.x }}/{{ $d.s }}"#),
        want: vec![Some("2/filled".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ $nested := dict "x" 1 }}{{ $d := dict "m" $nested }}{{ $_ := mergeOverwrite $d (dict "m" (dict "x" 2)) }}{{ $nested.x }}"#),
        want: vec![Some("2".to_owned())]
    );
}

#[test]
fn nil_merge_destination_is_allocated_only_for_a_nonnil_source() {
    for function in ["merge", "mustMerge", "mergeOverwrite", "mustMergeOverwrite"] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := fromJson "null" }}}}{{{{ $r := {function} $d (dict "x" "yes") }}}}{{{{ $r.x }}}}/{{{{ len $d }}}}"#)),
            want: vec![Some("yes/0".to_owned())]
        );
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := fromJson "null" }}}}{{{{ $r := {function} $d (fromJson "null") }}}}{{{{ len $r }}}}"#)),
            want: vec![Some("0".to_owned())]
        );
    }
}

#[test]
fn map_selection_and_pluck_copy_outer_structure_and_share_nested_maps() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := dict "x" 0 }}{{ $d := dict "m" $m }}{{ $o := omit $d "none" }}{{ $p := pluck "m" (pick $o "m") }}{{ $_ := set (index $p 0) "x" 2 }}{{ $m.x }}"#),
        want: vec![Some("2".to_owned())]
    );
}

#[test]
fn copying_lists_freeze_string_elements_and_share_nested_maps() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := splitList "," "b,a" }}{{ $c := append $s "c" }}{{ $_ := sortAlpha $s }}{{ index $c 0 }}"#),
        want: vec![Some("b".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ $m := dict "x" 0 }}{{ $a := list $m }}{{ $b := prepend (append $a 9) 8 }}{{ $_ := set (index $b 1) "x" 3 }}{{ len $a }}/{{ $m.x }}"#),
        want: vec![Some("1/3".to_owned())]
    );
}

#[test]
fn aliased_merge_order_does_not_choose_one_map_traversal() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := dict "x" 0 }}{{ $d := dict "a" $m "b" $m }}{{ $_ := mergeOverwrite $d (dict "a" (dict "x" 1) "b" (dict "x" 2)) }}{{ $m.x }}"#),
        want: vec![None]
    );
}

#[test]
fn empty_slice_ignores_indices_after_their_evaluation() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict }}{{ $_ := slice (list) -1 99 (set $d "x" "yes") }}{{ $d.x }}"#),
        want: vec![Some("yes".to_owned())]
    );
}

#[test]
fn pure_calls_preserve_absent_present_and_conditional_membership() -> eyre::Result<()> {
    use helm_schema_core::{GuardValue, Predicate};
    use std::collections::BTreeSet;

    for call in [
        r"randAlphaNum 1",
        r#"printf "%d" .Values.number"#,
        "now",
        "uuidv4",
    ] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict "x" 1 }}}}{{{{ $_ := unset $d "x" }}}}{{{{ $_ := {call} }}}}{{{{ hasKey $d "x" }}}}"#)),
            want: vec![Some("false".to_owned())]
        );
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict "x" 1 }}}}{{{{ $_ := {call} }}}}{{{{ hasKey $d "x" }}}}"#)),
            want: vec![Some("true".to_owned())]
        );
        let source = format!(
            r#"{{{{ $d := dict }}}}{{{{ if .Values.choose }}}}{{{{ $_ := set $d "x" 1 }}}}{{{{ end }}}}{{{{ $_ := {call} }}}}{{{{ hasKey $d "x" }}}}"#
        );
        let execution = execute_document(
            &source,
            Some("templates/test.yaml"),
            &IrAnalysisDb::new(&DefineIndex::new()),
        );
        let mut present = BTreeSet::new();
        let mut absent = BTreeSet::new();
        for site in execution.sites().filter(|site| site.rendered()) {
            for alternative in site.value().alternatives() {
                let condition = Predicate::all(vec![
                    site.condition().clone(),
                    alternative.condition().clone(),
                ])
                .normalize_boolean();
                match alternative.value().literal() {
                    Some(GuardValue::Bool(true)) => {
                        present.insert(condition);
                    }
                    Some(GuardValue::Bool(false)) => {
                        absent.insert(condition);
                    }
                    Some(
                        GuardValue::Null
                        | GuardValue::String(_)
                        | GuardValue::Int(_)
                        | GuardValue::Float(_),
                    )
                    | None => eyre::bail!("membership lost its pure-call proof"),
                }
            }
        }
        sim_assert_eq!(have: present, want: BTreeSet::from([Predicate::truthy_path("choose")]));
        sim_assert_eq!(have: absent, want: BTreeSet::from([Predicate::truthy_path("choose").negated()]));
    }
    Ok(())
}

fn writer_defines() -> DefineIndex {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_writers.tpl",
        indoc::indoc!(
            r#"
        {{ define "writer.keep" }}{{ end }}
        {{ define "writer.insert" }}{{ $_ := set . "x" 1 }}{{ end }}
        {{ define "writer.remove" }}{{ $_ := unset . "x" }}{{ end }}
        {{ define "writer.nested" }}{{ $_ := set .inner "x" .replacement }}{{ end }}
    "#
        ),
    );
    defines
}

#[test]
fn unresolved_writers_do_not_manufacture_map_membership() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&writer_defines());
    for writer in [
        r"{{ $_ := include (toString .Values.helper) $d }}",
        r"{{ $_ := tpl (toString .Values.program) $d }}",
    ] {
        for source in [
            format!(
                r#"{{{{ $d := dict "x" 1 }}}}{{{{ $_ := unset $d "x" }}}}{writer}{{{{ hasKey $d "x" }}}}"#
            ),
            format!(r#"{{{{ $d := dict "x" 1 }}}}{writer}{{{{ hasKey $d "x" }}}}"#),
            format!(
                r#"{{{{ $d := dict }}}}{{{{ if .Values.choose }}}}{{{{ $_ := set $d "x" 1 }}}}{{{{ end }}}}{writer}{{{{ hasKey $d "x" }}}}"#
            ),
        ] {
            let execution = execute_document(&source, Some("templates/test.yaml"), &db);
            let site = execution
                .sites()
                .last()
                .ok_or_eyre("missing membership observation")?;
            sim_assert_eq!(have: site.value().literal(), want: None);
            assert!(site.value().identity().is_none());
            sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::False);
        }
    }
    Ok(())
}

#[test]
fn pure_call_cannot_prove_the_original_ghost_key_abort() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $d := dict "x" 1 }}{{ $_ := unset $d "x" }}{{ $_ := randAlphaNum 1 }}{{ if hasKey $d "x" }}{{ fail "ghost key" }}{{ end }}ok"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::False);
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("ok".to_owned())]);
}

#[test]
fn unresolved_writer_preserves_transitive_input_influences() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&writer_defines());
    for writer in [
        r"{{ $_ := include (toString .Values.helper) $outer }}",
        r"{{ $_ := tpl (toString .Values.program) $outer }}",
    ] {
        let source = format!(
            r#"{{{{ $inner := dict "x" .Values.seed }}}}{{{{ $outer := dict "inner" $inner "replacement" .Values.replacement }}}}{writer}{{{{ $outer.inner.x }}}}"#
        );
        let execution = execute_document(&source, Some("templates/test.yaml"), &db);
        let site = execution.sites().last().ok_or_eyre("missing output site")?;
        let influences = site.value().inputs();
        assert!(influences.contains(&helm_schema_core::ValuesPath::from_segments(["seed"])));
        assert!(
            influences.contains(&helm_schema_core::ValuesPath::from_segments([
                "replacement"
            ]))
        );
        assert!(site.value().identity().is_none());
    }
    Ok(())
}

#[test]
fn slice_views_preserve_capacity_and_ignore_extra_indices() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := splitList "," "c,b,a" }}{{ $v := slice $s 0 1 1 }}{{ join "," (slice $v 0 3) }}"#),
        want: vec![Some("c,b,a".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ $s := list "a" "b" "c" }}{{ $v := slice $s 1 2 }}{{ toJson (slice $v 0 2) }}"#),
        want: vec![Some(r#"["b","c"]"#.to_owned())]
    );
}

#[test]
fn string_subview_sort_uses_live_slots_and_leaves_the_prefix_untouched() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := splitList "," "z,b,a" }}{{ $v := slice $s 1 3 }}{{ $slot := index $v 0 }}{{ $slot }}|{{ $_ := sortAlpha $v }}{{ $slot }}|{{ join "," $s }}"#),
        want: vec![Some("b|a|z,a,b".to_owned())]
    );
}

#[test]
fn copied_windows_keep_capacity_with_a_zeroed_tail() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := splitList "," "a,b,c" }}{{ $v := slice $s 0 1 }}{{ $c := deepCopy $v }}{{ join "," (slice $c 0 3) }}"#),
        want: vec![Some("a,,".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ $s := list "a" "b" "c" }}{{ $v := slice $s 0 1 }}{{ $c := deepCopy $v }}{{ toJson (slice $c 0 3) }}"#),
        want: vec![Some(r#"["a",null,null]"#.to_owned())]
    );
}

#[test]
fn to_strings_skips_nil_members_and_retains_zeroed_capacity() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := toStrings (list "a" nil "b") }}{{ join "," $s }}|{{ join "," (slice $s 0 3) }}"#),
        want: vec![Some("a,b|a,b,".to_owned())]
    );
}

#[test]
fn unknown_dict_keys_do_not_mutate_an_existing_heap() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict "x" "kept" }}{{ $_ := dict .Values.key .Values.value }}{{ $d.x }}"#),
        want: vec![Some("kept".to_owned())]
    );
}

#[test]
fn unproved_capacity_never_manufactures_a_bounds_rejection() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ $s := mustFromJson "[1,2,3]" }}{{ $_ := slice $s 0 4 }}ok"#,
        r"{{ $s := append (list 1 2 3) 4 }}{{ $_ := slice $s 0 6 }}ok",
    ] {
        let execution = execute_document(source, Some("templates/test.yaml"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::False);
    }
    let execution = execute_document(
        r"{{ $_ := slice (list 1 2 3) 0 4 }}",
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::True);
}

#[test]
fn concrete_non_sequence_intrinsics_abort_but_empty_concat_is_typed_nil() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r"{{ first (dict) }}",
        r#"{{ last "text" }}"#,
        r"{{ slice 1 0 }}",
        r#"{{ append (dict) "x" }}"#,
        r"{{ concat (list) (dict) }}",
    ] {
        let execution = execute_document(source, Some("templates/test.yaml"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::True);
    }
    sim_assert_eq!(
        have: texts(r"{{ eq (concat) nil }}"),
        want: vec![Some("true".to_owned())]
    );
}

#[test]
fn unknown_decoders_preserve_format_nilness_and_allocate_fresh_maps() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for (function, abort) in [
        ("fromJson", helm_schema_core::Predicate::False),
        ("fromYaml", helm_schema_core::Predicate::True),
    ] {
        let source = format!(
            r#"{{{{ $v := {function} (toString .Values.encoded) }}}}{{{{ if ne $v nil }}}}{{{{ fail "not nil" }}}}{{{{ end }}}}ok"#
        );
        let execution = execute_document(&source, Some("templates/test.yaml"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: abort);
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict "x" "kept" }}}}{{{{ $v := {function} (toString .Values.encoded) }}}}{{{{ $_ := set $v "x" "new" }}}}{{{{ $d.x }}}}"#)),
            want: vec![Some("kept".to_owned())]
        );
    }
}

#[test]
fn non_overwrite_merge_skips_nil_interface_sources_without_ghost_membership() {
    for function in ["merge", "mustMerge"] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict }}}}{{{{ $_ := {function} $d (dict "x" nil) }}}}{{{{ if hasKey $d "x" }}}}{{{{ fail "ghost" }}}}{{{{ end }}}}ok"#)),
            want: vec![Some("ok".to_owned())]
        );
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict "flag" false "empty" "" "typed" (fromJson "null") }}}}{{{{ $_ := {function} $d (dict "flag" nil "empty" nil "typed" nil) }}}}{{{{ toJson $d }}}}"#)),
            want: vec![Some(r#"{"empty":"","flag":false,"typed":null}"#.to_owned())]
        );
    }
    for function in ["mergeOverwrite", "mustMergeOverwrite"] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $d := dict }}}}{{{{ $_ := {function} $d (dict "x" nil) }}}}{{{{ hasKey $d "x" }}}}"#)),
            want: vec![Some("true".to_owned())]
        );
    }
}

#[test]
fn scalar_overwrite_recovers_a_fixed_key_on_an_open_input_map() {
    for function in ["mergeOverwrite", "mustMergeOverwrite"] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $_ := {function} .Values (dict "choose" false) (dict "choose" true) }}}}{{{{ .Values.choose }}}}"#)),
            want: vec![Some("true".to_owned())]
        );
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $_ := {function} .Values (dict "flag" false "empty" "" "nil" nil) }}}}{{{{ toJson (pick .Values "flag" "empty" "nil") }}}}"#)),
            want: vec![Some(r#"{"empty":"","flag":false,"nil":null}"#.to_owned())]
        );
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $old := dict "x" "kept" }}}}{{{{ $_ := set .Values "choose" $old }}}}{{{{ $_ := {function} .Values (dict "choose" true) }}}}{{{{ $old.x }}}}/{{{{ .Values.choose }}}}"#)),
            want: vec![Some("kept/true".to_owned())]
        );
    }
}

#[test]
fn returned_map_views_preserve_implicit_input_tail_influences() -> eyre::Result<()> {
    let root = helm_schema_core::ValuesPath::from_segments(std::iter::empty::<String>());
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ $r := set .Values "x" "new" }}{{ $r }}"#,
        r#"{{ $r := unset .Values "x" }}{{ $r }}"#,
        r#"{{ $r := mergeOverwrite .Values (dict "x" "new") }}{{ $r }}"#,
        r"{{ $r := merge .Values }}{{ $r }}",
    ] {
        let execution = execute_document(source, Some("templates/test.yaml"), &db);
        let site = execution
            .sites()
            .last()
            .ok_or_eyre("missing returned map site")?;
        assert!(
            site.value().inputs().contains(&root),
            "lost implicit input tail: {source}"
        );
    }
    Ok(())
}

#[test]
fn merge_without_sources_returns_an_unnamed_map_view() {
    sim_assert_eq!(
        have: texts(r"{{ typeOf (merge .Template) }}"),
        want: vec![Some("map[string]interface {}".to_owned())]
    );
}

#[test]
fn split_string_map_preserves_its_go_view_and_exact_unicode_members() {
    sim_assert_eq!(
        have: texts(r#"{{ typeOf (split "-" "a-b") }}|{{ toJson (split "-" "a-b") }}|{{ toJson (split "" "é😀x") }}"#),
        want: vec![Some(r#"map[string]string|{"_0":"a","_1":"b"}|{"_0":"é","_1":"😀","_2":"x"}"#.to_owned())]
    );
    for (count, expected) in [
        (-1, r#"{"_0":"a","_1":"b","_2":"c"}"#),
        (0, "{}"),
        (1, r#"{"_0":"a-b-c"}"#),
        (2, r#"{"_0":"a","_1":"b-c"}"#),
    ] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ toJson (splitn "-" {count} "a-b-c") }}}}"#)),
            want: vec![Some(expected.to_owned())]
        );
    }
    sim_assert_eq!(
        have: texts(r#"{{ toJson (splitn "" 2 "é😀x") }}|{{ toJson (splitn "" 1 "") }}"#),
        want: vec![Some(r#"{"_0":"é","_1":"😀x"}|{}"#.to_owned())]
    );
}

#[test]
fn string_map_signature_mismatch_is_distinct_from_values_and_unnamed_maps() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for operation in [
        r#"get $m "_0""#,
        "keys $m",
        r#"hasKey $m "_0""#,
        r#"set $m "_0" "new""#,
        r#"unset $m "_0""#,
        r#"merge $m (dict "_0" "new")"#,
        r#"mergeOverwrite $m (dict "_0" "new")"#,
    ] {
        let source = format!(r#"{{{{ $m := split "-" "a-b" }}}}{{{{ $_ := {operation} }}}}"#);
        sim_assert_eq!(
            have: execute_document(&source, Some("templates/test.yaml"), &db).abort_predicate(),
            want: helm_schema_core::Predicate::True
        );
        for map in [".Values", r#"dict "_0" "a" "_1" "b""#] {
            let source = format!(r"{{{{ $m := {map} }}}}{{{{ $_ := {operation} }}}}");
            sim_assert_eq!(
                have: execute_document(&source, Some("templates/test.yaml"), &db).abort_predicate(),
                want: helm_schema_core::Predicate::False
            );
        }
    }
}

#[test]
fn selected_map_views_abort_only_on_the_string_map_arm_after_bool_validation() {
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $m := ternary (split "-" "a-b") (dict "_0" "a") .Values.flag }}{{ $_ := get $m "_0" }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    let boolean = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("flag"),
        schema_type: "boolean".to_owned(),
    });
    let normal = Predicate::all(vec![boolean, Predicate::truthy_path("flag").negated()]);
    let expected = normal.negated();
    let actual = execution.abort_predicate();
    assert!(
        actual.exactly_implies(&expected),
        "unexpected abort: {actual:?}; expected {expected:?}"
    );
    assert!(
        expected.exactly_implies(&actual),
        "missing abort: {actual:?}; expected {expected:?}"
    );
}

#[test]
fn unknown_split_contents_keep_string_members_without_invalidating_old_objects() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict "x" "kept" }}{{ $m := split (toString .Values.separator) (toString .Values.text) }}{{ $d.x }}|{{ typeOf $m }}|{{ typeOf (index $m "missing") }}|{{ eq (index $m "missing") nil }}"#),
        want: vec![Some("kept|map[string]string|string|false".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r"{{ toJson (splitn (toString .Values.separator) 0 (toString .Values.text)) }}"),
        want: vec![Some("{}".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ toJson (splitn (toString .Values.separator) 1 "whole") }}"#),
        want: vec![Some(r#"{"_0":"whole"}"#.to_owned())]
    );
}

#[test]
fn string_map_lookup_of_an_absent_member_is_the_zero_string() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := split "-" "a-b" }}{{ typeOf (index $m "missing") }}/{{ eq (index $m "missing") nil }}/{{ len (index $m "missing") }}"#),
        want: vec![Some("string/false/0".to_owned())]
    );
}

#[test]
fn splitn_zero_does_not_suppress_eager_argument_abort() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ splitn "-" 0 (fail "eager argument") }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::True);
    sim_assert_eq!(
        have: texts(r#"{{ typeOf (splitn "-" (int .Values.count) "a-b" ) }}"#),
        want: vec![Some("map[string]string".to_owned())]
    );
}

#[test]
fn yaml_plain_booleans_resolve_before_alias_and_merge_expansion() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := fromYaml "base: &base {enabled: yes, disabled: OFF}\nchild: {<<: *base}\nquoted: 'yes'" }}{{ $m.child.enabled }}/{{ $m.child.disabled }}/{{ $m.quoted }}"#),
        want: vec![Some("true/false/yes".to_owned())]
    );
    sim_assert_eq!(
        have: texts(r#"{{ first (fromYamlArray "- on") }}"#),
        want: vec![Some("true".to_owned())]
    );
}

#[test]
fn yaml_mapping_keys_use_the_decoded_boolean_or_integer_spelling() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := fromYaml "on: enabled\nfalse: disabled\n1: one\n-2: two" }}{{ get $m "true" }}/{{ get $m "false" }}/{{ get $m "1" }}/{{ get $m "-2" }}"#),
        want: vec![Some("enabled/disabled/one/two".to_owned())]
    );
}

#[test]
fn yaml_merge_preserves_explicit_keys_and_the_first_source() {
    sim_assert_eq!(
        have: texts(r#"{{ $m := fromYaml "a: &a {x: first, picked: old}\nb: &b {x: second, z: kept}\nchild: {<<: [*a, *b], picked: new}" }}{{ $m.child.x }}/{{ $m.child.picked }}/{{ $m.child.z }}"#),
        want: vec![Some("first/new/kept".to_owned())]
    );
}

#[test]
fn yaml_empty_documents_allocate_nonnil_writable_containers() {
    for document in ["", "null", "~", "Null"] {
        sim_assert_eq!(
            have: texts(&format!(r#"{{{{ $m := fromYaml {document:?} }}}}{{{{ $_ := set $m "x" "written" }}}}{{{{ $m.x }}}}/{{{{ eq $m nil }}}}/{{{{ eq (fromYamlArray {document:?}) nil }}}}"#)),
            want: vec![Some("written/false/false".to_owned())]
        );
    }
}

#[test]
fn strslice_consumers_drop_nil_without_changing_scalar_conversion() {
    sim_assert_eq!(
        have: texts(r#"{{ first (toStrings (list nil "b")) }}/{{ join "," (sortAlpha (list "b" nil "a")) }}/{{ join "," (list nil "a" nil "b" nil) }}/{{ toString nil }}"#),
        want: vec![Some("b/a,b/a,b/<nil>".to_owned())]
    );
}

#[test]
fn default_selects_the_first_given_value_after_eager_operand_evaluation() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict }}{{ default "fallback" "first" (set $d "x" "written") }}/{{ $d.x }}/{{ "last" | default "fallback" "" }}"#),
        want: vec![Some("first/written/fallback".to_owned())]
    );
}

#[test]
fn widened_destination_cannot_hide_a_mutated_nested_alias() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let mut extra_keys = String::new();
    for index in 0..128 {
        write!(extra_keys, r#" "k{index}" 0"#)?;
    }
    let source = format!(
        r#"{{{{ $shared := dict "x" "old" }}}}{{{{ $dst := dict "shared" $shared{extra_keys} }}}}{{{{ $_ := mergeOverwrite $dst (dict "shared" (dict "x" "new")) }}}}{{{{ if eq $shared.x "old" }}}}{{{{ fail "stale hidden alias" }}}}{{{{ end }}}}ok"#
    );
    let execution = execute_document(&source, Some("templates/test.yaml"), &db);
    sim_assert_eq!(have: execution.abort_predicate(), want: helm_schema_core::Predicate::False);
    Ok(())
}
