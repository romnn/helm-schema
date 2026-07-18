use test_util::prelude::sim_assert_eq;

use super::*;

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

#[test]
fn plain_string_provider_preimage_rejects_yaml_unsafe_spellings() {
    let src = indoc! {r#"
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
    "#};
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
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          hostNetwork: {{ .Values.hostNetwork }}
          containers:
            - name: test
              image: busybox
    "#};
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
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            - port: {{ .Values.port }}
    "#};
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
    let schema = schema_for_values_yaml(parse_ir(src), Some("pdb:\n  minAvailable: 1\n"));

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
    let src = indoc! {r#"
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
    "#};
    let values_yaml = "stateful: false\nstrategy: {}\n";
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
    let values_yaml = "workloadKind: Deployment\nupdateStrategy: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
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
        Some("service:\n  port: null\n"),
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

/// A conditional branch rendering a DIRECT scalar hole into a
/// provider-required field (a Service `port`) backprojects presence and
/// non-nullability of the source leaf under the branch's guards: Helm
/// renders a missing or null source as an explicit null the provider
/// rejects. The dormant arm stays open, and a `default` fallback abstains
/// (absence renders the fallback instead).
#[test]
fn provider_required_field_requires_direct_source_leaf() {
    let guarded = indoc! {r#"
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
    "#};
    let values_yaml = indoc! {"
        svc:
          enabled: false
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

    let defaulted = indoc! {r#"
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
    "#};
    let schema = schema_for_values_yaml(parse_ir(defaulted), Some(values_yaml));
    let instance = serde_json::json!({ "svc": { "enabled": true, "port": null } });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a default fallback renders on absence, so the source stays optional; schema={schema}"
    );
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
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
            .any(|variant| variant.get("type").and_then(Value::as_str) == Some("string")),
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
fn surveyor_metric_relabelings_keeps_crd_provider_evidence() {
    let src = test_util::read_testdata("charts/surveyor/templates/serviceMonitor.yaml");
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/surveyor/templates/_helpers.tpl",
        &test_util::read_testdata("charts/surveyor/templates/_helpers.tpl"),
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml: serde_yaml::Value =
        serde_yaml::from_str(&test_util::read_testdata("charts/surveyor/values.yaml"))
            .expect("values yaml");
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
    let resolved =
        crate::path_resolver::PathSchemaResolver::new(&schema_signals, &values_yaml, &provider)
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
        ValuesSchemaInput::new(&schema_signals, &provider).with_values_yaml(Some(
            &test_util::read_testdata("charts/surveyor/values.yaml"),
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
}

#[test]
fn zalando_extra_envs_keeps_podspec_envvar_shape() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/zalando-postgres-operator/templates/_helpers.tpl",
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl"),
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml: serde_yaml::Value = serde_yaml::from_str(&test_util::read_testdata(
        "charts/zalando-postgres-operator/values.yaml",
    ))
    .expect("values yaml");
    let provider = production_chain_provider();

    let resolved =
        crate::path_resolver::PathSchemaResolver::new(&schema_signals, &values_yaml, &provider)
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
        ValuesSchemaInput::new(&schema_signals, &provider).with_values_yaml(Some(
            &test_util::read_testdata("charts/zalando-postgres-operator/values.yaml"),
        )),
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
        },
    ];

    let schema_signals = schema_signals_for(uses);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &RelabelingsProvider)
            .with_values_yaml(Some("serviceMonitor:\n  metricRelabelings: []\n")),
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
        },
        ContractUse {
            source_expr: "second".to_string(),
            path: YamlPath(vec!["spec".to_string(), "second".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: Some(resource),
            provenance: Vec::new(),
            has_string_contract: false,
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
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
    let uses = parse_ir(
        r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
        "#,
    );
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
    let src = indoc! {r#"
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
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("scheduler:\n  command: ~\n"));

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

/// Helm's YAML resolver reads hex, explicit octal, binary, and legacy
/// leading-zero spellings as integers, so a bare token in any of those
/// forms reparses away from the string a provider slot requires (velero's
/// unquoted BackupStorageLocation provider).
#[test]
fn plain_string_slot_excludes_non_decimal_integer_spellings() {
    let src = indoc! {r#"
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
    "#};
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

    let src = indoc! {r#"
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
    "#};
    let ir = parse_ir(src);
    let schema_signals = ir.into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &VolumeProvider)
            .with_values_yaml(Some("tmpVolume:\n  emptyDir: {}\n")),
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

    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          volumes:
            - name: config
              secret:
                secretName: {{ tpl .Values.objectName . }}
    "#};
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
