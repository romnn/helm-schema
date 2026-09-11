//! Ownership contracts for evaluated values: what an expression's result
//! still knows about the value that produced it.
//!
//! These pin the facts a later ownership migration must preserve when the
//! conditioned scalar program moves onto the binding leaf.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::parse_expr_text;
use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::scalar_value::TruthCondition;

/// An opaque primary and its literal fallback remain distinct ordered
/// candidates, and only the fallback carries a modeled runtime scalar.
///
/// `default` selects by Helm truthiness of the primary, so erasing either
/// candidate — or claiming a scalar for the unmodeled `printf` output —
/// would make the selection unrepresentable.
#[test]
fn default_retains_an_explicit_unknown_primary() -> eyre::Result<()> {
    let expressions = parse_expr_text(r#"printf "%q" .Values.alpha | default "fallback""#);
    let result = eval_expr(
        expressions
            .first()
            .ok_or_eyre("missing parsed expression")?,
        &EvalEnv::default(),
    );
    let AbstractValue::FirstTruthy(candidates) = result
        .value
        .clone()
        .ok_or_eyre("default did not retain its selected value")?
    else {
        return Err(eyre::eyre!(
            "default did not retain ordered candidates: {result:#?}"
        ));
    };

    sim_assert_eq!(have: candidates.len(), want: 2);
    sim_assert_eq!(
        have: candidates.first().map(AbstractValue::paths),
        want: Some([ValuesPath::parse("alpha")].into_iter().collect()),
    );
    sim_assert_eq!(
        have: candidates.get(1).cloned(),
        want: Some(AbstractValue::StringSet(
            ["fallback".to_string()].into_iter().collect(),
        )),
    );
    // The primary's runtime value is a `%q` rendering, not the raw path, so
    // the selection has no modeled scalar to dispatch on.
    sim_assert_eq!(have: result.scalar_dispatch, want: None);
    Ok(())
}

/// `default` selects literal null and truthy non-string primaries by Helm
/// truthiness.
#[test]
fn default_type_truth_handles_null_and_truthy_nonstring_primaries() -> eyre::Result<()> {
    for (source, expected) in [
        (r#"typeIs "string" (default "fallback" nil)"#, true),
        (r#"typeIs "boolean" (default "fallback" true)"#, true),
        (r#"typeIs "string" (default "fallback" true)"#, false),
    ] {
        let expressions = parse_expr_text(source);
        let result = eval_expr(
            expressions
                .first()
                .ok_or_eyre("missing parsed expression")?,
            &EvalEnv::default(),
        );
        sim_assert_eq!(
            have: result.truth,
            want: TruthCondition::Exact(if expected {
                Predicate::True
            } else {
                Predicate::False
            }),
            "source={source}",
        );
    }
    Ok(())
}

/// Type tests follow the runtime arm selected by an ordered `default` value.
#[test]
fn default_type_truth_tracks_the_selected_dynamic_value() -> eyre::Result<()> {
    let expressions = parse_expr_text(r#"typeIs "string" (default "fallback" .Values.primary)"#);
    let result = eval_expr(
        expressions
            .first()
            .ok_or_eyre("missing parsed expression")?,
        &EvalEnv::default(),
    );
    let predicate = PredicateMemo::new().normalize(Predicate::Or(vec![
        Predicate::truthy_path("primary").negated(),
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("primary"),
            schema_type: "string".to_string(),
        }),
    ]));

    sim_assert_eq!(
        have: result.truth,
        want: TruthCondition::Exact(predicate),
    );
    Ok(())
}
