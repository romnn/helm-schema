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

/// cilium's configmap compares `toString .Values.kubeProxyReplacement`
/// against `"true"`/`"false"` through a coalesce chain and aborts on
/// anything else, so the accepted domain is the `toString` PREIMAGE of the
/// two spellings: raw Booleans render exactly like their string forms
/// (helm-verified), while any other truthy scalar aborts. The chain also
/// folds the `"<nil>"` rendering to `""` and rescues the Helm-empty result
/// through `coalesce … "false"`, so the empty, null, and literal-`"<nil>"`
/// spellings render as well (all helm-verified).
#[test]
fn cilium_kube_proxy_replacement_accepts_raw_booleans() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::accepted("raw Boolean true", json!({ "kubeProxyReplacement": true })),
            SemanticCase::accepted(
                "raw Boolean false",
                json!({ "kubeProxyReplacement": false }),
            ),
            SemanticCase::accepted("string spelling", json!({ "kubeProxyReplacement": "true" })),
            SemanticCase::accepted(
                "empty string selects the coalesce default",
                json!({ "kubeProxyReplacement": "" }),
            ),
            SemanticCase::accepted(
                "null folds to empty and selects the coalesce default",
                json!({ "kubeProxyReplacement": null }),
            ),
            SemanticCase::accepted(
                "the literal <nil> spelling folds to empty too",
                json!({ "kubeProxyReplacement": "<nil>" }),
            ),
            // The fail-implication arm rejects at the instance root
            // (`if … then false`), not at the property.
            SemanticCase::rejected(
                "legacy strict spelling aborts the render",
                "",
                json!({ "kubeProxyReplacement": "strict" }),
            ),
            SemanticCase::rejected(
                "numeric value stringifies to a rejected spelling",
                "",
                json!({ "kubeProxyReplacement": 1 }),
            ),
        ],
    )
}

/// cilium's removed-option gate stringifies the dug value before testing
/// truthiness (`(dig "proxy" "prometheus" "enabled" "" .Values.AsMap) |
/// toString`), so an explicitly-DISABLED removed option still aborts:
/// `"false"`, `"0"`, and `"<nil>"` are truthy strings. Only a missing key
/// or a raw empty string renders; the sibling `port` disjunct keeps
/// ordinary Helm truthiness (all polarities helm-verified). The
/// explicit-null polarity (rejected: it renders truthy `"<nil>"`) is
/// pinned by the gen reproducer — this harness's override compositor
/// deletes null keys the way helm's value coalescing does, so a null
/// case cannot be expressed here.
#[test]
fn cilium_removed_options_abort_even_when_disabled() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::accepted(
                "absent leaf renders",
                json!({ "proxy": { "prometheus": {} } }),
            ),
            SemanticCase::accepted(
                "raw empty string stringifies to the falsy empty rendering",
                json!({ "proxy": { "prometheus": { "enabled": "" } } }),
            ),
            SemanticCase::rejected(
                "explicitly disabled still aborts (truthy \"false\")",
                "/proxy",
                json!({ "proxy": { "prometheus": { "enabled": false } } }),
            ),
            SemanticCase::rejected(
                "enabled aborts",
                "/proxy",
                json!({ "proxy": { "prometheus": { "enabled": true } } }),
            ),
            SemanticCase::rejected(
                "the sibling port disjunct keeps Helm truthiness",
                "/proxy",
                json!({ "proxy": { "prometheus": { "port": 9095 } } }),
            ),
        ],
    )
}

/// promtail renders every `extraPorts` member into a provider-REQUIRED
/// Service `port` AND an unconditional pod `containerPort`: a member
/// without `containerPort` emits an explicit null the strict provider
/// rejects — even when `service.port` fills the Service side, the pod
/// port stays null (all polarities helm-rendered and checked against the
/// committed provider bundle).
#[test]
fn promtail_extra_port_members_require_the_container_port() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "promtail",
        vec![
            SemanticCase::rejected(
                "an empty member renders null ports",
                "/extraPorts",
                json!({ "extraPorts": { "audit": {} } }),
            ),
            SemanticCase::rejected(
                "service.port alone leaves the pod port null",
                "/extraPorts",
                json!({ "extraPorts": { "audit": { "service": { "port": 80 } } } }),
            ),
            SemanticCase::accepted(
                "containerPort fills both sinks",
                json!({ "extraPorts": { "audit": { "containerPort": 1234 } } }),
            ),
            SemanticCase::accepted(
                "containerPort beside a service port",
                json!({ "extraPorts": { "audit": { "containerPort": 1234,
                    "service": { "port": 80 } } } }),
            ),
        ],
    )
}

/// kube-state-metrics renders probe `httpHeaders` members' `name` and
/// `value` — both provider-required — for every item once the probe is
/// enabled; `[{}]` renders null header fields the strict provider
/// rejects, while the disabled probe renders nothing (helm-verified).
#[test]
fn kube_state_metrics_probe_headers_require_name_and_value() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kube-state-metrics",
        vec![
            SemanticCase::rejected(
                "an empty header member renders null name and value",
                "/startupProbe/httpGet/httpHeaders",
                json!({ "startupProbe": { "enabled": true,
                    "httpGet": { "httpHeaders": [{}] } } }),
            ),
            SemanticCase::accepted(
                "populated headers render",
                json!({ "startupProbe": { "enabled": true,
                    "httpGet": { "httpHeaders":
                        [{ "name": "X-Audit", "value": "audit" }] } } }),
            ),
            SemanticCase::accepted(
                "the disabled probe renders nothing",
                json!({ "startupProbe": { "httpGet": { "httpHeaders": [{}] } } }),
            ),
        ],
    )
}

/// kyverno's `kyverno.deployment.replicas` helper aborts on
/// `eq (int .) 0` for any non-string, non-nil argument — every
/// controller's `replicas: 0` terminates Helm while `1` renders, and the
/// string `"0"` escapes through the helper's own `kindIs "string"`
/// dispatch (all polarities helm-verified).
#[test]
fn kyverno_zero_replicas_abort_through_the_template_helper() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kyverno",
        vec![
            SemanticCase::rejected(
                "admission controller zero replicas abort",
                "/admissionController/replicas",
                json!({ "admissionController": { "replicas": 0 } }),
            ),
            SemanticCase::rejected(
                "reports controller zero replicas abort",
                "/reportsController/replicas",
                json!({ "reportsController": { "replicas": 0 } }),
            ),
            SemanticCase::accepted(
                "nonzero replicas render",
                json!({ "admissionController": { "replicas": 2 } }),
            ),
            SemanticCase::accepted(
                "a string spelling escapes the kind dispatch",
                json!({ "admissionController": { "replicas": "0" } }),
            ),
        ],
    )
}

/// redis-ha's ConfigMap renders `redis.conf: |` followed by a column-zero
/// `{{- include "config-redis.conf" . }}`: the include's output continues
/// the block scalar, so ranged `redis.config` members are pure text and
/// any scalar spelling renders (helm-verified). Anchoring the include as
/// `data`-level structure provider-typed the members `type: null`,
/// rejecting even argo-cd's own `save: '""'` default once `redis-ha` is
/// enabled. The strict `tpl` string-program contract on `customConfig`
/// must survive the text adoption.
#[test]
fn oauth2_proxy_redis_ha_config_members_render_as_block_text() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "oauth2-proxy",
        vec![
            SemanticCase::accepted(
                "string config members render as block text",
                json!({ "redis-ha": { "enabled": true,
                    "redis": { "config": { "maxmemory": "100mb" } } } }),
            ),
            SemanticCase::accepted(
                "raw scalars stringify in the loop body",
                json!({ "redis-ha": { "enabled": true,
                    "redis": { "config": { "repl-diskless-sync": true } } } }),
            ),
            SemanticCase::accepted(
                "string custom config renders through tpl",
                json!({ "redis-ha": { "enabled": true,
                    "redis": { "customConfig": "maxmemory 100mb" } } }),
            ),
            SemanticCase::rejected(
                "tpl still requires a string program",
                "/redis-ha/redis/customConfig",
                json!({ "redis-ha": { "enabled": true,
                    "redis": { "customConfig": { "bad": true } } } }),
            ),
        ],
    )
}

/// The redis StandaloneUrl helper fails ("please set
/// sessionStorage.redis.standalone.connectionUrl or enable the redis
/// subchart via redis-ha.enabled") when the standalone client has no
/// explicit url and `redis-ha.enabled` is false. The caller gate is
/// `eq (default "" .Values.sessionStorage.redis.clientType) "standalone"`
/// and the subchart test is `eq (include "oauth2-proxy.redis.enabled" .)
/// "true"` over a helper whose body is one boolean expression — both must
/// decode for the terminal to reach the schema (helm-verified all three
/// ways).
#[test]
fn oauth2_proxy_standalone_redis_requires_a_connection_url() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "oauth2-proxy",
        vec![
            SemanticCase::rejected(
                "standalone client without a connection url aborts",
                "",
                json!({ "sessionStorage": { "type": "redis",
                    "redis": { "clientType": "standalone" } } }),
            ),
            SemanticCase::accepted(
                "an explicit connection url renders",
                json!({ "sessionStorage": { "type": "redis",
                    "redis": { "clientType": "standalone",
                        "standalone": { "connectionUrl": "redis://myredis:6379" } } } }),
            ),
            SemanticCase::accepted(
                "the enabled redis subchart computes the url",
                json!({ "sessionStorage": { "type": "redis",
                    "redis": { "clientType": "standalone" } },
                    "redis-ha": { "enabled": true } }),
            ),
        ],
    )
}

/// argo-cd vendors the same redis-ha chart, and its own values file sets
/// `redis-ha.redis.config.save: '""'` — enabling the dependency must not
/// reject the chart's own defaults (helm renders them).
#[test]
fn argo_cd_redis_ha_own_defaults_render_when_enabled() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "argo-cd",
        vec![SemanticCase::accepted(
            "enabling redis-ha keeps the chart's own config defaults",
            json!({ "redis-ha": { "enabled": true } }),
        )],
    )
}

/// traefik's OTLP `resourceAttributes` render as per-member flag loops
/// through the `traefik.oltpCommonParams` helper inside the
/// `fromYaml | toYaml` pod-template roundtrip. Map members render (any
/// value kind — the loop stringifies), a list renders too, and only a
/// non-rangeable scalar aborts Helm. The lane's guards ride the
/// `with .addX | toString` family, whose truthiness is a RENDERING test
/// (all polarities helm-verified).
#[test]
fn traefik_otlp_resource_attributes_render_as_flag_loops() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::accepted(
                "string members render as flags",
                json!({ "tracing": { "otlp": { "enabled": true, "resourceAttributes": { "env": "prod" } } } }),
            ),
            SemanticCase::accepted(
                "non-string members stringify in the loop body",
                json!({ "tracing": { "otlp": { "enabled": true, "resourceAttributes": { "env": 7 } } } }),
            ),
            SemanticCase::accepted(
                "a list is rangeable too",
                json!({ "tracing": { "otlp": { "enabled": true, "resourceAttributes": ["a"] } } }),
            ),
            SemanticCase::accepted(
                "metrics rides the same helper",
                json!({ "metrics": { "otlp": { "enabled": true, "resourceAttributes": { "env": "prod" } } } }),
            ),
            SemanticCase::rejected(
                "a scalar is not rangeable",
                "/tracing/otlp/resourceAttributes",
                json!({ "tracing": { "otlp": { "enabled": true, "resourceAttributes": 7 } } }),
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
            // The IPv6 arm is the parser's exact language, not a
            // hex-and-colons superset: a bare colon and a zoned address
            // both abort `genSignedCert` (helm-verified).
            SemanticCase::rejected(
                "bare-colon Hubble IP SAN",
                "/hubble/tls/server/extraIpAddresses/0",
                json!({
                    "hubble": { "tls": { "server": { "extraIpAddresses": [":"] } } }
                }),
            ),
            SemanticCase::rejected(
                "zoned Hubble IP SAN",
                "/hubble/tls/server/extraIpAddresses/0",
                json!({
                    "hubble": { "tls": { "server": { "extraIpAddresses": ["fe80::1%eth0"] } } }
                }),
            ),
            SemanticCase::accepted(
                "string Hubble SANs",
                json!({
                    "hubble": {
                        "tls": {
                            "server": {
                                "extraIpAddresses": [
                                    "10.0.0.7",
                                    "2001:db8::1",
                                    "::ffff:10.0.0.7",
                                    "1:2:3:4:5:6:1.2.3.4",
                                    "1:2:3:4:5:6:7::"
                                ],
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

/// The `ddot-collector-gateway-image` helper replaces a FALSY tag with the
/// agent-version fallback before its semver checks, so an empty or null
/// `otelAgentGateway.image.tag` renders (helm-verified) and only a truthy
/// non-version string reaches the parser and aborts.
#[test]
fn datadog_otel_gateway_empty_tag_selects_the_agent_version_fallback()
-> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "datadog",
        vec![
            SemanticCase::accepted(
                "empty tag takes the fallback",
                json!({ "otelAgentGateway": { "enabled": true, "image": { "tag": "" } } }),
            ),
            SemanticCase::accepted(
                "null tag takes the fallback",
                json!({ "otelAgentGateway": { "enabled": true, "image": { "tag": null } } }),
            ),
            SemanticCase::accepted(
                "explicit valid tag",
                json!({ "otelAgentGateway": { "enabled": true, "image": { "tag": "7.70.0" } } }),
            ),
            SemanticCase::rejected(
                "truthy non-version tag reaches the parser",
                "/otelAgentGateway/image/tag",
                json!({ "otelAgentGateway": { "enabled": true, "image": { "tag": "junk" } } }),
            ),
        ],
    )
}

/// The OTLP verify helpers run with the dot bound to the endpoint SCALAR
/// (`include "verify-otlp-grpc-endpoint-prefix" .grpc.endpoint`): their
/// `hasPrefix "unix:" .` and `not (regexMatch ":[0-9]+$" .)` terminals
/// must bind the caller's endpoint path under the daemonset's apiKey and
/// grpc-enabled gates (helm-verified each way). The port-suffixed unix
/// spelling isolates the prefix terminal — the port test alone admits it.
#[test]
fn datadog_otlp_grpc_endpoints_reject_the_unix_protocol() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "datadog",
        vec![
            SemanticCase::rejected(
                "a unix endpoint with a port suffix aborts on the prefix",
                "",
                json!({ "datadog": { "apiKey": "dummykey", "otlp": { "receiver": {
                    "protocols": { "grpc": { "enabled": true,
                        "endpoint": "unix:///tmp/otlp.sock:4317" } } } } } }),
            ),
            SemanticCase::rejected(
                "a portless endpoint aborts",
                "/datadog/otlp/receiver/protocols/grpc/endpoint",
                json!({ "datadog": { "apiKey": "dummykey", "otlp": { "receiver": {
                    "protocols": { "grpc": { "enabled": true,
                        "endpoint": "0.0.0.0" } } } } } }),
            ),
            SemanticCase::accepted(
                "a host:port endpoint renders",
                json!({ "datadog": { "apiKey": "dummykey", "otlp": { "receiver": {
                    "protocols": { "grpc": { "enabled": true,
                        "endpoint": "0.0.0.0:4317" } } } } } }),
            ),
            SemanticCase::accepted(
                "the disabled receiver keeps any endpoint open",
                json!({ "datadog": { "apiKey": "dummykey", "otlp": { "receiver": {
                    "protocols": { "grpc": { "enabled": false,
                        "endpoint": "unix:///tmp/otlp.sock" } } } } } }),
            ),
        ],
    )
}

/// traefik's `traefik.getLocalPluginType` helper renders a local plugin
/// through mutually exclusive arms — a `type` from the
/// hostPath/inlinePlugin/localPath enum, or the legacy bare `hostPath` —
/// and `fail`s otherwise. The member requirements are the DISJUNCTION of
/// those arms (helm-verified each way): conjoining them rejected both
/// documented shapes, and the missing enum accepted an unknown `type`.
#[test]
fn traefik_local_plugins_keep_their_alternative_shapes() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "traefik",
        vec![
            SemanticCase::accepted(
                "legacy hostPath shape",
                json!({ "experimental": { "localPlugins": { "my-plugin": {
                    "moduleName": "github.com/x/y", "hostPath": "/plugins/y"
                } } } }),
            ),
            SemanticCase::accepted(
                "inline plugin shape",
                json!({ "experimental": { "localPlugins": { "my-plugin": {
                    "moduleName": "github.com/x/y",
                    "type": "inlinePlugin",
                    "source": { "main.go": "package main" }
                } } } }),
            ),
            SemanticCase::accepted(
                "typed hostPath shape",
                json!({ "experimental": { "localPlugins": { "my-plugin": {
                    "moduleName": "github.com/x/y",
                    "type": "hostPath",
                    "hostPath": "/plugins/y"
                } } } }),
            ),
            SemanticCase::rejected(
                "unknown type aborts even beside a hostPath",
                "/experimental/localPlugins",
                json!({ "experimental": { "localPlugins": { "my-plugin": {
                    "moduleName": "github.com/x/y", "type": "bogus", "hostPath": "/x"
                } } } }),
            ),
            SemanticCase::rejected(
                "neither type nor hostPath aborts",
                "/experimental/localPlugins",
                json!({ "experimental": { "localPlugins": { "my-plugin": {
                    "moduleName": "github.com/x/y"
                } } } }),
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

/// external-secrets' `renderSecurityContext` removes `fsGroup`,
/// `runAsUser`, and `runAsGroup` with a guard-scoped `omit` when the
/// OpenShift adaptation runs (`adaptSecurityContext` "force", or "auto" on
/// a detected OpenShift cluster). The removed keys' provider typing binds
/// exactly where the omit certainly does not run — `adaptSecurityContext:
/// disabled` with a live render gate — while "force" and the
/// cluster-dependent "auto" accept any value there. Keys the omit never
/// touches keep their typing in every mode. Each polarity reproduces
/// under `helm template --skip-schema-validation` + kubeconform.
#[test]
fn external_secrets_omitted_security_context_keys_scope_their_typing()
-> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "external-secrets",
        vec![
            SemanticCase::accepted(
                "force mode removes runAsUser before the sink",
                json!({
                    "global": { "compatibility": { "openshift": { "adaptSecurityContext": "force" } } },
                    "securityContext": { "runAsUser": "audit" },
                }),
            ),
            SemanticCase::accepted(
                "auto mode is cluster-dependent, so the key stays open",
                json!({ "securityContext": { "runAsUser": "audit" } }),
            ),
            SemanticCase::rejected(
                "disabled mode keeps the key on the rendered Deployment",
                "/securityContext/runAsUser",
                json!({
                    "global": { "compatibility": { "openshift": { "adaptSecurityContext": "disabled" } } },
                    "securityContext": { "runAsUser": "audit" },
                }),
            ),
            SemanticCase::accepted(
                "disabled mode with a valid integer",
                json!({
                    "global": { "compatibility": { "openshift": { "adaptSecurityContext": "disabled" } } },
                    "securityContext": { "runAsUser": 1000 },
                }),
            ),
            SemanticCase::accepted(
                "a disabled render gate never reaches the provider slot",
                json!({
                    "global": { "compatibility": { "openshift": { "adaptSecurityContext": "disabled" } } },
                    "securityContext": { "enabled": false, "runAsUser": "audit" },
                }),
            ),
            SemanticCase::rejected(
                "a never-omitted key keeps its typing even under force",
                "/securityContext/runAsNonRoot",
                json!({
                    "global": { "compatibility": { "openshift": { "adaptSecurityContext": "force" } } },
                    "securityContext": { "runAsNonRoot": "audit" },
                }),
            ),
        ],
    )
}

/// oauth2-proxy's `deprecation.yaml` aborts rendering when a legacy
/// `ingress.extraPaths[].backend.serviceName`/`servicePort` is set while
/// the capability helper resolves the `networking.k8s.io/v1` Ingress api.
/// With `kubeVersion` pinned at or above 1.19 the abort is certain; the
/// old api keeps the legacy format, and without a pinned version the
/// selection is cluster-dependent, so the schema abstains. Each polarity
/// reproduces under `helm template`.
#[test]
fn oauth2_proxy_legacy_extra_paths_abort_under_the_v1_ingress_api() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "oauth2-proxy",
        vec![
            SemanticCase::rejected(
                "legacy serviceName under a pinned modern kubeVersion",
                "/ingress/extraPaths",
                json!({ "kubeVersion": "1.30.0", "ingress": { "enabled": true,
                    "hosts": ["h.example"], "extraPaths": [
                        { "path": "/*", "backend": { "serviceName": "ssl-redirect" } }
                    ] } }),
            ),
            SemanticCase::rejected(
                "legacy servicePort under a pinned modern kubeVersion",
                "/ingress/extraPaths",
                json!({ "kubeVersion": "1.30.0", "ingress": { "enabled": true,
                    "hosts": ["h.example"], "extraPaths": [
                        { "path": "/*", "backend": { "servicePort": "use-annotation" } }
                    ] } }),
            ),
            SemanticCase::accepted(
                "legacy format renders on the v1beta1 api",
                json!({ "kubeVersion": "1.18.0", "ingress": { "enabled": true,
                    "hosts": ["h.example"], "extraPaths": [
                        { "path": "/*", "backend": { "serviceName": "ssl-redirect",
                            "servicePort": "use-annotation" } }
                    ] } }),
            ),
            SemanticCase::accepted(
                "modern v1 backend",
                json!({ "kubeVersion": "1.30.0", "ingress": { "enabled": true,
                    "hosts": ["h.example"], "extraPaths": [
                        { "path": "/*", "pathType": "ImplementationSpecific",
                          "backend": { "service": { "name": "ssl-redirect",
                              "port": { "name": "use-annotation" } } } }
                    ] } }),
            ),
            SemanticCase::accepted(
                "unpinned kubeVersion leaves the capability selection open",
                json!({ "ingress": { "enabled": true, "hosts": ["h.example"],
                    "extraPaths": [
                        { "path": "/*", "backend": { "serviceName": "ssl-redirect" } }
                    ] } }),
            ),
            SemanticCase::accepted(
                "checkDeprecation disabled tolerates the legacy format",
                json!({ "checkDeprecation": false, "kubeVersion": "1.30.0",
                    "ingress": { "enabled": true, "hosts": ["h.example"],
                        "extraPaths": [
                            { "path": "/*", "backend": { "serviceName": "ssl-redirect" } }
                        ] } }),
            ),
        ],
    )
}

/// signoz's `renderAdditionalEnv` reads each member through `range keys .
/// | sortAlpha` + `pluck . $dict | first` — a same-map member projection
/// the analyzer resolves — but then gates every render on a case-folding
/// dedup accumulator (`not (hasKey $processedKeys (upper .))`). A member
/// with case-colliding siblings can therefore be SHADOWED by an earlier
/// key and never render, so the multi-member map stays open — but a
/// SINGLETON member cannot be shadowed (the accumulator is provably empty
/// on the first iteration), so the provider's EnvVar shape binds it under
/// a `maxProperties: 1` bound.
#[test]
fn signoz_additional_env_members_stay_open_under_dedup_shadowing() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "signoz-signoz",
        vec![
            SemanticCase::rejected(
                "a singleton member cannot be shadowed, so the EnvVar shape binds",
                "/signoz/additionalEnvs",
                json!({ "signoz": { "additionalEnvs": { "AUDIT": { "value": 7 } } } }),
            ),
            SemanticCase::accepted(
                "a valid singleton EnvVar member",
                json!({ "signoz": { "additionalEnvs": { "AUDIT": { "value": "ok" } } } }),
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

/// cilium's validators bound integer domains through `ge`/`le` chains: the
/// envoy `baseID` window rejects both sides via the De Morgan'd
/// `not (and (ge …) (le …))` fail, and the ENI/AlibabaCloud policy-drop
/// check rejects a cluster.id inside either affected window while the
/// literal `extraConfig` opt-out and the no-ENI configuration stay open
/// (all polarities verified under `helm template`).
#[test]
fn cilium_inclusive_comparator_chains_bound_integer_domains() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "negative envoy baseID",
                "/envoy/baseID",
                json!({ "envoy": { "baseID": -5 } }),
            ),
            SemanticCase::rejected(
                "envoy baseID above 4294967295",
                "/envoy/baseID",
                json!({ "envoy": { "baseID": 4_294_967_296_i64 } }),
            ),
            SemanticCase::accepted(
                "envoy baseID inside the window",
                json!({ "envoy": { "baseID": 42 } }),
            ),
            SemanticCase::rejected(
                "cluster id in the 128-255 window under ENI",
                "",
                json!({ "cluster": { "id": 200, "name": "c1" }, "eni": { "enabled": true } }),
            ),
            SemanticCase::rejected(
                "cluster id in the 384-511 window under AlibabaCloud",
                "",
                json!({ "cluster": { "id": 450, "name": "c1" }, "alibabacloud": { "enabled": true } }),
            ),
            SemanticCase::accepted(
                "cluster id below the window under ENI",
                json!({ "cluster": { "id": 127, "name": "c1" }, "eni": { "enabled": true } }),
            ),
            SemanticCase::accepted(
                "affected cluster id without an affected datapath",
                json!({ "cluster": { "id": 200, "name": "c1" } }),
            ),
            SemanticCase::accepted(
                "affected cluster id with the unsafe-skb opt-out",
                json!({
                    "cluster": { "id": 200, "name": "c1" },
                    "eni": { "enabled": true },
                    "extraConfig": { "allow-unsafe-policy-skb-usage": "true" }
                }),
            ),
        ],
    )
}

/// cilium's provider-mode gates: `ne (.Values.routingMode | default
/// "native") "native"` aborts GKE with tunnel routing (the AKS-BYOCNI
/// twin defaults to "tunnel" and aborts native), and the ingress /
/// Gateway API `externalTrafficPolicy` tests negate the
/// Cluster-or-Local equality disjunction exactly instead of weakening
/// to truthiness (every polarity helm-verified).
#[test]
fn cilium_provider_modes_pin_routing_and_traffic_policy_domains() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "cilium",
        vec![
            SemanticCase::rejected(
                "gke with tunnel routing aborts",
                "",
                json!({ "gke": { "enabled": true }, "routingMode": "tunnel",
                    "ipam": { "mode": "kubernetes" } }),
            ),
            SemanticCase::accepted(
                "gke with native routing renders",
                json!({ "gke": { "enabled": true }, "routingMode": "native",
                    "ipam": { "mode": "kubernetes" },
                    "ipv4NativeRoutingCIDR": "10.0.0.0/8" }),
            ),
            SemanticCase::rejected(
                "aks-byocni with native routing aborts",
                "",
                json!({ "aksbyocni": { "enabled": true }, "routingMode": "native",
                    "ipv4NativeRoutingCIDR": "10.0.0.0/8" }),
            ),
            SemanticCase::accepted(
                "aks-byocni with tunnel routing renders",
                json!({ "aksbyocni": { "enabled": true }, "routingMode": "tunnel" }),
            ),
            SemanticCase::rejected(
                "an unlisted ingress traffic policy aborts",
                "/ingressController",
                json!({ "ingressController": { "enabled": true, "service": {
                    "type": "LoadBalancer", "externalTrafficPolicy": "Foo" } } }),
            ),
            SemanticCase::accepted(
                "the Local ingress traffic policy renders",
                json!({ "ingressController": { "enabled": true, "service": {
                    "type": "LoadBalancer", "externalTrafficPolicy": "Local" } } }),
            ),
            SemanticCase::rejected(
                "an unlisted gateway traffic policy aborts",
                "/gatewayAPI",
                json!({ "gatewayAPI": { "enabled": true,
                    "externalTrafficPolicy": "Foo" } }),
            ),
            SemanticCase::accepted(
                "the Cluster gateway traffic policy renders",
                json!({ "gatewayAPI": { "enabled": true,
                    "externalTrafficPolicy": "Cluster" } }),
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

/// airflow renders root `labels` through the checksum annotations too:
/// every secret/configmap template is re-rendered by
/// `include (print $.Template.BasePath …) . | sha256sum`, whose digest
/// text lands at a string-typed annotation slot. The digest must not
/// type `labels` against that slot: a map is the normal input (the
/// direct `with .Values.labels` + `toYaml` renders keep it a labels
/// map), while a truthy string still aborts at the `mustMerge` sites.
#[test]
fn airflow_checksum_annotations_do_not_string_type_root_labels() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "map-shaped root labels render through every template",
                json!({ "labels": { "team": "data" } }),
            ),
            SemanticCase::rejected(
                "truthy scalar root labels terminate the mustMerge sites",
                "/labels",
                json!({ "labels": "oops" }),
            ),
        ],
    )
}

/// airflow renders a Helm-falsy root `labels` with default values: every
/// `with .Values.labels` guard skips it, and every `mustMerge` site sits
/// behind `if or .Values.labels .Values.<component>.labels`, whose gate is
/// dead while both operands are falsy. The strict map contract binds only
/// when a partner makes the gate live — `mustMerge`'s typed
/// `map[string]any` parameters then abort on any non-map operand
/// (helm-verified both ways) — so the falsy family must stay open at the
/// base while the or-gated fail arm keeps the live-gate combination
/// rejected.
#[test]
fn airflow_falsy_root_labels_render_while_live_merge_gates_bind() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::accepted(
                "empty-string root labels leave every merge gate dead",
                json!({ "labels": "" }),
            ),
            SemanticCase::accepted(
                "empty-list root labels leave every merge gate dead",
                json!({ "labels": [] }),
            ),
            SemanticCase::accepted(
                "false root labels leave every merge gate dead",
                json!({ "labels": false }),
            ),
            SemanticCase::rejected(
                "a truthy scheduler partner makes the merge gate live, so a \
                 falsy non-map root labels aborts mustMerge",
                "/labels",
                json!({
                    "labels": "",
                    "scheduler": { "labels": { "team": "data" } }
                }),
            ),
        ],
    )
}

/// airflow's worker templates rebuild `.Values.workers` per worker set:
/// `workersMergeValues` merges each `workers.celery.sets[]` entry over the
/// celery-merged workers map, and `set $globals.Values "workers" $workers`
/// rebinds the values context for the manifest body. A truthy set member
/// that reaches a strict map consumer (`mustMerge .Values.workers.labels`,
/// the securityContext helpers' `hasKey` probes, the merge recursion
/// itself) must therefore be a map, while map-shaped per-set overrides
/// stay open.
#[test]
fn airflow_worker_set_overrides_bind_strict_member_kinds() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "airflow",
        vec![
            SemanticCase::rejected(
                "scalar labels in a worker set terminates mustMerge",
                "/workers/celery/sets",
                json!({ "workers": { "celery": { "sets": [
                    { "name": "heavy", "labels": "oops" }
                ] } } }),
            ),
            SemanticCase::rejected(
                "scalar persistence in a worker set terminates the merge recursion",
                "/workers/celery/sets",
                json!({ "workers": { "celery": { "sets": [
                    { "name": "heavy", "persistence": "oops" }
                ] } } }),
            ),
            SemanticCase::accepted(
                "map labels override renders",
                json!({ "workers": { "celery": { "sets": [
                    { "name": "heavy", "labels": { "tier": "heavy" } }
                ] } } }),
            ),
            SemanticCase::accepted(
                "structured per-set overrides render",
                json!({ "workers": { "celery": { "sets": [
                    { "name": "highcpu", "replicas": 2, "queue": "highcpu",
                      "persistence": { "enabled": true },
                      "resources": { "requests": { "cpu": "1" } } }
                ] } } }),
            ),
        ],
    )
}

/// vault's `.mode` is assigned across the five arms of `vault.mode`
/// (`$_ := set . "mode" "…"`), and the HTTPRoute / redundancy-zone fails
/// sit behind `ne .mode "external"` / `eq .mode …` gates over that
/// dispatch. The joined root-set value dispatch decodes those gates
/// exactly: the parentRefs and redundancy-zone requirements bind under
/// the internal modes, while an external vault address keeps every gated
/// document dormant (helm-verified each way). The corpus policy version
/// is v1.29, below the redundancy feature's `>= 1.35` cluster-version
/// fail, so even the otherwise-complete combination aborts
/// (`helm template --kube-version 1.29.0` fails with "requires
/// Kubernetes >= 1.35") — the capabilities-semver lane binds it.
#[test]
fn vault_mode_dispatch_binds_httproute_and_redundancy_zone_fails() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "vault",
        vec![
            SemanticCase::rejected(
                "an enabled httproute without parentRefs aborts",
                "",
                json!({ "server": { "httproute": { "enabled": true } } }),
            ),
            SemanticCase::accepted(
                "an enabled httproute with parentRefs renders",
                json!({ "server": { "httproute": { "enabled": true,
                    "parentRefs": [ { "name": "gw" } ] } } }),
            ),
            SemanticCase::accepted(
                "external mode keeps the httproute gate dormant",
                json!({ "injector": { "externalVaultAddr": "https://vault.example.com" },
                    "server": { "httproute": { "enabled": true } } }),
            ),
            SemanticCase::rejected(
                "redundancy zones without ha mode abort",
                "",
                json!({ "server": { "ha": { "raft": {
                    "redundancyZones": { "enabled": true } } } } }),
            ),
            SemanticCase::rejected(
                "redundancy zones with ha but without raft abort",
                "",
                json!({ "server": { "ha": { "enabled": true, "raft": {
                    "redundancyZones": { "enabled": true } } } } }),
            ),
            SemanticCase::rejected(
                "the full combination still aborts below the required cluster version",
                "",
                json!({ "server": { "ha": { "enabled": true, "raft": {
                    "enabled": true,
                    "config": "storage \"raft\" {\n  autopilot_redundancy_zone = \"VAULT_REDUNDANCY_ZONE\"\n}\n",
                    "redundancyZones": { "enabled": true } } } } }),
            ),
            SemanticCase::accepted(
                "external mode keeps the redundancy-zone gates dormant",
                json!({ "injector": { "externalVaultAddr": "https://vault.example.com" },
                    "server": { "ha": { "raft": {
                        "redundancyZones": { "enabled": true } } } } }),
            ),
        ],
    )
}

/// kube-prometheus-stack's grafana dashboard documents are gated on
/// `semverCompare` tests over `default .Capabilities.KubeVersion.GitVersion
/// .Values.kubeTargetVersionOverride`: under the corpus policy version the
/// gates hold constantly, so the operator lane's matchLabels fail binds,
/// while a pre-1.14 override turns every dashboard document off exactly
/// (helm-verified each way).
#[test]
fn kube_prometheus_stack_dashboard_gates_decode_the_version_policy() -> color_eyre::eyre::Result<()>
{
    assert_chart_cases(
        "kube-prometheus-stack",
        vec![
            SemanticCase::rejected(
                "the operator dashboards without matchLabels abort",
                "",
                json!({ "grafana": { "operator": {
                    "dashboardsConfigMapRefEnabled": true } } }),
            ),
            SemanticCase::accepted(
                "operator dashboards with matchLabels render",
                json!({ "grafana": { "operator": {
                    "dashboardsConfigMapRefEnabled": true,
                    "matchLabels": { "app": "grafana" } } } }),
            ),
            SemanticCase::accepted(
                "a pre-1.14 version override keeps the dashboards dormant",
                json!({ "grafana": { "operator": {
                    "dashboardsConfigMapRefEnabled": true } },
                    "kubeTargetVersionOverride": "1.13.0" }),
            ),
        ],
    )
}
