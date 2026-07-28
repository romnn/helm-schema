use test_util::prelude::sim_assert_eq;

use crate::merge::merge_two_schemas;

/// Two member carriers for the same host merge member-wise even when one of
/// them declines to type the host itself.
///
/// A host whose object-ness is claimed only under a guard leaves an untyped
/// carrier behind (`{properties: …}` with no `type`), and the base tree then
/// merges it with the typed carrier its other members build. Unioning the
/// two instead makes EVERY member optional to satisfy — a document typed
/// wrong under one carrier simply satisfies the other — which is how one
/// guarded leaf under kube-prometheus-stack's `defaultRules.rules` dropped
/// the declared boolean typing of its 37 siblings.
///
/// The merged carrier stays untyped: re-stamping `type: object` would
/// reinstate the unconditional host claim the untyped side deliberately
/// dropped.
#[test]
fn untyped_member_carrier_merges_with_its_typed_sibling() {
    let untyped = serde_json::json!({
        "additionalProperties": {},
        "properties": { "alertmanager": { "type": "boolean" } },
    });
    let typed = serde_json::json!({
        "type": "object",
        "additionalProperties": {},
        "properties": { "configReloaders": { "type": "boolean" } },
    });

    sim_assert_eq!(
        have: merge_two_schemas(untyped, typed),
        want: serde_json::json!({
            "additionalProperties": {},
            "properties": {
                "alertmanager": { "type": "boolean" },
                "configReloaders": { "type": "boolean" },
            },
        }),
    );
}

/// A carrier that says more than "these members are typed" keeps its
/// alternation: only a plain member carrier can be conjoined this way, and a
/// non-object domain beside it is a real alternative rather than more
/// evidence about the same object.
#[test]
fn untyped_scalar_alternative_stays_a_union() {
    let scalar = serde_json::json!({ "minLength": 1 });
    let typed = serde_json::json!({
        "type": "object",
        "properties": { "enabled": { "type": "boolean" } },
    });

    let merged = merge_two_schemas(scalar.clone(), typed.clone());
    assert!(
        merged.get("anyOf").is_some(),
        "a non-carrier fragment keeps its union: merged={merged}"
    );
}
