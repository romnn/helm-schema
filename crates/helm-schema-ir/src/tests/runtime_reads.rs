use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::{DocumentExecution, Field, Remainder, State, Value, execute_document};

fn execution(source: &str) -> DocumentExecution {
    execute_document(
        source,
        Some("templates/test.yaml"),
        &IrAnalysisDb::new(&DefineIndex::new()),
    )
}

fn unknown_regions(execution: &DocumentExecution) -> BTreeSet<ValuesPath> {
    execution.unknown_read_regions().cloned().collect()
}

#[test]
fn keyed_reads_preserve_the_child_without_opening_the_root() {
    for source in [
        r#"{{ index .Values "foo" | quote }}"#,
        r#"{{ $key := "foo" }}{{ index .Values $key | quote }}"#,
        r#"{{ get .Values "foo" | quote }}"#,
        r#"{{ hasKey .Values "foo" | quote }}"#,
    ] {
        let execution = execution(source);
        assert!(
            execution
                .reads()
                .iter()
                .any(|read| read.values_path == ValuesPath::parse("foo")),
            "{source}"
        );
        assert!(
            !unknown_regions(&execution).contains(&ValuesPath::default()),
            "{source}"
        );
    }
}

#[test]
fn whole_and_dynamic_consumers_open_only_the_consumed_region() {
    for source in [
        r#"{{ toYaml .Values }}"#,
        r#"{{ toJson .Values }}"#,
        r#"{{ .Values }}"#,
        r#"{{ $key := .Values.key }}{{ index .Values $key }}"#,
        r#"{{ $key := .Values.key }}{{ hasKey .Values $key }}"#,
        r#"{{ range .Values }}{{ . }}{{ end }}"#,
    ] {
        let execution = execution(source);
        assert!(
            unknown_regions(&execution).contains(&ValuesPath::default()),
            "{source}"
        );
    }
    let nested = execution(r#"{{ toYaml .Values.config }}"#);
    sim_assert_eq!(have: unknown_regions(&nested), want: BTreeSet::from([ValuesPath::parse("config")]));
}

#[test]
fn dependency_inputs_and_mutation_aliases_have_distinct_traversals() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let first = state.heap.map(Remainder::Closed);
    let second = state.heap.map(Remainder::Closed);
    let first_id = state.map_target(&first).ok_or_eyre("first map missing")?;
    let second_id = state.map_target(&second).ok_or_eyre("second map missing")?;
    state.heap.write(
        &first_id,
        "a",
        Field::Present(Value::input(ValuesPath::parse("alpha"))),
    );
    state.heap.write(
        &second_id,
        "b",
        Field::Present(Value::input(ValuesPath::parse("beta"))),
    );

    // Unknown references retain their mutation footprint without importing sibling provenance.
    for value in [
        Value::input(ValuesPath::parse("alpha")),
        Value::influenced(BTreeSet::from([ValuesPath::parse("alpha")])),
    ] {
        let frozen = state.heap.freeze(&value);
        sim_assert_eq!(have: frozen.inputs(), want: BTreeSet::from([ValuesPath::parse("alpha")]));
        sim_assert_eq!(have: frozen.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("alpha"), ValuesPath::parse("beta")]));
    }
    sim_assert_eq!(have: state.heap.freeze(&first).inputs(), want: BTreeSet::from([ValuesPath::parse("alpha")]));
    Ok(())
}

#[test]
fn pure_fallbacks_preserve_heap_values_and_input_identity() {
    for call in [
        r#"printf "%d" .Values.number"#,
        "randAlphaNum 1",
        "now",
        "uuidv4",
    ] {
        let source =
            format!(r#"{{{{ $d := dict "x" "old" }}}}{{{{ $_ := {call} }}}}{{{{ get $d "x" }}}}"#);
        let execution = execution(&source);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("old".to_owned())]);
        assert!(
            !unknown_regions(&execution).contains(&ValuesPath::default()),
            "{source}"
        );
    }
    let formatted = execution(r#"{{ printf "%d" .Values.count }}"#);
    let inputs: BTreeSet<_> = formatted
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| site.value().inputs())
        .collect();
    sim_assert_eq!(have: inputs, want: BTreeSet::from([ValuesPath::parse("count")]));
    sim_assert_eq!(have: formatted.abort_predicate(), want: Predicate::False);
}

#[test]
fn function_catalog_keeps_unknown_writers_conservative() {
    use crate::function_semantics::{HeapEffect, function_semantics};
    for function in ["set", "unset", "merge", "mustMergeOverwrite", "sortAlpha"] {
        sim_assert_eq!(have: function_semantics(function).heap_effect, want: HeapEffect::Operands);
    }
    for function in ["printf", "randAlphaNum", "now", "uuidv4", "toYaml"] {
        sim_assert_eq!(have: function_semantics(function).heap_effect, want: HeapEffect::ReadOnly);
    }
    for function in ["include", "template", "tpl", "call", "unresolvedWriter"] {
        sim_assert_eq!(have: function_semantics(function).heap_effect, want: HeapEffect::Unknown);
    }
}

#[test]
fn widened_field_reads_keep_the_recorded_dependency_set() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let value = state.heap.map(Remainder::Closed);
    let id = state.map_target(&value).ok_or_eyre("map missing")?;
    state.heap.write(
        &id,
        "input",
        Field::Present(Value::input(ValuesPath::parse("alpha"))),
    );
    crate::value::operations::invalidate(&mut state);
    let result = state.field(&value, "other");
    sim_assert_eq!(have: state.heap.freeze(&result).inputs(), want: BTreeSet::from([ValuesPath::parse("alpha")]));
    assert!(result.identity().is_none());
    Ok(())
}

#[test]
fn presence_reads_survive_materialization_and_known_field_proofs() -> eyre::Result<()> {
    let materialized = execution(
        r#"{{ $_ := set .Values.config "bar" 1 }}{{ hasKey .Values.config "foo" | quote }}"#,
    );
    assert!(
        materialized
            .reads()
            .iter()
            .any(|read| read.values_path == ValuesPath::parse("config.foo"))
    );
    assert!(!unknown_regions(&materialized).contains(&ValuesPath::default()));

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Input);
    let id = state.map_target(&map).ok_or_eyre("map missing")?;
    state.heap.write(
        &id,
        "known",
        Field::Present(Value::input(ValuesPath::parse("original"))),
    );
    let invocation = crate::value::Invocation {
        callee: crate::value::Callee::Function("hasKey".to_owned()),
        operands: vec![
            crate::value::Operand {
                value: map.clone(),
                site: 0,
                form: crate::value::ArgumentForm::Evaluated,
            },
            crate::value::Operand {
                value: Value::string("known"),
                site: 1,
                form: crate::value::ArgumentForm::DirectLiteral,
            },
        ],
        site: 0,
    };
    state.record_invocation_reads(&invocation);
    sim_assert_eq!(have: state.field_presence(&map, "known").constant(), want: Some(&helm_schema_core::GuardValue::Bool(true)));
    assert!(
        state
            .reads
            .iter()
            .any(|read| read.path == ValuesPath::parse("original"))
    );
    Ok(())
}

#[test]
fn input_membership_uses_presence_independently_of_nilness() {
    sim_assert_eq!(
        have: execution(r#"{{ if hasKey .Values "mode" }}{{ fail "present" }}{{ end }}"#).abort_predicate(),
        want: Predicate::from(helm_schema_core::Guard::HasKey {
            path: ValuesPath::default(),
            key: "mode".to_owned(),
        })
    );
}

#[test]
fn empty_input_keys_widen_the_container_without_claiming_the_parent_identity() {
    for source in [
        r#"{{ if not (hasKey .Values "") }}{{ fail "missing" }}{{ end }}"#,
        r#"{{ if ne (index .Values "") "x" }}{{ fail "different" }}{{ end }}"#,
        r#"{{ if ne (get .Values "") "x" }}{{ fail "different" }}{{ end }}"#,
    ] {
        let execution = execution(source);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False, "{source}");
        sim_assert_eq!(have: unknown_regions(&execution), want: BTreeSet::from([ValuesPath::default()]), "{source}");
    }
    let execution = execution(
        r#"{{ $d := dict "" "x" }}{{ if or (not (hasKey $d "")) (ne (index $d "") "x") }}{{ fail "lost" }}{{ end }}"#,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: unknown_regions(&execution), want: BTreeSet::new());
}
