use test_util::prelude::sim_assert_eq;

use crate::RootPolicy;
use crate::merge::merge_two_schemas;
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;

#[test]
fn executed_unconditional_member_access_still_requires_an_object() {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ .Values.a.b | quote }}
    "};
    let schema = super::schema_for(super::parse_ir(source));
    for (instance, accepted) in [
        (serde_json::json!({ "a": true }), false),
        (serde_json::json!({ "a": { "b": "value" } }), true),
    ] {
        sim_assert_eq!(
            have: super::schema_accepts_instance(&schema, &instance),
            want: accepted,
        );
    }
}

#[test]
fn executed_guarded_member_access_keeps_its_original_domain() {
    let source = indoc::indoc! {r#"
        {{- if and .Values.internalTLS.enabled (or (eq .Values.internalTLS.certSource "manual") (eq .Values.internalTLS.certSource "secret")) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ .Values.internalTLS.core.secretName | quote }}
        {{- end }}
    "#};
    let schema = super::schema_for(super::parse_ir(source));
    for (enabled, cert_source, accepted) in [
        (false, "auto", true),
        (true, "auto", true),
        (true, "manual", false),
        (true, "secret", false),
    ] {
        let instance = serde_json::json!({ "internalTLS": {
            "enabled": enabled, "certSource": cert_source, "core": true,
        } });
        sim_assert_eq!(
            have: super::schema_accepts_instance(&schema, &instance),
            want: accepted,
        );
    }
}

#[test]
fn descendant_navigation_does_not_assert_intermediate_objects() {
    for reserve_host in [false, true] {
        let mut document = SchemaDocument::new_root_object(RootPolicy::Open);
        if reserve_host {
            document.insert_path_schema(&["tls".into(), "core".into()], SchemaNode::empty());
        }
        document.insert_path_schema(
            &["tls".into(), "core".into(), "secretName".into()],
            SchemaNode::empty(),
        );
        sim_assert_eq!(
            have: document.into_value(),
            want: serde_json::json!({
                "type": "object",
                "properties": { "tls": {
                    "additionalProperties": {},
                    "properties": { "core": {
                        "additionalProperties": {},
                        "properties": { "secretName": {} },
                    } },
                } },
            }),
        );
    }
}

#[test]
fn descendant_navigation_preserves_an_independent_object_contract() {
    for contract in [
        SchemaNode::unknown_object(),
        SchemaNode::from_value(serde_json::json!({
            "type": "object", "additionalProperties": {},
        })),
    ] {
        let mut document = SchemaDocument::new_root_object(RootPolicy::Open);
        document.insert_path_schema(&["tls".into()], contract);
        document.insert_path_schema(
            &["tls".into(), "core".into(), "secretName".into()],
            SchemaNode::empty(),
        );
        sim_assert_eq!(
            have: document.into_value(),
            want: serde_json::json!({
                "type": "object",
                "properties": { "tls": {
                    "type": "object",
                    "additionalProperties": {},
                    "properties": { "core": {
                        "additionalProperties": {},
                        "properties": { "secretName": {} },
                    } },
                } },
            }),
        );
    }
}

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
