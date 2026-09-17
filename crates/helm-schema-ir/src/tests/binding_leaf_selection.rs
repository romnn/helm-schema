//! Adversarial witnesses for the F78/A1 narrowed value-lane rule.
//!
//! These are chart-free: each builds the `LocalBinding` shape directly and
//! reads it through `eval_expr`, so the witness pins the rule itself rather
//! than one chart's route to it.

use crate::abstract_value::AbstractValue;
use crate::eval_effect::CaptureKind;
use crate::eval_env::{BindingDecision, BindingEvaluationMode, EvalEnv, LocalBinding};
use crate::expr_eval::eval_expr;
use crate::scalar_value::TruthCondition;
use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_core::Predicate;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::sim_assert_eq;

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn capture_path(value: &str) -> helm_schema_core::ValuesPath {
    helm_schema_core::ValuesPath::parse(value)
}

/// Whether the value records that something outside its named alternatives
/// can still reach this read.
fn records_unresolved_width(value: &AbstractValue) -> bool {
    match value {
        AbstractValue::Top | AbstractValue::Unknown => true,
        AbstractValue::Choice(choices) => choices
            .iter()
            .any(|choice| matches!(choice, AbstractValue::Top | AbstractValue::Unknown)),
        _ => false,
    }
}

fn member_host_captures(result: &crate::eval_effect::EvalResult) -> usize {
    result
        .effects
        .observed_facts
        .captures
        .iter()
        .filter(|capture| matches!(capture.kind, CaptureKind::MemberAccess { .. }))
        .count()
}

/// A read of a binding whose remainder is unresolved must record that
/// remainder, whether the read takes the whole binding or one member of it.
///
/// `Select(Unknown, candidate, Unknown)` is exactly what
/// `SymbolicLocalState::widen_changed_fragment_bindings` builds at a range
/// exit whose iteration count is not exact: the `Unknown` branch stands for
/// every value the loop did NOT produce, starting with the variable's entry
/// value. Dropping it from a member read turns "this member is
/// `first.child`, or something the analyzer cannot model" into "this member
/// IS `first.child`", which is a closed claim the chart never made.
#[test]
fn selector_read_of_an_unresolved_binding_keeps_the_unknown_remainder() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "cfg".to_string(),
        LocalBinding::unresolved_with_candidate(
            BindingDecision::new(TruthCondition::Unknown),
            LocalBinding::direct(values_path!("first")),
        ),
    );

    let whole = eval_expr(&single_expr("$cfg"), &env);
    let member = eval_expr(&single_expr("$cfg.child"), &env);

    sim_assert_eq!(
        have: whole.value.as_ref().map(records_unresolved_width),
        want: Some(true),
        "the whole-binding read records the unresolved remainder: {whole:#?}",
    );
    sim_assert_eq!(
        have: member.value.as_ref().map(records_unresolved_width),
        want: Some(true),
        "the member read must record the same remainder: {member:#?}",
    );
}

/// A read whose decision proves one arm reports that arm plus the
/// unresolved remainder, and does NOT attribute the unprovable sibling's
/// identity.
///
/// `Partial { when_true: truthy(flag), when_false: False }` is what
/// `or .Values.flag (include "unknown" .)` produces: the decision names WHEN
/// the true arm is taken and can say nothing about the other. Naming the
/// sibling anyway does not widen the read, it TIGHTENS it: a fragment sink
/// consumes the value lane as a conjunction, so every identity in the join
/// is bound at the document position it reaches and two identities reaching
/// incompatible positions on mutually exclusive arms become one obligation
/// no document satisfies. `Unknown` says the same "something else can reach
/// here" without minting an obligation the chart never made.
#[test]
fn mixed_selection_read_reports_the_proven_arm_and_the_unresolved_remainder() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "cfg".to_string(),
        LocalBinding::select(
            BindingDecision::new(TruthCondition::Partial {
                when_true: Predicate::truthy_path("flag"),
                when_false: Predicate::False,
            }),
            LocalBinding::direct(values_path!("proven")),
            LocalBinding::direct(values_path!("unproven")),
        ),
    );

    let read = eval_expr(&single_expr("$cfg"), &env);

    sim_assert_eq!(
        have: read.value.as_ref().map(AbstractValue::paths),
        want: Some(BTreeSet::from([capture_path("proven")])),
        "the unprovable sibling contributes width, not identity: {read:#?}",
    );
    sim_assert_eq!(
        have: read.value.as_ref().map(records_unresolved_width),
        want: Some(true),
        "the remainder the decision cannot name is still recorded: {read:#?}",
    );
}

/// The same rule where the two arms wrap ONE values path at incompatible
/// document positions: the join must not co-assert both.
///
/// This is longhorn's `$imagePullSecrets` and airflow's `image_pull_secrets`
/// in miniature. Both append `dict "name" <item>` on the arm their `kindIs
/// "string"` test selects and the item itself on the other, so the list the
/// sink receives places the SAME values path either at
/// `imagePullSecrets[*].name` (a string) or at `imagePullSecrets[*]` (a
/// `LocalObjectReference`). Joining both identities makes the sink demand
/// both of one value, which only `null` satisfies — and `helm template`
/// renders `imagePullSecrets: [{}]` for either chart without complaint, so
/// that obligation is a false rejection, not a discovered constraint.
#[test]
fn a_proven_arm_does_not_co_assert_a_sibling_binding_of_the_same_path() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "secrets".to_string(),
        LocalBinding::select(
            BindingDecision::new(TruthCondition::Partial {
                when_true: Predicate::truthy_path("named"),
                when_false: Predicate::False,
            }),
            LocalBinding::direct(AbstractValue::Dict(BTreeMap::from([(
                "name".to_string(),
                values_path!("pullSecrets"),
            )]))),
            LocalBinding::direct(values_path!("pullSecrets")),
        ),
    );

    let read = eval_expr(&single_expr("$secrets"), &env);
    let alternatives = read
        .value
        .as_ref()
        .map(|value| match value {
            AbstractValue::Choice(choices) => choices.iter().cloned().collect::<BTreeSet<_>>(),
            other => BTreeSet::from([other.clone()]),
        })
        .unwrap_or_default();

    sim_assert_eq!(
        have: alternatives.contains(&values_path!("pullSecrets")),
        want: false,
        "the bare item binding is not co-asserted with the wrapped one: {read:#?}",
    );
    sim_assert_eq!(
        have: alternatives.contains(&AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            values_path!("pullSecrets"),
        )]))),
        want: true,
        "the proven arm's binding is still reported: {read:#?}",
    );
}

/// The strict lane must scope its obligation by the proven arm's condition
/// and place nothing on the unprovable sibling.
#[test]
fn mixed_selection_proves_only_the_decidable_arm() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "cfg".to_string(),
        LocalBinding::select(
            BindingDecision::new(TruthCondition::Partial {
                when_true: Predicate::truthy_path("flag"),
                when_false: Predicate::False,
            }),
            LocalBinding::direct(values_path!("proven")),
            LocalBinding::direct(values_path!("unproven")),
        ),
    );

    let read = eval_expr(&single_expr("$cfg"), &env);
    let proven = read
        .proven_operands
        .as_ref()
        .map(|proven| (proven.known.len(), proven.has_unresolved));

    sim_assert_eq!(have: proven, want: Some((1, true)));

    let strict = eval_expr(&single_expr(r#"semverCompare "<1.0.0" $cfg"#), &env);
    let scoped = strict
        .effects
        .observed_facts
        .captures
        .iter()
        .map(|capture| {
            (
                capture
                    .conjunction
                    .iter()
                    .any(|predicate| predicate == &Predicate::truthy_path("flag")),
                format!("{:?}", capture.kind)
                    .chars()
                    .take(24)
                    .collect::<String>(),
            )
        })
        .collect::<BTreeSet<_>>();

    sim_assert_eq!(
        have: scoped.iter().all(|(scoped_by_flag, _)| *scoped_by_flag),
        want: true,
        "every strict obligation is scoped by the proven arm: {scoped:#?}",
    );
}

/// A `Values`-rooted selector read of a local that holds a plain values path
/// must not manufacture member-host obligations over a literal `Values`
/// segment.
///
/// Inside a helper, `$` is bound to the helper's argument
/// (`SymbolicLocalState::with_root`), so `include "h" .Values.sub` makes `$`
/// the value at `sub`. `bound_values_member` reports no `Values` member for
/// a bare values path, which is why the pre-patch code recorded nothing
/// here.
#[test]
fn values_rooted_selector_on_a_values_path_root_records_no_member_obligation() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        String::new(),
        LocalBinding::new(values_path!("sub"), BindingEvaluationMode::Evaluated),
    );

    let read = eval_expr(&single_expr("$.Values.child"), &env);

    sim_assert_eq!(
        have: member_host_captures(&read),
        want: 0,
        "a Values-rooted read of a values-path root records no member host: {read:#?}",
    );
}

/// The same read with a `Direct` boundary, which is the other arm of the
/// fall-through the refactor introduced.
#[test]
fn values_rooted_direct_selector_on_a_values_path_root_records_no_member_obligation() {
    let mut env = EvalEnv::default();
    env.locals
        .insert("ctx".to_string(), LocalBinding::direct(values_path!("sub")));

    let read = eval_expr(&single_expr("$ctx.Values.child"), &env);

    sim_assert_eq!(
        have: member_host_captures(&read),
        want: 0,
        "a Values-rooted read of a values-path local records no member host: {read:#?}",
    );
}
