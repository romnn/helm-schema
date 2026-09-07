use helm_schema_core::{Conjunction, Guard, GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::scope_capture_conditions;
use crate::eval_effect::{CaptureKind, FailCapture};
use crate::range_modes::RangeModes;

fn capture(path: &str, value: GuardValue) -> FailCapture {
    FailCapture {
        conjunction: Conjunction::new([Predicate::Guard(Guard::Eq {
            path: ValuesPath::parse(path),
            value,
        })]),
        ranged: RangeModes::default(),
        kind: CaptureKind::AbsenceAborts {
            path: ValuesPath::parse("required"),
        },
    }
}

fn ambient() -> (Conjunction, RangeModes) {
    let mut ranged = RangeModes::default();
    for path in ["config", "config.*"] {
        ranged.mark_input_identity(&ValuesPath::parse(path));
        ranged.mark_member_identity(&ValuesPath::parse(path));
    }
    let conjunction = Conjunction::new([
        Predicate::Guard(Guard::Range {
            path: ValuesPath::parse("config"),
        }),
        Predicate::Guard(Guard::Range {
            path: ValuesPath::parse("config.*"),
        }),
        Predicate::Guard(Guard::Truthy {
            path: ValuesPath::parse("config.*"),
        }),
        Predicate::Guard(Guard::TypeIs {
            path: ValuesPath::parse("config.*"),
            schema_type: "object".to_string(),
        }),
        Predicate::Guard(Guard::TypeIs {
            path: ValuesPath::parse("config.*.*"),
            schema_type: "string".to_string(),
        }),
        Predicate::Guard(Guard::TypeIs {
            path: ValuesPath::parse("config.*.*"),
            schema_type: "array".to_string(),
        })
        .negated(),
        Predicate::Or(vec![
            Predicate::Guard(Guard::Eq {
                path: ValuesPath::parse("config.*.*"),
                value: GuardValue::Null,
            }),
            Predicate::Guard(Guard::Absent {
                path: ValuesPath::parse("config.*.*"),
            }),
        ])
        .negated(),
    ]);
    (conjunction, ranged)
}

#[test]
fn selected_mapping_member_discharges_only_proved_caller_scope() {
    let mut body = capture("config.section.program", GuardValue::string("program"));
    body.ranged.mark_input_identity(&ValuesPath::parse("items"));
    let (mut ambient, mut ranged) = ambient();
    let enabled = Predicate::Guard(Guard::Truthy {
        path: ValuesPath::parse("enabled"),
    });
    ambient.push(enabled.clone());
    ranged.mark_input_identity(&ValuesPath::parse("other"));
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged);
    let mut expected = body.conjunction.clone();
    expected.push(enabled);
    let mut expected_modes = body.ranged.clone();
    expected_modes.mark_input_identity(&ValuesPath::parse("other"));
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: expected_modes);
}

#[test]
fn unknown_sibling_member_condition_retains_iteration_binding() {
    let body = capture("config.section.program", GuardValue::string("program"));
    let (mut ambient, ranged) = ambient();
    ambient.push(Predicate::Guard(Guard::Truthy {
        path: ValuesPath::parse("config.*.enabled"),
    }));
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}

#[test]
fn non_string_and_empty_equalities_do_not_prove_iteration_scope() {
    for value in [
        GuardValue::Null,
        GuardValue::string(""),
        GuardValue::Int(4),
        GuardValue::Bool(false),
    ] {
        let body = capture("config.section.program", value);
        let (ambient, ranged) = ambient();
        let mut expected = ambient.clone();
        expected.extend(body.conjunction.iter().cloned());
        let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
        sim_assert_eq!(have: conjunction, want: expected);
        sim_assert_eq!(have: modes, want: ranged);
    }
}

#[test]
fn wildcard_selection_does_not_invent_mapping_shape() {
    let body = capture("config.*.program", GuardValue::string("program"));
    let (ambient, ranged) = ambient();
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}

#[test]
fn string_leaf_is_not_an_iterable_witness() {
    let body = capture("config.section.program", GuardValue::string("program"));
    let path = ValuesPath::parse("config.section.program");
    let ambient = Conjunction::new([Predicate::Guard(Guard::Range { path: path.clone() })]);
    let mut ranged = RangeModes::default();
    ranged.mark_input_identity(&path);
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}

#[test]
fn derived_and_decoded_ranges_keep_their_scope() {
    for decoded in [false, true] {
        let body = capture("config.section.program", GuardValue::string("program"));
        let path = ValuesPath::parse("config");
        let ambient = Conjunction::new([Predicate::Guard(Guard::Range { path: path.clone() })]);
        let mut ranged = RangeModes::default();
        ranged.mark_member_identity(&path);
        if decoded {
            ranged.mark_input_identity(&path);
            ranged.mark_json_decoded(&path);
        }
        let mut expected = ambient.clone();
        expected.extend(body.conjunction.iter().cloned());
        let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
        sim_assert_eq!(have: conjunction, want: expected);
        sim_assert_eq!(have: modes, want: ranged);
    }
}

#[test]
fn member_dependent_capture_target_keeps_its_binding() {
    let mut body = capture("config.section.program", GuardValue::string("program"));
    body.kind = CaptureKind::ValueType {
        path: ValuesPath::parse("config.*.port"),
        schema_type: "integer".to_string(),
        null_aborts: true,
    };
    let (ambient, ranged) = ambient();
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}

#[test]
fn body_owned_range_survives_caller_scope_simplification() {
    let mut body = capture("config.section.program", GuardValue::string("program"));
    body.ranged
        .mark_input_identity(&ValuesPath::parse("config"));
    let (ambient, ranged) = ambient();
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged);
    sim_assert_eq!(have: conjunction, want: body.conjunction);
    sim_assert_eq!(have: modes, want: body.ranged);
}

#[test]
fn current_range_key_capture_keeps_its_binding() {
    let mut body = capture("config.section.program", GuardValue::string("program"));
    body.kind = CaptureKind::RangeKeyStrings {
        paths: std::collections::BTreeSet::from([ValuesPath::parse("config")]),
    };
    let (ambient, ranged) = ambient();
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}

#[test]
fn body_range_key_predicate_keeps_its_binding() {
    let mut body = capture("config.section.program", GuardValue::string("program"));
    body.conjunction
        .push(Predicate::Guard(Guard::RangeKeyEquals {
            path: ValuesPath::parse("config"),
            key: "other".to_string(),
        }));
    let (ambient, ranged) = ambient();
    let mut expected = ambient.clone();
    expected.extend(body.conjunction.iter().cloned());
    let (conjunction, modes) = scope_capture_conditions(&body, ambient, ranged.clone());
    sim_assert_eq!(have: conjunction, want: expected);
    sim_assert_eq!(have: modes, want: ranged);
}
