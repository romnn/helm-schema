//! Whole-chart semantic pins for runtime contracts found by the chart-corpus re-audit.
//!
//! Every case composes a sparse override over the vendored chart defaults. Rejected cases exercise
//! a Helm operation that terminates or emits invalid YAML, while accepted siblings keep the nearby
//! supported lane open so a broad type restriction cannot masquerade as a fix.
//!
//! A case is compared with the same chart's composed-default validation errors. This keeps an
//! unrelated umbrella-child defect from hiding whether the audited override added the expected
//! path-specific rejection.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

struct SemanticCase {
    label: &'static str,
    overrides: Value,
    accepted: bool,
    rejected_path: Option<&'static str>,
}

impl SemanticCase {
    fn accepted(label: &'static str, overrides: Value) -> Self {
        Self {
            label,
            overrides,
            accepted: true,
            rejected_path: None,
        }
    }

    fn rejected(label: &'static str, rejected_path: &'static str, overrides: Value) -> Self {
        Self {
            label,
            overrides,
            accepted: false,
            rejected_path: Some(rejected_path),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ValidationFailure {
    instance_path: String,
    message: String,
}

fn assert_chart_cases(chart: &str, cases: Vec<SemanticCase>) -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path(chart)?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| color_eyre::eyre::eyre!("compile {chart} schema: {error}"))?;
    let validation_errors = |instance: &Value| {
        validator
            .iter_errors(instance)
            .map(|error| ValidationFailure {
                instance_path: error.instance_path().to_string(),
                message: error.to_string(),
            })
            .collect::<BTreeSet<_>>()
    };
    let defaults = chart_instances::with_override(chart, json!({}))?;
    let baseline_errors = validation_errors(&defaults);
    let mut failures = Vec::new();

    for case in cases {
        let instance = chart_instances::with_override(chart, case.overrides)?;
        let errors = validation_errors(&instance);
        let additional_errors: Vec<_> = errors.difference(&baseline_errors).cloned().collect();
        if case.accepted {
            if !additional_errors.is_empty() {
                failures.push(format!(
                    "{} should validate, got new errors: {additional_errors:#?}",
                    case.label
                ));
            }
        } else {
            let rejected_path = case.rejected_path.expect("rejected case path");
            let path_prefix = format!("{rejected_path}/");
            if !additional_errors.iter().any(|error| {
                if rejected_path.is_empty() {
                    return error.instance_path.is_empty();
                }
                if error.instance_path.is_empty() {
                    return false;
                }
                let error_prefix = format!("{}/", error.instance_path);
                error.instance_path == rejected_path
                    || error.instance_path.starts_with(&path_prefix)
                    || rejected_path.starts_with(&error_prefix)
            }) {
                failures.push(format!(
                    "{} should be rejected at {rejected_path}; new errors were: {additional_errors:#?}",
                    case.label
                ));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("{chart}:\n{}", failures.join("\n")))
    }
}

#[test]
fn fluent_bit_extra_containers_rejects_scalar_complement() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "fluent-bit",
        vec![
            SemanticCase::rejected(
                "numeric extraContainers",
                "/extraContainers",
                json!({ "extraContainers": 7 }),
            ),
            SemanticCase::accepted(
                "templated extraContainers",
                json!({ "extraContainers": "- name: audit\n  image: busybox:1.36" }),
            ),
            SemanticCase::accepted(
                "structured extraContainers",
                json!({ "extraContainers": [{ "name": "audit", "image": "busybox:1.36" }] }),
            ),
        ],
    )
}

#[test]
fn minio_extra_containers_rejects_scalar_complement() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "minio",
        vec![
            SemanticCase::rejected(
                "numeric extraContainers",
                "/extraContainers",
                json!({ "extraContainers": 7 }),
            ),
            SemanticCase::accepted(
                "templated extraContainers",
                json!({ "extraContainers": "- name: audit\n  image: busybox:1.36" }),
            ),
            SemanticCase::accepted(
                "structured extraContainers",
                json!({ "extraContainers": [{ "name": "audit", "image": "busybox:1.36" }] }),
            ),
        ],
    )
}

#[test]
fn oauth2_proxy_consumers_follow_the_selected_helper_branch() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "oauth2-proxy",
        vec![
            SemanticCase::rejected(
                "map kubeVersion",
                "/kubeVersion",
                json!({ "kubeVersion": { "major": 1 } }),
            ),
            SemanticCase::accepted("string kubeVersion", json!({ "kubeVersion": "1.29.0" })),
            SemanticCase::accepted(
                "ignored map configFile in generated alpha mode",
                json!({
                    "alphaConfig": { "enabled": true },
                    "config": {
                        "forceLegacyConfig": false,
                        "configFile": { "ignored": true }
                    }
                }),
            ),
            SemanticCase::rejected(
                "map configFile in live inline-custom mode",
                "/config/configFile",
                json!({
                    "alphaConfig": { "enabled": true },
                    "config": {
                        "forceLegacyConfig": true,
                        "configFile": { "invalid": true }
                    }
                }),
            ),
            SemanticCase::accepted(
                "string configFile in live inline-custom mode",
                json!({
                    "alphaConfig": { "enabled": true },
                    "config": {
                        "forceLegacyConfig": true,
                        "configFile": "email_domains = [\"*\"]"
                    }
                }),
            ),
        ],
    )
}

#[test]
fn istiod_string_and_object_consumers_reject_wrong_kinds() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "istiod",
        vec![
            SemanticCase::rejected(
                "map remotePilotAddress",
                "/global/remotePilotAddress",
                json!({
                    "global": { "remotePilotAddress": { "host": "1.2.3.4" } },
                    "istiodRemote": { "enabled": true }
                }),
            ),
            SemanticCase::accepted(
                "string remotePilotAddress",
                json!({
                    "global": { "remotePilotAddress": "1.2.3.4" },
                    "istiodRemote": { "enabled": true }
                }),
            ),
            SemanticCase::rejected(
                "scalar gateways passed to pick",
                "/gateways",
                json!({ "gateways": 7 }),
            ),
            SemanticCase::accepted("default object gateways", json!({})),
        ],
    )
}

#[test]
fn istiod_constructed_tpl_selector_reaches_removed_value_fail() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "istiod",
        vec![
            SemanticCase::rejected(
                "truthy removed Stackdriver option",
                "",
                json!({
                    "telemetry": {
                        "v2": { "stackdriver": { "disableOutbound": true } }
                    }
                }),
            ),
            SemanticCase::accepted(
                "empty removed Stackdriver option",
                json!({
                    "telemetry": {
                        "v2": { "stackdriver": { "disableOutbound": "" } }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn vault_fullname_override_requires_a_string_when_truthy() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "vault",
        vec![
            SemanticCase::rejected(
                "truthy map fullnameOverride",
                "/fullnameOverride",
                json!({ "fullnameOverride": { "bad": true } }),
            ),
            SemanticCase::rejected(
                "truthy list fullnameOverride",
                "/fullnameOverride",
                json!({ "fullnameOverride": ["bad"] }),
            ),
            SemanticCase::accepted(
                "empty map fullnameOverride",
                json!({ "fullnameOverride": {} }),
            ),
            SemanticCase::accepted(
                "empty list fullnameOverride",
                json!({ "fullnameOverride": [] }),
            ),
            SemanticCase::rejected(
                "numeric fullnameOverride",
                "/fullnameOverride",
                json!({ "fullnameOverride": 7 }),
            ),
            SemanticCase::rejected(
                "boolean fullnameOverride",
                "/fullnameOverride",
                json!({ "fullnameOverride": true }),
            ),
            SemanticCase::accepted(
                "string fullnameOverride",
                json!({ "fullnameOverride": "vault-audit" }),
            ),
        ],
    )
}

#[test]
fn promtail_string_consumers_and_range_keys_keep_their_domains() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "promtail",
        vec![
            SemanticCase::rejected(
                "numeric image tag",
                "/image/tag",
                json!({ "image": { "tag": 7 } }),
            ),
            SemanticCase::rejected(
                "boolean image tag",
                "/image/tag",
                json!({ "image": { "tag": true } }),
            ),
            SemanticCase::accepted("string image tag", json!({ "image": { "tag": "3.0.0" } })),
            SemanticCase::rejected(
                "array extraPorts with integer range keys",
                "/extraPorts",
                json!({
                    "extraPorts": [{
                        "containerPort": 1514,
                        "service": { "port": 1514 }
                    }]
                }),
            ),
            SemanticCase::accepted(
                "map extraPorts with string range keys",
                json!({
                    "extraPorts": {
                        "syslog": {
                            "containerPort": 1514,
                            "service": { "port": 1514 }
                        }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn nfs_archive_on_delete_keeps_quoted_scalar_forms() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "nfs-subdir-external-provisioner",
        vec![
            SemanticCase::accepted(
                "string archiveOnDelete",
                json!({ "storageClass": { "archiveOnDelete": "false" } }),
            ),
            SemanticCase::accepted(
                "boolean archiveOnDelete",
                json!({ "storageClass": { "archiveOnDelete": false } }),
            ),
        ],
    )
}

#[test]
fn postgres_operator_extra_envs_requires_a_sequence() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "zalando-postgres-operator",
        vec![
            SemanticCase::rejected(
                "string extraEnvs",
                "/extraEnvs",
                json!({ "extraEnvs": "audit" }),
            ),
            SemanticCase::accepted(
                "EnvVar list",
                json!({ "extraEnvs": [{ "name": "AUDIT", "value": "true" }] }),
            ),
        ],
    )
}

#[test]
fn postgres_operator_ui_extra_envs_requires_a_sequence() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "zalando-postgres-operator-ui",
        vec![
            SemanticCase::rejected(
                "string extraEnvs",
                "/extraEnvs",
                json!({ "extraEnvs": "audit" }),
            ),
            SemanticCase::accepted(
                "EnvVar list",
                json!({ "extraEnvs": [{ "name": "AUDIT", "value": "true" }] }),
            ),
        ],
    )
}

#[test]
fn external_dns_affinity_preserves_the_with_skip_domain() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "external-dns",
        vec![
            SemanticCase::accepted(
                "false affinity skipped by with",
                json!({ "affinity": false }),
            ),
            SemanticCase::accepted("zero affinity skipped by with", json!({ "affinity": 0 })),
            SemanticCase::accepted("empty affinity skipped by with", json!({ "affinity": "" })),
            SemanticCase::rejected(
                "truthy string affinity",
                "/affinity",
                json!({ "affinity": "audit" }),
            ),
            SemanticCase::accepted(
                "object affinity",
                json!({ "affinity": { "podAntiAffinity": {} } }),
            ),
        ],
    )
}

#[test]
fn external_dns_helper_return_partitions_the_webhook_provider_contract()
-> color_eyre::eyre::Result<()> {
    let provider = |name: &str, pull_policy: Value| {
        json!({
            "provider": {
                "name": name,
                "webhook": {
                    "image": {
                        "repository": "example/webhook",
                        "tag": "1.0",
                        "pullPolicy": pull_policy
                    }
                }
            }
        })
    };
    assert_chart_cases(
        "external-dns",
        vec![
            SemanticCase::rejected(
                "active webhook with a non-string image pull policy",
                "/provider/webhook/image/pullPolicy",
                provider("webhook", json!(7)),
            ),
            SemanticCase::accepted("dormant webhook image policy", provider("aws", json!(7))),
            SemanticCase::accepted(
                "active webhook with a string image pull policy",
                provider("webhook", json!("IfNotPresent")),
            ),
        ],
    )
}

#[test]
fn grafana_nested_ranges_and_has_key_require_object_members() -> color_eyre::eyre::Result<()> {
    let dashboards = json!({ "default": { "audit": { "json": "{}" } } });
    assert_chart_cases(
        "grafana",
        vec![
            SemanticCase::rejected(
                "false dashboard provider member",
                "/dashboardProviders/dashboardproviders.yaml",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": { "dashboardproviders.yaml": false }
                }),
            ),
            SemanticCase::rejected(
                "zero dashboard provider member",
                "/dashboardProviders/dashboardproviders.yaml",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": { "dashboardproviders.yaml": 0 }
                }),
            ),
            SemanticCase::rejected(
                "empty dashboard provider member",
                "/dashboardProviders/dashboardproviders.yaml",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": { "dashboardproviders.yaml": "" }
                }),
            ),
            SemanticCase::rejected(
                "truthy dashboard provider member",
                "/dashboardProviders/dashboardproviders.yaml",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": { "dashboardproviders.yaml": "audit" }
                }),
            ),
            SemanticCase::rejected(
                "list dashboard provider member",
                "/dashboardProviders/dashboardproviders.yaml",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": { "dashboardproviders.yaml": [] }
                }),
            ),
            SemanticCase::accepted(
                "object dashboard provider member",
                json!({
                    "dashboards": dashboards.clone(),
                    "dashboardProviders": {
                        "dashboardproviders.yaml": {
                            "apiVersion": 1,
                            "providers": []
                        }
                    }
                }),
            ),
            SemanticCase::rejected(
                "scalar dashboard passed to hasKey",
                "/dashboards/default/audit",
                json!({ "dashboards": { "default": { "audit": 7 } } }),
            ),
            SemanticCase::accepted(
                "object dashboard passed to hasKey",
                json!({ "dashboards": dashboards }),
            ),
        ],
    )
}

#[test]
fn sealed_secrets_liveness_probe_keeps_its_object_type() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "sealed-secrets",
        vec![
            SemanticCase::rejected(
                "string livenessProbe",
                "/livenessProbe",
                json!({ "livenessProbe": "audit" }),
            ),
            SemanticCase::accepted(
                "object livenessProbe",
                json!({
                    "livenessProbe": {
                        "enabled": true,
                        "initialDelaySeconds": 0,
                        "periodSeconds": 10
                    }
                }),
            ),
        ],
    )
}

#[test]
fn jaeger_range_body_requires_string_arguments() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "jaeger",
        vec![
            SemanticCase::rejected(
                "integer jaeger.args",
                "/jaeger/args",
                json!({ "jaeger": { "args": 7 } }),
            ),
            SemanticCase::accepted(
                "string argument list",
                json!({ "jaeger": { "args": ["--query.base-path=/jaeger"] } }),
            ),
        ],
    )
}

#[test]
fn jenkins_range_body_requires_string_plugins() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "jenkins",
        vec![
            SemanticCase::rejected(
                "integer controller.installPlugins",
                "/controller/installPlugins",
                json!({ "controller": { "installPlugins": 7 } }),
            ),
            SemanticCase::accepted(
                "string plugin list",
                json!({ "controller": { "installPlugins": ["git:5.10.1"] } }),
            ),
        ],
    )
}

#[test]
fn velero_range_values_and_merge_operands_keep_their_domains() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "velero",
        vec![
            SemanticCase::rejected(
                "numeric credentials extraEnvVars member",
                "/credentials/extraEnvVars/TOKEN",
                json!({ "credentials": { "extraEnvVars": { "TOKEN": 7 } } }),
            ),
            SemanticCase::accepted(
                "string credentials extraEnvVars member",
                json!({ "credentials": { "extraEnvVars": { "TOKEN": "secret" } } }),
            ),
            SemanticCase::rejected(
                "string podSecurityContext passed to merge",
                "/podSecurityContext",
                json!({ "podSecurityContext": "audit" }),
            ),
            SemanticCase::accepted(
                "object podSecurityContext",
                json!({ "podSecurityContext": { "runAsNonRoot": true } }),
            ),
        ],
    )
}

/// Destination-first `merge podSecurityContext securityContext` gives the
/// preferred object's keys precedence, so a legacy member types exactly
/// where the preferred object lacks it.
#[test]
fn velero_merge_shadowing_scopes_legacy_security_context() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "velero",
        vec![
            SemanticCase::rejected(
                "active legacy runAsUser reaches the rendered pod context",
                "/securityContext/runAsUser",
                json!({ "securityContext": { "runAsUser": { "bad": true } } }),
            ),
            SemanticCase::accepted(
                "shadowed legacy runAsUser never renders",
                json!({
                    "podSecurityContext": { "runAsUser": 1000 },
                    "securityContext": { "runAsUser": { "bad": true } }
                }),
            ),
            SemanticCase::accepted(
                "integer legacy runAsUser renders valid",
                json!({ "securityContext": { "runAsUser": 1000 } }),
            ),
        ],
    )
}

#[test]
fn surveyor_range_members_and_intermediate_secrets_are_structural() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "surveyor",
        vec![
            SemanticCase::rejected(
                "scalar account array member",
                "/config/jetstream/accounts",
                json!({ "config": { "jetstream": { "enabled": true, "accounts": [7] } } }),
            ),
            SemanticCase::rejected(
                "scalar account map member",
                "/config/jetstream/accounts",
                json!({ "config": { "jetstream": { "enabled": true, "accounts": { "A": 7 } } } }),
            ),
            SemanticCase::accepted(
                "object account array member",
                json!({
                    "config": {
                        "jetstream": { "enabled": true, "accounts": [{ "name": "A" }] }
                    }
                }),
            ),
            SemanticCase::accepted(
                "object account map member",
                json!({
                    "config": {
                        "jetstream": {
                            "enabled": true,
                            "accounts": { "A": { "name": "A" } }
                        }
                    }
                }),
            ),
            SemanticCase::rejected(
                "credentials without intermediate secret",
                "/config/credentials",
                json!({ "config": { "credentials": { "audit": 1 } } }),
            ),
            SemanticCase::accepted(
                "credentials with intermediate secret",
                json!({
                    "config": {
                        "credentials": { "secret": { "name": "nats-creds", "key": "sys.creds" } }
                    }
                }),
            ),
            SemanticCase::rejected(
                "password without intermediate secret",
                "/config/password",
                json!({ "config": { "password": { "audit": 1 } } }),
            ),
            SemanticCase::accepted(
                "password with intermediate secret",
                json!({
                    "config": {
                        "password": { "secret": { "name": "nats-auth", "key": "password" } }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn harbor_string_comparisons_reject_composite_operands() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "harbor",
        vec![
            SemanticCase::rejected(
                "map logLevel",
                "/logLevel",
                json!({ "logLevel": { "level": "info" } }),
            ),
            SemanticCase::accepted("string logLevel", json!({ "logLevel": "info" })),
            SemanticCase::rejected(
                "string internal TLS selector",
                "/internalTLS/enabled",
                json!({ "internalTLS": { "enabled": "true" } }),
            ),
            SemanticCase::accepted(
                "Boolean internal TLS selector",
                json!({ "internalTLS": { "enabled": true } }),
            ),
        ],
    )
}

#[test]
fn signoz_comparisons_and_guarded_ranges_keep_exact_scope() -> color_eyre::eyre::Result<()> {
    let config = json!({
        "route": { "receiver": "audit" },
        "receivers": [{ "name": "audit" }]
    });
    assert_chart_cases(
        "signoz-signoz",
        vec![
            SemanticCase::rejected(
                "map global storageClass",
                "/global/storageClass",
                json!({ "global": { "storageClass": { "name": "standard" } } }),
            ),
            SemanticCase::rejected(
                "list global storageClass",
                "/global/storageClass",
                json!({ "global": { "storageClass": ["standard"] } }),
            ),
            SemanticCase::rejected(
                "numeric global storageClass",
                "/global/storageClass",
                json!({ "global": { "storageClass": 7 } }),
            ),
            SemanticCase::accepted(
                "string global storageClass",
                json!({ "global": { "storageClass": "standard" } }),
            ),
            SemanticCase::accepted(
                "string templates under disabled alertmanager",
                json!({
                    "alertmanager": {
                        "enabled": false,
                        "config": config.clone(),
                        "templates": "audit"
                    }
                }),
            ),
            SemanticCase::rejected(
                "string templates under live alertmanager range",
                "/alertmanager/templates",
                json!({
                    "alertmanager": {
                        "enabled": true,
                        "config": config.clone(),
                        "templates": "audit"
                    }
                }),
            ),
            SemanticCase::accepted(
                "map templates under live alertmanager range",
                json!({
                    "alertmanager": {
                        "enabled": true,
                        "config": config,
                        "templates": { "audit.tmpl": "audit" }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn kube_state_metrics_collection_consumers_keep_documented_unions() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "kube-state-metrics",
        vec![
            SemanticCase::rejected(
                "integer collectors",
                "/collectors",
                json!({ "collectors": 7 }),
            ),
            SemanticCase::accepted("collector list", json!({ "collectors": ["pods"] })),
            SemanticCase::accepted(
                "namespace list",
                json!({ "namespaces": ["default", "kube-system"] }),
            ),
            SemanticCase::accepted(
                "comma-separated namespaces",
                json!({ "namespaces": "default,kube-system" }),
            ),
        ],
    )
}

#[test]
fn kube_prometheus_stack_must_uniq_requires_a_list() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kube-prometheus-stack",
        vec![
            SemanticCase::rejected(
                "scalar denyNamespaces",
                "/prometheusOperator/denyNamespaces",
                json!({ "prometheusOperator": { "denyNamespaces": 7 } }),
            ),
            SemanticCase::accepted(
                "denyNamespaces list",
                json!({ "prometheusOperator": { "denyNamespaces": ["kube-system"] } }),
            ),
        ],
    )
}

#[test]
fn argo_cd_concat_rejects_all_non_list_operands() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "argo-cd",
        vec![
            SemanticCase::rejected(
                "false global.env",
                "/global/env",
                json!({ "global": { "env": false } }),
            ),
            SemanticCase::rejected(
                "zero global.env",
                "/global/env",
                json!({ "global": { "env": 0 } }),
            ),
            SemanticCase::rejected(
                "empty global.env",
                "/global/env",
                json!({ "global": { "env": "" } }),
            ),
            SemanticCase::rejected(
                "false controller.env",
                "/controller/env",
                json!({ "controller": { "env": false } }),
            ),
            SemanticCase::accepted(
                "EnvVar arrays",
                json!({
                    "global": { "env": [{ "name": "GLOBAL_AUDIT", "value": "true" }] },
                    "controller": { "env": [{ "name": "AUDIT", "value": "true" }] }
                }),
            ),
        ],
    )
}

#[test]
fn cloudnative_pg_merge_rejects_falsy_wrong_kinds_when_live() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cloudnative-pg",
        vec![
            SemanticCase::rejected(
                "false config.data under namespaced merge",
                "/config/data",
                json!({ "config": { "clusterWide": false, "data": false } }),
            ),
            SemanticCase::rejected(
                "zero config.data under namespaced merge",
                "/config/data",
                json!({ "config": { "clusterWide": false, "data": 0 } }),
            ),
            SemanticCase::rejected(
                "empty config.data under namespaced merge",
                "/config/data",
                json!({ "config": { "clusterWide": false, "data": "" } }),
            ),
            SemanticCase::accepted(
                "object config.data under namespaced merge",
                json!({
                    "config": {
                        "clusterWide": false,
                        "data": { "INHERITED_LABELS": "environment" }
                    }
                }),
            ),
            SemanticCase::accepted(
                "object config.data under cluster-wide serialization",
                json!({
                    "config": {
                        "clusterWide": true,
                        "data": { "INHERITED_LABELS": "environment" }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn airflow_webserver_contract_binds_under_version_guard() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "map base_url with Airflow 3 webserver branch disabled",
                json!({
                    "airflowVersion": "3.2.2",
                    "config": { "webserver": { "base_url": { "host": "airflow" } } }
                }),
            ),
            SemanticCase::accepted(
                "URL base_url with Airflow 2 webserver branch live",
                json!({
                    "airflowVersion": "2.11.0",
                    "config": { "webserver": { "base_url": "https://airflow.example.com" } }
                }),
            ),
            SemanticCase::rejected(
                "map base_url with Airflow 2 webserver branch live",
                "/config/webserver/base_url",
                json!({
                    "airflowVersion": "2.11.0",
                    "config": { "webserver": { "base_url": { "host": "airflow" } } }
                }),
            ),
        ],
    )
}

#[test]
fn airflow_version_requires_semver_lexical_form() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected(
                "invalid airflowVersion",
                "/airflowVersion",
                json!({ "airflowVersion": "garbage" }),
            ),
            SemanticCase::accepted("valid airflowVersion", json!({ "airflowVersion": "3.2.2" })),
        ],
    )
}

#[test]
fn airflow_security_context_priority_keeps_dormant_fallbacks_open() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "deprecated scalar context is dormant behind the preferred pod context",
                json!({
                    "workers": {
                        "securityContexts": { "pod": { "runAsUser": 50000 } },
                        "securityContext": 7
                    }
                }),
            ),
            SemanticCase::accepted(
                "deprecated object context remains a valid live fallback",
                json!({
                    "workers": {
                        "securityContexts": { "pod": {} },
                        "securityContext": { "runAsUser": 50000 }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn postgres_operator_ui_team_elements_are_formatted_as_text() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "zalando-postgres-operator-ui",
        vec![
            SemanticCase::accepted("numeric teams", json!({ "envs": { "teams": [7, 8] } })),
            SemanticCase::accepted(
                "structured teams",
                json!({ "envs": { "teams": [{ "name": "acid" }] } }),
            ),
            SemanticCase::accepted(
                "ordinary string teams",
                json!({ "envs": { "teams": ["acid"] } }),
            ),
        ],
    )
}

#[test]
fn kyverno_image_pull_secret_keys_require_a_map() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kyverno",
        vec![
            SemanticCase::rejected(
                "array passed to keys",
                "/imagePullSecrets",
                json!({ "imagePullSecrets": [{ "name": "regcred" }] }),
            ),
            SemanticCase::accepted(
                "map passed to keys",
                json!({
                    "imagePullSecrets": {
                        "regcred": {
                            "registry": "registry.example.com",
                            "username": "audit",
                            "password": "secret"
                        }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn bitnami_redis_ternary_selector_requires_boolean() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "bitnami-redis",
        vec![
            SemanticCase::rejected(
                "string auth enabled",
                "/auth/enabled",
                json!({ "auth": { "enabled": "true" } }),
            ),
            SemanticCase::accepted(
                "Boolean auth enabled",
                json!({ "auth": { "enabled": true } }),
            ),
        ],
    )
}

/// traefik aborts on any `ingressRoute` key containing an uppercase
/// character (RFC 1123 resource names); the key domain lowers to
/// `propertyNames`.
#[test]
fn traefik_ingress_route_keys_must_be_lowercase() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::rejected(
                "uppercase ingressRoute key",
                "/ingressRoute",
                json!({ "ingressRoute": { "Audit": { "enabled": false } } }),
            ),
            SemanticCase::accepted(
                "lowercase ingressRoute key",
                json!({ "ingressRoute": { "audit": { "enabled": false } } }),
            ),
        ],
    )
}

/// sealed-secrets' ranged annotation/label members abort on any value that
/// is not a TRUTHY string — the empty string is falsy and takes the `fail`
/// arm just like a non-string.
#[test]
fn sealed_secrets_private_key_metadata_members_must_be_truthy_strings()
-> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "sealed-secrets",
        vec![
            SemanticCase::rejected(
                "empty-string annotation member",
                "/privateKeyAnnotations",
                json!({ "privateKeyAnnotations": { "audit": "" } }),
            ),
            SemanticCase::rejected(
                "numeric label member",
                "/privateKeyLabels",
                json!({ "privateKeyLabels": { "audit": 7 } }),
            ),
            SemanticCase::accepted(
                "truthy string members",
                json!({
                    "privateKeyAnnotations": { "audit": "ok" },
                    "privateKeyLabels": { "audit": "ok" }
                }),
            ),
        ],
    )
}

/// cilium forbids `extraEnv` entries named after its backoff variables while
/// `k8sClientExponentialBackoff` (default-enabled) is live; disabling the
/// feature reopens the names.
#[test]
fn cilium_backoff_env_name_collisions_are_rejected_while_live() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "colliding env name under the default-enabled backoff",
                "/extraEnv",
                json!({ "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }] }),
            ),
            SemanticCase::accepted(
                "unrelated env name",
                json!({ "extraEnv": [{ "name": "AUDIT", "value": "1" }] }),
            ),
            SemanticCase::accepted(
                "colliding env name with the backoff disabled",
                json!({
                    "k8sClientExponentialBackoff": { "enabled": false },
                    "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }]
                }),
            ),
        ],
    )
}

/// A truthy non-string ACL password reaches `sha256sum` inside the ranged
/// users body and aborts rendering; a string hashes, and every Helm-falsy
/// spelling escapes to the `nopass` arm through the `default ""` local.
#[test]
fn bitnami_redis_acl_passwords_reaching_sha256sum_must_be_strings() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "bitnami-redis",
        vec![
            SemanticCase::rejected(
                "numeric ACL user password",
                "/auth/acl/users/0/password",
                json!({
                    "auth": {
                        "acl": {
                            "enabled": true,
                            "users": [{ "username": "audit", "password": 7 }]
                        }
                    }
                }),
            ),
            SemanticCase::accepted(
                "string ACL user password",
                json!({
                    "auth": {
                        "acl": {
                            "enabled": true,
                            "users": [{ "username": "audit", "password": "s3cret" }]
                        }
                    }
                }),
            ),
            SemanticCase::accepted(
                "passwordless ACL user selects nopass",
                json!({
                    "auth": {
                        "acl": {
                            "enabled": true,
                            "users": [{ "username": "audit" }]
                        }
                    }
                }),
            ),
        ],
    )
}

/// Helm coalesces LISTS atomically: a replacement list's members reach the
/// template verbatim, so a `enabled: null` member must survive instance
/// composition and validate against the nil-tolerant comparison operand
/// (cilium's `ne $cluster.enabled false` clustermesh arm).
#[test]
fn cilium_replacement_list_members_keep_literal_nulls() -> color_eyre::eyre::Result<()> {
    let composed = chart_instances::with_override(
        "cilium",
        json!({
            "clustermesh": {
                "config": {
                    "clusters": [{ "name": "c1", "enabled": null, "ips": ["1.1.1.1"] }]
                }
            }
        }),
    )?;
    sim_assert_eq!(
        have: composed.pointer("/clustermesh/config/clusters/0/enabled"),
        want: Some(&Value::Null),
        "the compositor must not scrub nulls inside replacement lists"
    );
    assert_chart_cases(
        "cilium",
        vec![SemanticCase::accepted(
            "null enabled member of a replacement cluster list",
            json!({
                "clustermesh": {
                    "config": {
                        "clusters": [{ "name": "c1", "enabled": null, "ips": ["1.1.1.1"] }]
                    }
                }
            }),
        )],
    )
}

/// A null override composes through Helm's map-key deletion, so the
/// accepted instance validates the key's ABSENCE (the release-namespace
/// fallback), not a literal null at the property.
#[test]
fn kube_state_metrics_null_namespace_override_composes_to_absent_key()
-> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kube-state-metrics",
        vec![
            SemanticCase::accepted(
                "null namespaceOverride deletes the key and selects the release namespace",
                json!({ "namespaceOverride": null }),
            ),
            SemanticCase::accepted(
                "string namespaceOverride",
                json!({ "namespaceOverride": "audit" }),
            ),
        ],
    )
}

#[test]
fn nats_operator_executes_file_backed_client_auth_template() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "nats-operator",
        vec![
            SemanticCase::rejected(
                "numeric auth user",
                "/cluster/auth/users/0",
                json!({ "cluster": { "auth": { "users": [7] } } }),
            ),
            SemanticCase::accepted(
                "credential auth user",
                json!({
                    "cluster": {
                        "auth": {
                            "users": [{ "username": "audit", "password": "secret" }]
                        }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn minio_executes_base_path_template_partials() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "minio",
        vec![
            SemanticCase::rejected("numeric bucket", "/buckets/0", json!({ "buckets": [7] })),
            SemanticCase::accepted("named bucket", json!({ "buckets": [{ "name": "audit" }] })),
        ],
    )
}

#[test]
fn sealed_secrets_helper_propagates_semver_domain() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "sealed-secrets",
        vec![
            SemanticCase::rejected(
                "invalid helper-propagated kubeVersion",
                "/kubeVersion",
                json!({ "kubeVersion": "garbage" }),
            ),
            SemanticCase::accepted(
                "valid helper-propagated kubeVersion",
                json!({ "kubeVersion": "v1.30.0" }),
            ),
        ],
    )
}

#[test]
fn cilium_duration_parser_keeps_its_lexical_domain() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "invalid conntrack duration",
                "/conntrackGCInterval",
                json!({ "conntrackGCInterval": "garbage" }),
            ),
            SemanticCase::accepted(
                "valid conntrack duration",
                json!({ "conntrackGCInterval": "30s" }),
            ),
        ],
    )
}

#[test]
fn traefik_helper_propagates_semver_domain() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::rejected(
                "invalid versionOverride",
                "/versionOverride",
                json!({ "versionOverride": "garbage" }),
            ),
            SemanticCase::accepted(
                "valid versionOverride",
                json!({ "versionOverride": "v3.7.6" }),
            ),
        ],
    )
}

#[test]
fn urlquery_total_conversion_accepts_structured_passwords() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "map metadata password",
                json!({ "data": { "metadataConnection": { "pass": { "key": "value" } } } }),
            ),
            SemanticCase::accepted(
                "list metadata password",
                json!({ "data": { "metadataConnection": { "pass": ["value"] } } }),
            ),
            SemanticCase::accepted(
                "string metadata password",
                json!({ "data": { "metadataConnection": { "pass": "secret" } } }),
            ),
        ],
    )
}

#[test]
fn vault_object_selector_keeps_or_selected_shape_alternatives() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "vault",
        vec![
            SemanticCase::accepted(
                "preferred structured object selector",
                json!({
                    "injector": {
                        "webhook": {
                            "objectSelector": { "matchLabels": { "audit": "true" } }
                        }
                    }
                }),
            ),
            SemanticCase::accepted(
                "preferred templated object selector",
                json!({
                    "injector": {
                        "webhook": {
                            "objectSelector": "matchLabels:\n  audit: {{ .Release.Name | quote }}"
                        }
                    }
                }),
            ),
            SemanticCase::accepted(
                "legacy templated object selector",
                json!({
                    "injector": {
                        "webhook": { "objectSelector": "" },
                        "objectSelector": "matchLabels:\n  audit: legacy"
                    }
                }),
            ),
        ],
    )
}

#[test]
fn vault_quoted_external_address_accepts_total_textual_forms() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "vault",
        vec![
            SemanticCase::accepted(
                "mapping external Vault address",
                json!({ "global": { "externalVaultAddr": { "host": "vault" } } }),
            ),
            SemanticCase::accepted(
                "list external Vault address",
                json!({ "global": { "externalVaultAddr": ["vault"] } }),
            ),
            SemanticCase::accepted(
                "string external Vault address",
                json!({ "global": { "externalVaultAddr": "https://vault.example" } }),
            ),
        ],
    )
}

#[test]
fn prometheus_namespace_text_pipeline_accepts_structured_inputs() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "prometheus",
        vec![
            SemanticCase::accepted(
                "mapping namespace input",
                json!({
                    "rbac": { "create": true },
                    "server": {
                        "namespaces": { "audit": "namespace" },
                        "useExistingClusterRoleName": "audit"
                    }
                }),
            ),
            SemanticCase::accepted(
                "list namespace input",
                json!({
                    "rbac": { "create": true },
                    "server": {
                        "namespaces": ["default", "kube-system"],
                        "useExistingClusterRoleName": "audit"
                    }
                }),
            ),
        ],
    )
}

#[test]
fn trivy_ignore_policy_prefix_requires_string_values() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "trivy-operator",
        vec![
            SemanticCase::rejected(
                "mapping ignore policy",
                "/trivy/ignorePolicy",
                json!({ "trivy": { "ignorePolicy": { "bad": true } } }),
            ),
            SemanticCase::accepted(
                "string ignore policy",
                json!({ "trivy": { "ignorePolicy": "package trivy" } }),
            ),
            SemanticCase::accepted(
                "unrelated dynamic map member",
                json!({ "trivy": { "unrelatedAudit": { "bad": true } } }),
            ),
        ],
    )
}

#[test]
fn traefik_invalid_kind_guard_preserves_falsy_present_values() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::accepted(
                "falsy present hostUsers",
                json!({ "deployment": { "hostUsers": false } }),
            ),
            SemanticCase::accepted(
                "truthy Boolean hostUsers",
                json!({ "deployment": { "hostUsers": true } }),
            ),
        ],
    )
}

#[test]
fn cilium_certificate_sans_require_string_members() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "numeric Hubble IP SAN",
                "/hubble/tls/server/extraIpAddresses/0",
                json!({
                    "hubble": { "tls": { "server": { "extraIpAddresses": [7] } } }
                }),
            ),
            SemanticCase::rejected(
                "numeric Hubble DNS SAN",
                "/hubble/tls/server/extraDnsNames/0",
                json!({
                    "hubble": { "tls": { "server": { "extraDnsNames": [7] } } }
                }),
            ),
            SemanticCase::rejected(
                "non-address Hubble IP SAN",
                "/hubble/tls/server/extraIpAddresses/0",
                json!({
                    "hubble": { "tls": { "server": { "extraIpAddresses": ["not-an-ip"] } } }
                }),
            ),
            SemanticCase::accepted(
                "string Hubble SANs",
                json!({
                    "hubble": {
                        "tls": {
                            "server": {
                                "extraIpAddresses": ["10.0.0.7", "2001:db8::1"],
                                "extraDnsNames": ["audit.example"]
                            }
                        }
                    }
                }),
            ),
        ],
    )
}

#[test]
fn loki_minio_user_index_requires_a_first_user_when_live() -> color_eyre::eyre::Result<()> {
    let live_enterprise = json!({
        "enterprise": { "enabled": true },
        "loki": {
            "storage": {
                "bucketNames": { "admin": "admin", "chunks": "chunks", "ruler": "ruler" }
            },
            "useTestSchema": true
        },
        "minio": { "enabled": true }
    });
    assert_chart_cases(
        "loki",
        vec![
            SemanticCase::rejected(
                "empty MinIO users in live enterprise gateway",
                "/minio/users",
                json!({
                    "enterprise": { "enabled": true },
                    "loki": {
                        "storage": {
                            "bucketNames": {
                                "admin": "admin",
                                "chunks": "chunks",
                                "ruler": "ruler"
                            }
                        },
                        "useTestSchema": true
                    },
                    "minio": { "enabled": true, "users": [] }
                }),
            ),
            SemanticCase::accepted("first MinIO user available", live_enterprise),
            SemanticCase::accepted(
                "empty MinIO users while enterprise gateway is dormant",
                json!({
                    "loki": { "useTestSchema": true },
                    "minio": { "enabled": false, "users": [] }
                }),
            ),
        ],
    )
}

#[test]
fn loki_chart_authored_htpasswd_program_requires_selected_credentials()
-> color_eyre::eyre::Result<()> {
    let live = |basic_auth: Value| {
        json!({
            "gateway": { "basicAuth": basic_auth },
            "loki": {
                "storage": {
                    "bucketNames": {
                        "admin": "admin",
                        "chunks": "chunks",
                        "ruler": "ruler"
                    }
                },
                "useTestSchema": true
            }
        })
    };
    assert_chart_cases(
        "loki",
        vec![
            SemanticCase::rejected(
                "selected default htpasswd program without username",
                "",
                live(json!({ "enabled": true, "password": "pass" })),
            ),
            SemanticCase::rejected(
                "selected default htpasswd program without password",
                "",
                live(json!({ "enabled": true, "username": "user" })),
            ),
            SemanticCase::accepted(
                "selected default htpasswd program with credentials",
                live(json!({
                    "enabled": true,
                    "username": "user",
                    "password": "pass"
                })),
            ),
            SemanticCase::accepted(
                "literal htpasswd override replaces the default program",
                live(json!({ "enabled": true, "htpasswd": "audit:hash" })),
            ),
        ],
    )
}

#[test]
fn coredns_prometheus_address_requires_a_host_port_split() -> color_eyre::eyre::Result<()> {
    let server = |parameters| {
        json!([{
            "zones": [{ "zone": "." }],
            "port": 53,
            "plugins": [{ "name": "prometheus", "parameters": parameters }]
        }])
    };
    assert_chart_cases(
        "coredns",
        vec![
            SemanticCase::rejected(
                "one-segment Prometheus address",
                "/servers/0/plugins/0/parameters",
                json!({ "servers": server("9153") }),
            ),
            SemanticCase::accepted(
                "host and port Prometheus address",
                json!({ "servers": server("0.0.0.0:9153") }),
            ),
        ],
    )
}

#[test]
fn falco_removed_config_accumulator_forbids_present_legacy_keys() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "falco",
        vec![
            SemanticCase::rejected(
                "empty removed driver key is still present",
                "",
                json!({ "driver": { "ebpf": {} } }),
            ),
            SemanticCase::rejected(
                "falsy removed Falco key is still present",
                "",
                json!({ "falco": { "grpc": false } }),
            ),
            SemanticCase::accepted("removed keys absent", json!({})),
        ],
    )
}

#[test]
fn jaeger_http_route_validator_requires_a_parent_reference() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "jaeger",
        vec![
            SemanticCase::rejected(
                "enabled HTTPRoute without parent references",
                "/jaeger",
                json!({ "jaeger": { "httproute": { "enabled": true, "parentRefs": [] } } }),
            ),
            SemanticCase::accepted(
                "enabled HTTPRoute with a parent reference",
                json!({
                    "jaeger": {
                        "httproute": {
                            "enabled": true,
                            "parentRefs": [{ "name": "gateway" }]
                        }
                    }
                }),
            ),
        ],
    )
}

/// flux2's shared `template.image` helper slices every controller tag
/// with `substr 0 7` before comparing against `sha256:`, so a non-string tag
/// terminates rendering while ordinary and digest tags render.
#[test]
fn flux2_controller_tags_require_substr_string_subjects() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "flux2",
        vec![
            SemanticCase::rejected(
                "mapping kustomize-controller tag",
                "/kustomizeController/tag",
                json!({ "kustomizeController": { "tag": { "bad": true } } }),
            ),
            SemanticCase::rejected("list CLI tag", "/cli/tag", json!({ "cli": { "tag": [1] } })),
            SemanticCase::accepted(
                "plain version tag",
                json!({ "kustomizeController": { "tag": "v1.2.3" } }),
            ),
            SemanticCase::accepted("digest tag", json!({ "cli": { "tag": "sha256:0123abcd" } })),
        ],
    )
}

/// `semverCompare ">=X" (default "X" .Values.upgradeCompatibility)`
/// only parses the raw value on its truthy arm. Every Helm-empty input
/// selects the literal fallback and renders, while a truthy non-semver
/// input still terminates inside the parser.
#[test]
fn cilium_upgrade_compatibility_keeps_helm_empty_inputs_open() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::accepted(
                "false selects the fallback",
                json!({ "upgradeCompatibility": false }),
            ),
            SemanticCase::accepted(
                "empty map selects the fallback",
                json!({ "upgradeCompatibility": {} }),
            ),
            SemanticCase::accepted(
                "plain version string",
                json!({ "upgradeCompatibility": "1.14" }),
            ),
            SemanticCase::rejected(
                "truthy non-semver string",
                "/upgradeCompatibility",
                json!({ "upgradeCompatibility": "garbage" }),
            ),
            SemanticCase::rejected(
                "truthy map reaches the parser",
                "/upgradeCompatibility",
                json!({ "upgradeCompatibility": { "a": 1 } }),
            ),
        ],
    )
}

/// cloudnative-pg selects `default .Chart.Name .Values.nameOverride`
/// before `trunc`/`contains`, and reads `namespaceOverride` only inside its
/// own truthy `if`; Helm-empty overrides substitute or skip and render.
#[test]
fn cloudnative_pg_override_fallbacks_keep_helm_empty_inputs_open() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cloudnative-pg",
        vec![
            SemanticCase::accepted("false name override", json!({ "nameOverride": false })),
            SemanticCase::accepted("empty map name override", json!({ "nameOverride": {} })),
            SemanticCase::accepted("string name override", json!({ "nameOverride": "custom" })),
            SemanticCase::rejected(
                "truthy map survives selection and aborts trunc",
                "/nameOverride",
                json!({ "nameOverride": { "a": 1 } }),
            ),
            SemanticCase::accepted(
                "false namespace override is skipped",
                json!({ "namespaceOverride": false }),
            ),
            SemanticCase::accepted(
                "string namespace override",
                json!({ "namespaceOverride": "ns" }),
            ),
        ],
    )
}

/// the master template's direct `ternary "no" "yes" .Values.auth.enabled`
/// call sits under `gt (int64 .Values.master.count) 0` plus the
/// standalone/sentinel partition. The Boolean contract must hold on BOTH
/// architecture arms, while scaling the master to zero keeps the whole
/// partition dead.
#[test]
fn bitnami_redis_auth_ternary_holds_across_architecture_partitions() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "bitnami-redis",
        vec![
            SemanticCase::rejected(
                "replication string auth.enabled",
                "/auth/enabled",
                json!({ "auth": { "enabled": "true" } }),
            ),
            SemanticCase::rejected(
                "standalone string auth.enabled",
                "/auth/enabled",
                json!({ "architecture": "standalone", "auth": { "enabled": "true" } }),
            ),
            SemanticCase::accepted(
                "standalone Boolean auth.enabled",
                json!({ "architecture": "standalone", "auth": { "enabled": true } }),
            ),
            SemanticCase::accepted(
                "replication Boolean auth.enabled",
                json!({ "auth": { "enabled": true } }),
            ),
            SemanticCase::accepted(
                "scaled-to-zero master never runs the ternary",
                json!({
                    "architecture": "standalone",
                    "master": { "count": 0 },
                    "auth": { "enabled": "true" }
                }),
            ),
        ],
    )
}

/// velero ranges `.Values.schedules` and emits each member as a
/// `velero.io/v1 Schedule`, so the chart-local CRD's member schema applies
/// through `additionalProperties` to every (arbitrarily named) entry.
#[test]
fn velero_schedule_members_carry_the_crd_member_schema() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "velero",
        vec![
            SemanticCase::rejected(
                "string paused on a ranged member",
                "/schedules/audit/paused",
                json!({ "schedules": { "audit": { "schedule": "0 0 * * *", "paused": "audit" } } }),
            ),
            SemanticCase::accepted(
                "Boolean paused on a ranged member",
                json!({ "schedules": { "audit": { "schedule": "0 0 * * *", "paused": true } } }),
            ),
            SemanticCase::rejected(
                "invalid hook onError enum",
                "/schedules/audit/template",
                json!({
                    "schedules": { "audit": { "schedule": "0 0 * * *", "template": {
                        "hooks": { "resources": [{ "name": "h", "post": [{
                            "exec": { "command": ["/bin/x"], "onError": "audit" }
                        }] }] }
                    } } }
                }),
            ),
            SemanticCase::accepted(
                "valid hook onError enum",
                json!({
                    "schedules": { "audit": { "schedule": "0 0 * * *", "template": {
                        "hooks": { "resources": [{ "name": "h", "post": [{
                            "exec": { "command": ["/bin/x"], "onError": "Fail" }
                        }] }] }
                    } } }
                }),
            ),
            SemanticCase::accepted(
                "disabled member skips the document",
                json!({ "schedules": { "audit": { "disabled": true, "paused": "audit" } } }),
            ),
        ],
    )
}

/// `extraEnvConfigMaps` is a user-keyed map the deployment destructures
/// (`range $key, $value`), so a complete member renders while the member
/// contract still enforces the `required "Must specify key!"` read and the
/// structural `.name` access.
#[test]
fn cluster_autoscaler_env_config_map_members_stay_open_with_contracts()
-> color_eyre::eyre::Result<()> {
    let live = json!({ "autoDiscovery": { "clusterName": "audit-cluster" } });
    let merge = |extra: serde_json::Value| {
        let mut base = live.clone();
        base.as_object_mut()
            .expect("object")
            .insert("extraEnvConfigMaps".to_string(), extra);
        base
    };
    assert_chart_cases(
        "cluster-autoscaler",
        vec![
            SemanticCase::accepted(
                "complete dynamic member",
                merge(json!({ "AUDIT": { "name": "cfg", "key": "value" } })),
            ),
            SemanticCase::rejected(
                "member missing the required key",
                "/extraEnvConfigMaps",
                merge(json!({ "AUDIT": { "name": "cfg" } })),
            ),
            SemanticCase::rejected(
                "scalar member cannot host field reads",
                "/extraEnvConfigMaps",
                merge(json!({ "AUDIT": 7 })),
            ),
        ],
    )
}

/// every `server.remoteWrite` member's `url` reaches `tpl` (a strict
/// string consumer) inside the serverFiles dispatch, so a non-string URL
/// terminates rendering while string URLs pass.
#[test]
fn prometheus_remote_write_urls_require_strings() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "prometheus",
        vec![
            SemanticCase::rejected(
                "numeric remoteWrite url",
                "/server/remoteWrite",
                json!({ "server": { "remoteWrite": [{ "url": 7 }] } }),
            ),
            SemanticCase::rejected(
                "member missing url",
                "/server/remoteWrite",
                json!({ "server": { "remoteWrite": [{ "name": "x" }] } }),
            ),
            SemanticCase::accepted(
                "string remoteWrite url",
                json!({ "server": { "remoteWrite": [{ "url": "http://mimir/push" }] } }),
            ),
        ],
    )
}

/// datadog's `check-dca-version` helper converts the exact tag
/// `latest` to `1.20.0` before `semverCompare`, so the raw-input contract
/// accepts the sentinel while other lexically invalid tags still terminate
/// rendering. `doNotCheckTag` disables the whole check.
#[test]
fn datadog_cluster_agent_latest_tag_survives_the_version_check() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "datadog",
        vec![
            SemanticCase::accepted(
                "the latest sentinel is reassigned before parsing",
                json!({ "clusterAgent": { "image": { "tag": "latest" } } }),
            ),
            SemanticCase::accepted(
                "ordinary version tag",
                json!({ "clusterAgent": { "image": { "tag": "1.26.0" } } }),
            ),
            SemanticCase::rejected(
                "non-sentinel invalid version",
                "/clusterAgent/image/tag",
                json!({ "clusterAgent": { "image": { "tag": "garbage" } } }),
            ),
            SemanticCase::accepted(
                "doNotCheckTag skips the check entirely",
                json!({ "clusterAgent": { "image": { "tag": "garbage", "doNotCheckTag": true } } }),
            ),
        ],
    )
}

/// traefik's `traefik.proxyVersion` helper strips the documented
/// `latest-`/`experimental-` prefixes, replaces `master` with the chart's
/// appVersion, and trims any `@digest` suffix before its version checks, so
/// those raw forms render while an untouched non-version string still
/// terminates.
#[test]
fn traefik_transformed_tag_sentinels_survive_the_version_checks() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::accepted(
                "latest- prefixed version",
                json!({ "image": { "tag": "latest-v3.6.0" } }),
            ),
            SemanticCase::accepted(
                "experimental- prefixed version",
                json!({ "image": { "tag": "experimental-v3.6.0" } }),
            ),
            SemanticCase::accepted(
                "master appVersion sentinel",
                json!({ "image": { "tag": "master" } }),
            ),
            SemanticCase::accepted(
                "digest-suffixed version",
                json!({ "image": { "tag": "v3.6.0@sha256:0123abcd" } }),
            ),
            SemanticCase::accepted("plain version", json!({ "image": { "tag": "v3.6.0" } })),
            SemanticCase::rejected(
                "bare latest is not stripped",
                "/image/tag",
                json!({ "image": { "tag": "latest" } }),
            ),
            SemanticCase::rejected(
                "untouched non-version string",
                "/image/tag",
                json!({ "image": { "tag": "audit" } }),
            ),
        ],
    )
}

/// zalando's operator manually double-quotes its assembled image
/// scalar (`image: "{{ .registry }}/{{ .repository }}:{{ .tag }}"`), so a
/// raw `"` inside any component corrupts the completed quoted token while
/// ordinary strings and numbers format safely.
#[test]
fn zalando_operator_quoted_image_scalar_excludes_raw_quotes() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "zalando-postgres-operator",
        vec![
            SemanticCase::rejected(
                "registry with embedded double quote",
                "/image/registry",
                json!({ "image": { "registry": "bad\"quote" } }),
            ),
            SemanticCase::rejected(
                "registry with backslash escape",
                "/image/registry",
                json!({ "image": { "registry": "back\\slash" } }),
            ),
            SemanticCase::accepted(
                "ordinary registry",
                json!({ "image": { "registry": "ghcr.io" } }),
            ),
            SemanticCase::accepted("numeric registry", json!({ "image": { "registry": 7 } })),
        ],
    )
}

/// tempo assembles `image: {{ .registry }}/{{ .repository }}:{{ .tag }}`
/// unquoted, so a list registry opens a flow sequence at the token start and
/// breaks the final YAML; maps and strings format as plain text.
#[test]
fn tempo_assembled_image_scalar_excludes_token_initial_lists() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "tempo",
        vec![
            SemanticCase::rejected(
                "list registry",
                "/tempo/registry",
                json!({ "tempo": { "registry": ["a", "b"] } }),
            ),
            SemanticCase::rejected(
                "empty list registry",
                "/tempo/registry",
                json!({ "tempo": { "registry": [] } }),
            ),
            SemanticCase::accepted(
                "plain registry",
                json!({ "tempo": { "registry": "docker.io" } }),
            ),
            SemanticCase::accepted(
                "map registry renders as plain text",
                json!({ "tempo": { "registry": { "a": "b" } } }),
            ),
        ],
    )
}

/// flux2 embeds `logLevel` after the literal `--log-level=` prefix in
/// every controller command, so Helm totally formats any value into one
/// argument string; the `default "info"` fallback documents intent without
/// constraining the input kind.
#[test]
fn flux2_prefixed_log_level_accepts_every_input_kind() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "flux2",
        vec![
            SemanticCase::accepted("map log level", json!({ "logLevel": { "a": "b" } })),
            SemanticCase::accepted("list log level", json!({ "logLevel": ["a"] })),
            SemanticCase::accepted("false selects the default", json!({ "logLevel": false })),
            SemanticCase::accepted("plain log level", json!({ "logLevel": "info" })),
        ],
    )
}

/// Adjudication: a re-audit claimed aws-load-balancer-controller's
/// `nameOverride: "null"` should validate, but rendering it produces
/// `app.kubernetes.io/name: null` on every resource and the v1.35.0 strict
/// schemas reject a null label value (`labels.additionalProperties` is
/// `string`), so the plain-token exclusion correctly keeps rejecting it.
#[test]
fn aws_lbc_null_spelling_name_override_stays_rejected() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "aws-load-balancer-controller",
        vec![
            SemanticCase::rejected(
                "null-spelling name override",
                "/nameOverride",
                json!({ "clusterName": "test", "nameOverride": "null" }),
            ),
            SemanticCase::accepted(
                "ordinary name override",
                json!({ "clusterName": "test", "nameOverride": "ok-name" }),
            ),
        ],
    )
}

/// `tpl (toYaml .Values.X) .` re-renders the serialized fragment,
/// so the parsed placement carries through to the sequence/provider slot
/// exactly like a bare `toYaml` splice: cloudnative-pg's `additionalEnv`
/// and airflow's scheduler fragments reject scalar inputs that break the
/// completed document while structured inputs render.
#[test]
fn tpl_serialized_fragments_keep_their_structural_slots() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cloudnative-pg",
        vec![
            SemanticCase::rejected(
                "scalar additionalEnv breaks the env sequence",
                "/additionalEnv",
                json!({ "additionalEnv": 7 }),
            ),
            SemanticCase::accepted(
                "EnvVar list renders",
                json!({ "additionalEnv": [{ "name": "A", "value": "b" }] }),
            ),
            SemanticCase::accepted(
                "templated EnvVar value renders",
                json!({ "additionalEnv": [{ "name": "A", "value": "{{ .Release.Name }}" }] }),
            ),
        ],
    )?;
    // Airflow's scheduler fragments carry the same contract, but their
    // rejections come from the PodSpec provider slots, which the corpus
    // environment's empty caches cannot supply; the provider-backed gen
    // test `tpl_serialized_fragment_projects_the_provider_slot` pins them.
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "list scheduler command",
                json!({ "scheduler": { "command": ["bash"] } }),
            ),
            SemanticCase::accepted(
                "null command selects the image default",
                json!({ "scheduler": { "command": null } }),
            ),
        ],
    )
}

/// external-secrets' webhook PDB header reads
/// `.Values.webhook.podDisruptionBudget.enabled`, so a truthy non-object
/// host terminates rendering while the object form (and the default) render.
#[test]
fn external_secrets_pdb_header_member_requires_an_object_host() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "external-secrets",
        vec![
            SemanticCase::rejected(
                "integer podDisruptionBudget",
                "/webhook/podDisruptionBudget",
                json!({ "webhook": { "podDisruptionBudget": 7 } }),
            ),
            SemanticCase::rejected(
                "array podDisruptionBudget",
                "/webhook/podDisruptionBudget",
                json!({ "webhook": { "podDisruptionBudget": [1] } }),
            ),
            SemanticCase::rejected(
                "boolean podDisruptionBudget",
                "/webhook/podDisruptionBudget",
                json!({ "webhook": { "podDisruptionBudget": true } }),
            ),
            SemanticCase::accepted(
                "object podDisruptionBudget",
                json!({ "webhook": { "podDisruptionBudget": { "enabled": false } } }),
            ),
        ],
    )
}

/// signoz's `renderAdditionalEnv` reads each member through `range keys .
/// | sortAlpha` + `pluck . $dict | first` — a same-map member projection
/// the analyzer resolves — but then gates every render on a case-folding
/// dedup accumulator (`not (hasKey $processedKeys (upper .))`). A member
/// can therefore be SHADOWED by an earlier case-colliding key and never
/// render, so a blanket per-member EnvVar constraint would falsely reject
/// `{audit: {value: 7}, AUDIT: {value: "ok"}}`; the schema soundly keeps
/// the members open. If a future increment starts rejecting the numeric
/// case below, it must prove the shadowed-member instance stays accepted.
#[test]
fn signoz_additional_env_members_stay_open_under_dedup_shadowing() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "signoz-signoz",
        vec![
            SemanticCase::accepted(
                "numeric EnvVar value abstains under the dedup accumulator",
                json!({ "signoz": { "additionalEnvs": { "AUDIT": { "value": 7 } } } }),
            ),
            SemanticCase::accepted(
                "case-colliding shadowed member renders nothing",
                json!({ "signoz": { "additionalEnvs": {
                    "AUDIT": { "value": "ok" }, "audit": { "value": 7 }
                } } }),
            ),
            SemanticCase::accepted(
                "scalar member renders through the quoted value lane",
                json!({ "signoz": { "additionalEnvs": { "AUDIT": 7 } } }),
            ),
        ],
    )
}

/// minio renders each `environment` range KEY at the EnvVar `name:` slot
/// (`statefulset.yaml`). A list supplies integer indices there — `name: 0`
/// renders but the provider rejects the non-string name — so non-empty
/// lists are excluded while the map lane, the empty list (zero
/// iterations), and absence stay open.
#[test]
fn minio_environment_list_lane_is_excluded_at_the_name_slot() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "minio",
        vec![
            SemanticCase::rejected(
                "non-empty list supplies integer keys",
                "/environment",
                json!({ "environment": ["audit"] }),
            ),
            SemanticCase::accepted(
                "map keys are strings",
                json!({ "environment": { "AUDIT": "ok" } }),
            ),
            SemanticCase::accepted(
                "empty list runs zero iterations",
                json!({ "environment": [] }),
            ),
        ],
    )
}

/// airflow's celery-broker sentinel (`check-values.yaml`) accumulates a
/// Boolean while ranging `env` and terminates when neither
/// `data.brokerUrlSecretName` nor an item named
/// `AIRFLOW__CELERY__BROKER_URL_CMD` exists. The flag joins to the
/// existential over `env` and lowers to a `contains`-backed terminal
/// clause under the celery/redis guards.
#[test]
fn airflow_celery_broker_sentinel_requires_a_matching_env_item() -> color_eyre::eyre::Result<()> {
    let celery = |extra: serde_json::Value| {
        let mut base = json!({
            "executor": "CeleryExecutor",
            "redis": { "enabled": true, "passwordSecretName": "redis-secret" }
        });
        for (key, value) in extra.as_object().expect("object").iter() {
            base.as_object_mut()
                .expect("object")
                .insert(key.clone(), value.clone());
        }
        base
    };
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected("no broker source terminates", "", celery(json!({}))),
            SemanticCase::rejected(
                "a differently named env item terminates",
                "",
                celery(json!({ "env": [{ "name": "OTHER", "value": "x" }] })),
            ),
            SemanticCase::accepted(
                "brokerUrlSecretName satisfies the sentinel",
                celery(json!({ "data": { "brokerUrlSecretName": "broker-secret" } })),
            ),
            SemanticCase::accepted(
                "a matching env item satisfies the sentinel",
                celery(
                    json!({ "env": [{ "name": "AIRFLOW__CELERY__BROKER_URL_CMD", "value": "cmd" }] }),
                ),
            ),
        ],
    )
}

/// cilium's `validate.yaml` states scalar domains through `fail` guards:
/// the 32-character `cluster.name` length bound, the internal-or-external
/// `kvstoreMode` membership, and the 255-or-511 `maxConnectedClusters`
/// coerced inequality pair. The sound-subset lowerings reject the
/// strengthened domains while coerced spellings outside them stay open.
#[test]
fn cilium_scalar_domain_validators_reject_out_of_domain_values() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "cluster name over 32 characters",
                "/cluster/name",
                json!({ "cluster": { "name": "a".repeat(40) } }),
            ),
            SemanticCase::accepted(
                "cluster name within the bound",
                json!({ "cluster": { "name": "good-name" } }),
            ),
            SemanticCase::rejected(
                "kvstoreMode outside the literal membership",
                "",
                json!({ "clustermesh": { "apiserver": { "kvstoremesh": { "kvstoreMode": "bogus" } } } }),
            ),
            SemanticCase::accepted(
                "kvstoreMode in the membership",
                json!({ "clustermesh": { "apiserver": { "kvstoremesh": { "kvstoreMode": "internal" } } } }),
            ),
            SemanticCase::rejected(
                "maxConnectedClusters outside 255-or-511",
                "/clustermesh",
                json!({ "clustermesh": { "maxConnectedClusters": 300 } }),
            ),
            SemanticCase::accepted(
                "maxConnectedClusters at the alternate bound",
                json!({ "clustermesh": { "maxConnectedClusters": 511 } }),
            ),
            SemanticCase::accepted(
                "numeric string stays outside the raw-integer subset",
                json!({ "clustermesh": { "maxConnectedClusters": "255" } }),
            ),
        ],
    )
}

/// airflow's `check-values.yaml` terminates below the minimum supported
/// version through `semverCompare "<2.11.0"`; the comparator's exact
/// pattern subset now reaches the terminal clause.
#[test]
fn airflow_minimum_version_terminal_rejects_older_semver() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected(
                "below the minimum supported version",
                "",
                json!({ "airflowVersion": "2.10.0" }),
            ),
            SemanticCase::accepted(
                "at the minimum supported version",
                json!({ "airflowVersion": "2.11.0" }),
            ),
        ],
    )
}

/// jenkins' `controller.replicas` helper binds the int cast to a local
/// (`$replicas := int (default 1 …)`) and fails outside 0..=1: the cast
/// provenance rides the binding, so both disjuncts reach the terminal
/// clause through the raw-integer subsets.
#[test]
fn jenkins_controller_replicas_domain_is_bounded() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "jenkins",
        vec![
            SemanticCase::rejected(
                "replicas above the domain",
                "/controller/replicas",
                json!({ "controller": { "replicas": 2 } }),
            ),
            SemanticCase::rejected(
                "negative replicas below the domain",
                "/controller/replicas",
                json!({ "controller": { "replicas": -1 } }),
            ),
            SemanticCase::accepted(
                "the scale-down replica count",
                json!({ "controller": { "replicas": 0 } }),
            ),
            SemanticCase::accepted(
                "the single supported replica",
                json!({ "controller": { "replicas": 1 } }),
            ),
            SemanticCase::rejected(
                "a clean decimal spelling coerces into the failing domain",
                "/controller/replicas",
                json!({ "controller": { "replicas": "5" } }),
            ),
            SemanticCase::accepted(
                "a decimal spelling inside the domain",
                json!({ "controller": { "replicas": "1" } }),
            ),
        ],
    )
}

/// airflow's scheduler selects its workload kind through an inline local
/// (`kind: {{ if $stateful }}StatefulSet{{ else }}Deployment{{ end }}`)
/// and guards the strategy slots with the same local: each row concretizes
/// to its arm's kind, so the provider projection follows the partition and
/// stays scoped to the arm's liveness.
#[test]
fn airflow_scheduler_kind_partition_scopes_strategy_providers() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected(
                "numeric strategy in the live Deployment arm",
                "/scheduler/strategy",
                json!({ "scheduler": { "strategy": 7 } }),
            ),
            SemanticCase::rejected(
                "a StatefulSet-only member at the Deployment strategy slot",
                "/scheduler/strategy",
                json!({ "scheduler": { "strategy": { "rollingUpdate": { "partition": 1 } } } }),
            ),
            SemanticCase::accepted(
                "a Deployment strategy in the live arm",
                json!({ "scheduler": { "strategy": { "type": "RollingUpdate" } } }),
            ),
            SemanticCase::accepted(
                "numeric strategy is harmless while the Deployment arm is dead",
                json!({ "executor": "LocalExecutor", "scheduler": { "strategy": 7 } }),
            ),
            SemanticCase::rejected(
                "numeric updateStrategy in the live StatefulSet arm",
                "/scheduler/updateStrategy",
                json!({ "executor": "LocalExecutor", "scheduler": { "updateStrategy": 7 } }),
            ),
            SemanticCase::accepted(
                "a StatefulSet updateStrategy in the live arm",
                json!({
                    "executor": "LocalExecutor",
                    "scheduler": { "updateStrategy": { "rollingUpdate": { "partition": 1 } } },
                }),
            ),
        ],
    )
}

/// nats renders each `extraResources` item as its own document
/// (`extra-resources.yaml`): Helm decodes every manifest as a mapping, so
/// scalar and list items cannot become resources; object items (including
/// `$tplYaml` wrappers) stay open.
#[test]
fn nats_extra_resources_items_must_be_objects() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "nats",
        vec![
            SemanticCase::rejected(
                "Boolean item cannot decode as a resource",
                "/extraResources/0",
                json!({ "extraResources": [true] }),
            ),
            SemanticCase::rejected(
                "list item cannot decode as a resource",
                "/extraResources/0",
                json!({ "extraResources": [["audit"]] }),
            ),
            SemanticCase::accepted(
                "object item stays open",
                json!({ "extraResources": [{ "apiVersion": "v1", "kind": "ConfigMap",
                    "metadata": { "name": "extra" } }] }),
            ),
            SemanticCase::accepted(
                "wrapper item is an object",
                json!({ "extraResources": [{ "$tplYaml": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: extra" }] }),
            ),
        ],
    )
}

/// bitnami-postgresql's live parent templates unconditionally include
/// `common.*` helpers that only the tagged `common` library defines:
/// disabling the tag makes Helm abort with `no template
/// "common.names.fullname"`, so the inactive tag states are terminal.
#[test]
fn bitnami_postgresql_disabled_common_tag_loses_live_helpers() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "bitnami-postgresql",
        vec![
            SemanticCase::rejected(
                "explicitly disabled library tag",
                "",
                json!({ "tags": { "bitnami-common": false } }),
            ),
            SemanticCase::accepted(
                "explicitly enabled library tag",
                json!({ "tags": { "bitnami-common": true } }),
            ),
            SemanticCase::accepted("absent tag defaults to enabled", json!({ "tags": {} })),
        ],
    )
}

/// airflow's counter-pin for the optional-helper implication: the
/// `bitnami-common` tag belongs to the POSTGRESQL child's own library
/// dependency, so disabling the tag is terminal only while the postgresql
/// child itself is active.
#[test]
fn airflow_disabled_common_tag_is_scoped_to_the_postgresql_child() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected(
                "disabled tag under the default active child",
                "",
                json!({ "tags": { "bitnami-common": false } }),
            ),
            SemanticCase::accepted(
                "disabled tag with the child disabled",
                json!({ "tags": { "bitnami-common": false }, "postgresql": { "enabled": false } }),
            ),
        ],
    )
}
