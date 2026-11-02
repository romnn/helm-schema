use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::{ReadCoverage, State, Value};
use crate::analysis_db::IrAnalysisDb;
use crate::observed_facts::CaptureKind;

#[test]
fn input_nil_predicate_and_complement_are_boolean_normal_forms() {
    let paths = [
        ValuesPath::default(),
        ValuesPath::parse("parent.child"),
        ValuesPath::parse("array.0.member"),
        ValuesPath::parse("collection.*.member"),
        ValuesPath::from_segments(["literal.dot", "*", "back\\slash"]),
        ValuesPath::from_segments((0..256).map(|index| format!("level{index}"))),
    ];
    for path in paths {
        let predicate = super::kinds::input_nil_predicate(&path);
        for form in [predicate.clone(), predicate.negated()] {
            sim_assert_eq!(have: form.clone(), want: form.normalize_boolean());
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HostObservation {
    path: ValuesPath,
    condition: Predicate,
}

fn execute(source: &str) -> super::DocumentExecution {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    crate::value::tests::execute_document(source, None, &db)
}

fn hosts(execution: &super::DocumentExecution) -> BTreeSet<HostObservation> {
    execution
        .exits
        .iter()
        .flat_map(|exit| exit.state.reads.iter())
        .filter(|read| matches!(read.coverage, ReadCoverage::MemberHost { .. }))
        .map(|read| HostObservation {
            path: read.path.clone(),
            condition: read.condition.clone().normalize_boolean(),
        })
        .collect()
}

#[test]
fn chained_raw_member_access_records_a_host_without_a_container_read() {
    let execution = execute("{{ .Values.probe.port }}");
    sim_assert_eq!(
        have: hosts(&execution),
        want: BTreeSet::from([HostObservation {
            path: ValuesPath::parse("probe"),
            condition: Predicate::True,
        }])
    );
    // Host requirements use their own capture lane, never declaration-seeding dependency reads.
    assert!(
        execution
            .reads()
            .iter()
            .all(|read| read.values_path != ValuesPath::parse("probe"))
    );
    let captures = execution
        .observed_facts()
        .captures
        .iter()
        .filter(|capture| matches!(capture.kind, CaptureKind::MemberAccess { .. }))
        .map(|capture| capture.conjunction.clone())
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: captures,
        want: vec![helm_schema_core::Conjunction::new([Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("probe"),
            schema_type: "object".to_owned(),
        }).negated()])]
    );
}

#[test]
fn grouped_and_bound_receivers_keep_missing_and_null_hosts_outside_the_requirement() {
    let path = ValuesPath::parse("probe");
    let condition = Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::Eq {
            path: path.clone(),
            value: GuardValue::Null,
        }),
    ])
    .negated()
    .normalize_boolean();
    for source in [
        "{{ (.Values.probe).port }}",
        "{{ $probe := .Values.probe }}{{ $probe.port }}",
    ] {
        let execution = execute(source);
        sim_assert_eq!(
            have: hosts(&execution),
            want: BTreeSet::from([HostObservation { path: path.clone(), condition: condition.clone() }])
        );
    }
}

#[test]
fn conditional_member_hosts_retain_the_executed_ambient_guard() {
    let execution = execute("{{ if .Values.enabled }}{{ .Values.probe.port }}{{ end }}");
    sim_assert_eq!(
        have: hosts(&execution),
        want: BTreeSet::from([HostObservation {
            path: ValuesPath::parse("probe"),
            condition: Predicate::truthy_path("enabled"),
        }])
    );
    sim_assert_eq!(
        have: hosts(&execute("{{ if false }}{{ .Values.probe.port }}{{ end }}")),
        want: BTreeSet::new()
    );
}

#[test]
fn replacement_and_materialized_mutated_hosts_do_not_constrain_original_input_kinds() {
    for source in [
        "{{ $_ := set .Values \"probe\" (dict \"port\" 443) }}{{ .Values.probe.port }}",
        "{{ $_ := set .Values.probe \"port\" 443 }}{{ .Values.probe.port }}",
        "{{ $probe := dict \"port\" 443 }}{{ $probe.port }}",
    ] {
        sim_assert_eq!(have: hosts(&execute(source)), want: BTreeSet::new());
    }
}

#[test]
fn summarized_execution_drops_host_proofs_instead_of_making_them_unconditional() {
    let mut state = State::new(Value::nil());
    state.reads.insert(super::RuntimeRead {
        coverage: ReadCoverage::MemberHost { allows_nil: false },
        path: ValuesPath::parse("probe"),
        condition: Predicate::truthy_path("enabled"),
    });
    let exits = super::control::summarize_exits(super::eval::normal(state, Value::nil()));
    let execution = super::DocumentExecution::new(exits, crate::value::FileOutput::Manifest);
    sim_assert_eq!(have: hosts(&execution), want: BTreeSet::new());
    assert!(
        execution
            .observed_facts()
            .captures
            .iter()
            .all(|capture| { !matches!(capture.kind, CaptureKind::MemberAccess { .. }) })
    );
}

#[test]
fn nil_safe_entry_domain_guards_every_later_host_in_the_same_chain() {
    let path = ValuesPath::parse("probe");
    let live = Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::Eq {
            path: path.clone(),
            value: GuardValue::Null,
        }),
    ])
    .negated()
    .normalize_boolean();
    for source in [
        "{{ $probe := .Values.probe }}{{ $probe.child.grand }}",
        "{{ (.Values.probe).child.grand }}",
    ] {
        sim_assert_eq!(
            have: hosts(&execute(source)),
            want: BTreeSet::from([
                HostObservation { path: path.clone(), condition: live.clone() },
                HostObservation { path: ValuesPath::parse("probe.child"), condition: live.clone() },
            ])
        );
    }
    // A direct chain still attempts both hosts, so missing either receiver remains an error.
    sim_assert_eq!(
        have: hosts(&execute("{{ .Values.probe.child.grand }}")),
        want: BTreeSet::from([
            HostObservation { path, condition: Predicate::True },
            HostObservation { path: ValuesPath::parse("probe.child"), condition: Predicate::True },
        ])
    );
}

#[test]
fn selected_entry_liveness_preserves_exact_arm_guards_and_abstains_on_partial_coverage() {
    let state = State::new(Value::nil());
    let choose = Predicate::truthy_path("choose");
    let exact = state
        .heap
        .arena
        .decision(choose.clone(), choose.negated(), super::Coverage::Exact);
    let selected = Value::select(
        exact,
        Value::input(ValuesPath::parse("left")),
        Value::input(ValuesPath::parse("right")),
    );
    let expected = Predicate::Or(vec![
        Predicate::all(vec![
            choose.clone(),
            super::kinds::input_nil_predicate(&ValuesPath::parse("left")).negated(),
        ]),
        Predicate::all(vec![
            choose.negated(),
            super::kinds::input_nil_predicate(&ValuesPath::parse("right")).negated(),
        ]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: state.selector_entry_domain(&selected).unwrap_or(Predicate::False), want: expected);
    let partial =
        state
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, super::Coverage::Partial);
    let selected = Value::select(
        partial,
        Value::input(ValuesPath::parse("left")),
        Value::input(ValuesPath::parse("right")),
    );
    sim_assert_eq!(have: state.selector_entry_domain(&selected).unwrap_or(Predicate::False), want: Predicate::False);
    sim_assert_eq!(have: state.selector_entry_domain(&Value::unknown()).unwrap_or(Predicate::False), want: Predicate::False);
}

#[test]
fn typed_map_entries_remain_live_even_when_their_map_backing_is_nil() {
    let mut state = State::new(Value::nil());
    let nil_map = Value::leaf(super::Datum::Map {
        view: super::MapType::Interface,
        target: None,
    });
    let map = state.heap.map(super::Remainder::Closed);
    sim_assert_eq!(have: state.selector_entry_domain(&nil_map).unwrap_or(Predicate::False), want: Predicate::True);
    sim_assert_eq!(have: state.selector_entry_domain(&map).unwrap_or(Predicate::False), want: Predicate::True);
    sim_assert_eq!(have: state.selector_entry_domain(&Value::nil()).unwrap_or(Predicate::False), want: Predicate::False);
}

/// Allocating unrelated storage must not change a raw host obligation.
#[test]
fn unrelated_materialization_preserves_member_observations() {
    let expected = hosts(&execute("{{ $_ := .Values.probe.port }}"));
    for source in [
        r#"{{ $unrelated := dict "z" 1 }}{{ $_ := .Values.probe.port }}"#,
        r#"{{ $_ := set .Values "unrelated" (dict "z" 1) }}{{ $_ := .Values.probe.port }}"#,
    ] {
        sim_assert_eq!(have: hosts(&execute(source)), want: expected);
    }
}

/// A later replacement cannot erase an access that already executed.
#[test]
fn member_observations_before_replacement_survive_the_final_heap() {
    sim_assert_eq!(
        have: hosts(&execute(r#"{{ $_ := .Values.probe.port }}{{ $_ := set .Values "probe" (dict "port" 1) }}{{ $_ := .Values.probe.port }}"#)),
        want: BTreeSet::from([HostObservation {
            path: ValuesPath::parse("probe"),
            condition: Predicate::True,
        }])
    );
}

#[test]
fn final_members_are_optional_but_intermediate_members_are_hosts() {
    sim_assert_eq!(
        have: hosts(&execute("{{ $_ := .Values.probe.child }}")),
        want: BTreeSet::from([HostObservation {
            path: ValuesPath::parse("probe"),
            condition: Predicate::True,
        }])
    );
    sim_assert_eq!(
        have: hosts(&execute("{{ $_ := .Values.probe.child.grand }}")),
        want: BTreeSet::from([
            HostObservation { path: ValuesPath::parse("probe"), condition: Predicate::True },
            HostObservation { path: ValuesPath::parse("probe.child"), condition: Predicate::True },
        ])
    );
}

#[test]
fn helpers_retain_direct_member_observations_and_correlated_guards() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "member" }}{{ $_ := .Values.probe.port }}{{ end }}"#,
    );
    let db = IrAnalysisDb::new(&defines);
    let helper = crate::value::tests::execute_document(
        r#"{{ if .Values.enabled }}{{ $_ := include "member" . }}{{ end }}"#,
        None,
        &db,
    );
    sim_assert_eq!(
        have: hosts(&helper),
        want: BTreeSet::from([HostObservation {
            path: ValuesPath::parse("probe"),
            condition: Predicate::truthy_path("enabled"),
        }])
    );
}

#[test]
fn unvisited_loop_bodies_and_post_break_accesses_contribute_no_hosts() {
    for source in [
        "{{ range list }}{{ $_ := $.Values.probe.port }}{{ end }}",
        "{{ range list 1 }}{{ break }}{{ $_ := $.Values.probe.port }}{{ end }}",
    ] {
        sim_assert_eq!(have: hosts(&execute(source)), want: BTreeSet::new());
    }
}

#[test]
fn repeated_accesses_share_one_logical_host_observation() {
    for count in [1, 2, 8, 32] {
        let source = "{{ $_ := .Values.probe.port }}".repeat(count);
        sim_assert_eq!(
            have: hosts(&execute(&source)),
            want: BTreeSet::from([HostObservation {
                path: ValuesPath::parse("probe"),
                condition: Predicate::True,
            }])
        );
    }
}

/// A backing allocation without a write cannot erase the input receiver's failure domain.
#[test]
fn receiver_materialization_preserves_the_same_member_obligation() -> eyre::Result<()> {
    let path = ValuesPath::parse("probe");
    let mut state = State::new(Value::nil());
    let typed = Value {
        node: std::rc::Rc::new(super::ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Unknown(super::UnknownValue {
                types: super::TypeCoverage::Map(super::MapType::Interface),
                aliases: super::Reach::Top,
            }),
            origin: super::Origin {
                derivation: None,
                identity: Some(path.clone()),
                influences: BTreeSet::new(),
            },
        })),
    };
    state
        .map_target(&typed)
        .ok_or_eyre("receiver backing missing")?;
    state.record_member_host(
        &Value::input(path.clone()),
        &Predicate::True,
        super::query::MemberAccess::Selector,
    );
    let execution = super::DocumentExecution::new(
        super::eval::normal(state, Value::nil()),
        crate::value::FileOutput::Manifest,
    );
    sim_assert_eq!(
        have: hosts(&execution),
        want: BTreeSet::from([HostObservation { path, condition: Predicate::True }])
    );
    Ok(())
}
