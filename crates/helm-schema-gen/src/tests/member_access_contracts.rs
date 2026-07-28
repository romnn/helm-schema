use indoc::{formatdoc, indoc};
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
            // The strict receiver's presence claim shares its `strict`
            // ancestor with the enabled gate, so the clause anchors here
            // instead of at the document root.
            "allOf": [{
                "if": { "allOf": [
                    {
                        "properties": { "enabled": { "$ref": "#/$defs/helm-truthy" } },
                        "required": ["enabled"],
                        "type": "object",
                    },
                    { "anyOf": [
                        { "not": { "properties": { "receiver": {} },
                            "required": ["receiver"], "type": "object" } },
                        { "properties": { "receiver": { "enum": [null] } },
                            "required": ["receiver"], "type": "object" },
                    ] },
                ] },
                "then": false,
            }],
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
    let all_of = vec![
        serde_json::json!({
            "if": grouped_receiver_present,
            "then": root_property_schema(
                "grouped",
                serde_json::json!({
                    "additionalProperties": {},
                    "properties": {
                        "receiver": { "anyOf": [{ "type": "object" }] },
                    },
                }),
            ),
        }),
        serde_json::json!({
            "if": strict_enabled,
            "then": root_property_schema(
                "strict",
                serde_json::json!({
                    "additionalProperties": {},
                    "properties": {
                        "receiver": { "anyOf": [{ "type": "object" }] },
                    },
                }),
            ),
        }),
        // The chains' own receivers are navigated unconditionally: the inner
        // `.Values.grouped.receiver` read and the `strict.enabled` header
        // both abort on a deleted host.
        navigated_host_clause(&["grouped"]),
        navigated_host_clause(&["strict"]),
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

/// A member-local predicate cannot be represented as a root Draft 7 guard,
/// so it must never become an unconditional item/value constraint. It is
/// still exact INSIDE the member: the item slot states the gate beside the
/// body's own contract, so `template` binds for the members the chart's
/// `if $item.enabled` routes to `tpl` and stays open for the rest.
#[test]
fn member_local_guard_does_not_leak_its_string_contract() {
    let schema = member_local_guard_schema();
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
    // Every range implication reaches its members through the same four
    // lanes: array items, map values, the zero-iteration integer bound, and
    // null.
    let member_lanes = |member: &Value| {
        serde_json::json!({
            "additionalProperties": {},
            "properties": {
                "items": {
                    "anyOf": [
                        { "items": member, "type": "array" },
                        { "additionalProperties": member, "type": "object" },
                        { "maximum": 0, "type": "integer" },
                        { "type": "null" },
                    ]
                }
            },
        })
    };
    // The unconditional arm's carrier stays untyped: it must hold vacuously
    // for falsy ancestors a `with` chain would skip. Grafting the untyped
    // `enabled` carrier into the arm keeps the member's OBJECT kind — the
    // typeless carrier conjoins into the typed member slot instead of
    // widening it into a union alternative.
    //
    // Two gated arms beside it, one per fact the gate scopes: `tpl`'s operand
    // must be a string, and — because `tpl` aborts on nil — it must be there
    // at all. A non-object member cannot satisfy the selector, so it passes
    // both arms unconstrained.
    let gate = serde_json::json!({
        "properties": { "enabled": { "$ref": "#/$defs/helm-truthy" } },
        "required": ["enabled"],
        "type": "object",
    });
    let gated_type = serde_json::json!({
        "if": gate,
        "then": {
            "properties": { "template": { "type": "string" } },
            "type": "object",
        },
    });
    let gated_presence = serde_json::json!({
        "if": gate,
        "then": { "required": ["template"], "type": "object" },
    });
    let all_of = vec![
        member_lanes(&object_member),
        member_lanes(&gated_type),
        member_lanes(&gated_presence),
    ];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, true)
    );
}

/// The schema of the member-local guard shape above, shared with the
/// acceptance cases below.
fn member_local_guard_schema() -> Value {
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
    schema_for_values_yaml(parse_ir(src), Some("items: []\n"))
}

/// The gate's own semantics: `tpl` binds the members `if $item.enabled`
/// routes to it and nothing else.
#[test]
fn member_local_guard_binds_only_the_members_it_selects() {
    let schema = member_local_guard_schema();
    for (instance, want, label) in [
        (
            serde_json::json!({ "items": [{ "enabled": false, "template": 7 }] }),
            true,
            "a dead member consumer stays open",
        ),
        (
            serde_json::json!({ "items": [{ "enabled": true, "template": "body" }] }),
            true,
            "a live string renders",
        ),
        (
            serde_json::json!({ "items": [{ "enabled": true, "template": 7 }] }),
            false,
            "a live non-string operand aborts",
        ),
        (
            serde_json::json!({ "items": [{ "enabled": true }] }),
            false,
            "a live member omitting the operand aborts",
        ),
        (
            serde_json::json!({ "items": [{ "enabled": false }, { "enabled": true }] }),
            false,
            "one live member among dead ones still aborts",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "member-local guard scope ({label}): instance={instance}; want={want}; schema={schema}"
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
        Some(indoc! {"
            affinity: {}
        "}),
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
        Some(indoc! {"
            webhook:
              create: true
              podDisruptionBudget:
                enabled: false
                minAvailable: 1
        "}),
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

/// Navigation ABORTS on a nil receiver, so every host read outside its own
/// presence gate must exist in the coalesced document — the state a user's
/// `null` deletion produces (metrics-server's `apiService: null` aborts with
/// "nil pointer evaluating interface {}.create"). The claim reaches
/// TOP-LEVEL hosts, which have no parent slot for a `required` member, and
/// it survives the chart's own mapping default, which the render-grade
/// presence relaxation would otherwise drop. Nil-safe grouped receivers and
/// `with`-scoped hosts keep every absent state open.
#[test]
fn navigated_hosts_must_exist_in_the_coalesced_document() {
    let src = indoc! {r"
        {{- if .Values.apiService.create }}
        apiVersion: v1
        kind: Service
        metadata:
          name: x
        {{- end }}
        ---
        {{- if .Values.rbac.serviceAccount.create }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: y
        {{- end }}
        ---
        {{- if (.Values.nilSafe).enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: n
        {{- end }}
        ---
        {{- with .Values.gated }}
        {{- if .enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: g
        {{- end }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        apiService:
          create: true
        rbac:
          serviceAccount:
            create: true
        nilSafe: {}
        gated: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let composed = serde_json::json!({
        "apiService": { "create": true },
        "rbac": { "serviceAccount": { "create": true } },
        "nilSafe": {},
        "gated": {},
    });
    let without = |path: &[&str]| {
        let mut instance = composed.clone();
        let mut node = &mut instance;
        let Some((leaf, parents)) = path.split_last() else {
            return instance;
        };
        for segment in parents {
            node = &mut node[*segment];
        }
        if let Some(object) = node.as_object_mut() {
            object.remove(*leaf);
        }
        instance
    };
    for (instance, want, label) in [
        (composed.clone(), true, "the coalesced defaults render"),
        (
            without(&["apiService"]),
            false,
            "a deleted top-level host aborts the header read",
        ),
        (
            without(&["rbac"]),
            false,
            "a deleted host ancestor aborts the chained read",
        ),
        (
            without(&["rbac", "serviceAccount"]),
            false,
            "a deleted nested host aborts, default-supplied or not",
        ),
        (
            without(&["nilSafe"]),
            true,
            "a nil-safe grouped receiver renders when deleted",
        ),
        (
            without(&["gated"]),
            true,
            "a `with`-scoped host renders when deleted",
        ),
        (
            serde_json::json!({ "apiService": {}, "rbac": { "serviceAccount": { "create": true } },
                "nilSafe": {}, "gated": {} }),
            true,
            "an empty host map reads its member as nil",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "navigated host presence ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Ten independently gated reads of one host, more than the arm cap admits.
fn gated_host_reads() -> String {
    (1..=10)
        .map(|index| {
            format!(
                "  {{{{- if .Values.g{index} }}}}\n  \
                 g{index}: {{{{ .Values.host.leaf | quote }}}}\n  {{{{- end }}}}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One host reached from many nested branches states ONE abort condition,
/// not one per branch. The guard sets are read as a disjunction, so an arm
/// refining another (`ce ∧ g1` beside `ce`) holds nowhere the weaker arm
/// does not and drops out exactly. Without that reduction the redundancy
/// alone crossed the fanout cap and the whole claim disappeared — which is
/// how cilium's `cni`, `eni`, `image` and eleven more hosts stayed optional
/// while `helm template` aborted on every one of them.
#[test]
fn redundant_branch_refinements_keep_one_host_claim() {
    let src = formatdoc! {r"
        {{{{- if .Values.componentEnabled }}}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          base: {{{{ .Values.host.leaf | quote }}}}
        {gated}
        {{{{- end }}}}
    ",
        gated = gated_host_reads()
    };
    let values_yaml = indoc! {"
        componentEnabled: true
        host:
          leaf: v
        g1: false
    "};
    let schema = schema_for_values_yaml(parse_ir(&src), Some(values_yaml));
    for (instance, want, label) in [
        (
            serde_json::json!({ "componentEnabled": true, "host": { "leaf": "v" } }),
            true,
            "the declared document renders",
        ),
        (
            serde_json::json!({ "componentEnabled": true }),
            false,
            "the live component aborts on the deleted host",
        ),
        (
            serde_json::json!({ "componentEnabled": false }),
            true,
            "the dormant component never reads the host",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "subsumed host arms ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Past the arm cap the claim NARROWS rather than vanishing: every arm is on
/// its own a state where the access runs, so keeping a bounded subset states
/// a real (smaller) abort region while dropping them all states nothing.
#[test]
fn host_claims_past_the_arm_cap_keep_a_bounded_subset() {
    let src = formatdoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {gated}
    ",
        gated = gated_host_reads()
    };
    let values_yaml = indoc! {"
        host:
          leaf: v
        g1: false
    "};
    let schema = schema_for_values_yaml(parse_ir(&src), Some(values_yaml));
    let all_gates = |value: bool| {
        (1..=10)
            .map(|index| (format!("g{index}"), serde_json::json!(value)))
            .collect::<serde_json::Map<_, _>>()
    };
    let mut live = all_gates(true);
    let mut dormant = all_gates(false);
    let mut declared = all_gates(true);
    declared.insert("host".to_string(), serde_json::json!({ "leaf": "v" }));
    for (instance, want, label) in [
        (
            serde_json::Value::Object(declared),
            true,
            "the declared document renders",
        ),
        (
            serde_json::Value::Object(std::mem::take(&mut live)),
            false,
            "a live gate aborts on the deleted host",
        ),
        (
            serde_json::Value::Object(std::mem::take(&mut dormant)),
            true,
            "no gate reaches the host",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "capped host arms ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Go stores a pipeline result through `reflect.ValueOf(value.Interface())`,
/// which turns a nil interface into an INVALID reflect value, and field
/// access on an invalid receiver yields zero instead of aborting. So a
/// `:=`-bound local is nil-SAFE for its own hop — argo-cd renders with
/// `global.affinity` deleted although `$preset := .Values.global.affinity`
/// is followed by `$preset.podAntiAffinity` — while a present non-object
/// still aborts ("can't evaluate field") and the hop BELOW the variable
/// still aborts on its own missing key. A RANGE member variable comes
/// straight from `MapIndex` and never gets that unwrap, so it keeps the
/// direct chain's abort; `ranged_member_access_rejects_falsy_members_only_when_live`
/// pins that side.
#[test]
fn assigned_locals_are_nil_safe_for_their_own_hop() {
    let src = indoc! {r"
        {{- $preset := .Values.host }}
        {{- if eq $preset.leaf.deep 'x' }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "};
    let values_yaml = indoc! {"
        host:
          leaf:
            deep: x
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want, label) in [
        (
            serde_json::json!({ "host": { "leaf": { "deep": "x" } } }),
            true,
            "the declared document renders",
        ),
        (
            serde_json::json!({}),
            true,
            "a deleted variable source navigates to nil without aborting",
        ),
        (
            serde_json::json!({ "host": 7 }),
            false,
            "a present non-object variable source cannot host the field",
        ),
        (
            serde_json::json!({ "host": {} }),
            false,
            "the hop below the variable aborts on its own missing key",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "assigned-local nil safety ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Ranging a local dict that a `set` overlaid on a values-backed map still
/// visits that map's members, so the members' own consumers bind to them:
/// navigating one aborts on a present non-mapping exactly as a direct range
/// would (traefik's `$services := .Values.service.additionalServices` plus a
/// synthetic "default" entry).
#[test]
fn overlaid_range_members_keep_their_member_contracts() {
    let src = indoc! {r#"
        {{- $services := .Values.additionalServices }}
        {{- $services = set $services "default" (omit .Values.service "additionalServices") }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $name, $service := $services }}
          {{- if ne $service.enabled false }}
          {{ $name }}: live
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalServices: {}
        service:
          enabled: true
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (overrides, want) in [
        (
            serde_json::json!({ "additionalServices": { "audit": { "enabled": true } } }),
            true,
        ),
        (serde_json::json!({ "additionalServices": {} }), true),
        // A present non-mapping member aborts the member navigation.
        (
            serde_json::json!({ "additionalServices": { "audit": false } }),
            false,
        ),
        (
            serde_json::json!({ "additionalServices": { "audit": "x" } }),
            false,
        ),
        (
            serde_json::json!({ "additionalServices": { "audit": 7 } }),
            false,
        ),
        (
            serde_json::json!({ "additionalServices": { "audit": [] } }),
            false,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the overlaid range binds its member contracts: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The parent's own values.yaml declares only the `parentOnly` hosts, so
/// the subchart declarations are what a missing key reads instead of nil,
/// and only they come back when a root is deleted whole.
fn dependency_owned_values_yaml() -> &'static str {
    indoc! {"
        sub:
          parentOnly:
            create: true
          subDeclared:
            enabled: true
        other:
          subDeclared:
            enabled: true
        gated:
          enabled: true
          parentOnly:
            create: true
        refilled:
          subDeclared:
            enabled: true
          parentOnly:
            create: true
    "}
}

fn dependency_owned_host_schema() -> Value {
    let src = indoc! {r"
        {{- if .Values.sub.parentOnly.create }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: p
        {{- end }}
        ---
        {{- if .Values.sub.subDeclared.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: s
        {{- end }}
        ---
        {{- if .Values.other.subDeclared.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: o
        {{- end }}
        ---
        {{- if .Values.gated.enabled }}
        {{- if .Values.gated.parentOnly.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: g
        {{- end }}
        {{- end }}
        ---
        {{- if .Values.refilled.subDeclared.enabled }}
        {{- if .Values.refilled.parentOnly.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: r
        {{- end }}
        {{- end }}
    "};
    let subchart_yaml = indoc! {"
        sub:
          subDeclared:
            enabled: true
        other:
          subDeclared:
            enabled: true
        gated:
          subDeclared:
            enabled: true
        refilled:
          subDeclared:
            enabled: true
    "};
    let mut contract = parse_ir(src);
    contract.push_pathless_dependency_fragment("sub");
    contract.push_pathless_dependency_fragment("other");
    contract.push_pathless_dependency_fragment("gated");
    contract.push_pathless_dependency_fragment("refilled");
    schema_for_dependency_values_yaml(
        contract,
        dependency_owned_values_yaml(),
        subchart_yaml,
        subchart_yaml,
    )
}

/// A navigated host under a DEPENDENCY values root binds while that root
/// survives: a deletion INSIDE a present root sticks through every later
/// merge stage and reaches the consumer as nil, whoever declared the key
/// (measured against helm v4.2.3 on a parent/subchart pair). A key the
/// subchart itself declares still fills at its own coalesce stage when the
/// parent-level document simply omits it.
#[test]
fn dependency_owned_hosts_bind_while_their_root_survives() {
    let schema = dependency_owned_host_schema();
    for (overrides, want, label) in [
        (serde_json::json!({}), true, "the coalesced defaults render"),
        (
            serde_json::json!({ "sub": { "parentOnly": null } }),
            false,
            "a deletion inside a present root sticks and aborts",
        ),
        (
            serde_json::json!({ "sub": { "subDeclared": null } }),
            true,
            "an omitted subchart-declared host reads its own default",
        ),
        (
            serde_json::json!({ "gated": { "parentOnly": null } }),
            false,
            "a gated read binds its host inside the live root",
        ),
        (
            serde_json::json!({ "gated": { "enabled": false, "parentOnly": null } }),
            true,
            "the dormant gate keeps the deletion open",
        ),
        (
            serde_json::json!({ "refilled": { "parentOnly": null } }),
            false,
            "a subchart-gated read binds its host inside the live root",
        ),
        (
            serde_json::json!({
                "refilled": { "subDeclared": { "enabled": false }, "parentOnly": null },
            }),
            true,
            "its dormant gate keeps the deletion open",
        ),
        (
            serde_json::json!({ "refilled": {} }),
            true,
            "an empty table coalesces the parent's own defaults back in",
        ),
    ] {
        let instance = composed_instance(dependency_owned_values_yaml(), overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "dependency-owned host presence ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }

    let surviving_null = serde_json::json!({
        "sub": { "parentOnly": { "create": true }, "subDeclared": null },
        "other": { "subDeclared": { "enabled": true } },
    });
    assert!(
        !schema_accepts_instance(&schema, &surviving_null),
        "a surviving null reads as nil whoever declared the key: schema={schema}"
    );
}

/// Deleting the ROOT hands the whole subtree back to the subchart's own
/// defaults (`coalesceDeps` recreates the table and coalesces them in),
/// taking the parent's keys for that root with it — so a parent-only read
/// aborts there while a subchart-declared one renders. A gate the refill
/// keeps alive carries its host's read along: the clause holds against the
/// refill whatever else the document says, which is a document-level fact
/// even where the clause's own anchor sits inside the root. A gate the
/// parent alone declares goes with the deletion and silences the read.
#[test]
fn deleted_dependency_roots_refill_from_their_subchart() {
    let schema = dependency_owned_host_schema();
    for (overrides, want, label) in [
        (
            serde_json::json!({ "sub": null }),
            false,
            "a deleted root drops the parent's own keys for it",
        ),
        (
            serde_json::json!({ "other": null }),
            true,
            "a deleted root refills every key the subchart declares",
        ),
        (
            serde_json::json!({ "gated": null }),
            true,
            "the parent-declared gate goes with the deletion",
        ),
        (
            serde_json::json!({ "refilled": null }),
            false,
            "a refilled gate keeps the parent-only read live",
        ),
    ] {
        let instance = composed_instance(dependency_owned_values_yaml(), overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "deleted dependency root ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A root-scoped `{{- with .Values }}` rebinds the dot to the values
/// DOCUMENT, so a `.member.field` read inside navigates exactly as
/// `.Values.member.field` does and aborts the same way on a deleted
/// member. nats wraps the whole body of `nats.defaultValues` in one, and
/// while the root did not count as a navigation base its five hosts stayed
/// optional against helm's "nil pointer evaluating interface {}.name".
///
/// The gate is the document's OWN truthiness rather than `true`: a mapping
/// is Helm-falsy only when empty, so a document whose every key was
/// null-deleted never enters the `with` and renders.
#[test]
fn root_scoped_with_navigates_the_document_it_rebinds() {
    let helpers = indoc! {r#"
        {{- define "chart.defaults" }}
        {{- with .Values }}
        {{- $name := .service.name | default "svc" }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- include "chart.defaults" . }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.other | quote }}
    "#};
    let values_yaml = indoc! {"
        service:
          port: 80
        other: cm
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (
            serde_json::json!({ "service": { "port": 80 }, "other": "cm" }),
            true,
            "the coalesced defaults render",
        ),
        (
            serde_json::json!({ "other": "cm" }),
            false,
            "a host deleted under the rebound dot aborts the navigation",
        ),
        (
            serde_json::json!({}),
            true,
            "an empty document is falsy, so the `with` body never runs",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "root-scoped with ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A helper called with a CALL DICT navigates its members' identities: in
/// `include "x" (dict "config" .Values.pdb)` the body's `.config.enabled`
/// reads `.Values.pdb.enabled`, and a deleted `pdb` binds the member to nil,
/// which aborts with "nil pointer evaluating interface {}.enabled" exactly
/// like the direct spelling would.
///
/// The dict itself still carries no identity — reading `.ctx` says nothing
/// about the root context — so only a member whose bound VALUE is a raw
/// values path contributes a host claim, and only where the body navigates
/// PAST that member.
#[test]
fn call_dict_members_navigate_the_paths_they_bind() {
    let helpers = indoc! {r#"
        {{- define "chart.pdb" -}}
        {{- if .config.enabled }}
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        metadata:
          name: pdb
        spec:
          minAvailable: {{ .config.minAvailable }}
        {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- include "chart.pdb" (dict "ctx" $ "config" .Values.pdb) }}
    "#};
    let values_yaml = indoc! {"
        pdb:
          enabled: false
          minAvailable: 1
        other: keep
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (
            serde_json::json!({ "pdb": { "enabled": false, "minAvailable": 1 }, "other": "keep" }),
            true,
            "the coalesced defaults render",
        ),
        (
            serde_json::json!({ "other": "keep" }),
            false,
            "the bound member reads nil and the navigation aborts",
        ),
        (
            serde_json::json!({ "pdb": "scalar", "other": "keep" }),
            false,
            "a scalar cannot host the member the body reads",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "call-dict member navigation ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A `semverCompare` conjunct over the Capabilities-defaulted Kubernetes
/// version decodes exactly, so the member access BEHIND it keeps its host
/// claim. kube-prometheus-stack gates every rule file on
/// `and (semverCompare ">=1.14.0-0" $v) (semverCompare "<9.9.9-9" $v)
/// .Values.defaultRules.create`, and Go's `and` short-circuits, so the
/// host is dereferenced exactly when both comparisons hold: with the
/// override unset that is the policy version's own verdict, and a
/// too-old override makes the deletion safe.
#[test]
fn semver_gated_member_access_keeps_its_host_claim() {
    let src = indoc! {r#"
        {{- $kubeTargetVersion := default .Capabilities.KubeVersion.GitVersion .Values.kubeTargetVersionOverride }}
        {{- if and (semverCompare ">=1.14.0-0" $kubeTargetVersion) (semverCompare "<9.9.9-9" $kubeTargetVersion) .Values.defaultRules.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.other | quote }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        kubeTargetVersionOverride: ""
        defaultRules:
          create: true
        other: cm
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_kubernetes_version(src, "1.29.0"),
        Some(values_yaml),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({
                "kubeTargetVersionOverride": "",
                "defaultRules": { "create": true },
                "other": "cm",
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            serde_json::json!({ "kubeTargetVersionOverride": "", "other": "cm" }),
            false,
            "the policy version satisfies both comparisons, so the host is read",
        ),
        (
            serde_json::json!({ "kubeTargetVersionOverride": "1.10.0", "other": "cm" }),
            true,
            "a too-old override short-circuits the and before the host",
        ),
        (
            serde_json::json!({ "kubeTargetVersionOverride": "1.30.0", "other": "cm" }),
            false,
            "an override that satisfies both comparisons reads the host again",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "semver-gated member access ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}
