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
fn airflow_webserver_contract_abstains_at_unlowerable_version_guard() -> color_eyre::eyre::Result<()>
{
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

#[test]
fn kube_state_metrics_namespace_fallback_accepts_null() -> color_eyre::eyre::Result<()> {
    assert_chart_cases(
        "kube-state-metrics",
        vec![
            SemanticCase::accepted(
                "null namespaceOverride selects release namespace",
                json!({ "namespaceOverride": null }),
            ),
            SemanticCase::accepted(
                "string namespaceOverride",
                json!({ "namespaceOverride": "audit" }),
            ),
        ],
    )
}
