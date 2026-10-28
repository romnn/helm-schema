use std::collections::BTreeSet;
use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use crate::value::{AbortReason, DocumentExecution, Flow};

fn execution(source: &str) -> DocumentExecution {
    execute_document(
        source,
        Some("templates/test.yaml"),
        &IrAnalysisDb::new(&DefineIndex::new()),
    )
}

fn nil(path: &str) -> Predicate {
    Predicate::invalid_kind_path(path).normalize_boolean()
}

fn kind(path: &str, schema_type: &str) -> Predicate {
    Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse(path),
        schema_type: schema_type.to_owned(),
    })
}

fn equivalent(have: &Predicate, want: &Predicate) {
    sim_assert_eq!(
        have: (have.exactly_implies(want), want.exactly_implies(have)),
        want: (true, true),
        "have: {have:?}; want: {want:?}"
    );
}

#[test]
fn raw_scalar_comparisons_keep_nil_and_check_the_actual_runtime_kind() {
    for operator in ["eq", "ne"] {
        for (right, accepted) in [
            (
                "1",
                Predicate::Or(vec![nil("input"), kind("input", "integer")]).normalize_boolean(),
            ),
            (
                "1.5",
                Predicate::Or(vec![nil("input"), kind("input", "number")]).normalize_boolean(),
            ),
            (
                "true",
                Predicate::Or(vec![nil("input"), kind("input", "boolean")]).normalize_boolean(),
            ),
            (
                r#""x""#,
                Predicate::Or(vec![nil("input"), kind("input", "string")]).normalize_boolean(),
            ),
            ("nil", Predicate::True),
            (r#"(dict "x" 1)"#, nil("input")),
            ("(list 1)", nil("input")),
            (
                r#"(fromJson "null")"#,
                Predicate::Or(vec![nil("input"), kind("input", "object")]).normalize_boolean(),
            ),
            (
                r#"(fromJsonArray "null")"#,
                Predicate::Or(vec![nil("input"), kind("input", "array")]).normalize_boolean(),
            ),
        ] {
            let source = format!("{{{{ {operator} .Values.input {right} }}}}");
            sim_assert_eq!(have: execution(&source).abort_predicate(), want: accepted.negated().normalize_boolean(), "{source}");
        }
    }
}

#[test]
fn integer_comparison_keeps_equal_and_unequal_normal_loader_results() {
    let accepted = Predicate::Or(vec![nil("input"), kind("input", "integer")]).normalize_boolean();
    for operator in ["eq", "ne"] {
        let execution = execution(&format!("{{{{ {operator} .Values.input 1 }}}}"));
        let values: BTreeSet<_> = execution
            .sites()
            .filter(|site| site.rendered())
            .flat_map(|site| site.value().alternatives())
            .filter_map(|alternative| alternative.value().literal().cloned())
            .collect();
        sim_assert_eq!(have: values, want: BTreeSet::from([GuardValue::Bool(true), GuardValue::Bool(false)]));
    }
    let equal = Predicate::from(Guard::Eq {
        path: ValuesPath::parse("input"),
        value: GuardValue::Int(1),
    });
    equivalent(
        &execution(r#"{{ if eq .Values.input 1 }}{{ fail "equal" }}{{ end }}ok"#).abort_predicate(),
        &Predicate::Or(vec![accepted.negated(), equal.clone()]).normalize_boolean(),
    );
    equivalent(
        &execution(r#"{{ if ne .Values.input 1 }}{{ fail "unequal" }}{{ end }}ok"#)
            .abort_predicate(),
        &Predicate::all(vec![accepted, equal])
            .negated()
            .normalize_boolean(),
    );
}

#[test]
fn raw_pair_comparability_accepts_nil_or_matching_scalar_kinds() {
    let mut accepted = vec![nil("left")];
    for schema_type in ["boolean", "string", "number", "integer"] {
        accepted.push(Predicate::all(vec![
            kind("left", schema_type),
            Predicate::Or(vec![nil("right"), kind("right", schema_type)]),
        ]));
    }
    for schema_type in ["object", "array"] {
        accepted.push(Predicate::all(vec![
            kind("left", schema_type),
            nil("right"),
        ]));
    }
    let accepted = Predicate::Or(accepted).normalize_boolean();
    for operator in ["eq", "ne"] {
        let execution = execution(&format!("{{{{ {operator} .Values.left .Values.right }}}}"));
        sim_assert_eq!(have: execution.abort_predicate(), want: accepted.negated().normalize_boolean());
    }
    let reflected = execution("{{ eq .Values.input .Values.input }}");
    let accepted = Predicate::Or(vec![
        nil("input"),
        kind("input", "boolean"),
        kind("input", "number"),
        kind("input", "integer"),
        kind("input", "string"),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: reflected.abort_predicate(), want: accepted.negated().normalize_boolean());
    assert!(
        reflected
            .sites()
            .filter(|site| site.rendered())
            .all(|site| site.value().literal() == Some(&GuardValue::Bool(true)))
    );
}

#[test]
fn raw_pair_keeps_the_precise_both_nil_equality_arm() {
    let execution = execution("{{ eq .Values.left .Values.right }}");
    let mut true_arms = Vec::new();
    for site in execution.sites().filter(|site| site.rendered()) {
        for alternative in site.value().alternatives() {
            if alternative.value().literal() == Some(&GuardValue::Bool(true)) {
                true_arms.push(Predicate::all(vec![
                    site.condition().clone(),
                    alternative.condition().clone(),
                ]));
            }
        }
    }
    equivalent(
        &Predicate::Or(true_arms).normalize_boolean(),
        &Predicate::all(vec![nil("left"), nil("right")]).normalize_boolean(),
    );
}

#[test]
fn typed_nil_and_nonnil_containers_follow_go_comparability() {
    for (source, output) in [
        (r#"{{ eq (fromJson "null") nil }}"#, "true"),
        (r#"{{ ne nil (fromJsonArray "null") }}"#, "false"),
        (r#"{{ eq (fromJson "null") (dict) }}"#, "false"),
        (r#"{{ eq (fromJsonArray "null") (list) }}"#, "false"),
        (r"{{ eq (int 1) (int64 1) }}", "true"),
    ] {
        let execution = execution(source);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some(output.to_owned())]);
    }
    for source in [
        r#"{{ eq (fromJson "null") (fromJsonArray "null") }}"#,
        "{{ eq (dict) (dict) }}",
        "{{ eq (list) (list) }}",
        "{{ eq (int 1) (float64 1) }}",
    ] {
        sim_assert_eq!(have: execution(source).abort_predicate(), want: Predicate::True);
    }
}

#[test]
fn equality_stops_comparing_after_the_first_match() {
    for (expression, aborts) in [
        (r#"eq "x" "x" (dict)"#, false),
        (r#"eq "x" (dict) "x""#, true),
        (r#"eq "x" "y" "x" (dict)"#, false),
        (r#"eq "x" "y" (dict) "x""#, true),
    ] {
        let execution = execution(&format!("{{{{ {expression} }}}}"));
        sim_assert_eq!(have: execution.abort_predicate(), want: if aborts { Predicate::True } else { Predicate::False });
    }
}

#[test]
fn equality_arguments_run_eagerly_before_comparison_begins() -> eyre::Result<()> {
    let execution = execution(
        r#"{{ $d := dict "x" "old" }}{{ eq "x" "x" (set $d "x" "new") }}/{{ get $d "x" }}"#,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("true/new".to_owned())]);
    sim_assert_eq!(have: self::execution(r#"{{ eq "x" "x" (fail "eager") }}"#).abort_predicate(), want: Predicate::True);
    let arity = self::execution(r#"{{ ne "x" "x" (fail "not reached") }}"#);
    let exit = arity
        .exits
        .first()
        .ok_or_eyre("arity check produced no exit")?;
    let Flow::Abort(reason) = &exit.flow else {
        eyre::bail!("invalid arity reached a normal result");
    };
    sim_assert_eq!(have: reason, want: &AbortReason::Arity);
    Ok(())
}

#[test]
fn equality_preserves_selected_nil_container_alternatives() {
    let source = r#"{{ eq (ternary (fromJson "null") (dict) .Values.choose) (dict) }}"#;
    let accepted = Predicate::all(vec![
        kind("choose", "boolean"),
        Predicate::truthy_path("choose"),
    ])
    .normalize_boolean();
    equivalent(
        &execution(source).abort_predicate(),
        &accepted.negated().normalize_boolean(),
    );
}

#[test]
fn mixed_numeric_pairs_do_not_switch_loader_kind_mid_invocation() {
    let equal = Predicate::from(Guard::Eq {
        path: ValuesPath::parse("input"),
        value: GuardValue::Int(1),
    });
    let accepted = Predicate::Or(vec![
        nil("input"),
        Predicate::all(vec![kind("input", "integer"), equal]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: execution("{{ eq .Values.input 1 2.0 }}").abort_predicate(), want: accepted.negated().normalize_boolean());
}

#[test]
fn numeric_loader_alternatives_do_not_escape_as_raw_type_proofs() {
    let result = execution(r#"{{ $_ := eq .Values.input 1 }}{{ typeIs "int64" .Values.input }}"#);
    assert!(
        result
            .sites()
            .filter(|site| site.rendered())
            .flat_map(|site| site.value().alternatives())
            .any(|alternative| alternative.value().literal().is_none())
    );
    assert!(
        result
            .all_sites()
            .filter(|site| site.value().identity() == Some(&ValuesPath::parse("input")))
            .all(|site| site.value().go_type() == crate::value::kinds::GoType::Unknown)
    );
}

#[test]
fn independent_numeric_comparisons_keep_one_normal_continuation() -> eyre::Result<()> {
    let baseline = execution("before");
    let baseline_exit = baseline
        .exits
        .iter()
        .find(|exit| matches!(exit.flow, Flow::Normal(_)))
        .ok_or_eyre("baseline normal exit missing")?;
    let baseline_objects: usize = baseline_exit
        .state
        .heap
        .version
        .pages
        .values()
        .map(|page| page.len())
        .sum();
    for count in [1, 2, 4, 8, 16] {
        let mut source = "before".to_owned();
        for ordinal in 0..count {
            write!(source, "{{{{ $_ := eq .Values.x{ordinal} 1 }}}}")?;
        }
        let result = execution(&source);
        sim_assert_eq!(have: result.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1, "{count}");
        assert!(
            result.exits.len() <= count + 1,
            "{count}: {} exits",
            result.exits.len()
        );
        let normal = result
            .exits
            .iter()
            .find(|exit| matches!(exit.flow, Flow::Normal(_)))
            .ok_or_eyre("normal comparison exit missing")?;
        let objects: usize = normal
            .state
            .heap
            .version
            .pages
            .values()
            .map(|page| page.len())
            .sum();
        sim_assert_eq!(have: objects, want: baseline_objects);
        sim_assert_eq!(have: normal.state.output.exact_text(), want: Some("before".to_owned()));
    }
    Ok(())
}

#[test]
fn numeric_comparisons_do_not_multiply_continuations_between_files() {
    struct NumericFile {
        source: String,
        path: String,
    }
    for count in [1, 2, 4, 8, 16] {
        let files: Vec<_> = (0..count)
            .map(|ordinal| NumericFile {
                source: format!("{{{{ $_ := eq .Values.x{ordinal} 1 }}}}"),
                path: format!("templates/{ordinal}.yaml"),
            })
            .collect();
        let templates: Vec<_> = files
            .iter()
            .map(|file| crate::symbolic::SymbolicTemplate {
                source: &file.source,
                source_path: &file.path,
            })
            .collect();
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let documents = crate::value::execute_documents(&templates, &db);
        sim_assert_eq!(have: documents.len(), want: count);
        for document in documents {
            sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
            assert!(document.exits.len() <= 2);
        }
    }
}

#[test]
fn comparison_dispatch_exhaustion_widens_without_mutating_caller_heap() {
    let mut source = r#"before{{ $d := dict "x" "old" }}{{ $_ := eq .Values.input"#.to_owned();
    for _ in 0..256 {
        source.push_str(" 1");
    }
    source.push_str(r#" }}{{ get $d "x" }}"#);
    let result = execution(&source);
    sim_assert_eq!(have: result.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: result.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("beforeold".to_owned())]);
}

#[test]
fn comparison_local_paths_preserve_heap_observations_and_caller_decisions() -> eyre::Result<()> {
    let result = execution(indoc::indoc! {r#"
        {{- $d := dict "x" "old" -}}
        {{- if .Values.enabled -}}
        {{- $x := .Values.input -}}
        {{- get $d "x" -}}
        {{- $_ := eq $x 1 -}}
        {{- get $d "x" -}}
        {{- end -}}
    "#});
    let old_sites: Vec<_> = result
        .sites()
        .filter(|site| {
            site.rendered() && site.value().literal() == Some(&GuardValue::string("old"))
        })
        .collect();
    let first = old_sites
        .first()
        .ok_or_eyre("earlier observation was lost")?;
    let last = old_sites.last().ok_or_eyre("later observation was lost")?;
    assert!(old_sites.len() >= 2);
    assert!(std::rc::Rc::ptr_eq(&first.value().heap, &last.value().heap));
    sim_assert_eq!(have: first.condition(), want: &Predicate::truthy_path("enabled"));
    for site in old_sites {
        sim_assert_eq!(have: Predicate::all(vec![site.condition().clone(), Predicate::truthy_path("enabled").negated()]).normalize_boolean(), want: Predicate::False);
    }
    Ok(())
}

#[test]
fn comparison_budget_neighbors_keep_the_existing_pair_and_preimage_charges() {
    for (pairs, exact) in [(62, true), (63, true), (64, false)] {
        let mut source = "{{ eq nil".to_owned();
        for _ in 0..pairs {
            source.push_str(" 1");
        }
        source.push_str(" }}");
        let result = execution(&source);
        sim_assert_eq!(have: result.abort_predicate(), want: Predicate::False);
        let expected = exact.then(|| "false".to_owned());
        sim_assert_eq!(have: result.text().map(|(_, value)| value.exact_text()).collect::<Vec<_>>(), want: vec![expected], "{pairs}");
    }
}

#[test]
fn comparison_requires_a_pair_after_eager_argument_evaluation() {
    for (source, reason) in [
        ("{{ eq 1 }}", AbortReason::Arity),
        (r#"{{ eq (fail "eager") }}"#, AbortReason::Fail),
    ] {
        let execution = execution(source);
        sim_assert_eq!(have: execution.exits.len(), want: 1);
        assert!(
            execution
                .exits
                .iter()
                .all(|exit| matches!(exit.flow, Flow::Abort(actual) if actual == reason))
        );
    }
}
