use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn grouped_selector_receiver_is_optional_but_present_scalars_fail() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          grouped: {{ (.Values.grouped.receiver).leaf | quote }}
          {{- if .Values.strict.enabled }}
          strict: {{ .Values.strict.receiver.leaf | quote }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        grouped: {}
        strict:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let grouped_receiver_present = serde_json::json!({
        "not": {
            "anyOf": [
                {
                    "not": {
                        "properties": {
                            "grouped": {
                                "properties": { "receiver": {} },
                                "required": ["receiver"],
                                "type": "object",
                            },
                        },
                        "required": ["grouped"],
                        "type": "object",
                    },
                },
                {
                    "properties": {
                        "grouped": {
                            "properties": { "receiver": { "enum": [null] } },
                            "required": ["receiver"],
                            "type": "object",
                        },
                    },
                    "required": ["grouped"],
                    "type": "object",
                },
            ],
        },
    });
    let strict_enabled = serde_json::json!({
        "properties": {
            "strict": {
                "properties": { "enabled": { "$ref": "#/$defs/helm-truthy" } },
                "required": ["enabled"],
                "type": "object",
            },
        },
        "required": ["strict"],
        "type": "object",
    });
    let mut properties = serde_json::Map::new();
    properties.insert(
        "grouped".to_string(),
        serde_json::json!({
            "additionalProperties": {},
            "properties": {
                "receiver": {
                    "additionalProperties": {},
                    "properties": { "leaf": {} },
                },
            },
            "type": "object",
        }),
    );
    properties.insert(
        "strict".to_string(),
        serde_json::json!({
            "additionalProperties": {},
            "properties": {
                "enabled": {
                    "anyOf": [
                        { "not": { "$ref": "#/$defs/helm-truthy" } },
                        { "type": "boolean" },
                    ],
                },
                "receiver": {
                    "additionalProperties": {},
                    "properties": { "leaf": {} },
                },
            },
            "type": "object",
        }),
    );
    // Arms sharing one encoded condition conjoin their contents into a
    // single `if C then allOf [...]` (the emitter's size-bounding merge).
    let all_of = vec![
        serde_json::json!({
            "if": grouped_receiver_present,
            "then": { "allOf": [
                root_property_schema(
                    "grouped",
                    serde_json::json!({
                        "additionalProperties": {},
                        "properties": {
                            "receiver": { "anyOf": [{ "type": "object" }] },
                        },
                    }),
                ),
                root_property_schema(
                    "grouped",
                    serde_json::json!({ "required": ["receiver"], "type": "object" }),
                ),
            ] },
        }),
        serde_json::json!({
            "if": strict_enabled,
            "then": { "allOf": [
                root_property_schema(
                    "strict",
                    serde_json::json!({
                        "additionalProperties": {},
                        "properties": {
                            "receiver": { "anyOf": [{ "type": "object" }] },
                        },
                    }),
                ),
                root_property_schema(
                    "strict",
                    serde_json::json!({ "required": ["receiver"], "type": "object" }),
                ),
            ] },
        }),
    ];
    for instance in [
        serde_json::json!({ "grouped": {}, "strict": { "enabled": false } }),
        serde_json::json!({ "grouped": { "receiver": null }, "strict": { "enabled": false } }),
        serde_json::json!({ "grouped": { "receiver": {} }, "strict": { "enabled": false } }),
        serde_json::json!({ "grouped": {}, "strict": { "enabled": false, "receiver": "skipped" } }),
        serde_json::json!({
            "grouped": {},
            "strict": { "enabled": true, "receiver": {} }
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "absent/null grouped receivers and object receivers render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({
            "grouped": { "receiver": "not-an-object" },
            "strict": { "enabled": false }
        }),
        serde_json::json!({ "grouped": {}, "strict": { "enabled": true } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "present scalar grouped receivers and missing strict receivers fail: instance={instance}; schema={schema}"
        );
    }
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, true)
    );
}

/// A `hasKey` guard on the rendered leaf is already enforced by property
/// presence. Its provider schema must therefore occupy the empty leaf slot
/// directly instead of turning that scalar slot into an object host.
#[test]
fn present_key_guard_keeps_scalar_provider_schema_at_leaf() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              {{- if hasKey .Values.global "hostUsers" }}
              hostUsers: {{ .Values.global.hostUsers }}
              {{- end }}
              containers:
                - name: test
                  image: test
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("global: {}\n"));

    for instance in [
        serde_json::json!({ "global": {} }),
        serde_json::json!({ "global": { "hostUsers": true } }),
        serde_json::json!({ "global": { "hostUsers": false } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "absent and boolean hostUsers values render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "global": { "hostUsers": "false" } })
        ),
        "an unquoted Boolean string reparses to the provider's Boolean field: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "global": { "hostUsers": "audit" } })
        ),
        "a non-Boolean string cannot satisfy the provider field: {schema}"
    );
}

/// A parent synthesized only to carry a member-host implication must not
/// import unrelated declared siblings into a per-template schema.
#[test]
fn synthetic_member_parent_does_not_seed_unreferenced_values_siblings() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          port: {{ .Values.master.containerPorts.redis | quote }}
    "};
    let values_yaml = indoc! {"
        master:
          containerPorts:
            redis: 6379
          unrelated:
            imported: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema
            .pointer("/properties/master/properties/unrelated")
            .is_none(),
        "a requirement-only parent must not seed an unconsumed sibling: {schema}"
    );
    assert!(
        schema
            .pointer("/properties/master/properties/containerPorts/properties/redis")
            .is_some(),
        "the genuinely consumed descendant must remain represented: {schema}"
    );
}

/// A member-local predicate cannot be represented as a root Draft 7 guard.
/// Its body contract must therefore abstain instead of becoming an
/// unconditional item/value constraint.
#[test]
fn member_local_guard_does_not_leak_its_string_contract() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          output: |-
            {{- range $item := .Values.items }}
            {{- if $item.enabled }}
            {{ tpl $item.template $ }}
            {{- end }}
            {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("items: []\n"));
    // The member-local predicate cannot lower as a document guard, but its
    // `enabled` lookup still proves the structural member host in every
    // array/map lane. The host stays untyped in the broad default lane so
    // the unconditional range implication below remains the strict owner.
    let open_member = serde_json::json!({
        "additionalProperties": {},
        "properties": { "enabled": {} },
    });
    let object_member = serde_json::json!({
        "additionalProperties": {},
        "properties": { "enabled": {} },
        "type": "object",
    });
    let mut properties = serde_json::Map::new();
    properties.insert(
        "items".to_string(),
        serde_json::json!({
            "anyOf": [
                { "items": open_member, "type": "array" },
                { "items": object_member.clone(), "type": "array" },
                { "type": "integer" },
                { "type": "null" },
                { "additionalProperties": object_member.clone(), "type": "object" },
            ]
        }),
    );
    // The unconditional arm's carrier stays untyped: it must hold vacuously
    // for falsy ancestors a `with` chain would skip. Grafting the untyped
    // `enabled` carrier into the arm keeps the member's OBJECT kind — the
    // typeless carrier conjoins into the typed member slot instead of
    // widening it into a union alternative.
    let all_of = vec![serde_json::json!({
        "additionalProperties": {},
        "properties": {
            "items": {
                "anyOf": [
                    { "items": object_member.clone(), "type": "array" },
                    {
                        "additionalProperties": object_member,
                        "type": "object",
                    },
                    { "maximum": 0, "type": "integer" },
                    { "type": "null" },
                ]
            }
        },
    })];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, false)
    );

    for instance in [
        serde_json::json!({ "items": [{ "enabled": false, "template": 7 }] }),
        serde_json::json!({ "items": [{ "enabled": true, "template": "body" }] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "dead member consumers and live strings remain valid: instance={instance}; schema={schema}"
        );
    }
}

/// Interior carriers of conditional arms must hold
/// vacuously for falsy ancestors that a `with` chain skips at runtime, so
/// only the truthy states carry the leaf's iterable requirement.
#[test]
fn nested_with_chain_range_keeps_falsy_ancestors_valid() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: d
        spec:
          template:
            spec:
              {{- with .Values.affinity }}
              affinity:
              {{- with .podAffinity }}
                podAffinity:
                  {{- with .preferredDuringSchedulingIgnoredDuringExecution }}
                  preferredDuringSchedulingIgnoredDuringExecution:
                  {{- range . }}
                    - weight: {{ .weight }}
                  {{- end }}
                  {{- end }}
              {{- end }}
              {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(
            "affinity: {}
",
        ),
    );

    for instance in [
        serde_json::json!({ "affinity": false }),
        serde_json::json!({ "affinity": 0 }),
        serde_json::json!({ "affinity": "" }),
        serde_json::json!({ "affinity": {} }),
        serde_json::json!({ "affinity": {
            "podAffinity": { "preferredDuringSchedulingIgnoredDuringExecution": [{ "weight": 1 }] }
        } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "falsy ancestors are skipped by the with chain and valid lists render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "affinity": {
                "podAffinity": { "preferredDuringSchedulingIgnoredDuringExecution": "audit" }
            } }),
        ),
        "a live truthy non-iterable still fails the range: {schema}"
    );
}

/// A bare `*` member row must not collapse its container to an array-only
/// shape: `range` iterates maps as well as lists, so a map member ranged
/// inside an outer list item (velero's storage-location `annotations`)
/// keeps both collection lanes and accepts the declared map form.
#[test]
fn nested_member_range_keeps_map_lane_in_member_arm() {
    let src = indoc! {r#"
        {{- if typeIs "[]interface {}" .Values.locations }}
        {{- range .Values.locations }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .name | default "d" }}
          {{- with .annotations }}
          annotations:
              {{- range $key, $value := . }}
            {{- $key | nindent 4 }}: {{ $value | quote }}
            {{- end }}
          {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        locations:
        - name:
          annotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "locations": [{ "name": "d", "annotations": {} }] }),
        serde_json::json!({ "locations": [{ "name": "d", "annotations": { "a": "b" } }] }),
        serde_json::json!({ "locations": "ignored" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "map-form annotations render and non-lists skip the typeIs branch: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "locations": [7] })),
        "a scalar item fails the member reads inside the range: {schema}"
    );
}

/// an `if` header's chained selector (`and .Values.webhook.create
/// .Values.webhook.podDisruptionBudget.enabled`) field-accesses `.enabled`
/// on the intermediate map, so a non-object host aborts rendering even
/// though the region's own body never renders for it. The member-host arm
/// must survive the sibling `hasKey` dispatch inside the body
/// (external-secrets' webhook `PodDisruptionBudget`).
#[test]
fn header_member_read_requires_an_object_host_beside_body_dispatch() {
    let src = indoc! {r#"
        {{- if and .Values.webhook.create .Values.webhook.podDisruptionBudget.enabled }}
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          {{- if hasKey .Values.webhook.podDisruptionBudget "maxUnavailable" }}
          maxUnavailable: {{ .Values.webhook.podDisruptionBudget.maxUnavailable }}
          {{- else if hasKey .Values.webhook.podDisruptionBudget "minAvailable" }}
          minAvailable: {{ .Values.webhook.podDisruptionBudget.minAvailable }}
          {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(
            "webhook:\n  create: true\n  podDisruptionBudget:\n    enabled: false\n    minAvailable: 1\n",
        ),
    );
    // The coalesced document carries the declared `create: true`; with it
    // null-deleted the header short-circuits before the member read.
    for (instance, want) in [
        (
            serde_json::json!({ "webhook": { "create": true, "podDisruptionBudget": 7 } }),
            false,
        ),
        (
            serde_json::json!({ "webhook": { "create": true, "podDisruptionBudget": [1] } }),
            false,
        ),
        (
            serde_json::json!({ "webhook": { "podDisruptionBudget": { "enabled": false } } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "instance={instance}; schema={schema}"
        );
    }
}

/// The nack shape: a declared mapping default whose ONLY consumer is the
/// nil-safe grouped read `((.Values.global).labels)`. Helm's null-deletion
/// renders `global: null` (the receiver goes absent and the grouped chain
/// yields nil instead of aborting), so the declared default's base typing
/// must admit null while present non-null scalars keep aborting through
/// the presence-guarded member-host arm.
#[test]
fn nil_safe_grouped_receiver_with_declared_default_admits_null() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          {{- with ((.Values.global).labels) }}
          labels:
            {{- toYaml . | nindent 4 }}
          {{- end }}
        data: {}
    "};
    let values_yaml = indoc! {"
        global:
          labels: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    sim_assert_eq!(
        have: schema.pointer("/properties/global/type") == Some(&serde_json::json!("object")),
        want: false,
        "declared-default base must not pin bare `type: object`: {schema}",
    );
    for (instance, want) in [
        (serde_json::json!({ "global": null }), true),
        (serde_json::json!({}), true),
        (serde_json::json!({ "global": {} }), true),
        (
            serde_json::json!({ "global": { "labels": { "a": "b" } } }),
            true,
        ),
        (serde_json::json!({ "global": 42 }), false),
        (serde_json::json!({ "global": "oops" }), false),
        (serde_json::json!({ "global": false }), false),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "instance={instance}; want={want}; schema={schema}"
        );
    }
}
