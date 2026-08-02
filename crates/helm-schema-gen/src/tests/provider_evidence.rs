use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete schema equality fixture pins every projected and omitted helper member together"
)]
fn helper_range_keeps_omitted_members_out_of_provider_projection() {
    #[derive(Debug)]
    struct ConfigMapDataProvider;

    impl ResourceSchemaOracle for ConfigMapDataProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            match use_.path.0.as_slice() {
                [data] if data == "data" => Some(ProviderSchemaFragment::new(serde_json::json!({
                    "additionalProperties": { "type": "string" },
                    "type": "object",
                }))),
                [data, member] if data == "data" && member == "{*}" => {
                    Some(ProviderSchemaFragment::new(serde_json::json!({
                        "type": "string",
                    })))
                }
                _ => None,
            }
        }
    }

    let helpers = indoc! {r#"
        {{- define "test.params" -}}
        {{- $config := omit .Values.params "create" -}}
        {{- range $key, $value := $config }}
        {{ $key }}: {{ toString $value | toYaml }}
        {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.params.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: params
        data:
          {{- include "test.params" . | nindent 2 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        params:
          create: true
          value: example
    "};
    let ir = parse_ir_with_helpers(src, helpers);
    let signals = schema_signals_for(ir);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &ConfigMapDataProvider)
            .with_values_yaml(Some(values_yaml)),
    );

    let expected = serde_json::json!({
        "$defs": {
            "t": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" },
                ],
            },
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "properties": {
                        "params": {
                            "properties": {
                                "create": { "$ref": "#/$defs/t" },
                            },
                            "required": ["create"],
                            "type": "object",
                        },
                    },
                    "required": ["params"],
                    "type": "object",
                },
                "then": {
                    "allOf": [
                        {
                            "additionalProperties": {},
                            "properties": {
                                "params": {
                                    "anyOf": [
                                        { "type": "array" },
                                        { "type": "object" },
                                        { "type": "null" },
                                    ],
                                },
                            },
                        },
                        {
                            "additionalProperties": {},
                            "properties": {
                                "params": { "type": "object" },
                            },
                        },
                    ],
                },
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "params": {
                                    "properties": {
                                        "create": { "$ref": "#/$defs/t" },
                                    },
                                    "required": ["create"],
                                    "type": "object",
                                },
                            },
                            "required": ["params"],
                            "type": "object",
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": { "params": {} },
                                        "required": ["params"],
                                        "type": "object",
                                    },
                                },
                                {
                                    "properties": {
                                        "params": { "enum": [null] },
                                    },
                                    "required": ["params"],
                                    "type": "object",
                                },
                            ],
                        },
                    ],
                },
                "then": false,
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "not": {
                                "properties": { "params": {} },
                                "required": ["params"],
                                "type": "object",
                            },
                        },
                        {
                            "properties": {
                                "params": { "enum": [null] },
                            },
                            "required": ["params"],
                            "type": "object",
                        },
                    ],
                },
                "then": false,
            },
        ],
        "properties": {
            "params": {
                "additionalProperties": {},
                "allOf": [{
                    "if": {
                        "properties": {
                            "create": { "$ref": "#/$defs/t" },
                        },
                        "required": ["create"],
                        "type": "object",
                    },
                    "then": {
                        "anyOf": [
                            { "not": { "$ref": "#/$defs/t" } },
                            { "type": "array" },
                            { "type": "null" },
                            { "type": "object" },
                        ],
                    },
                }],
                "properties": {
                    "create": {},
                },
                "type": "object",
            },
        },
        "type": "object",
    });

    sim_assert_eq!(have: schema, want: expected);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete schema equality fixture keeps the omitted-member host and leaf contracts correlated"
)]
fn always_omitted_provider_member_yields_declared_typing_to_its_leaf() {
    #[derive(Debug)]
    struct SecurityContextProvider;

    impl ResourceSchemaOracle for SecurityContextProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            (use_.path.0 == ["spec", "containers[*]", "securityContext"]).then(|| {
                ProviderSchemaFragment::new(serde_json::json!({
                    "additionalProperties": false,
                    "properties": {
                        "runAsNonRoot": { "type": "boolean" },
                    },
                    "type": "object",
                }))
            })
        }
    }

    let helpers = indoc! {r#"
        {{- define "test.securityContext" -}}
        {{- omit .Values.securityContext "enabled" | toYaml -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.securityContext.enabled }}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: test
              securityContext: {{ include "test.securityContext" . | nindent 8 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        securityContext:
          enabled: true
          runAsNonRoot: true
    "};
    let ir = parse_ir_with_helpers(src, helpers);
    let signals = schema_signals_for(ir);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &SecurityContextProvider)
            .with_values_yaml(Some(values_yaml)),
    );

    let expected = serde_json::json!({
        "$defs": {
            "t": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" },
                ],
            },
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "properties": {
                        "securityContext": {
                            "properties": {
                                "enabled": { "$ref": "#/$defs/t" },
                            },
                            "required": ["enabled"],
                            "type": "object",
                        },
                    },
                    "required": ["securityContext"],
                    "type": "object",
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "securityContext": { "type": "object" },
                    },
                },
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "securityContext": {
                                    "properties": {
                                        "enabled": { "$ref": "#/$defs/t" },
                                    },
                                    "required": ["enabled"],
                                    "type": "object",
                                },
                            },
                            "required": ["securityContext"],
                            "type": "object",
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": { "securityContext": {} },
                                        "required": ["securityContext"],
                                        "type": "object",
                                    },
                                },
                                {
                                    "properties": {
                                        "securityContext": { "enum": [null] },
                                    },
                                    "required": ["securityContext"],
                                    "type": "object",
                                },
                            ],
                        },
                    ],
                },
                "then": false,
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "not": {
                                "properties": { "securityContext": {} },
                                "required": ["securityContext"],
                                "type": "object",
                            },
                        },
                        {
                            "properties": {
                                "securityContext": { "enum": [null] },
                            },
                            "required": ["securityContext"],
                            "type": "object",
                        },
                    ],
                },
                "then": false,
            },
        ],
        "properties": {
            "securityContext": {
                "additionalProperties": {},
                "allOf": [{
                    "if": {
                        "properties": {
                            "enabled": { "$ref": "#/$defs/t" },
                        },
                        "required": ["enabled"],
                        "type": "object",
                    },
                    "then": {
                        "additionalProperties": false,
                        "properties": {
                            "enabled": {},
                            "runAsNonRoot": { "type": "boolean" },
                        },
                        "type": "object",
                    },
                }],
                "properties": {
                    "enabled": {},
                },
                "type": "object",
            },
        },
        "type": "object",
    });

    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn quoted_empty_membership_scopes_raw_provider_preimages() {
    let raw = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          {{- if not (has (quote .Values.limit) (list "" (quote ""))) }}
          revisionHistoryLimit: {{ .Values.limit }}
          {{- end }}
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              containers:
                - name: test
                  image: busybox
    "#};
    let schema = schema_for_values_yaml(parse_ir(raw), Some("limit: ''\n"));

    for (instance, want, label) in [
        (
            serde_json::json!({ "limit": { "bad": true } }),
            false,
            "map",
        ),
        (serde_json::json!({ "limit": false }), false, "false"),
        (serde_json::json!({ "limit": 7 }), true, "integer"),
        (serde_json::json!({ "limit": "7" }), true, "numeric string"),
        (serde_json::json!({ "limit": "" }), true, "empty string"),
        (serde_json::json!({ "limit": null }), true, "null"),
        (serde_json::json!({}), true, "absent"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "raw membership {label}: instance={instance}; schema={schema}"
        );
    }

    let converted = raw.replace(
        "revisionHistoryLimit: {{ .Values.limit }}",
        "revisionHistoryLimit: {{ .Values.limit | int64 }}",
    );
    let schema = schema_for_values_yaml(parse_ir(&converted), Some("limit: ''\n"));
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "limit": { "bad": true } })),
        "the int64 conversion makes a live map provider-safe without typing the raw input: {schema}"
    );
}

/// A collection spliced raw into a plain scalar slot follows the YAML node
/// kind produced after Go formatting. A safe mapping becomes a string such as
/// `map[key:value]`, while a sequence spelling opens a flow sequence and
/// remains outside the provider string domain.
#[test]
fn plain_scalar_slots_accept_go_formatted_mappings() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: busybox
              volumeMounts:
                - name: data
                  mountPath: {{ .Values.home }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("home: /var/lib\n"));

    for (instance, want, label) in [
        (serde_json::json!({ "home": "/var/data" }), true, "a path"),
        (
            serde_json::json!({ "home": { "a": "b" } }),
            true,
            "a mapping",
        ),
        (
            serde_json::json!({ "home": ["/var/data"] }),
            false,
            "a sequence",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "formatted collection {label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn plain_string_provider_preimage_rejects_yaml_unsafe_spellings() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: busybox
              env:
                - name: AUDIT
                  value: {{ .Values.value }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("value: safe\n"));

    for (value, want, label) in [
        (serde_json::json!("safe"), true, "ordinary string"),
        (
            serde_json::json!("repo:tag"),
            true,
            "colon without separation",
        ),
        (serde_json::json!("repo: bad"), false, "mapping separator"),
        (
            serde_json::json!("%bad"),
            false,
            "forbidden leading indicator",
        ),
        (serde_json::json!("false"), false, "implicit Boolean"),
        (serde_json::json!("yes"), false, "YAML 1.1 Boolean alias"),
        (serde_json::json!("7"), false, "implicit number"),
        (
            serde_json::json!("1_000"),
            false,
            "underscore-separated number",
        ),
        (serde_json::json!("1."), false, "trailing-dot float"),
        (
            serde_json::json!("+.nan"),
            true,
            "signed NaN stays a string",
        ),
        (
            serde_json::json!("1e999"),
            true,
            "float overflow stays a string",
        ),
        (serde_json::json!("line\nbreak"), false, "line break"),
    ] {
        let instance = serde_json::json!({ "value": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "plain YAML {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A Boolean provider slot accepts every spelling the YAML 1.1 resolver
/// reads back as a Boolean — crossplane renders `hostNetwork: yes` into a
/// valid manifest, so rejecting the alias set falsely narrows the input.
#[test]
fn boolean_slot_accepts_every_resolver_boolean_spelling() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          hostNetwork: {{ .Values.hostNetwork }}
          containers:
            - name: test
              image: busybox
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("hostNetwork: false\n"));

    for (value, want, label) in [
        (serde_json::json!(true), true, "native Boolean"),
        (serde_json::json!("yes"), true, "yes alias"),
        (serde_json::json!("off"), true, "off alias"),
        (serde_json::json!("Y"), true, "single-letter alias"),
        (serde_json::json!("TRUE"), true, "uppercase spelling"),
        (serde_json::json!("yeah"), false, "non-token string"),
    ] {
        let instance = serde_json::json!({ "hostNetwork": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "boolean spelling {label}: instance={instance}; schema={schema}"
        );
    }
}

/// An integer provider slot accepts every spelling the YAML 1.1 resolver
/// reads back as an in-range integer: signs, underscore separators, and
/// radix prefixes all reparse to the integer the slot needs (metrics-server
/// renders `port: +443` into a valid Service).
#[test]
fn integer_slot_accepts_every_resolver_integer_spelling() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            - port: {{ .Values.port }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("port: 443\n"));

    for (value, want, label) in [
        (serde_json::json!(443), true, "native integer"),
        (serde_json::json!("443"), true, "decimal string"),
        (serde_json::json!("+443"), true, "signed decimal"),
        (serde_json::json!("1_000"), true, "underscore separator"),
        (serde_json::json!("0x1F"), true, "hex literal"),
        (
            serde_json::json!("_443"),
            false,
            "leading underscore stays a string",
        ),
        (serde_json::json!("4.5"), false, "float spelling"),
        (serde_json::json!("not-a-port"), false, "non-numeric string"),
    ] {
        let instance = serde_json::json!({ "port": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "integer spelling {label}: instance={instance}; schema={schema}"
        );
    }
}

/// `genSignedCert` passes every ip-list entry through `net.ParseIP` and
/// aborts rendering on nil, so items must additionally spell an IP address —
/// not merely a string (cilium's Hubble certificate SANs).
#[test]
fn signed_cert_ip_list_items_require_the_ip_lexical_domain() {
    let src = indoc! {r#"
        {{- $cert := genSelfSignedCert "audit.example" .Values.ips (list "audit.example") 365 }}
        apiVersion: v1
        kind: Secret
        metadata:
          name: test
        data:
          tls.crt: {{ $cert.Cert | b64enc }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("ips: []\n"));

    for (value, want, label) in [
        (serde_json::json!(["10.0.0.7"]), true, "IPv4"),
        (serde_json::json!(["::1"]), true, "IPv6 loopback"),
        (
            serde_json::json!(["2001:db8::8a2e:370:7334"]),
            true,
            "IPv6 full form",
        ),
        (
            serde_json::json!(["::ffff:10.0.0.7"]),
            true,
            "IPv4-mapped IPv6",
        ),
        (
            serde_json::json!(["not-an-ip"]),
            false,
            "non-address string",
        ),
        (
            serde_json::json!(["999.999.999.999"]),
            false,
            "out-of-range octets",
        ),
        (
            serde_json::json!(["10.0.0.07"]),
            false,
            "leading-zero octet",
        ),
        (serde_json::json!([7]), false, "non-string item"),
    ] {
        let instance = serde_json::json!({ "ips": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "ip list item {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A `typeOf`-dispatched numeric lane still renders into the provider slot,
/// so the arm's typing must keep the provider's constraint: policy/v1
/// `minAvailable` is int-or-string, and a fractional float in the selected
/// numeric lane renders a manifest the API server rejects (sealed-secrets'
/// PDB dispatch).
#[test]
fn typeof_dispatched_numeric_lane_keeps_the_provider_intersection() {
    let src = indoc! {r#"
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          {{- if regexMatch "64$" (typeOf .Values.pdb.minAvailable) }}
          minAvailable: {{ .Values.pdb.minAvailable }}
          {{- end }}
          selector:
            matchLabels:
              app: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            pdb:
              minAvailable: 1
        "}),
    );

    for (value, want, label) in [
        (serde_json::json!(1), true, "integer"),
        (
            serde_json::json!(2.0),
            true,
            "integral float renders as integer",
        ),
        (
            serde_json::json!("50%"),
            true,
            "string skips the numeric arm",
        ),
        (serde_json::json!(1.5), false, "fractional float"),
    ] {
        let instance = serde_json::json!({ "pdb": { "minAvailable": value } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "dispatched numeric lane {label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn inline_conditional_kind_candidates_reach_the_matching_provider_path() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: {{ if .Values.stateful }}StatefulSet{{ else }}Deployment{{ end }}
        metadata:
          name: test
        spec:
          {{- if .Values.stateful }}
          serviceName: test
          {{- else }}
          strategy: {{ toYaml .Values.strategy | nindent 4 }}
          {{- end }}
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              containers:
              - name: test
                image: busybox
    "};
    let values_yaml = indoc! {"
        stateful: false
        strategy: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "stateful": false, "strategy": 7 })
        ),
        "Deployment strategy is object-typed: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "stateful": true, "strategy": 7 })
        ),
        "the strategy value is dormant in the StatefulSet branch: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "stateful": false,
                "strategy": { "type": "Recreate" }
            })
        ),
        "a valid Deployment strategy remains accepted: {schema}"
    );
}

#[test]
fn values_selected_kind_partitions_provider_contracts() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: {{ .Values.workloadKind }}
        metadata:
          name: test
        spec:
          {{- if not (eq .Values.workloadKind "DaemonSet") }}
          replicas: 1
          {{- end }}
          {{- if eq .Values.workloadKind "StatefulSet" }}
          serviceName: test
          {{- end }}
          {{- if eq .Values.workloadKind "Deployment" }}
          strategy: {{ toYaml .Values.updateStrategy | nindent 4 }}
          {{- else }}
          updateStrategy: {{ toYaml .Values.updateStrategy | nindent 4 }}
          {{- end }}
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              containers:
              - name: test
                image: busybox
    "#};
    let values_yaml = indoc! {"
        workloadKind: Deployment
        updateStrategy: {}
    "};
    let signals = parse_ir(src).finalize().into_schema_signals();
    let schema = schema_for_values_yaml(signals, Some(values_yaml));
    let stateful_only = serde_json::json!({
        "rollingUpdate": { "partition": "not-an-integer" }
    });
    let deployment_only = serde_json::json!({
        "rollingUpdate": { "maxSurge": false }
    });

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "workloadKind": "Deployment",
                "updateStrategy": deployment_only.clone()
            })
        ),
        "DeploymentStrategy types rollingUpdate.maxSurge as a string or integer: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "workloadKind": "StatefulSet",
                "updateStrategy": deployment_only
            })
        ),
        "StatefulSetStrategy leaves Deployment-only rollingUpdate fields open: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "workloadKind": "StatefulSet",
                "updateStrategy": stateful_only.clone()
            })
        ),
        "StatefulSetStrategy types rollingUpdate.partition as an integer: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "workloadKind": "Deployment",
                "updateStrategy": stateful_only.clone()
            })
        ),
        "DeploymentStrategy leaves StatefulSet-only rollingUpdate fields open: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "workloadKind": "CustomWorkload",
                "updateStrategy": stateful_only
            })
        ),
        "an unknown kind remains an explicit unconstrained complement: {schema}"
    );
}

#[test]
fn helper_return_disjunction_partitions_downstream_provider_contracts() {
    let helpers = indoc! {r#"
        {{- define "provider.name" -}}
        {{- if eq (typeOf .Values.provider) "string" -}}
        {{- .Values.provider -}}
        {{- else -}}
        {{- .Values.provider.name -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- $provider_name := tpl (include "provider.name" .) $ -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            image: busybox
          {{- if eq $provider_name "webhook" }}
          - name: webhook
            image: webhook:1.0
            livenessProbe: {{ toYaml .Values.provider.webhook.livenessProbe | nindent 6 }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), None);

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "provider": {
                    "name": "webhook",
                    "webhook": { "livenessProbe": { "failureThreshold": "audit" } }
                }
            })
        ),
        "the selected webhook helper arm must apply the Probe provider schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "provider": {
                    "name": "aws",
                    "webhook": { "livenessProbe": { "failureThreshold": "audit" } }
                }
            })
        ),
        "the unselected webhook helper arm must leave its probe dormant: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "provider": {
                    "name": "webhook",
                    "webhook": { "livenessProbe": { "failureThreshold": 2 } }
                }
            })
        ),
        "a provider-valid probe remains accepted in the selected helper arm: {schema}"
    );
}

#[test]
fn helper_literal_or_override_return_applies_integer_preimage_to_the_override() {
    let helpers = indoc! {r#"
        {{- define "version.default" -}}
        {{- $old := index . 0 -}}
        {{- $new := index . 1 -}}
        {{- $default := index . 2 -}}
        {{- if kindIs "invalid" $default -}}
          {{- if semverCompare ">= 1.22-0" "1.29.0" -}}
            {{- print $new -}}
          {{- else -}}
            {{- print $old -}}
          {{- end -}}
        {{- else -}}
          {{- print $default -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          selector:
            app: test
          ports:
          - name: metrics
            port: {{ include "version.default" (list 10252 10257 .Values.service.port) }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            service:
              port: null
        "}),
    );

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "service": { "port": "audit" } })
        ),
        "a selected nonnumeric override renders an invalid Service port: {schema}"
    );
    for port in [
        serde_json::json!(10257),
        serde_json::json!("10257"),
        serde_json::Value::Null,
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({ "service": { "port": port.clone() } })
            ),
            "a provider-valid override or the literal-default arm must validate: port={port}; schema={schema}"
        );
    }
}

/// A typed ternary chooses between raw string output and a structural
/// `toYaml` result. The selector must scope each output candidate so the
/// string lane does not reject structured provider input.
#[test]
fn ternary_type_selector_partitions_helper_output_candidates() {
    let helpers = indoc! {r#"
        {{- define "repro.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) -}}
        {{- if contains "{{" (toJson .value) -}}
          {{- tpl $value .context -}}
        {{- else -}}
          {{- $value -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          containers:
            - name: probe
              image: busybox
              env:
                {{- include "repro.render" (dict "value" .Values.extraEnvVars "context" $) | nindent 8 }}
    "#};
    let values_yaml = "extraEnvVars: []\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "extraEnvVars": [{ "name": "AUDIT", "value": "ok" }],
            }),
            true,
            "structured provider input",
        ),
        (
            serde_json::json!({
                "extraEnvVars": [{ "name": "AUDIT", "value": 7 }],
            }),
            false,
            "invalid structured provider input",
        ),
        (
            serde_json::json!({
                "extraEnvVars": "- name: AUDIT\n  value: ok",
            }),
            true,
            "templated string input",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A helper's left-trimmed root output starts at column zero before the
/// caller applies `nindent`. Source indentation inside the helper must not
/// move an appended serialized list beneath the preceding item's last field.
#[test]
fn trimmed_helper_output_keeps_caller_sequence_item_placement() {
    let helpers = indoc! {r#"
        {{- define "repro.render" -}}
          {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) -}}
          {{- if contains "{{" (toJson .value) -}}
            {{- tpl $value .context -}}
          {{- else -}}
            {{- $value -}}
          {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          containers:
            - name: probe
              image: busybox
              env:
                - name: STATIC
                  value: present
                {{- include "repro.render" (dict "value" .Values.extraEnvVars "context" $) | nindent 8 }}
    "#};
    let values_yaml = "extraEnvVars: []\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "extraEnvVars": [{ "name": "AUDIT", "value": "ok" }],
            }),
            true,
            "a valid appended EnvVar item",
        ),
        (
            serde_json::json!({
                "extraEnvVars": [{ "name": "AUDIT", "value": 7 }],
            }),
            false,
            "an invalid appended EnvVar value",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A sound render-count subset may activate provider-backed rows, but an
/// unresolved helper row has no sink contract and must not project the
/// declared list sample as a string-only runtime restriction.
#[test]
fn provider_subset_does_not_activate_unresolved_helper_fallback_typing() {
    let helpers = indoc! {r#"
        {{- define "repro.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) -}}
        {{- if contains "{{" (toJson .value) -}}
          {{- tpl $value .context -}}
        {{- else -}}
          {{- $value -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- range $index := until (int .Values.count) }}
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe-{{ $index }}
        spec:
          containers:
            - name: probe
              image: busybox
              env:
                {{- include "repro.render" (dict "value" .Values.extraEnvVars "context" $) | nindent 8 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        count: 1
        extraEnvVars: []
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (override_, label) in [
        (
            serde_json::json!({
                "extraEnvVars": [{ "name": "AUDIT", "value": "ok" }],
            }),
            "structured helper input",
        ),
        (
            serde_json::json!({
                "extraEnvVars": "- name: AUDIT\n  value: ok",
            }),
            "templated string input",
        ),
    ] {
        let instance = composed_instance(values_yaml, override_);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A rendered-map merge decodes every layer through Helm's map-only
/// `fromYaml`. Non-mapping inputs contribute no members, while mapping
/// inputs retain ordered provider typing.
#[test]
fn parsed_map_merge_layers_type_only_mapping_source_shapes() {
    let helpers = bitnami_tplvalues_helpers();
    let source = indoc! {r#"
        {{- if and (gt (int64 .Values.count) 0) (or .Values.annotations .Values.commonAnnotations) }}
        {{- $annotations := include "common.tplvalues.merge" (dict "values" (list .Values.annotations .Values.commonAnnotations) "context" .) }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
          annotations: {{- include "common.tplvalues.render" (dict "value" $annotations "context" .) | nindent 4 }}
        spec:
          selector:
            app: probe
          ports:
            - name: http
              port: 80
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        count: 1
        annotations: {}
        commonAnnotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (override_, want, label) in [
        (
            serde_json::json!({ "commonAnnotations": [] }),
            true,
            "a falsy array layer leaves the guarded document dormant",
        ),
        (
            serde_json::json!({ "commonAnnotations": null }),
            true,
            "a null layer leaves the guarded document dormant",
        ),
        (
            serde_json::json!({ "annotations": 7 }),
            true,
            "a scalar layer is discarded by the map decoder",
        ),
        (
            serde_json::json!({ "annotations": [{ "audit": "ok" }] }),
            true,
            "an array layer is discarded by the map decoder",
        ),
        (
            serde_json::json!({ "annotations": { "audit": 7 } }),
            false,
            "an unshadowed mapping member reaches the provider",
        ),
        (
            serde_json::json!({ "annotations": { "audit": "ok" } }),
            true,
            "a provider-valid mapping member renders",
        ),
    ] {
        let instance = composed_instance(values_yaml, override_);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A parsed-map layer may execute unconditionally, but its map decoder still
/// discards Helm-falsy non-mappings before the provider sees the result.
#[test]
fn unconditionally_evaluated_parsed_map_layer_keeps_helm_falsy_shapes() {
    let helpers = bitnami_tplvalues_helpers();
    let source = indoc! {r#"
        {{- $annotations := include "common.tplvalues.merge" (dict "values" (list .Values.commonAnnotations) "context" .) }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
          annotations: {{- include "common.tplvalues.render" (dict "value" $annotations "context" .) | nindent 4 }}
        spec:
          selector:
            app: probe
          ports:
            - name: http
              port: 80
    "#};
    let values_yaml = "commonAnnotations: {}\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (value, want, label) in [
        (serde_json::json!([]), true, "an empty array"),
        (serde_json::json!(false), true, "false"),
        (serde_json::json!(0), true, "zero"),
        (serde_json::json!(null), true, "null"),
        (
            serde_json::json!({ "audit": "ok" }),
            true,
            "a provider-valid mapping",
        ),
        (
            serde_json::json!({ "audit": 7 }),
            false,
            "a provider-invalid mapping member",
        ),
    ] {
        let instance = serde_json::json!({ "commonAnnotations": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "unconditional parsed-map layer ({label}): instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn guarded_named_port_sink_does_not_widen_unconditional_numeric_sink() -> eyre::Result<()> {
    let source = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          selector:
            app: test
          ports:
            - name: http
              port: {{ .Values.port }}
        ---
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
            - http:
                paths:
                  - path: /
                    pathType: Prefix
                    backend:
                      service:
                        name: test
                        port:
                          {{- if typeIs "string" .Values.port }}
                          name: {{ .Values.port }}
                          {{- else }}
                          number: {{ .Values.port }}
                          {{- end }}
    "#};
    let signals = parse_ir(source).finalize().into_schema_signals();
    let evidence = signals
        .evidence_for("port")
        .ok_or_eyre("port evidence should be present")?;
    assert!(
        !evidence.provider_schema_uses.is_empty()
            && evidence
                .conditional_overlays
                .iter()
                .any(|overlay| !overlay.evidence.provider_schema_uses.is_empty()),
        "the test must exercise both unconditional and guarded provider sinks: {evidence:#?}"
    );
    let schema = schema_for_values_yaml(signals, Some("port: 80\n"));

    for (port, want, label) in [
        (serde_json::json!(80), true, "integer"),
        (serde_json::json!("80"), true, "numeric string"),
        (serde_json::json!("http"), false, "arbitrary string"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "port": port })) == want,
            "the unconditional Service numeric contract must dominate the guarded Ingress \
             named-port arm for {label}: {schema}"
        );
    }

    Ok(())
}

#[test]
fn self_guarded_numeric_sink_keeps_helm_falsy_values_dormant() {
    let source = indoc! {r"
        {{- if .Values.port }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          selector:
            app: test
          ports:
            - name: http
              port: {{ .Values.port }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(source), Some("port: 0\n"));

    for (port, want, label) in [
        (serde_json::json!(80), true, "integer"),
        (serde_json::json!("80"), true, "numeric string"),
        (serde_json::json!("http"), false, "truthy arbitrary string"),
        (serde_json::json!(false), true, "false"),
        (serde_json::json!(0), true, "zero"),
        (serde_json::json!(""), true, "empty string"),
        (serde_json::Value::Null, true, "null"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "port": port })) == want,
            "only truthy values reach the numeric provider sink for {label}: {schema}"
        );
    }
}

/// A serialized sibling occurrence cannot erase the Helm-falsy complement of
/// a self-guarded provider use. The helper substitutes a constant namespace
/// for every falsy raw spelling before the provider sees it.
#[test]
fn serialized_sibling_keeps_self_guarded_provider_falsy_complement() {
    let helpers = indoc! {r#"
        {{- define "repro.namespace" -}}
          {{- if .Values.namespaceOverride -}}
            {{- .Values.namespaceOverride -}}
          {{- else -}}
            {{- "default" -}}
          {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: repro
          namespace: {{ include "repro.namespace" . }}
        spec:
          containers:
            - name: repro
              image: example.invalid/repro
              {{- if .Values.emitArgument }}
              args:
                - --namespace={{ include "repro.namespace" . }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        emitArgument: false
        namespaceOverride: ''
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (override_value, want, label) in [
        (
            serde_json::json!({}),
            true,
            "an empty mapping takes the fallback",
        ),
        (
            serde_json::json!([]),
            true,
            "an empty list takes the fallback",
        ),
        (
            serde_json::json!("custom"),
            true,
            "a valid truthy namespace reaches the provider",
        ),
        (
            serde_json::json!({ "member": "value" }),
            true,
            "a truthy mapping formats to a plain namespace string",
        ),
        (
            serde_json::json!(7),
            false,
            "a number formats to a numeric YAML token",
        ),
    ] {
        let instance = serde_json::json!({
            "emitArgument": true,
            "namespaceOverride": override_value,
        });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A conditional branch rendering a DIRECT scalar hole into a
/// provider-required field (a Service `port`) backprojects presence and
/// non-nullability of the source leaf under the branch's guards: Helm
/// renders a missing or null source as an explicit null the provider
/// rejects. The dormant arm stays open, and a `default` fallback abstains
/// (absence renders the fallback instead).
#[test]
fn provider_required_field_requires_direct_source_leaf() {
    let guarded = indoc! {r"
        {{- if .Values.svc.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
          - port: {{ .Values.svc.port }}
            name: http
        {{- end }}
    "};
    let values_yaml = indoc! {"
        svc:
          enabled: false
          port: 80
    "};
    let schema = schema_for_values_yaml(parse_ir(guarded), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "svc": { "enabled": false } }),
            true,
            "dormant branch stays open",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true, "port": 80 } }),
            true,
            "present integer port renders a valid Service",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true } }),
            false,
            "missing port renders a provider-invalid null",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true, "port": null } }),
            false,
            "explicit null port renders a provider-invalid null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }

    let defaulted = indoc! {r"
        {{- if .Values.svc.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
          - port: {{ .Values.svc.port | default 9090 }}
            name: http
        {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(defaulted), Some(values_yaml));
    let instance = serde_json::json!({ "svc": { "enabled": true, "port": null } });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a default fallback renders on absence, so the source stays optional; schema={schema}"
    );
}

/// An unconditional direct hole has the same provider-required presence
/// contract as a guarded one: deleting the source still renders an explicit
/// null into the required slot.
#[test]
fn unconditional_provider_required_field_requires_direct_source_leaf() {
    let source = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
            - name: http
              port: {{ .Values.svc.port }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(source),
        Some(indoc! {"
            svc:
              port: 80
        "}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "svc": { "port": 80 } }),
            true,
            "present integer port renders a valid Service",
        ),
        (
            serde_json::json!({ "svc": {} }),
            false,
            "missing port renders a provider-invalid null",
        ),
        (
            serde_json::json!({ "svc": { "port": null } }),
            false,
            "explicit null port renders a provider-invalid null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A chart may intentionally leave a provider input unset for the user to
/// supply at install time. Presence backprojection must not make that shipped
/// default document invalid.
#[test]
fn unset_unconditional_provider_source_preserves_chart_default() {
    let source = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
            - name: http
              port: {{ .Values.svc.port }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(source),
        Some(indoc! {"
            svc:
              port:
        "}),
    );
    let instance = serde_json::json!({ "svc": {} });

    assert!(
        schema_accepts_instance(&schema, &instance),
        "an intentionally unset source remains valid in the shipped defaults: schema={schema}"
    );
}

/// The same default-preservation rule applies when a provider-invalid render
/// is live on the shipped document. A generated schema cannot require setup
/// values that the chart itself deliberately leaves unset.
#[test]
fn live_guarded_unset_provider_source_preserves_chart_default() {
    let source = indoc! {r"
        {{- if .Values.svc.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
            - name: http
              port: {{ .Values.svc.port }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        svc:
          enabled: true
          port:
    "};
    let schema = schema_for_values_yaml(parse_ir(source), Some(values_yaml));
    let instance = serde_json::json!({ "svc": { "enabled": true } });

    assert!(
        schema_accepts_instance(&schema, &instance),
        "a live intentionally unset source remains valid in the shipped defaults: schema={schema}"
    );
}

/// A dependency-owned default is restored by Helm's subchart coalescing even
/// when the parent document omits the leaf. It must therefore not become a
/// parent-level presence requirement.
#[test]
fn dependency_default_suppresses_parent_provider_source_presence() {
    let source = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
            - name: http
              port: {{ .Values.child.port }}
    "};
    let values_yaml = indoc! {"
        child:
          port: 80
    "};
    let schema =
        schema_for_dependency_values_yaml(parse_ir(source), values_yaml, values_yaml, values_yaml);
    let instance = serde_json::json!({ "child": {} });

    assert!(
        schema_accepts_instance(&schema, &instance),
        "the dependency refills its provider source before rendering: schema={schema}"
    );
}

/// Layered maps refill leaves only for the dependency's named entries. A
/// parent may omit `containerPort` from an overridden built-in port, while a
/// newly added enabled port still renders null without that leaf.
#[test]
fn dependency_defaults_refill_named_ranged_provider_sources() {
    let source = indoc! {r"
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
              containers:
                - name: test
                  image: busybox
                  ports:
                  {{- range $name, $port := .Values.child.ports }}
                  {{- if $port.enabled }}
                    - name: {{ $name }}
                      containerPort: {{ $port.containerPort }}
                  {{- end }}
                  {{- end }}
    "};
    let composed_values = indoc! {"
        child:
          ports:
            otlp:
              enabled: true
              containerPort: 4317
    "};
    let deeper_stage_values = indoc! {"
        child:
          ports:
            otlp:
              containerPort: 4317
    "};
    let schema = schema_for_dependency_values_yaml(
        parse_ir(source),
        composed_values,
        deeper_stage_values,
        composed_values,
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "child": { "ports": {
                "otlp": { "enabled": true },
            } } }),
            true,
            "the dependency refills its named port",
        ),
        (
            serde_json::json!({ "child": { "ports": {
                "custom": { "enabled": true },
            } } }),
            false,
            "a new port has no dependency default",
        ),
        (
            serde_json::json!({ "child": { "ports": {
                "custom": { "enabled": true, "containerPort": 9000 },
            } } }),
            true,
            "a complete new port renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// An optional provider field still rejects an explicitly rendered null.
/// Direct source deletion therefore requires presence even when the field
/// itself is not listed in its parent's `required` array.
#[test]
fn optional_null_intolerant_provider_field_requires_direct_source_leaf() {
    let source = indoc! {r"
        apiVersion: v1
        kind: PersistentVolumeClaim
        metadata:
          name: probe
        spec:
          accessModes:
            - ReadWriteOnce
          resources:
            requests:
              storage: {{ .Values.storage.size }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(source),
        Some(indoc! {"
            storage:
              size: 1Gi
        "}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "storage": { "size": "1Gi" } }),
            true,
            "a quantity renders into the optional typed field",
        ),
        (
            serde_json::json!({ "storage": {} }),
            false,
            "a missing size renders an invalid explicit null",
        ),
        (
            serde_json::json!({ "storage": { "size": null } }),
            false,
            "an explicit null remains invalid",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A helper-computed root predicate remains the exact liveness guard for a
/// later provider slot. Its source leaf is required only while the derived
/// root field is true.
#[test]
fn helper_set_root_guard_scopes_provider_source_presence() {
    let helpers = indoc! {r#"
        {{- define "repro.enabled" -}}
        {{- $_ := set . "enabled" (or
          (eq (.Values.feature.enabled | toString) "true")
          (and
            (eq (.Values.feature.enabled | toString) "-")
            (eq (.Values.global.enabled | toString) "true"))) -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- template "repro.enabled" . -}}
        {{- if .enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
          annotations:
            listen: {{ printf ":%v" .Values.feature.port | quote }}
        spec:
          ports:
            - name: https
              port: 443
              targetPort: {{ .Values.feature.port }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        global:
          enabled: true
        feature:
          enabled: "-"
          port: 8080
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            composed_instance(values_yaml, serde_json::json!({})),
            true,
            "the live default carries its port",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "feature": { "port": null } }),
            ),
            false,
            "the live derived guard requires its source port",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "feature": { "enabled": false, "port": null },
                }),
            ),
            true,
            "a disabled feature keeps the provider slot dormant",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// Metadata emitted by a helper keeps its provider map type when the helper
/// is gated by a root field assigned from a complete mode-selection chain.
#[test]
fn helper_splice_root_mode_guard_keeps_metadata_member_type() {
    let helpers = indoc! {r#"
        {{- define "repro.mode" -}}
        {{- if .Values.external -}}
          {{- $_ := set . "mode" "external" -}}
        {{- else if .Values.dev -}}
          {{- $_ := set . "mode" "dev" -}}
        {{- else if .Values.ha -}}
          {{- $_ := set . "mode" "ha" -}}
        {{- else -}}
          {{- $_ := set . "mode" "standalone" -}}
        {{- end -}}
        {{- end -}}
        {{- define "repro.annotations" -}}
        {{- if and (ne .mode "dev") .Values.annotations }}
        annotations:
          {{- toYaml .Values.annotations | nindent 2 }}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- template "repro.mode" . -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
          {{- template "repro.annotations" . }}
        spec:
          containers:
            - name: probe
              image: busybox
    "#};
    let values_yaml = indoc! {r"
        external: false
        dev: false
        ha: false
        annotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "annotations": { "audit": 7 } }),
            ),
            false,
            "a live helper splice keeps the metadata value type",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "annotations": { "audit": "ok" } }),
            ),
            true,
            "string annotation values render",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "dev": true,
                    "annotations": { "audit": 7 },
                }),
            ),
            true,
            "dev mode keeps the helper splice dormant",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A custom root field is absent until the current template execution sets
/// it. Comparing that nil field with a string is exact, so the helper's
/// provider projection must not be discarded as approximate.
#[test]
fn helper_splice_absent_root_field_comparison_keeps_metadata_member_type() {
    let helpers = indoc! {r#"
        {{- define "repro.annotations" -}}
        {{- if and (ne .mode "dev") .Values.annotations }}
        annotations:
          {{- toYaml .Values.annotations | nindent 2 }}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: probe
          {{- template "repro.annotations" . }}
    "#};
    let values_yaml = "annotations: {}\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "annotations": { "audit": 7 } }),
            false,
            "the absent root field makes the helper branch live",
        ),
        (
            serde_json::json!({ "annotations": { "audit": "ok" } }),
            true,
            "string annotation values render",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A helper that selects a preferred metadata map with `or`/`default` keeps
/// each candidate's provider member type in the states where it supplies the
/// output.
#[test]
fn helper_splice_first_truthy_metadata_candidates_keep_member_types() {
    let helpers = indoc! {r#"
        {{- define "repro.annotations" -}}
        {{- if or .Values.current.annotations .Values.legacyAnnotations }}
        annotations:
          {{- $kind := typeOf (or .Values.current.annotations .Values.legacyAnnotations) }}
          {{- if eq $kind "string" }}
            {{- tpl (.Values.current.annotations | default .Values.legacyAnnotations) . | nindent 2 }}
          {{- else }}
            {{- toYaml (.Values.current.annotations | default .Values.legacyAnnotations) | nindent 2 }}
          {{- end }}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
          {{- template "repro.annotations" . }}
        spec:
          containers:
            - name: probe
              image: busybox
    "#};
    let values_yaml = indoc! {r"
        current:
          annotations: {}
        legacyAnnotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "current": { "annotations": { "audit": 7 } } }),
            ),
            false,
            "the preferred candidate keeps the metadata value type",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "legacyAnnotations": { "audit": 7 } }),
            ),
            false,
            "the fallback candidate keeps the metadata value type",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "current": { "annotations": { "audit": "ok" } },
                    "legacyAnnotations": { "audit": 7 },
                }),
            ),
            true,
            "the preferred candidate shadows a malformed fallback",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A helper nested inside another helper keeps the complete manifest path of
/// its output when the outer helper contributes an embedded provider object.
#[test]
fn nested_helper_splice_keeps_embedded_metadata_member_type() {
    let helpers = indoc! {r#"
        {{- define "repro.annotations" -}}
          annotations:
            {{- toYaml .Values.annotations | nindent 4 }}
        {{- end -}}
        {{- define "repro.claims" -}}
          volumeClaimTemplates:
            - apiVersion: v1
              kind: PersistentVolumeClaim
              metadata:
                name: data
                {{- include "repro.annotations" . | nindent 6 }}
              spec:
                accessModes:
                  - ReadWriteOnce
                resources:
                  requests:
                    storage: 1Gi
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: apps/v1
        kind: StatefulSet
        metadata:
          name: probe
        spec:
          selector:
            matchLabels:
              app: probe
          serviceName: probe
          template:
            metadata:
              labels:
                app: probe
            spec:
              containers:
                - name: probe
                  image: busybox
          {{ template "repro.claims" . }}
    "#};
    let values_yaml = "annotations: {}\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(source, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "annotations": { "audit": 7 } }),
            false,
            "numeric annotation member",
        ),
        (
            serde_json::json!({ "annotations": { "audit": "ok" } }),
            true,
            "string annotation member",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A negated selector over an ordered merge is certainly true when every
/// layer is falsy. That sound subset is enough to retain provider typing for
/// a payload rendered under the guard.
#[test]
fn negated_merged_layer_guard_scopes_provider_payloads() {
    let source = indoc! {r"
        {{- $gate := mergeOverwrite .Values.base.gate .Values.override.gate -}}
        {{- if and (not $gate.enabled) .Values.base.live }}
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          securityContext:
            {{- toYaml .Values.base.securityContext | nindent 4 }}
          containers:
            - name: probe
              image: busybox
        {{- end }}
    "};
    let schema = schema_for(parse_ir(source));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "base": {
                    "gate": { "enabled": false },
                    "live": true,
                    "securityContext": { "runAsUser": "oops" },
                },
                "override": { "gate": { "enabled": false } },
            }),
            false,
            "a live payload keeps the provider field type",
        ),
        (
            serde_json::json!({
                "base": {
                    "gate": { "enabled": false },
                    "live": true,
                    "securityContext": { "runAsUser": 1000 },
                },
                "override": { "gate": { "enabled": false } },
            }),
            true,
            "a valid live payload renders",
        ),
        (
            serde_json::json!({
                "base": {
                    "gate": { "enabled": false },
                    "live": true,
                    "securityContext": { "runAsUser": "oops" },
                },
                "override": { "gate": { "enabled": true } },
            }),
            true,
            "a truthy higher-priority guard layer keeps the payload dormant",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Merge-layer provider arms own the sink contract, including its ambient
/// render guard. Their synthetic source-selection branch must not separately
/// project the declared default's type while the resource is dormant.
#[test]
fn dormant_merge_layer_does_not_project_declared_default_type() {
    let source = indoc! {r"
        {{- $service := mergeOverwrite .Values.legacy .Values.current -}}
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          selector:
            app: probe
          ports:
            - name: http
              port: {{ $service.port }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        enabled: false
        legacy:
          port: 80
        current: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(source), Some(values_yaml));

    for (instance, want, label) in [
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "legacy": { "port": true } }),
            ),
            true,
            "a dormant resource ignores the source spelling",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "enabled": true,
                    "legacy": { "port": true },
                }),
            ),
            false,
            "a live resource keeps provider typing",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "enabled": true,
                    "legacy": { "port": 8080 },
                }),
            ),
            true,
            "a valid live source renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A source consumed both directly and through an ordered merge keeps the
/// direct sink contract beside the merge arm. The merge arm owns only the
/// layered use and cannot erase an independent consumer when it is dormant.
#[test]
fn merge_layer_preserves_an_independent_direct_consumer() {
    let source = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: direct
        spec:
          selector:
            app: probe
          ports:
            - name: direct
              port: {{ .Values.legacy.port }}
        ---
        {{- $service := mergeOverwrite .Values.legacy .Values.current -}}
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: layered
        spec:
          selector:
            app: probe
          ports:
            - name: layered
              port: {{ $service.port }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        enabled: false
        legacy:
          port: 80
        current: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(source), Some(values_yaml));

    for (instance, want, label) in [
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "legacy": { "port": "oops" } }),
            ),
            false,
            "the direct consumer remains typed while the merge arm is dormant",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({ "legacy": { "port": 8080 } }),
            ),
            true,
            "a valid direct value renders while the merge arm is dormant",
        ),
        (
            composed_instance(
                values_yaml,
                serde_json::json!({
                    "enabled": true,
                    "legacy": { "port": 8080 },
                }),
            ),
            true,
            "the same value remains valid when both consumers are live",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// An integer-cast comparison is not exact over every Helm input spelling,
/// but its positive integer region is exact. Provider typing can safely bind
/// inside that region and remain dormant outside it.
#[test]
fn int_cast_sound_subset_scopes_provider_payloads() {
    let source = indoc! {r"
        {{- if gt (int64 .Values.count) 0 }}
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          securityContext:
            {{- toYaml .Values.securityContext | nindent 4 }}
          containers:
            - name: probe
              image: busybox
        {{- end }}
    "};
    let schema = schema_for(parse_ir(source));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "count": 1,
                "securityContext": { "runAsUser": "oops" },
            }),
            false,
            "a live payload keeps the provider field type",
        ),
        (
            serde_json::json!({
                "count": 1,
                "securityContext": { "runAsUser": 1000 },
            }),
            true,
            "a valid live payload renders",
        ),
        (
            serde_json::json!({
                "count": 0,
                "securityContext": { "runAsUser": "oops" },
            }),
            true,
            "a zero count keeps the payload dormant",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A sound subset on an otherwise undecodable header is sufficient to type
/// the provider payload wherever that subset fires. Its Boolean shape is not
/// limited to one special guard family.
#[test]
fn compound_sound_subset_scopes_provider_payloads() {
    let live_subset = helm_schema_core::Predicate::all(vec![
        helm_schema_core::Predicate::truthy_path("enabled"),
        helm_schema_core::Predicate::Or(vec![
            helm_schema_core::Predicate::truthy_path("create"),
            helm_schema_core::Predicate::from(Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::String("live".to_string()),
            }),
        ]),
    ]);
    let contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "annotations".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "annotations".to_string()]),
        kind: ValueKind::YamlSerialized,
        condition: helm_schema_core::GuardDnf::from_conjunction([
            helm_schema_core::Predicate::approximate_with_sound_predicate(
                "repro",
                BTreeSet::new(),
                live_subset,
            ),
        ]),
        resource: Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
        provenance: Vec::new(),
        has_string_contract: false,
        stringified: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::new(),
        digest: false,
        merge_operand: false,
    }]);
    let schema = schema_for_values_yaml(
        contract,
        Some(indoc! {"
            enabled: false
            create: false
            mode: dormant
            annotations: {}
        "}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({
                "enabled": true,
                "create": false,
                "mode": "live",
                "annotations": 7,
            }),
            false,
            "a live compound subset types the payload",
        ),
        (
            serde_json::json!({
                "enabled": true,
                "create": true,
                "mode": "dormant",
                "annotations": { "audit": "ok" },
            }),
            true,
            "either exact alternative can activate a valid payload",
        ),
        (
            serde_json::json!({
                "enabled": false,
                "create": true,
                "mode": "live",
                "annotations": 7,
            }),
            true,
            "a malformed dormant payload stays open",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A selected scalar result can retain an operand as influence without
/// preserving its value. A sound execution subset must not project the
/// provider's output string type back onto the Boolean selector.
#[test]
fn sound_subset_does_not_type_scalar_selection_influence() {
    let source = indoc! {r#"
        {{- if or .Values.live .Release.IsUpgrade }}
        apiVersion: v1
        kind: Pod
        metadata:
          name: repro
        spec:
          containers:
            - name: repro
              image: busybox
              env:
                - name: FLAG
                  value: {{ ternary "true" "false" .Values.flag | quote }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        live: false
        flag: false
    "};
    let schema = schema_for_values_yaml(parse_ir(source), Some(values_yaml));

    for (flag, want, label) in [
        (serde_json::json!(false), true, "false selector"),
        (serde_json::json!(true), true, "true selector"),
        (serde_json::json!(7), false, "non-Boolean selector"),
    ] {
        let instance = composed_instance(
            values_yaml,
            serde_json::json!({
                "live": true,
                "flag": flag,
            }),
        );
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "ternary's own Boolean contract must survive without inheriting \
             the provider output type for a {label}: instance={instance}; schema={schema}"
        );
    }
}

/// Textual placement does not prove that a falsy source bypasses its
/// consumer. Strict formatters can still read empty maps and arrays.
#[test]
fn textual_rows_are_not_inherently_falsy_tolerant() -> eyre::Result<()> {
    let signals = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "repository".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "containers[*]".to_string(),
            "image".to_string(),
        ]),
        kind: ValueKind::Serialized,
        condition: helm_schema_core::GuardDnf::default(),
        resource: Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
        provenance: Vec::new(),
        has_string_contract: false,
        stringified: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::new(),
        digest: false,
        merge_operand: false,
    }])
    .finalize()
    .into_schema_signals();
    let evidence = signals
        .evidence_for("repository")
        .ok_or_eyre("expected textual repository evidence")?;

    sim_assert_eq!(
        have: evidence.facts.all_render_uses_falsy_tolerant,
        want: false
    );
    Ok(())
}

#[test]
fn pathless_dependency_fragment_root_keeps_values_mapping_open_with_descendants() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "webhook.serviceAccount.name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "webhook.enabled".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
        stringified: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_dependency_fragment("webhook");

    let schema = schema_for_values_yaml(
        contract,
        Some(indoc! {"
            webhook:
              enabled: false
              image:
                repository: webhook
              serviceAccount:
                name: webhook
        "}),
    );
    let webhook = schema
        .pointer("/properties/webhook")
        .expect("webhook schema");

    assert_ne!(
        webhook.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "pathless dependency fragment roots should stay open when descendants are inserted: {webhook}",
    );
}

#[test]
fn type_hint_only_descendant_preserves_object_input_branch() {
    let uses = vec![ContractUse {
        source_expr: "image".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "Service".to_string(),
        )),
        provenance: Vec::new(),
        has_string_contract: false,
        stringified: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }];
    let contract = with_type_hints(
        ContractIr::from_contract_uses(uses),
        &[("image.tag", "string")],
    );
    let schema = schema_for_values_yaml(&contract, Some("image: {}\n"));
    let variants = schema
        .pointer("/properties/image/anyOf")
        .and_then(Value::as_array)
        .expect("image schema should preserve object and scalar branches");

    assert!(
        variants.iter().any(|variant| {
            variant
                .pointer("/properties/tag/type")
                .and_then(Value::as_str)
                == Some("string")
        }),
        "type-hint descendant should preserve an object input branch with the hinted leaf: {schema:#}",
    );
    assert!(
        variants
            .iter()
            .any(|variant| schema_accepts_instance(variant, &Value::String("service".to_string()))),
        "rendered scalar sink should still preserve the scalar branch: {schema:#}",
    );
}

#[derive(Debug)]
struct DescriptionProvider;

impl ResourceSchemaOracle for DescriptionProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "description": "provider description",
            "type": "string",
        })))
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn surveyor_metric_relabelings_keeps_crd_provider_evidence() -> eyre::Result<()> {
    let src = test_util::read_testdata("charts/surveyor/templates/serviceMonitor.yaml")?;
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/surveyor/templates/_helpers.tpl",
        &test_util::read_testdata("charts/surveyor/templates/_helpers.tpl")?,
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml_source = test_util::read_testdata("charts/surveyor/values.yaml")?;
    let values_yaml: serde_yaml::Value =
        serde_yaml::from_str(&values_yaml_source).wrap_err("parse Surveyor values fixture")?;
    let provider = Chain::new(vec![
        Box::new(
            CrdsCatalogSchemaProvider::new()
                .with_cache_dir(
                    test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
                )
                .with_allow_download(false),
        ),
        Box::new(
            KubernetesJsonSchemaProvider::new("v1.35.0")
                .with_cache_dir(super::bundle_cache_dir())
                .with_allow_download(false)
                .with_api_version_guess(true),
        ),
    ])
    .with_inference_enabled(true);
    let resolved = crate::path_resolver::PathSchemaResolver::new(
        &schema_signals,
        &values_yaml,
        &serde_yaml::Value::Null,
        &provider,
    )
    .resolve_all();
    let resolved_metric_relabelings = resolved
        .iter()
        .find(|path| path.value_path == "serviceMonitor.metricRelabelings")
        .expect("resolved metricRelabelings");
    assert!(
        schema_signals
            .evidence_for("serviceMonitor.metricRelabelings")
            .is_some_and(|evidence| evidence.provider_schema_uses.is_empty()),
        "metricRelabelings provider evidence should not escape its render guard"
    );
    assert!(
        resolved_metric_relabelings
            .provider_schema_candidate
            .is_none(),
        "metricRelabelings should not have an unconditional provider candidate"
    );
    let overlay = schema_signals
        .evidence_for("serviceMonitor.metricRelabelings")
        .and_then(|evidence| evidence.conditional_overlays.first())
        .expect("metricRelabelings conditional overlay");
    assert!(
        !overlay.evidence.provider_schema_uses.is_empty(),
        "metricRelabelings conditional overlay should keep CRD provider schema uses"
    );
    assert!(
        !overlay.preserve_base_schema,
        "guarded-only metricRelabelings evidence should not preserve a typed base: {overlay:#?}"
    );
    let resolved_overlay = crate::path_resolver::PathSchemaResolver::resolve_single_path_evidence(
        &overlay
            .evidence
            .as_path_evidence("serviceMonitor.metricRelabelings"),
        &provider,
    );
    sim_assert_eq!(
        have: resolved_overlay.schema.pointer("/anyOf/0/type").and_then(Value::as_str),
        want: Some("array"),
        "resolved overlay schema should stay array-shaped: {}",
        resolved_overlay.schema
    );
    sim_assert_eq!(
        have: resolved_overlay
            .schema
            .pointer("/anyOf/0/items/properties/action/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "resolved overlay schema should keep relabel config item shape: {}",
        resolved_overlay.schema
    );

    let generated = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider)
            .with_values_yaml(Some(&values_yaml_source)),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": "replace" }]
                }
            }),
            true,
            "enabled provider-shaped relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": 7 }]
                }
            }),
            false,
            "enabled invalid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": false,
                    "metricRelabelings": 7
                }
            }),
            true,
            "disabled unconstrained relabeling",
        ),
    ] {
        assert!(
            schema_accepts_instance(&generated, &instance) == want,
            "{label}: instance={instance}; schema={generated}"
        );
    }
    Ok(())
}

#[test]
fn zalando_extra_envs_keeps_podspec_envvar_shape() -> eyre::Result<()> {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml")?;
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/zalando-postgres-operator/templates/_helpers.tpl",
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl")?,
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml_source =
        test_util::read_testdata("charts/zalando-postgres-operator/values.yaml")?;
    let values_yaml: serde_yaml::Value = serde_yaml::from_str(&values_yaml_source)
        .wrap_err("parse Zalando operator values fixture")?;
    let provider = production_chain_provider();

    let resolved = crate::path_resolver::PathSchemaResolver::new(
        &schema_signals,
        &values_yaml,
        &serde_yaml::Value::Null,
        &provider,
    )
    .resolve_all();
    let resolved_extra_envs = resolved
        .iter()
        .find(|path| path.value_path == "extraEnvs")
        .expect("resolved extraEnvs");
    assert!(
        resolved_extra_envs.provider_schema_candidate.is_some(),
        "extraEnvs should preserve provider schema candidate: {}; evidence={:#?}",
        resolved_extra_envs.schema,
        schema_signals.evidence_for("extraEnvs")
    );
    sim_assert_eq!(
        have: resolved_extra_envs
            .schema
            .pointer("/anyOf/0/type")
            .and_then(Value::as_str),
        want: Some("array"),
        "extraEnvs should stay array-shaped: {}",
        resolved_extra_envs.schema
    );
    sim_assert_eq!(
        have: resolved_extra_envs
            .schema
            .pointer("/anyOf/0/items/properties/name/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "extraEnvs should keep EnvVar item shape: {}",
        resolved_extra_envs.schema
    );

    let generated = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider)
            .with_values_yaml(Some(&values_yaml_source)),
    );
    let extra_envs = generated
        .pointer("/properties/extraEnvs")
        .expect("generated extraEnvs property");
    sim_assert_eq!(
        have: extra_envs.pointer("/anyOf/0/type").and_then(Value::as_str),
        want: Some("array"),
        "generated extraEnvs should stay array-shaped: {extra_envs}"
    );
    sim_assert_eq!(
        have: extra_envs
            .pointer("/anyOf/0/items/properties/name/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "generated extraEnvs should keep EnvVar item shape: {extra_envs}"
    );
    Ok(())
}

#[test]
fn unrelated_default_inside_set_does_not_mark_target_as_defaulted() {
    let helpers = indoc! {r#"
        {{- define "synth.defaultValues" }}
        {{- with .Values }}
        {{- $_ := set .serviceAccount "name" (printf "%s" (.other | default "fallback")) }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- include "synth.defaultValues" . }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ .Values.serviceAccount.name | quote }}
    "#};

    let ir = parse_ir_with_helpers(src, helpers);
    let projection = ir.clone().finalize();
    let guarded_target_uses: Vec<_> = projection
        .uses()
        .iter()
        .filter(|use_| {
            use_.source_expr == "serviceAccount.name"
                && use_.path.0 == ["metadata".to_string(), "name".to_string()]
        })
        .collect();
    assert!(
        !guarded_target_uses.is_empty(),
        "expected a rendered use for serviceAccount.name, got {ir:?}"
    );
    assert!(
        guarded_target_uses.iter().all(|use_| {
            !use_.single_guard_conjunction().iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Default { path } if path == "serviceAccount.name"
                )
            })
        }),
        "unrelated default must not mark serviceAccount.name as defaulted: {guarded_target_uses:#?}"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn guarded_fragment_array_provider_schema_stays_precise() {
    #[derive(Debug)]
    struct RelabelingsProvider;

    impl ResourceSchemaOracle for RelabelingsProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            (use_.value_path == "serviceMonitor.metricRelabelings"
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "endpoints[*]".to_string(),
                        "metricRelabelings".to_string(),
                    ])
            .then(|| {
                ProviderSchemaFragment::new(serde_json::json!({
                    "description": "provider relabelings",
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "action": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                }))
            })
        }
    }

    let uses = vec![
        ContractUse {
            source_expr: "serviceMonitor.metricRelabelings".to_string(),
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "serviceMonitor.enabled".to_string(),
            }]),
            resource: Some(ResourceRef::concrete(
                "monitoring.coreos.com/v1".to_string(),
                "ServiceMonitor".to_string(),
            )),
            provenance: Vec::new(),
            has_string_contract: false,
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
        ContractUse {
            source_expr: "serviceMonitor.metricRelabelings".to_string(),
            path: YamlPath(vec![
                "spec".to_string(),
                "endpoints[*]".to_string(),
                "metricRelabelings".to_string(),
            ]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "serviceMonitor.enabled".to_string(),
            }]),
            resource: Some(ResourceRef::concrete(
                "monitoring.coreos.com/v1".to_string(),
                "ServiceMonitor".to_string(),
            )),
            provenance: Vec::new(),
            has_string_contract: false,
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
    ];

    let schema_signals = schema_signals_for(uses);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &RelabelingsProvider).with_values_yaml(Some(
            indoc! {"
                serviceMonitor:
                  metricRelabelings: []
            "},
        )),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": "replace" }]
                }
            }),
            true,
            "enabled valid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": 7 }]
                }
            }),
            false,
            "enabled invalid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": false,
                    "metricRelabelings": 7
                }
            }),
            true,
            "disabled unconstrained relabeling",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn repeated_exact_provider_subtrees_emit_provider_definitions() {
    let resource = ResourceRef::concrete("example.io/v1".to_string(), "Example".to_string());
    let uses = vec![
        ContractUse {
            source_expr: "first".to_string(),
            path: YamlPath(vec!["spec".to_string(), "first".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: Some(resource.clone()),
            provenance: Vec::new(),
            has_string_contract: false,
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
        ContractUse {
            source_expr: "second".to_string(),
            path: YamlPath(vec!["spec".to_string(), "second".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: Some(resource),
            provenance: Vec::new(),
            has_string_contract: false,
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
    ];
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(ValuesSchemaInput::new(
        &schema_signals,
        &SharedObjectProvider,
    ));

    let expected_definition = serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        }
    });
    sim_assert_eq!(
        have: schema.pointer("/properties/first"),
        want: Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/second"),
        want: Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    sim_assert_eq!(
        have: schema.pointer("/$defs/providerSchema1"),
        want: Some(&expected_definition)
    );
}

#[test]
fn values_yaml_comments_override_provider_descriptions() {
    let uses = vec![ContractUse {
        source_expr: "name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
        provenance: Vec::new(),
        has_string_contract: false,
        stringified: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }];
    let descriptions = BTreeMap::from([("name".to_string(), "chart description".to_string())]);
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &DescriptionProvider)
            .with_values_yaml(Some("name: example\n"))
            .with_values_descriptions(&descriptions),
    );

    sim_assert_eq!(
        have: schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        want: Some("chart description")
    );
}

#[test]
fn values_yaml_comments_do_not_create_schema_paths() {
    let uses = parse_ir(indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
    "});
    let descriptions = BTreeMap::from([
        ("name".to_string(), "name description".to_string()),
        (
            "commentedOut.enabled".to_string(),
            "comment-only path".to_string(),
        ),
    ]);
    let provider = Chain::new(Vec::new());
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider)
            .with_values_yaml(Some("name: example\n"))
            .with_values_descriptions(&descriptions),
    );

    sim_assert_eq!(
        have: schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        want: Some("name description")
    );
    assert!(
        schema.pointer("/properties/commentedOut").is_none(),
        "description metadata must not create schema paths: {schema}"
    );
}

fn schema_has_format(schema: &Value, format: &str) -> bool {
    if schema.get("format").and_then(Value::as_str) == Some(format) {
        return true;
    }
    ["anyOf", "oneOf", "allOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(|variant| schema_has_format(variant, format))
}

#[test]
fn base64_encoded_secret_data_does_not_inherit_rendered_byte_format() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Secret
        metadata:
          name: example
        data:
          direct: {{ .Values.directSecretData }}
          encoded: {{ .Values.password | b64enc | quote }}
    "};
    let values_yaml = indoc! {r#"
        directSecretData: ""
        password: ""
    "#};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let direct = schema
        .pointer("/properties/directSecretData")
        .expect("directSecretData present");
    assert!(
        schema_has_format(direct, "byte"),
        "direct Secret.data input should keep provider byte format, got {direct}; schema={schema}"
    );

    let password = schema
        .pointer("/properties/password")
        .expect("password present");
    assert!(
        permits_type(password, "string"),
        "encoded input should remain string-like, got {password}; schema={schema}"
    );
    assert!(
        !schema_has_format(password, "byte"),
        "pre-encoded chart input must not inherit rendered Secret.data byte format, got {password}; schema={schema}"
    );
}

#[test]
fn included_encoded_secret_data_preserves_nullable_source_without_byte_format() {
    let helpers = indoc! {r#"
        {{- define "sample.passwordData" -}}
        {{- if .Values.password }}
        password: {{ .Values.password | b64enc | quote }}
        {{- end }}
        raw: {{ .Values.rawSecretData }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: example
        data:
          {{- include "sample.passwordData" . | nindent 2 }}
    "#};
    let values_yaml = indoc! {r#"
        password: ""
        rawSecretData: ""
    "#};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let password = schema
        .pointer("/properties/password")
        .expect("password present");

    assert!(
        !schema_has_format(password, "byte"),
        "pre-encoded helper input must not inherit rendered Secret.data byte format, got {password}; schema={schema}"
    );
    for (instance, want, label) in [
        (serde_json::json!({ "password": null }), true, "null"),
        (serde_json::json!({ "password": {} }), true, "empty object"),
        (serde_json::json!({ "password": "secret" }), true, "string"),
        (serde_json::json!({ "password": 7 }), false, "truthy number"),
        (
            serde_json::json!({ "password": { "bad": true } }),
            false,
            "truthy object",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "encoded helper input {label}: instance={instance}; schema={schema}"
        );
    }

    let raw = schema
        .pointer("/properties/rawSecretData")
        .expect("rawSecretData present");
    assert!(
        schema_has_format(raw, "byte"),
        "unencoded sibling helper input should still inherit Secret.data byte format, got {raw}; schema={schema}"
    );
}

/// `tpl (toYaml .Values.X) .` re-renders the serialized fragment,
/// so the provider slot projects back to the input exactly like a bare
/// `toYaml` splice (airflow's scheduler command and extraContainers).
#[test]
fn tpl_serialized_fragment_projects_the_provider_slot() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: scheduler
                  image: img
                  {{- if .Values.scheduler.command }}
                  command: {{ tpl (toYaml .Values.scheduler.command) . | nindent 12 }}
                  {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            scheduler:
              command: ~
        "}),
    );

    for (instance, want) in [
        (serde_json::json!({ "scheduler": { "command": 7 } }), false),
        (
            serde_json::json!({ "scheduler": { "command": ["bash"] } }),
            true,
        ),
        (
            serde_json::json!({ "scheduler": { "command": null } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the tpl-serialized command keeps the PodSpec string-array slot: \
             instance={instance}; schema={schema}"
        );
    }
}

#[derive(Debug)]
struct HostnamesProvider;

impl ResourceSchemaOracle for HostnamesProvider {
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        (use_.path.0 == ["spec", "hostnames"]).then(|| {
            ProviderSchemaFragment::new(serde_json::json!({
                "items": {
                    "minLength": 1,
                    "pattern": "^(\\*\\.)?[a-z0-9]([-a-z0-9]*[a-z0-9])?(\\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$",
                    "type": "string"
                },
                "maxItems": 16,
                "type": "array"
            }))
        })
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn tpl_serialized_fragment_preserves_structure_without_typing_program_strings_as_output() {
    let src = indoc! {r"
        {{- range $name, $route := .Values.route }}
        {{- if $route.enabled }}
        apiVersion: gateway.networking.k8s.io/v1
        kind: HTTPRoute
        metadata:
          name: {{ $name }}
        spec:
          {{- with $route.hostnames }}
          hostnames:
            {{- tpl (toYaml .) $ | nindent 4 }}
          {{- end }}
        {{- end }}
        {{- end }}
    "};
    let signals = schema_signals_for(parse_ir(src));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &HostnamesProvider).with_values_yaml(Some(indoc! {"
            route:
              main:
                enabled: false
                hostnames:
                  - '*.example.com'
        "})),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": true,
                        "hostnames": ["{{ .Values.global.environment }}.example.com"]
                    }
                }
            }),
            true,
            "template program",
        ),
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": true,
                        "hostnames": ["*.example.com"]
                    }
                }
            }),
            true,
            "valid literal",
        ),
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": true,
                        "hostnames": ["bad_host"]
                    }
                }
            }),
            false,
            "invalid action-free literal",
        ),
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": true,
                        "hostnames": {"bad": true}
                    }
                }
            }),
            false,
            "wrong collection kind",
        ),
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": true,
                        "hostnames": [7]
                    }
                }
            }),
            false,
            "wrong item kind",
        ),
        (
            serde_json::json!({
                "route": {
                    "main": {
                        "enabled": false,
                        "hostnames": ["bad_host"]
                    }
                }
            }),
            true,
            "disabled route",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "tpl-serialized hostnames {label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn helper_preserves_tpl_serialized_provider_preimage() {
    let helpers = indoc! {r#"
        {{- define "sample.hostnames" -}}
        {{- tpl (toYaml .Values.hostnames) . }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: gateway.networking.k8s.io/v1
        kind: HTTPRoute
        metadata:
          name: test
        spec:
          hostnames:
            {{- include "sample.hostnames" . | nindent 4 }}
    "#};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &HostnamesProvider)
            .with_values_yaml(Some("hostnames: []\n")),
    );

    for (hostnames, want, label) in [
        (
            serde_json::json!(["{{ .Values.global.environment }}.example.com"]),
            true,
            "template program",
        ),
        (serde_json::json!(["*.example.com"]), true, "valid literal"),
        (
            serde_json::json!(["bad_host"]),
            false,
            "invalid action-free literal",
        ),
        (serde_json::json!([7]), false, "wrong item kind"),
    ] {
        let instance = serde_json::json!({ "hostnames": hostnames });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "helper tpl-serialized hostnames {label}: instance={instance}; schema={schema}"
        );
    }
}

/// Helm's YAML resolver reads hex, explicit octal, binary, and legacy
/// leading-zero spellings as integers, so a bare token in any of those
/// forms reparses away from the string a provider slot requires (velero's
/// unquoted `BackupStorageLocation` provider).
#[test]
fn plain_string_slot_excludes_non_decimal_integer_spellings() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: {{ .Values.containerName }}
                  image: img
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("containerName: app\n"));

    for (instance, want) in [
        (serde_json::json!({ "containerName": "0x10" }), false),
        (serde_json::json!({ "containerName": "0o17" }), false),
        (serde_json::json!({ "containerName": "0123" }), false),
        (serde_json::json!({ "containerName": "0b101" }), false),
        (serde_json::json!({ "containerName": "app" }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "non-decimal integer spellings reparse away from the string slot: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A serialized fragment spliced beside a literal sibling (`- name: tmp`
/// above `toYaml .Values.tmpVolume | nindent`) completes an object the
/// template already gives that key: the provider slot's `required` must
/// not re-demand it from the user value (metrics-server's Volume slot),
/// while the slot's member typing still applies.
#[test]
fn template_supplied_sibling_keys_relax_provider_requiredness() {
    #[derive(Debug)]
    struct VolumeProvider;

    impl ResourceSchemaOracle for VolumeProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            (use_.value_path == "tmpVolume").then(|| {
                ProviderSchemaFragment::new(serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "emptyDir": { "type": "object", "additionalProperties": false },
                        "hostPath": {
                            "type": "object",
                            "properties": { "path": { "type": "string" } },
                            "additionalProperties": false
                        }
                    }
                }))
            })
        }
    }

    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              volumes:
                - name: tmp
                  {{- toYaml .Values.tmpVolume | nindent 10 }}
    "};
    let ir = parse_ir(src);
    let schema_signals = ir.into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &VolumeProvider).with_values_yaml(Some(indoc! {"
            tmpVolume:
              emptyDir: {}
        "})),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "tmpVolume": { "emptyDir": {} } }),
            true,
            "the template supplies name itself",
        ),
        (
            serde_json::json!({ "tmpVolume": { "hostPath": { "path": "/tmp" } } }),
            true,
            "other volume variants stay open",
        ),
        (
            serde_json::json!({ "tmpVolume": { "emptyDir": 7 } }),
            false,
            "the slot's member typing still applies",
        ),
        (
            serde_json::json!({ "tmpVolume": null }),
            false,
            "a null fragment corrupts the literal volume item",
        ),
        (
            serde_json::json!({}),
            false,
            "deleting the fragment corrupts the literal volume item",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A `tpl`-rendered splice gives the provider slot its OUTPUT, never the
/// raw program text, so the slot's string grammar must not back-project
/// onto the raw value (loki's `secretName: {{ tpl
/// .Values.loki.configObjectName . }}` with the templated default
/// `"{{ include \"loki.name\" . }}"`); `tpl`'s own string-input contract
/// still types the path.
#[test]
fn tpl_rendered_slots_keep_the_raw_program_open() {
    #[derive(Debug)]
    struct SecretNameProvider;

    impl ResourceSchemaOracle for SecretNameProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            (use_.value_path == "objectName").then(|| {
                ProviderSchemaFragment::new(serde_json::json!({
                    "type": "string",
                    "pattern": "^[a-z0-9.-]+$"
                }))
            })
        }
    }

    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          volumes:
            - name: config
              secret:
                secretName: {{ tpl .Values.objectName . }}
    "};
    let ir = parse_ir(src);
    let schema_signals = ir.into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &SecretNameProvider)
            .with_values_yaml(Some("objectName: \"{{ include \\\"repro.name\\\" . }}\"\n")),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "objectName": "{{ include \"repro.name\" . }}" }),
            true,
            "a raw template program renders through tpl",
        ),
        (
            serde_json::json!({ "objectName": "plain-name" }),
            true,
            "plain names render",
        ),
        (
            serde_json::json!({ "objectName": { "a": 1 } }),
            false,
            "tpl requires a string program",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// redis-ha's `ConfigMap` fills each `data` value with `redis.conf: |`
/// followed by a COLUMN-ZERO `{{- include "config-redis.conf" . }}`: the
/// include's rendered lines are deeper than the entry, so they continue
/// the still-open block scalar — pure text. Evaluating the include as
/// structure escaping to the parent anchors the helper's ranged `config`
/// members at the `data` field itself, whose object provider schema
/// scalar-restricts to `type: null` and rejects every member Helm renders
/// (oauth2-proxy and argo-cd with redis-ha enabled). The adopted lane must
/// keep the members open while preserving the helper's strict `tpl`
/// string-program contract on `customConfig`.
#[test]
fn block_scalar_adopted_includes_render_as_text_not_structure() {
    let helpers = indoc! {r#"
        {{- define "repro.conf" }}
        {{- if .Values.redis.customConfig }}
        {{ tpl .Values.redis.customConfig . | indent 4 }}
        {{- else }}
            dir "/data"
            port {{ .Values.redis.port }}
            {{- range $key, $value := .Values.redis.config }}
            {{ $key }} {{ $value }}
            {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          redis.conf: |
        {{- include "repro.conf" . }}
    "#};
    let values_yaml = indoc! {r"
        redis:
          port: 6379
          config: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (
            composed_instance(values_yaml, serde_json::json!({})),
            true,
            "defaults render",
        ),
        (
            serde_json::json!({ "redis": { "config": { "maxmemory": "100mb" } } }),
            true,
            "string members render as block text",
        ),
        (
            serde_json::json!({ "redis": { "config": { "save": "" } } }),
            true,
            "empty-string members render",
        ),
        (
            serde_json::json!({ "redis": { "config": { "repl-diskless-sync": true } } }),
            true,
            "raw scalars stringify in the loop body",
        ),
        (
            serde_json::json!({ "redis": { "customConfig": "maxmemory 100mb" } }),
            true,
            "string custom config renders through tpl",
        ),
        (
            serde_json::json!({ "redis": { "customConfig": { "bad": true } } }),
            false,
            "tpl requires a string program even under the block",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "block-adopted include ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// traefik's deployment routes its pod template through
/// `include "traefik.podTemplate" . | fromYaml | toYaml | nindent`, and a
/// NESTED helper renders ranged `resourceAttributes` members as container
/// flag args. The roundtrip lane must keep those member rows anchored at
/// the args ITEM depth: anchoring one level short provider-types them by
/// the Container fragment and scalar-restricts the map to `type: null`,
/// rejecting every member Helm renders.
#[test]
fn roundtrip_pod_templates_keep_ranged_flag_rows_at_item_depth() {
    let helpers = indoc! {r#"
        {{- define "repro.flags" }}
          {{- $path := .path -}}
          {{- $cfg := .cfg -}}
          {{- if $cfg.enabled }}
          - "--{{$path}}=true"
           {{- range $name, $value := $cfg.resourceAttributes }}
          -  "--{{$path}}.resourceAttributes.{{ $name }}={{ $value }}"
           {{- end }}
          {{- end }}
        {{- end }}
        {{- define "repro.podTemplate" -}}
        metadata:
          labels:
            app: test
        spec:
          containers:
            - name: test
              image: busybox
              args:
                {{- with .Values.tracing.otlp }}
                 {{- include "repro.flags" (dict "path" "tracing.otlp" "cfg" .) | nindent 8 }}
                {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template: {{ include "repro.podTemplate" . | fromYaml | toYaml | nindent 4 }}
    "#};
    let values_yaml = indoc! {r"
        tracing:
          otlp:
            enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (
            composed_instance(values_yaml, serde_json::json!({})),
            true,
            "defaults render",
        ),
        (
            serde_json::json!({ "tracing": { "otlp": { "enabled": true,
                "resourceAttributes": { "env": "prod" } } } }),
            true,
            "string members render as flags",
        ),
        (
            serde_json::json!({ "tracing": { "otlp": { "enabled": true,
                "resourceAttributes": { "env": 7 } } } }),
            true,
            "non-string members stringify in the loop body",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "roundtrip flag rows ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A ranged member LEAF rendered into a provider-REQUIRED field emits an
/// explicit null for every member missing the leaf, which strict provider
/// validation rejects (kube-state-metrics' probe `httpHeaders: [{}]`
/// renders null `name`/`value`; promtail's `extraPorts` render a null
/// Service `port`). Every member must carry the leaf present and
/// non-null; an empty or absent collection runs zero iterations and stays
/// open, and a member-scoped ELSE-arm guard becomes the escape
/// alternative of a per-member disjunction.
#[test]
fn ranged_member_leaves_of_required_provider_fields_bind_presence() {
    let src = indoc! {r"
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
              containers:
                - name: test
                  image: busybox
                  livenessProbe:
                    httpGet:
                      path: /healthz
                      port: http
                      {{- if .Values.probe.httpHeaders }}
                      httpHeaders:
                      {{- range $_, $header := .Values.probe.httpHeaders }}
                      - name: {{ $header.name }}
                        value: {{ $header.value }}
                      {{- end }}
                      {{- end }}
    "};
    let values_yaml = indoc! {r"
        probe:
          httpHeaders: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want, label) in [
        (
            composed_instance(values_yaml, serde_json::json!({})),
            true,
            "defaults render",
        ),
        (
            serde_json::json!({ "probe": { "httpHeaders": [] } }),
            true,
            "empty collection runs zero iterations",
        ),
        (
            serde_json::json!({ "probe": { "httpHeaders":
                [{ "name": "X-Audit", "value": "audit" }] } }),
            true,
            "populated headers render",
        ),
        (
            serde_json::json!({ "probe": { "httpHeaders": [{}] } }),
            false,
            "an empty member renders null name and value",
        ),
        (
            serde_json::json!({ "probe": { "httpHeaders": [{ "name": "X-Audit" }] } }),
            false,
            "a missing value renders null",
        ),
        (
            serde_json::json!({ "probe": { "httpHeaders":
                [{ "name": "X-Audit", "value": null }] } }),
            false,
            "an explicit null value renders null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "ranged required leaf ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A positive sibling guard activates a ranged member's provider slot only
/// for truthy members. Disabled members need no leaf, while enabled members
/// must carry the direct source that otherwise renders as null.
#[test]
fn positive_member_guard_scopes_required_provider_leaf_presence() {
    let src = indoc! {r"
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
              containers:
                - name: test
                  image: busybox
                  ports:
                  {{- range $name, $port := .Values.ports }}
                  {{- if $port.enabled }}
                    - name: {{ $name }}
                      containerPort: {{ $port.containerPort }}
                  {{- end }}
                  {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("ports: {}\n"));

    for (instance, want, label) in [
        (
            serde_json::json!({ "ports": { "audit": { "enabled": false } } }),
            true,
            "a disabled member renders no provider slot",
        ),
        (
            serde_json::json!({ "ports": { "audit": {
                "enabled": true,
                "containerPort": 8080,
            } } }),
            true,
            "an enabled member with a valid leaf renders",
        ),
        (
            serde_json::json!({ "ports": { "audit": { "enabled": true } } }),
            false,
            "an enabled member without its leaf renders null",
        ),
        (
            serde_json::json!({ "ports": { "audit": {
                "enabled": true,
                "containerPort": null,
            } } }),
            false,
            "an enabled member with an explicit null renders null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// The helper-projection variant of the presence binding: the range lives
/// in a pod-template helper consumed through `include … | fromYaml |
/// toYaml`, and the leaf renders through Sprig `quote` — which SKIPS nil
/// operands, so a missing or null source still forces an explicit null
/// into the provider-required `VolumeMount` `mountPath` (traefik's local
/// plugins).
#[test]
fn quoted_ranged_leaves_bind_presence_through_the_pod_template_projection() {
    let helpers = indoc! {r#"
        {{- define "test.podTemplate" }}
        metadata:
          labels:
            app: test
        spec:
          containers:
            - name: test
              image: busybox
              volumeMounts:
              {{- range $name, $plugin := .Values.plugins }}
              - name: {{ $name | replace "." "-" }}
                mountPath: {{ $plugin.mountPath | quote }}
              {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template: {{ include "test.podTemplate" . | fromYaml | toYaml | nindent 4 }}
    "#};
    let values_yaml = indoc! {r"
        plugins: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (
            composed_instance(values_yaml, serde_json::json!({})),
            true,
            "defaults render",
        ),
        (
            serde_json::json!({ "plugins": { "p": { "mountPath": "/x" } } }),
            true,
            "a member with mountPath renders",
        ),
        (
            serde_json::json!({ "plugins": { "p": {} } }),
            false,
            "a member without mountPath renders null",
        ),
        (
            serde_json::json!({ "plugins": { "p": { "mountPath": null } } }),
            false,
            "an explicit null mountPath renders null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "quoted ranged required leaf ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// The member-scoped else-arm variant: promtail's extra Services render
/// `port: {{ $values.containerPort }}` only for members WITHOUT a truthy
/// `service`, so the presence requirement carries the service escape.
#[test]
fn ranged_member_required_leaves_keep_the_else_arm_escape() {
    let src = indoc! {r"
        {{- range $key, $values := .Values.extraPorts }}
        ---
        apiVersion: v1
        kind: Service
        metadata:
          name: extra-{{ $key }}
        spec:
          ports:
            - name: {{ $key }}
              protocol: TCP
              {{- if $values.service }}
              port: {{ $values.service.port | default $values.containerPort }}
              {{- else }}
              port: {{ $values.containerPort }}
              {{- end }}
          selector:
            app: test
        {{- end }}
    "};
    let values_yaml = indoc! {r"
        extraPorts: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (member, want, label) in [
        (
            serde_json::json!({ "containerPort": 1234 }),
            true,
            "containerPort renders the port",
        ),
        (
            serde_json::json!({ "service": { "port": 80 } }),
            true,
            "a truthy service escapes the else arm",
        ),
        (serde_json::json!({}), false, "an empty member renders null"),
    ] {
        let instance = serde_json::json!({ "extraPorts": { "audit": member } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "else-arm escape ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A container item whose `-` marker is emitted from a conditional branch is
/// still a sequence item: the trailing keys the branch region does not cover
/// belong INSIDE it, so a `toYaml` splice among them resolves the provider
/// slot `containers[*].<key>` instead of `containers.<key>`, which owns no
/// schema at all (reloader's `resources`, `volumeMounts`, and
/// `securityContext` follow a digest/registry `if`/`else` chain that emits
/// the item's `- image:` line).
#[test]
fn branch_selected_sequence_items_keep_their_item_slot() -> eyre::Result<()> {
    let branch_selected = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
              {{- if .Values.digest }}
              - image: "busybox@{{ .Values.digest }}"
              {{- else }}
              - image: busybox
              {{- end }}
                name: test
                resources:
                  {{- toYaml .Values.resources | nindent 8 }}
    "#};
    let plain = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
              - image: busybox
                name: test
                resources:
                  {{- toYaml .Values.resources | nindent 8 }}
    "};

    for (label, src) in [("branch-selected", branch_selected), ("plain", plain)] {
        let signals = schema_signals_for(parse_ir(src));
        let evidence = signals
            .evidence_for("resources")
            .ok_or_eyre("resolved `resources` evidence")?;
        let slots: Vec<Vec<String>> = evidence
            .provider_schema_uses
            .iter()
            .map(|use_| use_.path.0.clone())
            .collect();
        sim_assert_eq!(
            have: slots,
            want: vec![vec![
                "spec".to_string(),
                "template".to_string(),
                "spec".to_string(),
                "containers[*]".to_string(),
                "resources".to_string(),
            ]],
            "{label} item slot"
        );
        assert!(
            schema_for(parse_ir(src))
                .pointer("/properties/resources/properties/limits")
                .is_some(),
            "{label}: the item slot must reach ResourceRequirements typing"
        );
    }
    Ok(())
}

/// An action-only line renders at its own column, but the CST attaches it to
/// whatever entry was still open — which can sit several levels deeper. The
/// escape has to be transitive: stranding the splice one level up puts it in
/// the wrong provider slot, which is where signoz's service-account pull
/// secrets and vault's injector `strategy:` used to land.
#[test]
fn bare_splices_escape_to_the_container_their_column_names() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "x.pullSecrets" -}}
        imagePullSecrets:
        {{- range .Values.imagePullSecrets }}
          - name: {{ . }}
        {{- end }}
        {{- end }}
        {{- define "x.saName" -}}
        fixed
        {{- end -}}
        {{- define "x.strategy" -}}
          strategy:
            type: {{ .Values.strategyType }}
        {{- end -}}
    "#};
    // The include follows `name: {{ … }}`, whose templated value opens a
    // scope, so the CST nests it two levels below its own column 0.
    let service_account = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ include "x.saName" . }}
        {{- include "x.pullSecrets" . }}
    "#};
    // The splice sits at column 2 under a `matchLabels:` whose members are at
    // 6, so it has to climb out of both `matchLabels` and `selector`.
    let deployment = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          {{ template "x.strategy" . }}
          template:
            metadata:
              labels:
                app: test
            spec:
              containers:
                - name: test
                  image: busybox
    "#};

    for (label, source, value_path, want) in [
        (
            "pull secret item",
            service_account,
            "imagePullSecrets.*",
            vec!["imagePullSecrets[*]", "name"],
        ),
        (
            "branch-selected strategy",
            deployment,
            "strategyType",
            vec!["spec", "strategy", "type"],
        ),
    ] {
        let signals = schema_signals_for(parse_ir_with_helpers(source, helpers));
        let evidence = signals
            .schema_evidence_by_value_path()
            .get(value_path)
            .ok_or_eyre(format!("{label}: no evidence for {value_path}"))?;
        let slots: BTreeSet<Vec<String>> = evidence
            .provider_schema_uses
            .iter()
            .chain(
                evidence
                    .conditional_overlays
                    .iter()
                    .flat_map(|overlay| &overlay.evidence.provider_schema_uses),
            )
            .map(|use_| use_.path.0.clone())
            .collect();
        let want: BTreeSet<Vec<String>> = [want.into_iter().map(str::to_string).collect()]
            .into_iter()
            .collect();
        sim_assert_eq!(have: slots, want: want, "{label}");
    }
    Ok(())
}
