use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, Predicate, PredicateKind, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

fn execute(source: &str) -> crate::value::DocumentExecution {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
}

fn identities(source: &str) -> BTreeSet<ValuesPath> {
    execute(source)
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| site.value().alternatives())
        .filter_map(|arm| arm.value().identity().cloned())
        .collect()
}

fn inputs(source: &str) -> BTreeSet<ValuesPath> {
    execute(source)
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| site.value().inputs())
        .collect()
}

fn outputs(source: &str) -> BTreeSet<(Predicate, Option<String>)> {
    execute(source)
        .executed_outputs()
        .into_iter()
        .map(|output| (output.condition, output.text))
        .collect()
}

fn selected_condition(source: &str) -> Predicate {
    Predicate::Or(
        execute(source)
            .text_sites()
            .filter(|site| site.bytes() == "selected")
            .map(|site| site.condition().clone())
            .collect(),
    )
    .normalize_boolean()
}

#[test]
fn unconditioned_binding_join_keeps_values_but_proves_no_mixed_mode_arm() {
    let source = r"{{ $value := ternary .Values.direct .Values.evaluated (regexMatch .Values.pattern .Values.unknown) }}{{ $value }}";
    let execution = execute(source);
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    assert!(!sites.is_empty());
    for site in sites {
        assert!(
            site.value()
                .alternatives()
                .iter()
                .any(|arm| !arm.coverage_exact())
        );
    }
    assert!(inputs(source).contains(&ValuesPath::parse("direct")));
    assert!(inputs(source).contains(&ValuesPath::parse("evaluated")));
}

#[test]
fn partial_binding_decision_keeps_proven_subsets_and_unresolved_remainder() {
    let source = r"{{ $value := ternary .Values.direct .Values.evaluated (or .Values.select_true (regexMatch .Values.pattern .Values.unknown)) }}{{ $value }}";
    let execution = execute(source);
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    assert!(!sites.is_empty());
    assert!(
        sites
            .iter()
            .flat_map(|site| site.value().alternatives())
            .any(|arm| !arm.coverage_exact())
    );
    assert!(inputs(source).contains(&ValuesPath::parse("direct")));
    assert!(inputs(source).contains(&ValuesPath::parse("evaluated")));
}

#[test]
fn unrelated_unknown_binding_decisions_do_not_compare_as_the_same_execution() {
    let source = indoc! {r#"
        {{- $first := "no" -}}{{- $second := "no" -}}
        {{- if regexMatch .Values.pattern .Values.first -}}{{- $first = "yes" -}}{{- end -}}
        {{- if regexMatch .Values.pattern .Values.second -}}{{- $second = "yes" -}}{{- end -}}
        {{- print $first "/" $second -}}
    "#};
    let values = outputs(source)
        .into_iter()
        .filter_map(|(_, text)| text)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: values, want: BTreeSet::from(["yes/yes".to_string(), "yes/no".to_string(), "no/yes".to_string(), "no/no".to_string()]));
}

#[test]
fn two_independent_unknown_decisions_expose_both_leaves_with_unresolved_remainder() {
    let source = indoc! {r"
        {{- $value := .Values.third -}}
        {{- if regexMatch .Values.pattern .Values.unknown1 -}}{{- $value = .Values.first -}}
        {{- else if regexMatch .Values.pattern .Values.unknown2 -}}{{- $value = .Values.second -}}{{- end -}}
        {{- $value -}}
    "};
    let paths = inputs(source);
    for path in ["first", "second", "third"] {
        assert!(
            paths.contains(&ValuesPath::parse(path)),
            "missing candidate {path}: {paths:?}"
        );
    }
    let execution = execute(source);
    assert!(
        execution
            .sites()
            .filter(|site| site.rendered())
            .flat_map(|site| site.value().alternatives())
            .any(|arm| !arm.coverage_exact())
    );
}

#[test]
fn symbolic_range_exit_widens_only_bindings_changed_by_the_iteration() {
    let stable = "{{ $cfg := .Values.items }}{{ $stable := .Values.stable }}{{ range .Values.items }}{{ $cfg = . }}{{ end }}{{ $stable }}";
    sim_assert_eq!(have: identities(stable), want: BTreeSet::from([ValuesPath::parse("stable")]));
    let changed =
        "{{ $cfg := .Values.items }}{{ range .Values.items }}{{ $cfg = . }}{{ end }}{{ $cfg }}";
    let execution = execute(changed);
    assert!(
        execution
            .sites()
            .filter(|site| site.rendered())
            .any(|site| site
                .value()
                .alternatives()
                .iter()
                .any(|arm| !arm.coverage_exact() || arm.value().identity().is_none()))
    );
}

fn branch_program(count: usize) -> String {
    let mut source = String::from("{{- $value := \"0\" -}}");
    for index in 1..count {
        let keyword = if index == 1 { "if" } else { "else if" };
        source.push_str("{{- ");
        source.push_str(keyword);
        source.push_str(" eq .Values.branch ");
        let index_text = index.to_string();
        source.push_str(&index_text);
        source.push_str(" -}}{{- $value = \"");
        source.push_str(&index_text);
        source.push_str("\" -}}");
    }
    source.push_str("{{- end -}}{{- $value -}}");
    source
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): local binding has no structural alternative cap; tracked in plan/value-algebra-r3b.md follow-ups"]
fn local_binding_has_no_structural_alternative_cap() {
    let values = outputs(&branch_program(129))
        .into_iter()
        .filter_map(|(_, text)| text)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: values, want: (0..129).map(|index| index.to_string()).collect::<BTreeSet<_>>());
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): scalar dispatch join keeps 128 arms and discards 129; tracked in plan/value-algebra-r3b.md follow-ups"]
fn scalar_dispatch_join_keeps_128_arms_and_discards_129() {
    let at_cap = outputs(&branch_program(128));
    sim_assert_eq!(have: at_cap.iter().filter(|(_, text)| text.is_some()).count(), want: 128);
    let execution = execute(&branch_program(129));
    assert!(
        execution
            .sites()
            .filter(|site| site.rendered())
            .any(|site| !site.coverage_exact() || site.value().literal().is_none()),
        "over-cap join claimed complete exact scalar coverage"
    );
}

#[test]
fn unchanged_partial_dispatch_survives_the_join_cap() {
    let source = branch_program(129);
    let unchanged = source.replace("{{- $value -}}", "{{- if true -}}{{- end -}}{{- $value -}}");
    sim_assert_eq!(have: outputs(&unchanged), want: outputs(&source));
}

#[test]
fn changed_nested_and_missing_dispatches_match_the_old_join() {
    let source = indoc! {r#"
        {{- $value := "0" -}}
        {{- if .Values.outer -}}
          {{- if .Values.inner -}}{{- $value = "nested-true" -}}{{- else -}}{{- $value = "nested-false" -}}{{- end -}}
        {{- else -}}
          {{- if .Values.present -}}{{- $value = "1" -}}{{- end -}}
        {{- end -}}
        {{- $value -}}
    "#};
    let values = outputs(source)
        .into_iter()
        .filter_map(|(_, text)| text)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: values, want: BTreeSet::from(["nested-true".to_string(), "nested-false".to_string(), "0".to_string(), "1".to_string()]));
}

#[test]
fn snapshot_restore_replaces_all_local_state_maps() {
    let source = r#"{{ $image := .Values.image }}{{ with true }}{{ $image := $.Values.otherImage }}{{ range $key := list "a" }}{{ end }}{{ end }}{{ $image }}"#;
    sim_assert_eq!(have: identities(source), want: BTreeSet::from([ValuesPath::parse("image")]));
}

#[test]
fn local_scope_restores_shadowed_fragment_value() {
    sim_assert_eq!(have: identities(r"{{ $name := .Values.outer }}{{ if true }}{{ $name := .Values.inner }}{{ end }}{{ $name }}"), want: BTreeSet::from([ValuesPath::parse("outer")]));
}

#[test]
fn local_scope_keeps_assignment_to_outer_fragment_value() {
    sim_assert_eq!(have: identities(r"{{ $name := .Values.outer }}{{ if true }}{{ $name = .Values.assigned }}{{ end }}{{ $name }}"), want: BTreeSet::from([ValuesPath::parse("assigned")]));
}

#[test]
fn local_scope_restores_shadowed_get_binding() {
    sim_assert_eq!(have: identities(r#"{{ $key := "key" }}{{ $value := get .Values.outer $key }}{{ if true }}{{ $value := get .Values.inner $key }}{{ end }}{{ $value }}"#), want: BTreeSet::from([ValuesPath::parse("outer.key")]));
}

#[test]
fn local_scope_keeps_assignment_to_outer_get_binding() {
    sim_assert_eq!(have: identities(r#"{{ $key := "key" }}{{ $value := get .Values.outer $key }}{{ if true }}{{ $value = get .Values.assigned $key }}{{ end }}{{ $value }}"#), want: BTreeSet::from([ValuesPath::parse("assigned.key")]));
}

#[test]
fn fragment_assignment_replaces_outer_get_binding() {
    sim_assert_eq!(have: identities(r#"{{ $key := "key" }}{{ $value := get .Values.outer $key }}{{ if true }}{{ $value = .Values.assigned }}{{ end }}{{ $value }}"#), want: BTreeSet::from([ValuesPath::parse("assigned")]));
}

#[test]
fn local_scope_restores_range_domain_shadowing_outer_binding() {
    sim_assert_eq!(have: identities(r#"{{ $key := .Values.outer }}{{ range $key := list "inner" }}{{ end }}{{ $key }}"#), want: BTreeSet::from([ValuesPath::parse("outer")]));
}

#[test]
fn local_scope_restores_default_paths_for_shadowed_declaration() {
    sim_assert_eq!(have: inputs(r"{{ $name := default .Values.outer.default .Values.outer }}{{ if true }}{{ $name := default .Values.inner.default .Values.inner }}{{ end }}{{ $name }}"), want: BTreeSet::from([ValuesPath::parse("outer.default"), ValuesPath::parse("outer")]));
}

#[test]
fn local_scope_keeps_default_paths_for_outer_assignment() {
    sim_assert_eq!(have: inputs(r"{{ $name := default .Values.outer.default .Values.outer }}{{ if true }}{{ $name = default .Values.assigned.default .Values.assigned }}{{ end }}{{ $name }}"), want: BTreeSet::from([ValuesPath::parse("assigned.default"), ValuesPath::parse("assigned")]));
}

#[test]
fn local_scope_restores_output_meta_for_shadowed_declaration() {
    sim_assert_eq!(have: inputs(r#"{{ $name := printf "%s" .Values.outer.output }}{{ if true }}{{ $name := printf "%s" .Values.inner.output }}{{ end }}{{ $name }}"#), want: BTreeSet::from([ValuesPath::parse("outer.output")]));
}

#[test]
fn local_scope_keeps_output_meta_for_outer_assignment() {
    sim_assert_eq!(have: inputs(r#"{{ $name := printf "%s" .Values.outer.output }}{{ if true }}{{ $name = printf "%s" .Values.assigned.output }}{{ end }}{{ $name }}"#), want: BTreeSet::from([ValuesPath::parse("assigned.output")]));
}

#[test]
fn local_scope_restores_truthy_reduction_for_shadowed_declaration() {
    let source = r#"{{ $message := .Values.outer }}{{ if true }}{{ $message := "" }}{{ end }}{{ if $message }}selected{{ end }}"#;
    sim_assert_eq!(have: selected_condition(source), want: Predicate::truthy_path("outer"));
}

#[test]
fn branch_join_keeps_bindings_present_in_all_outcomes() {
    let source = "{{ $name := .Values.entry }}{{ if .Values.choose }}{{ $name = .Values.first }}{{ else }}{{ $name = .Values.second }}{{ end }}{{ $name }}";
    sim_assert_eq!(have: identities(source), want: BTreeSet::from([ValuesPath::parse("first"), ValuesPath::parse("second")]));
}

#[test]
fn branch_join_unions_truthy_reductions_across_outcomes() {
    let source = "{{ $message := false }}{{ if .Values.legacy }}{{ $message = true }}{{ end }}{{ if $message }}selected{{ end }}";
    sim_assert_eq!(have: selected_condition(source), want: Predicate::truthy_path("legacy"));
}

#[test]
fn bounded_control_guards_preserve_exact_selected_truth() {
    let paths = [
        "guard.a", "guard.b", "guard.c", "guard.d", "guard.e", "guard.f", "result",
    ];
    let program = |empty: Option<&str>| {
        let guards = paths
            .iter()
            .take(6)
            .map(|path| {
                if empty == Some(*path) {
                    "false".to_owned()
                } else {
                    format!(".Values.{path}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let result = if empty == Some("result") {
            "false"
        } else {
            ".Values.result"
        };
        format!(
            "{{{{ $message := false }}}}{{{{ if and {guards} }}}}{{{{ $message = {result} }}}}{{{{ end }}}}{{{{ if $message }}}}selected{{{{ end }}}}"
        )
    };
    // These seven terms fit the structural control budget and remain exact across the join.
    let expected = Predicate::all(
        paths
            .iter()
            .map(|path| Predicate::truthy_path(*path))
            .collect(),
    )
    .normalize_boolean();
    sim_assert_eq!(have: selected_condition(&program(None)), want: expected);
    for path in paths {
        sim_assert_eq!(have: selected_condition(&program(Some(path))), want: Predicate::False);
    }
}

/// A truthiness formula's value under a partial assignment of its paths.
enum Truth {
    Known(bool),
    /// Undetermined until this path is assigned.
    Open(String),
}

/// Evaluates an exact formula over `Truthy` and `Not` guards; `None` for any other atom.
fn partial_truth(predicate: &Predicate, assignment: &BTreeMap<String, bool>) -> Option<Truth> {
    match predicate.kind() {
        PredicateKind::True => Some(Truth::Known(true)),
        PredicateKind::False => Some(Truth::Known(false)),
        PredicateKind::Approximate { .. } => None,
        PredicateKind::Guard(guard) => {
            let (path, negated) = match guard {
                Guard::Truthy { path } => (path, false),
                Guard::Not { path } => (path, true),
                _ => return None,
            };
            let path = path.encode();
            Some(match assignment.get(&path) {
                Some(value) => Truth::Known(*value != negated),
                None => Truth::Open(path),
            })
        }
        PredicateKind::Not(inner) => Some(match partial_truth(inner, assignment)? {
            Truth::Known(value) => Truth::Known(!value),
            Truth::Open(path) => Truth::Open(path),
        }),
        PredicateKind::And(parts) => junction(parts, assignment, false),
        PredicateKind::Or(parts) => junction(parts, assignment, true),
    }
}

/// `And` when `absorbing` is false, `Or` when it is true.
fn junction(
    parts: &[Predicate],
    assignment: &BTreeMap<String, bool>,
    absorbing: bool,
) -> Option<Truth> {
    let mut open = None;
    for part in parts {
        match partial_truth(part, assignment)? {
            Truth::Known(value) if value == absorbing => return Some(Truth::Known(absorbing)),
            Truth::Known(_) => {}
            Truth::Open(path) => {
                open.get_or_insert(path);
            }
        }
    }
    Some(open.map_or(Truth::Known(!absorbing), Truth::Open))
}

/// Decides equivalence by splitting on undetermined paths; `None` when an atom is not a
/// truthiness guard or the split budget runs out.
fn truth_equivalent(
    left: &Predicate,
    right: &Predicate,
    assignment: &mut BTreeMap<String, bool>,
    budget: &mut usize,
) -> Option<bool> {
    *budget = budget.checked_sub(1)?;
    let path = match (
        partial_truth(left, assignment)?,
        partial_truth(right, assignment)?,
    ) {
        (Truth::Known(left), Truth::Known(right)) => return Some(left == right),
        (Truth::Open(path), Truth::Known(_) | Truth::Open(_))
        | (Truth::Known(_), Truth::Open(path)) => path,
    };
    for value in [false, true] {
        assignment.insert(path.clone(), value);
        let equivalent = truth_equivalent(left, right, assignment, budget);
        assignment.remove(&path);
        if !equivalent? {
            return Some(false);
        }
    }
    Some(true)
}

/// Holds when `actual` is exact and is the same Boolean function as `expected`.
fn exactly_equivalent(actual: &Predicate, expected: &Predicate) -> bool {
    !actual.contains_approximation()
        && truth_equivalent(actual, expected, &mut BTreeMap::new(), &mut (1 << 16)) == Some(true)
}

/// Over the interpreter's bounds the selected output keeps its arm and is never published
/// as an exact predicate that differs from Helm's: it is either the exact correct predicate
/// or an explicit unknown (`Predicate::Approximate`). Two bounded shapes are covered: an
/// `and` with more operands than the 128-state dispatch bound, and a chain of sequential
/// guarded reassignments past the branch-join bound. The exact predicates are Go-template
/// semantics: the wide `and` selects iff every guard and `result` are truthy; the chain
/// selects iff the last taken guard's value is truthy.
///
/// Kept red on the current model (probe 2026-11-06): the 129-operand `and` publishes the
/// exact `truthy(result)`, discarding all 129 guard conjuncts, and six or more sequential
/// reassignments publish the exact `True`. Helm controls in
/// `transfer/receipts/over_cap_control_joins_widen_to_an_explicit_unknown` show both are
/// wrong. Missing mechanism: bounded control and value joins must either stay exact or
/// lower the dropped decisions to an explicit approximation over their inputs.
#[test]
fn over_cap_control_joins_widen_to_an_explicit_unknown() -> eyre::Result<()> {
    use std::fmt::Write as _;

    let guards = (0..129)
        .map(|index| format!(".Values.g{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let wide_and = format!(
        "{{{{ $message := false }}}}{{{{ if and {guards} }}}}{{{{ $message = .Values.result }}}}{{{{ end }}}}{{{{ if $message }}}}selected{{{{ end }}}}"
    );
    let wide_and_exact = Predicate::all(
        (0..129)
            .map(|index| Predicate::truthy_path(format!("g{index}")))
            .chain(std::iter::once(Predicate::truthy_path("result")))
            .collect(),
    );

    let mut sequential = String::from("{{ $message := false }}");
    for index in 0..6 {
        write!(
            sequential,
            "{{{{ if .Values.g{index} }}}}{{{{ $message = .Values.r{index} }}}}{{{{ end }}}}"
        )?;
    }
    sequential.push_str("{{ if $message }}selected{{ end }}");
    // The last writer wins: arm `index` decides when its guard holds and no later guard does.
    let sequential_exact = Predicate::Or(
        (0..6)
            .map(|index| {
                Predicate::all(
                    [
                        Predicate::truthy_path(format!("g{index}")),
                        Predicate::truthy_path(format!("r{index}")),
                    ]
                    .into_iter()
                    .chain(
                        (index + 1..6)
                            .map(|later| Predicate::truthy_path(format!("g{later}")).negated()),
                    )
                    .collect(),
                )
            })
            .collect(),
    );

    for (label, exact, wrong) in [
        (
            "wide and",
            &wide_and_exact,
            Predicate::truthy_path("result"),
        ),
        ("sequential joins", &sequential_exact, Predicate::True),
    ] {
        // The equivalence check decides at these sizes and separates today's exact-but-wrong
        // predicate from the correct one, so the acceptance below is neither vacuous nor lax.
        assert!(
            exactly_equivalent(&exact.clone().normalize_boolean(), exact),
            "{label}: the equivalence check did not decide the exact predicate"
        );
        assert!(
            !exactly_equivalent(&wrong, exact),
            "{label}: the equivalence check accepts the known-wrong predicate {wrong:?}"
        );
    }

    for (label, source, exact) in [
        ("wide and", wide_and, wide_and_exact),
        ("sequential joins", sequential, sequential_exact),
    ] {
        let execution = execute(&source);
        let selected = execution
            .text_sites()
            .filter(|site| site.bytes() == "selected")
            .count();
        sim_assert_eq!(have: selected, want: 1, "{label}: the selected arm must survive the bound");
        let condition = selected_condition(&source);
        assert!(
            condition != Predicate::False
                && (condition.contains_approximation() || exactly_equivalent(&condition, &exact)),
            "{label}: the over-cap condition is neither Helm's exact predicate nor an explicit \
             unknown: {condition:?}"
        );
    }
    Ok(())
}

#[test]
fn exact_if_join_conditions_an_untouched_entry_truthy_reduction() {
    let source = "{{ $continue := .Values.gate }}{{ if .Values.stop }}{{ $continue = false }}{{ end }}{{ if $continue }}selected{{ end }}";
    sim_assert_eq!(have: selected_condition(source), want: Predicate::all(vec![Predicate::truthy_path("gate"), Predicate::truthy_path("stop").negated()]).normalize_boolean());
}

#[test]
fn branch_join_intersects_chart_value_defaults() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $_ := set .Values "already" (dict "defaulted" "before") -}}
        {{- if .Values.choose -}}{{- $_ := set .Values "branch" (dict "only" "after") -}}{{- end -}}
        {{- .Values.already.defaulted -}}
    "#};
    let execution = execute(source);
    let output = execution
        .executed_outputs()
        .into_iter()
        .next()
        .ok_or_eyre("missing default output")?;
    sim_assert_eq!(have: output.text, want: Some("before".to_string()));
    assert!(
        execution
            .executed_outputs()
            .into_iter()
            .all(|output| output.text.as_deref() == Some("before"))
    );
    Ok(())
}
