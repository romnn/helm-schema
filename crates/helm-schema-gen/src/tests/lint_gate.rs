//! The `helm lint` gate: a constraint a lint document fails is conditioned
//! on the presence of the paths that document lacks, or withdrawn.

use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ConditionalGuard, GuardValue, ValuesPath};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionPolarity, HELM_TRUTHY_DEFINITION_NAME, build_condition_clauses,
    helm_truthy_definition_schema,
};
use crate::emission_report::{LintDocument, LintOutcome, LintWithdrawal};
use crate::schema_node::SchemaNode;
use crate::uncoalesced_root::{GuardEncoding, UncoalescedRootGate};

/// The values roots dependency charts own and their composed defaults.
struct Dependencies<'a> {
    roots: &'a BTreeSet<Vec<String>>,
    refill: &'a Value,
}

const NO_DEPENDENCIES: Dependencies<'static> = Dependencies {
    roots: &BTreeSet::new(),
    refill: &Value::Null,
};

/// Runs the gate on `if guards then then_schema`, anchored at `anchor`, and
/// returns the kept constraint, placed at its anchor, as a validatable
/// document with the records.
fn judge(
    root: &Value,
    coalesced: &Value,
    anchor: &[&str],
    guards: &[ConditionalGuard],
    then_schema: Value,
    polarity: ConditionPolarity,
    dependencies: &Dependencies<'_>,
) -> eyre::Result<(Option<Value>, Vec<LintWithdrawal>)> {
    let definitions = BTreeMap::new();
    let anchor = anchor
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let values_yaml_doc = serde_yaml::to_value(coalesced)?;
    let dependency_refill = serde_yaml::to_value(dependencies.refill)?;
    let absent = serde_yaml::Value::Null;
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &dependency_refill,
        dependency_roots: dependencies.roots,
    };
    let condition = SchemaNode::all_of(build_condition_clauses(
        guards,
        &anchor,
        &values_yaml_doc,
        absence,
        polarity,
    ));
    let mut gate = UncoalescedRootGate::new(Some(root), coalesced, &definitions);
    let kept = gate.lint_safe_then(
        &anchor,
        guards,
        &condition,
        &SchemaNode::foreign(then_schema),
        GuardEncoding {
            values_yaml_doc: &values_yaml_doc,
            absence,
            polarity,
        },
    );
    let document = kept.map(|then_schema| {
        let mut constraint = json!({
            "if": condition.into_value(),
            "then": then_schema.into_value(),
        });
        for segment in anchor.iter().rev() {
            constraint = if segment == "*" {
                json!({ "additionalProperties": constraint })
            } else {
                json!({ "properties": { segment.as_str(): constraint } })
            };
        }
        json!({
            "$defs": { HELM_TRUTHY_DEFINITION_NAME: helm_truthy_definition_schema() },
            "allOf": [constraint],
        })
    });
    Ok((document, gate.into_relaxed()))
}

/// Root `kid: {}`, dependency default `kid.p: true`: every override that
/// switches `kid.p` on also supplies it, so the clause's type check must
/// survive; only its presence requirement is lost.
#[test]
fn a_floor_failure_keeps_the_checks_behind_presence() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "kid": {} }),
        &json!({ "kid": { "p": true } }),
        &[],
        &[ConditionalGuard::Truthy {
            path: ValuesPath::parse("kid.p"),
        }],
        json!({ "properties": { "kid": {
            "required": ["p"],
            "properties": { "p": { "enum": [true, "on"] } },
        } } }),
        ConditionPolarity::Widen,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    let validator = jsonschema::validator_for(&document)?;
    assert!(
        validator.is_valid(&json!({ "kid": {} })),
        "the lint floor passes"
    );
    assert!(validator.is_valid(&json!({ "kid": { "p": true } })));
    assert!(
        !validator.is_valid(&json!({ "kid": { "p": "yes" } })),
        "a supplied value is still checked"
    );
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.p")],
        outcome: LintOutcome::Conditioned,
    }]);
    Ok(())
}

/// Root `kid: {enabled: false, p: null}`, dependency default `p: 80`, and a
/// clause "if enabled, `p` has no key". `helm lint -f {kid: {enabled: true}}`
/// keeps the root's present null, which coalescing deletes, so the clause
/// must be relaxed although neither the root as written (not enabled) nor
/// a null-free floor fails it.
#[test]
fn a_root_null_the_lint_document_keeps_decides() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "kid": { "enabled": false, "p": null } }),
        &json!({ "kid": { "enabled": false } }),
        &[],
        &[
            ConditionalGuard::Truthy {
                path: ValuesPath::parse("kid.enabled"),
            },
            ConditionalGuard::HasKey {
                path: ValuesPath::parse("kid"),
                key: "p".to_string(),
            },
        ],
        json!(false),
        ConditionPolarity::Narrow,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    let validator = jsonschema::validator_for(&document)?;
    assert!(
        validator.is_valid(&json!({ "kid": { "enabled": true, "p": null } })),
        "`helm lint -f {{kid: {{enabled: true}}}}` passes"
    );
    assert!(
        !validator.is_valid(&json!({ "kid": { "enabled": true, "p": 1 } })),
        "a supplied `p` still aborts the render"
    );
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.p")],
        outcome: LintOutcome::Conditioned,
    }]);
    Ok(())
}

/// Root `kid: {enabled: false}`; the dependency supplies `tls: {deploy:
/// false, paths: /ping, port: 8080}`, and a clause "if `kid.enabled` and
/// `tls.paths` is not empty, `tls.port` is a present integer" (a Service
/// port). `helm lint -f {kid: {enabled: true, tls: {deploy: true}}}` validates
/// an override-created `tls` table without `port`, which `helm template`
/// renders because the dependency default supplies it (Helm v4.2.3). The
/// floor itself lacks `tls`, so only the table an override creates exposes
/// the failure.
#[test]
fn an_override_created_table_decides_a_nested_requirement() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "kid": { "enabled": false } }),
        &json!({ "kid": {
            "enabled": false,
            "tls": { "deploy": false, "paths": "/ping", "port": 8080 },
        } }),
        &[],
        &[
            ConditionalGuard::Truthy {
                path: ValuesPath::parse("kid.enabled"),
            },
            ConditionalGuard::NotEq {
                path: ValuesPath::parse("kid.tls.paths"),
                value: GuardValue::String(String::new()),
            },
        ],
        json!({ "properties": { "kid": { "properties": { "tls": {
            "required": ["port"],
            "properties": { "port": { "type": "integer" } },
        } } } } }),
        ConditionPolarity::Widen,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    let validator = jsonschema::validator_for(&document)?;
    assert!(
        validator.is_valid(&json!({ "kid": { "enabled": true, "tls": { "deploy": true } } })),
        "`helm lint -f {{kid: {{enabled: true, tls: {{deploy: true}}}}}}` passes"
    );
    assert!(validator.is_valid(&json!({ "kid": {
        "enabled": true,
        "tls": { "deploy": true, "paths": "/ping", "port": 8080 },
    } })));
    assert!(
        !validator.is_valid(&json!({ "kid": {
            "enabled": true,
            "tls": { "deploy": false, "paths": "/ping", "port": "x" },
        } })),
        "`helm template --set kid.enabled=true,kid.tls.port=x` still fails"
    );
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Floor,
        deciding_paths: vec![
            ValuesPath::parse("kid.tls.paths"),
            ValuesPath::parse("kid.tls.port"),
        ],
        outcome: LintOutcome::Conditioned,
    }]);
    Ok(())
}

/// Root `on: false`; only the dependency declares `kid`, with default
/// `kid.grp`, and a clause "if `on` and `kid` is a table without `grp`,
/// abort" (a nil dereference). `helm lint -f {on: true, kid: {other: true}}`
/// validates an override-created `kid` without `grp`, which `helm template`
/// renders from the dependency default (Helm v4.2.3). The absence guard
/// already holds on the floor, which lacks `kid`; only the table an
/// override creates for a sibling member exposes the failure.
#[test]
fn an_override_created_table_decides_an_absence_guard() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "on": false }),
        &json!({ "on": false, "kid": { "grp": { "x": 1 } } }),
        &[],
        &[
            ConditionalGuard::Truthy {
                path: ValuesPath::parse("on"),
            },
            ConditionalGuard::Absent {
                path: ValuesPath::parse("kid.grp"),
            },
        ],
        json!(false),
        ConditionPolarity::Widen,
        &Dependencies {
            roots: &BTreeSet::from([vec!["kid".to_string()]]),
            refill: &json!({ "kid": { "grp": { "x": 1 } } }),
        },
    )?;
    sim_assert_eq!(have: document, want: None);
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.grp")],
        outcome: LintOutcome::Withdrawn,
    }]);
    Ok(())
}

/// An absence test on the deciding path contradicts its presence, so the
/// clause is withdrawn rather than conditioned into one that never fires.
#[test]
fn an_absence_guard_on_the_deciding_path_withdraws() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "kid": {} }),
        &json!({ "kid": { "grp": { "enabled": true } } }),
        &[],
        &[ConditionalGuard::Absent {
            path: ValuesPath::parse("kid.grp"),
        }],
        json!(false),
        ConditionPolarity::Narrow,
        &NO_DEPENDENCIES,
    )?;
    sim_assert_eq!(have: document, want: None);
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Root,
        deciding_paths: vec![ValuesPath::parse("kid.grp")],
        outcome: LintOutcome::Withdrawn,
    }]);
    Ok(())
}

/// Root `kid.entries: {}`; the dependency supplies `entries.alpha: {enabled:
/// false, p: 80}`, and a member contract "a member with `enabled` requires an
/// integer `p`", its member condition inside `then` (a `required` inside a
/// `range`). `helm lint -f {kid: {entries: {alpha: {enabled: true}}}}`
/// validates an override-created `alpha` without `p`, which `helm template`
/// renders with the dependency's `p: 80` (Helm v4.2.3,
/// witness/r4f/ranged). The member condition cannot be switched on from
/// outside the member, so the member's presence requirements decide.
#[test]
fn a_dependency_supplied_member_decides_a_member_requirement() -> eyre::Result<()> {
    let truthy = json!({ "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}") });
    let (document, relaxed) = judge(
        &json!({ "kid": { "entries": {} } }),
        &json!({ "kid": { "entries": { "alpha": { "enabled": false, "p": 80 } } } }),
        &[],
        &[],
        json!({ "properties": { "kid": { "properties": { "entries": {
            "additionalProperties": { "anyOf": [
                { "properties": { "enabled": { "not": truthy } } },
                { "required": ["p"], "properties": { "p": { "type": "integer" } } },
            ] },
        } } } } }),
        ConditionPolarity::Widen,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    let validator = jsonschema::validator_for(&document)?;
    assert!(
        validator.is_valid(&json!({ "kid": { "entries": { "alpha": { "enabled": true } } } })),
        "`helm lint -f {{kid: {{entries: {{alpha: {{enabled: true}}}}}}}}` passes"
    );
    assert!(validator.is_valid(&json!({ "kid": { "entries": {
        "alpha": { "enabled": true, "p": 80 },
    } } })));
    assert!(
        !validator.is_valid(&json!({ "kid": { "entries": {
            "alpha": { "enabled": true, "p": "x" },
        } } })),
        "a supplied `p` is still checked"
    );
    assert!(
        !validator.is_valid(&json!({ "kid": { "entries": {
            "alpha": { "enabled": false, "p": 80 },
            "beta": { "enabled": true },
        } } })),
        "`helm template --set kid.entries.beta.enabled=true` still aborts"
    );
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse(""),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.entries.alpha.p")],
        outcome: LintOutcome::Conditioned,
    }]);
    Ok(())
}

/// The same member requirement anchored at the ranged member, its member
/// condition an outer guard (`if kid.entries.*.enabled then p is a present
/// integer`). An override setting `kid.entries.alpha.enabled` creates the
/// member `alpha` as Helm coalesces it; the floor lacks every member, so
/// only the created member exposes the failure.
#[test]
fn an_override_created_member_decides_a_ranged_requirement() -> eyre::Result<()> {
    let (document, relaxed) = judge(
        &json!({ "kid": { "entries": {} } }),
        &json!({ "kid": { "entries": { "alpha": { "enabled": false, "p": 80 } } } }),
        &["kid", "entries", "*"],
        &[ConditionalGuard::Truthy {
            path: ValuesPath::parse("kid.entries.*.enabled"),
        }],
        json!({ "required": ["p"], "properties": { "p": { "type": "integer" } } }),
        ConditionPolarity::Widen,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    let validator = jsonschema::validator_for(&document)?;
    assert!(
        validator.is_valid(&json!({ "kid": { "entries": { "alpha": { "enabled": true } } } })),
        "`helm lint -f {{kid: {{entries: {{alpha: {{enabled: true}}}}}}}}` passes"
    );
    assert!(
        !validator.is_valid(&json!({ "kid": { "entries": {
            "alpha": { "enabled": true, "p": "x" },
        } } })),
        "a supplied `p` is still checked"
    );
    sim_assert_eq!(have: relaxed, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse("kid.entries.*"),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.entries.*.p")],
        outcome: LintOutcome::Conditioned,
    }]);
    Ok(())
}

/// A constraint no lint document fails stays exactly as it was.
#[test]
fn a_constraint_no_lint_document_fails_is_kept() -> eyre::Result<()> {
    let then_schema = json!({ "properties": { "kid": { "type": "object" } } });
    let (document, relaxed) = judge(
        &json!({ "kid": { "p": 1 } }),
        &json!({ "kid": { "p": 1 } }),
        &[],
        &[ConditionalGuard::Truthy {
            path: ValuesPath::parse("kid.p"),
        }],
        then_schema.clone(),
        ConditionPolarity::Widen,
        &NO_DEPENDENCIES,
    )?;
    let document = document.ok_or_eyre("the constraint was withdrawn")?;
    sim_assert_eq!(have: document.pointer("/allOf/0/then"), want: Some(&then_schema));
    sim_assert_eq!(have: relaxed, want: vec![]);
    Ok(())
}
