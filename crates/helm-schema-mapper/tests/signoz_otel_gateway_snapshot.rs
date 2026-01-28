#![recursion_limit = "4096"]

use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::vyt;
use serde_json::{json, Map, Value};
use std::cmp::Ordering;
use std::path::PathBuf;
use test_util::prelude::*;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn canonicalize_json(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(val) = obj.get(k) {
                    out.insert(k.clone(), canonicalize_json(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        _ => v.clone(),
    }
}

fn use_sort_key(u: &vyt::VYUse) -> (String, String, String, Vec<String>, Option<(String, String)>) {
    let kind = match u.kind {
        vyt::VYKind::Scalar => "Scalar",
        vyt::VYKind::Fragment => "Fragment",
    }
    .to_string();

    let resource = u
        .resource
        .as_ref()
        .map(|r| (r.api_version.clone(), r.kind.clone()));

    (
        u.source_expr.clone(),
        u.path.0.join("."),
        kind,
        u.guards.clone(),
        resource,
    )
}

fn cmp_uses(a: &vyt::VYUse, b: &vyt::VYUse) -> Ordering {
    use_sort_key(a).cmp(&use_sort_key(b))
}

fn json_pretty(v: &Value) -> eyre::Result<String> {
    let v = canonicalize_json(v);
    Ok(serde_json::to_string_pretty(&v).map_err(|e| eyre::eyre!(e))?)
}

fn ir_json_sort_key(v: &Value) -> (String, String, String, Vec<String>, Option<(String, String)>) {
    let obj = v.as_object().expect("ir array elements must be objects");
    let source_expr = obj
        .get("source_expr")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let path = obj
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let kind = obj
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let guards = obj
        .get("guards")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let resource = obj.get("resource").and_then(|r| {
        let r = r.as_object()?;
        Some((
            r.get("apiVersion")?.as_str()?.to_string(),
            r.get("kind")?.as_str()?.to_string(),
        ))
    });

    (source_expr, path, kind, guards, resource)
}

fn sort_ir_json_array_in_place(v: &mut Value) {
    let arr = v.as_array_mut().expect("expected ir json to be an array");
    arr.sort_by(|a, b| ir_json_sort_key(a).cmp(&ir_json_sort_key(b)));
}

fn uses_to_json(uses: &[vyt::VYUse]) -> Value {
    let mut out: Vec<Value> = Vec::with_capacity(uses.len());
    for u in uses {
        let mut m = Map::new();
        m.insert("source_expr".to_string(), Value::String(u.source_expr.clone()));
        m.insert("path".to_string(), Value::String(u.path.0.join(".")));
        m.insert(
            "kind".to_string(),
            Value::String(match u.kind {
                vyt::VYKind::Scalar => "Scalar".to_string(),
                vyt::VYKind::Fragment => "Fragment".to_string(),
            }),
        );
        m.insert(
            "guards".to_string(),
            Value::Array(u.guards.iter().cloned().map(Value::String).collect()),
        );
        if let Some(r) = &u.resource {
            let mut rm = Map::new();
            rm.insert("apiVersion".to_string(), Value::String(r.api_version.clone()));
            rm.insert("kind".to_string(), Value::String(r.kind.clone()));
            m.insert("resource".to_string(), Value::Object(rm));
        } else {
            m.insert("resource".to_string(), Value::Null);
        }
        out.push(Value::Object(m));
    }
    Value::Array(out)
}

fn run_vyt_on_chart_templates(chart: &helm_schema_chart::ChartSummary) -> eyre::Result<Vec<vyt::VYUse>> {
    let mut defs = vyt::DefineIndex::default();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        vyt::extend_define_index_from_str(&mut defs, &src)
            .wrap_err_with(|| format!("index defines in {}", p.as_str()))?;
    }
    let defs = std::sync::Arc::new(defs);

    let mut all: Vec<vyt::VYUse> = Vec::new();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        let parsed = helm_schema_template::parse::parse_gotmpl_document(&src)
            .ok_or_eyre("template parse returned None")
            .wrap_err_with(|| format!("parse template {}", p.as_str()))?;

        let mut uses = vyt::VYT::new(src)
            .with_defines(std::sync::Arc::clone(&defs))
            .run(&parsed.tree);
        all.append(&mut uses);
    }

    Ok(all)
}

fn expected_full_ir() -> Value {
    json!([
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "affinity"
        },
        {
            "guards": ["affinity"],
            "kind": "Fragment",
            "path": "spec.template.spec.affinity",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "affinity"
        },
        {
            "guards": ["affinity"],
            "kind": "Scalar",
            "path": "spec.template.spec.affinity",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "affinity"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "args"
        },
        {
            "guards": ["args"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].args[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "args.*"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": ["config"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": ["config", "config"],
            "kind": "Fragment",
            "path": "[*]",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": ["config"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": ["config", "config"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "config"
        },
        {
            "guards": ["config.create"],
            "kind": "Fragment",
            "path": "data.config.yaml",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "config"
        },
        {
            "guards": ["config.create"],
            "kind": "Scalar",
            "path": "data.config.yaml",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "config"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "config.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "config.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "env"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "env"
        },
        {
            "guards": ["env"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "env"
        },
        {
            "guards": ["env"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "env"
        },
        {
            "guards": ["env"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "env"
        },
        {
            "guards": ["externalSecrets.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.create"
        },
        {
            "guards": ["externalSecrets.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secretStoreRef"
        },
        {
            "guards": ["externalSecrets.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secretStoreRef.kind"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secretStoreRef.kind"
        },
        {
            "guards": ["externalSecrets.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secretStoreRef.name"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secretStoreRef.name"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Fragment",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Fragment",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "externalSecrets.secrets"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets.dataFrom"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "spec.dataFrom[*].extract.key",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.dataFrom"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Fragment",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Fragment",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "[*].secretKeyRef.key",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env.key"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "valueFrom.secretKeyRef.key",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets.env.key"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "[*].secretKeyRef.name",
            "resource": null,
            "source_expr": "externalSecrets.secrets.env.name"
        },
        {
            "guards": [
                "externalSecrets.create",
                "externalSecrets.secrets",
                "externalSecrets.secrets.env",
                "externalSecrets.secrets.env"
            ],
            "kind": "Scalar",
            "path": "valueFrom.secretKeyRef.name",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "externalSecrets.secrets.env.name"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "metadata.name",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.fullName"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "spec.target.name",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.fullName"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "metadata.name",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.name"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "spec.target.name",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.name"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "metadata.namespace",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.namespace"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "spec.secretStoreRef.kind",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.secretStoreKind"
        },
        {
            "guards": ["externalSecrets.create", "externalSecrets.secrets"],
            "kind": "Scalar",
            "path": "spec.secretStoreRef.name",
            "resource": { "apiVersion": "external-secrets.io/v1beta1", "kind": "ExternalSecret" },
            "source_expr": "externalSecrets.secrets.secretStoreName"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "fullnameOverride"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec[*]",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "hosts.*.host"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].imagePullPolicy",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.initContainers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.initContainers[*].imagePullPolicy",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].imagePullPolicy",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.pullPolicy"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.initContainers[*].imagePullPolicy",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.pullPolicy"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.repository"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.initContainers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.repository"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.tag"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.initContainers[*].image",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "image.tag"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "imagePullSecrets"
        },
        {
            "guards": ["imagePullSecrets"],
            "kind": "Fragment",
            "path": "spec.template.spec.imagePullSecrets",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "imagePullSecrets"
        },
        {
            "guards": ["imagePullSecrets"],
            "kind": "Scalar",
            "path": "spec.template.spec.imagePullSecrets",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "imagePullSecrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "ingress"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress"
        },
        {
            "guards": ["ingress", "ingress"],
            "kind": "Scalar",
            "path": "metadata.annotations",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "ingress"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.annotations"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "metadata.name",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "ingress.fullName"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.hosts"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.ingressClassName"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec.ingressClassName",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "ingress.ingressClassName"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.name"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "metadata.name",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "ingress.name"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.paths"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "ingress.tls"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": "spec.template.spec.containers[*].livenessProbe",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "livenessProbe"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].livenessProbe",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "livenessProbe"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["config.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["config.create", "fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["config.create", "fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ConfigMap" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["externalSecrets.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["externalSecrets.create", "fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["externalSecrets.create", "fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create", "fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create", "fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create", "fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create", "fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["serviceAccount.create", "serviceAccount.create", "fullnameOverride"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "nameOverride"
        },
        {
            "guards": [
                "serviceAccount.create",
                "serviceAccount.create",
                "fullnameOverride",
                "nameOverride"
            ],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": ["fullnameOverride", "nameOverride"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nodeSelector"
        },
        {
            "guards": ["nodeSelector"],
            "kind": "Fragment",
            "path": "spec.template.spec.nodeSelector",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nodeSelector"
        },
        {
            "guards": ["nodeSelector"],
            "kind": "Scalar",
            "path": "spec.template.spec.nodeSelector",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "nodeSelector"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec[*].http.paths[*]",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "paths.*.path"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec[*].http.paths[*].pathType",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "paths.*.pathType"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec[*].http.paths[*].backend.service.port.number",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "paths.*.port"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podAnnotations"
        },
        {
            "guards": ["podAnnotations"],
            "kind": "Fragment",
            "path": "spec.template.metadata.annotations",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podAnnotations"
        },
        {
            "guards": ["podAnnotations"],
            "kind": "Scalar",
            "path": "spec.template.metadata.annotations",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podAnnotations"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podLabels"
        },
        {
            "guards": ["podLabels"],
            "kind": "Fragment",
            "path": "spec.template.metadata.labels",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podLabels"
        },
        {
            "guards": ["podLabels"],
            "kind": "Scalar",
            "path": "spec.template.metadata.labels",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podLabels"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": "spec.template.spec.securityContext",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podSecurityContext"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.securityContext",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "podSecurityContext"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": "spec.template.spec.containers[*].readinessProbe",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "readinessProbe"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].readinessProbe",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "readinessProbe"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.replicas",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "replicaCount"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": "spec.template.spec.containers[*].resources",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "resources"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].resources",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "resources"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": "spec.template.spec.containers[*].securityContext",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "securityContext"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].securityContext",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "securityContext"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "service"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.type",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.annotations"
        },
        {
            "guards": ["service.annotations", "service.annotations"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.annotations"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*]",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].port",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].port",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].protocol",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].protocol",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].targetPort",
            "resource": null,
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].targetPort",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].ports[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].ports[*].containerPort",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].ports[*].protocol",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].ports[*].containerPort",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports.containerPort"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "service.ports.enabled"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports.enabled"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports.enabled"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": null,
            "source_expr": "service.ports.nodePort"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports.nodePort"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].protocol",
            "resource": null,
            "source_expr": "service.ports.protocol"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].protocol",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports.protocol"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].ports[*].protocol",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "service.ports.protocol"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].port",
            "resource": null,
            "source_expr": "service.ports.servicePort"
        },
        {
            "guards": ["service.ports", "service.ports.enabled"],
            "kind": "Scalar",
            "path": "[*].port",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.ports.servicePort"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "service.type"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.type"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": null,
            "source_expr": "service.type"
        },
        {
            "guards": ["service.ports", "service.ports.enabled", "service.type"],
            "kind": "Scalar",
            "path": "[*].nodePort",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.type"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "spec.type",
            "resource": { "apiVersion": "v1", "kind": "Service" },
            "source_expr": "service.type"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "serviceAccount"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "serviceAccount"
        },
        {
            "guards": ["serviceAccount.create", "serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "automountServiceAccountToken",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.annotations"
        },
        {
            "guards": ["serviceAccount.create", "serviceAccount.annotations"],
            "kind": "Fragment",
            "path": "metadata.annotations",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.annotations"
        },
        {
            "guards": ["serviceAccount.create", "serviceAccount.annotations"],
            "kind": "Scalar",
            "path": "metadata.annotations",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.annotations"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "automountServiceAccountToken",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.automount"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "serviceAccount.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "serviceAccount.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "serviceAccount.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.create"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": null,
            "source_expr": "serviceAccount.name"
        },
        {
            "guards": ["serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "serviceAccount.name"
        },
        {
            "guards": ["serviceAccount.create", "serviceAccount.create"],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "v1", "kind": "ServiceAccount" },
            "source_expr": "serviceAccount.name"
        },
        {
            "guards": ["ingress"],
            "kind": "Fragment",
            "path": "spec",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "tls"
        },
        {
            "guards": ["ingress"],
            "kind": "Fragment",
            "path": "spec.tls",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "tls"
        },
        {
            "guards": ["ingress"],
            "kind": "Scalar",
            "path": "spec.tls",
            "resource": { "apiVersion": "networking.k8s.io/v1", "kind": "Ingress" },
            "source_expr": "tls"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "tolerations"
        },
        {
            "guards": ["tolerations"],
            "kind": "Fragment",
            "path": "spec.template.spec.tolerations",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "tolerations"
        },
        {
            "guards": ["tolerations"],
            "kind": "Scalar",
            "path": "spec.template.spec.tolerations",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "tolerations"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumeMounts"
        },
        {
            "guards": ["volumeMounts"],
            "kind": "Fragment",
            "path": "spec.template.spec.containers[*].volumeMounts[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumeMounts"
        },
        {
            "guards": ["volumeMounts"],
            "kind": "Scalar",
            "path": "spec.template.spec.containers[*].volumeMounts[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumeMounts"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": "",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumes"
        },
        {
            "guards": ["volumes"],
            "kind": "Fragment",
            "path": "spec.template.spec.volumes[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumes"
        },
        {
            "guards": ["volumes"],
            "kind": "Scalar",
            "path": "spec.template.spec.volumes[*]",
            "resource": { "apiVersion": "apps/v1", "kind": "Deployment" },
            "source_expr": "volumes"
        }
    ])
}

#[test]
fn signoz_otel_gateway_expected_full_ir() -> eyre::Result<()> {
    Builder::default().build();

    let chart_root_path = crate_root().join("testdata/charts/signoz-signoz/charts/signoz-otel-gateway");
    if !chart_root_path.exists() {
        return Ok(());
    }

    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(env!("CARGO_MANIFEST_DIR")))
        .join("testdata/charts/signoz-signoz/charts/signoz-otel-gateway")?;

    let chart = load_chart(
        &chart_dir,
        &LoadOptions {
            include_tests: false,
            recurse_subcharts: false,
            auto_extract_tgz: true,
            respect_gitignore: false,
            include_hidden: false,
            ..Default::default()
        },
    )?;

    let mut uses = run_vyt_on_chart_templates(&chart)?;
    uses.sort_by(cmp_uses);

    let mut actual_ir_json = uses_to_json(&uses);
    sort_ir_json_array_in_place(&mut actual_ir_json);
    let actual_pretty = json_pretty(&actual_ir_json)?;

    let mut expected_ir_json = expected_full_ir();
    sort_ir_json_array_in_place(&mut expected_ir_json);
    let expected_pretty = json_pretty(&expected_ir_json)?;

    sim_assert_eq!(expected_pretty, actual_pretty);

    Ok(())
}
