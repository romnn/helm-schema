use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use helm_schema_ir::{ResourceRef, YamlPath};

use crate::{K8sSchemaProvider, candidate_filenames_for_resource, filename_for_resource};

/// Fetches and caches Kubernetes JSON Schemas from the
/// [yannh/kubernetes-json-schema](https://github.com/yannh/kubernetes-json-schema) repository.
#[derive(Debug)]
pub struct UpstreamK8sSchemaProvider {
    pub version_dir: String,
    pub cache_dir: PathBuf,
    pub allow_download: bool,
    pub base_url: String,

    mem: std::sync::Mutex<HashMap<String, Value>>,
}

impl UpstreamK8sSchemaProvider {
    pub fn new(version_dir: impl Into<String>) -> Self {
        Self {
            version_dir: version_dir.into(),
            cache_dir: default_k8s_schema_cache_dir(),
            allow_download: std::env::var("HELM_SCHEMA_ALLOW_NET")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            base_url: "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master"
                .to_string(),
            mem: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    pub fn with_allow_download(mut self, allow: bool) -> Self {
        self.allow_download = allow;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Load the full schema for a resource type.
    pub fn schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        self.load_resource_doc(resource).map(|(_, v)| v)
    }

    /// Load and fully expand a resource schema by resolving all reachable `$ref`s.
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let (filename, root) = self.load_resource_doc(resource)?;
        let mut ctx = ResolveCtx::new(self, filename.clone(), root);
        let root_doc = ctx.doc(&filename)?.clone();
        let (_, expanded) = expand_schema_node(&mut ctx, &filename, &root_doc, 0);
        Some(expanded)
    }

    fn load_resource_doc(&self, resource: &ResourceRef) -> Option<(String, Value)> {
        let mut candidates = candidate_filenames_for_resource(resource);
        if candidates.is_empty() {
            candidates.push(filename_for_resource(resource));
        }

        for filename in candidates {
            let key = format!("{}/{}", self.version_dir, filename);
            if let Some(v) = self.mem.lock().ok()?.get(&key).cloned() {
                return Some((filename, v));
            }

            let local = self.local_path_for(&filename);
            if !local.exists() {
                if !self.allow_download {
                    continue;
                }
                if self.download_to_cache(&filename, &local).is_err() {
                    continue;
                }
            }

            let bytes = fs::read(&local).ok()?;
            let v: Value = serde_json::from_slice(&bytes).ok()?;
            if let Ok(mut guard) = self.mem.lock() {
                guard.insert(key, v.clone());
            }
            return Some((filename, v));
        }

        if resource.api_version.trim().is_empty() {
            return self.load_resource_doc_by_kind_scan(&resource.kind);
        }

        None
    }

    fn load_resource_doc_by_kind_scan(&self, kind: &str) -> Option<(String, Value)> {
        let kind_lc = kind.to_ascii_lowercase();
        let dir = self.cache_dir.join(&self.version_dir);

        // First, scan local cache for files matching the kind.
        if let Ok(entries) = fs::read_dir(&dir) {
            for ent in entries.flatten() {
                let path = ent.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let filename = path.file_name()?.to_string_lossy().to_string();
                if !filename.starts_with(&format!("{kind_lc}-")) {
                    continue;
                }
                if let Some(result) = self.try_load_kind_file(&filename, kind) {
                    return Some(result);
                }
            }
        }

        // If downloads are enabled and nothing was found locally, try well-known
        // apiVersion patterns for this kind and attempt to download each candidate.
        if self.allow_download {
            for candidate in well_known_filenames_for_kind(kind) {
                let local = self.local_path_for(&candidate);
                if local.exists() {
                    continue; // already checked above
                }
                if self.download_to_cache(&candidate, &local).is_err() {
                    continue;
                }
                if let Some(result) = self.try_load_kind_file(&candidate, kind) {
                    return Some(result);
                }
            }
        }

        None
    }

    fn try_load_kind_file(&self, filename: &str, kind: &str) -> Option<(String, Value)> {
        let local = self.local_path_for(filename);
        let bytes = fs::read(&local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;

        let matches_kind = doc
            .get("x-kubernetes-group-version-kind")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().any(|e| {
                    e.get("kind")
                        .and_then(|v| v.as_str())
                        .is_some_and(|k| k == kind)
                })
            });

        if !matches_kind {
            return None;
        }

        let key = format!("{}/{}", self.version_dir, filename);
        if let Ok(mut guard) = self.mem.lock() {
            guard.insert(key, doc.clone());
        }
        Some((filename.to_string(), doc))
    }

    fn local_path_for(&self, filename: &str) -> PathBuf {
        self.cache_dir.join(&self.version_dir).join(filename)
    }

    fn download_to_cache(
        &self,
        filename: &str,
        local: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent = local.parent().ok_or("no parent dir")?;
        fs::create_dir_all(parent)?;

        let url = format!("{}/{}/{}", self.base_url, self.version_dir, filename);
        let resp = ureq::get(&url).call()?;
        let mut reader = resp.into_reader();
        let tmp = local.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            std::io::copy(&mut reader, &mut f)?;
        }
        fs::rename(&tmp, local)?;
        Ok(())
    }
}

impl K8sSchemaProvider for UpstreamK8sSchemaProvider {
    fn schema_for_use(&self, u: &helm_schema_ir::ValueUse) -> Option<Value> {
        let r = u.resource.as_ref()?;
        self.schema_for_resource_path(r, &u.path)
    }

    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let (filename, root) = self.load_resource_doc(resource)?;
        let mut ctx = ResolveCtx::new(self, filename.clone(), root);
        let (leaf_filename, leaf) = schema_at_ypath(&mut ctx, &filename, path)?;
        let (_, expanded) = expand_schema_node(&mut ctx, &leaf_filename, &leaf, 0);
        Some(expanded)
    }
}

// ---------------------------------------------------------------------------
// $ref resolution context
// ---------------------------------------------------------------------------

struct ResolveCtx<'a> {
    provider: &'a UpstreamK8sSchemaProvider,
    docs: HashMap<String, Value>,
    stack: HashSet<(String, String)>,
}

impl<'a> ResolveCtx<'a> {
    fn new(
        provider: &'a UpstreamK8sSchemaProvider,
        root_filename: String,
        root_doc: Value,
    ) -> Self {
        let mut docs = HashMap::new();
        docs.insert(root_filename, root_doc);
        Self {
            provider,
            docs,
            stack: HashSet::new(),
        }
    }

    fn normalize_ref_filename(current_filename: &str, file: &str) -> String {
        if file.is_empty() {
            return current_filename.to_string();
        }
        let trimmed = file.trim().trim_start_matches("./");
        trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
    }

    fn doc(&self, filename: &str) -> Option<&Value> {
        self.docs.get(filename)
    }

    fn load_doc(&mut self, filename: &str) -> Option<&Value> {
        if self.docs.contains_key(filename) {
            return self.docs.get(filename);
        }

        let local = self
            .provider
            .cache_dir
            .join(&self.provider.version_dir)
            .join(filename);

        if !local.exists() {
            if !self.provider.allow_download {
                return None;
            }
            let _ = self.provider.download_to_cache(filename, &local);
        }

        let bytes = fs::read(&local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        self.docs.insert(filename.to_string(), doc);
        self.docs.get(filename)
    }

    fn resolve_ref(&mut self, current_filename: &str, r: &str) -> Option<(String, Value)> {
        if let Some(ptr) = r.strip_prefix('#') {
            let doc = self.doc(current_filename)?;
            return doc
                .pointer(ptr)
                .cloned()
                .map(|v| (current_filename.to_string(), v));
        }

        let (file, ptr) = r.split_once('#').unwrap_or((r, ""));
        let filename = Self::normalize_ref_filename(current_filename, file);

        let doc = self.load_doc(&filename)?.clone();
        if ptr.is_empty() {
            Some((filename, doc))
        } else {
            doc.pointer(ptr).cloned().map(|v| (filename, v))
        }
    }
}

// ---------------------------------------------------------------------------
// Schema navigation
// ---------------------------------------------------------------------------

fn resolve_refs(
    ctx: &mut ResolveCtx<'_>,
    current_filename: &str,
    schema: &Value,
) -> Option<(String, Value)> {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        return ctx.resolve_ref(current_filename, r);
    }
    Some((current_filename.to_string(), schema.clone()))
}

fn schema_at_ypath(
    ctx: &mut ResolveCtx<'_>,
    root_filename: &str,
    path: &YamlPath,
) -> Option<(String, Value)> {
    let mut cur = ctx.doc(root_filename)?.clone();
    let mut cur_filename = root_filename.to_string();
    for seg in &path.0 {
        let (nf, ns) = descend_one(ctx, &cur_filename, &cur, seg)?;
        cur_filename = nf;
        cur = ns;
    }
    Some((cur_filename, cur))
}

fn descend_one(
    ctx: &mut ResolveCtx<'_>,
    current_filename: &str,
    schema: &Value,
    seg: &str,
) -> Option<(String, Value)> {
    let (schema_filename, schema) = resolve_refs(ctx, current_filename, schema)?;

    for keyword in &["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = schema.get(*keyword).and_then(|v| v.as_array()) {
            for s in arr {
                if let Some(v) = descend_one(ctx, &schema_filename, s, seg) {
                    return Some(v);
                }
            }
        }
    }

    let (key, is_array_item) = if let Some(k) = seg.strip_suffix("[*]") {
        (k, true)
    } else {
        (seg, false)
    };

    let mut next = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .and_then(|p| p.get(key))
        .cloned()
        .or_else(|| {
            schema.get("additionalProperties").and_then(|ap| {
                if ap.is_boolean() {
                    None
                } else {
                    Some(ap.clone())
                }
            })
        })?;

    if is_array_item {
        let (nf, ns) = resolve_refs(ctx, &schema_filename, &next)?;
        next = ns;
        let doc_key = nf;
        next = next.get("items").cloned().or_else(|| {
            next.get("prefixItems")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .cloned()
        })?;
        return Some((doc_key, next));
    }
    Some((schema_filename, next))
}

fn strip_ref(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = obj.clone();
    out.remove("$ref");
    Value::Object(out)
}

fn expand_schema_node(
    ctx: &mut ResolveCtx<'_>,
    current_filename: &str,
    schema: &Value,
    depth: usize,
) -> (String, Value) {
    if depth > 64 {
        return (current_filename.to_string(), schema.clone());
    }

    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let key = if let Some(ptr) = r.strip_prefix('#') {
            (current_filename.to_string(), format!("#{ptr}"))
        } else {
            let (file, ptr) = r.split_once('#').unwrap_or((r, ""));
            let filename = ResolveCtx::normalize_ref_filename(current_filename, file);
            (filename, format!("#{ptr}"))
        };

        if ctx.stack.contains(&key) {
            return (current_filename.to_string(), strip_ref(schema));
        }
        ctx.stack.insert(key.clone());

        let out = if let Some((nf, target)) = ctx.resolve_ref(current_filename, r) {
            expand_schema_node(ctx, &nf, &target, depth + 1)
        } else {
            (current_filename.to_string(), strip_ref(schema))
        };

        ctx.stack.remove(&key);
        return out;
    }

    for keyword in &["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = schema.get(*keyword).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for s in arr {
                out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
            }
            let mut obj = schema.as_object().cloned().unwrap_or_default();
            obj.insert(keyword.to_string(), Value::Array(out));
            return (current_filename.to_string(), Value::Object(obj));
        }
    }

    let mut obj = match schema.as_object() {
        Some(o) => o.clone(),
        None => return (current_filename.to_string(), schema.clone()),
    };

    for prop_key in &["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(props) = obj.get(*prop_key).and_then(|v| v.as_object()) {
            let mut new_props = Map::new();
            for (k, v) in props {
                new_props.insert(
                    k.clone(),
                    expand_schema_node(ctx, current_filename, v, depth + 1).1,
                );
            }
            obj.insert(prop_key.to_string(), Value::Object(new_props));
        }
    }

    for single_key in &["items", "contains", "not", "if", "then", "else"] {
        if let Some(sub) = obj.get(*single_key) {
            let sub = sub.clone();
            obj.insert(
                single_key.to_string(),
                expand_schema_node(ctx, current_filename, &sub, depth + 1).1,
            );
        }
    }

    for array_key in &["prefixItems"] {
        if let Some(arr) = obj.get(*array_key).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for s in arr {
                out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
            }
            obj.insert(array_key.to_string(), Value::Array(out));
        }
    }

    if let Some(ds) = obj.get("dependentSchemas").and_then(|v| v.as_object()) {
        let mut out = Map::new();
        for (k, v) in ds {
            out.insert(
                k.clone(),
                expand_schema_node(ctx, current_filename, v, depth + 1).1,
            );
        }
        obj.insert("dependentSchemas".to_string(), Value::Object(out));
    }

    if let Some(ap) = obj.get("additionalProperties")
        && !ap.is_boolean()
    {
        let ap = ap.clone();
        obj.insert(
            "additionalProperties".to_string(),
            expand_schema_node(ctx, current_filename, &ap, depth + 1).1,
        );
    }

    (current_filename.to_string(), Value::Object(obj))
}

/// When the apiVersion is unknown (empty), generate candidate filenames by
/// trying common API group + version combinations for the given kind.
fn well_known_filenames_for_kind(kind: &str) -> Vec<String> {
    let kind_lc = kind.to_ascii_lowercase();

    // Well-known apiVersion mappings for core K8s resource kinds.
    let api_versions: &[&str] = match kind {
        // Core API (v1)
        "Service"
        | "ConfigMap"
        | "Secret"
        | "Pod"
        | "Namespace"
        | "Node"
        | "PersistentVolume"
        | "PersistentVolumeClaim"
        | "ServiceAccount"
        | "Endpoints"
        | "Event"
        | "LimitRange"
        | "ResourceQuota"
        | "ReplicationController" => &["v1"],

        // apps/v1
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" => &["apps/v1"],

        // batch/v1
        "Job" | "CronJob" => &["batch/v1"],

        // networking.k8s.io/v1
        "NetworkPolicy" | "Ingress" | "IngressClass" => &["networking.k8s.io/v1"],

        // rbac.authorization.k8s.io/v1
        "Role" | "RoleBinding" | "ClusterRole" | "ClusterRoleBinding" => {
            &["rbac.authorization.k8s.io/v1"]
        }

        // policy/v1
        "PodDisruptionBudget" => &["policy/v1"],

        // autoscaling/v2
        "HorizontalPodAutoscaler" => &["autoscaling/v2", "autoscaling/v1"],

        // storage.k8s.io/v1
        "StorageClass" => &["storage.k8s.io/v1"],

        _ => &[],
    };

    let mut candidates = Vec::new();
    for api_version in api_versions {
        let resource = ResourceRef {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
        };
        for f in candidate_filenames_for_resource(&resource) {
            if !candidates.contains(&f) {
                candidates.push(f);
            }
        }
    }

    // As a last resort, try just `<kind>-v1.json` (covers core resources).
    let fallback = format!("{kind_lc}-v1.json");
    if !candidates.contains(&fallback) {
        candidates.push(fallback);
    }

    candidates
}

fn default_k8s_schema_cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("HELM_SCHEMA_K8S_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    PathBuf::from(".cache")
        .join("helm-schema")
        .join("kubernetes-json-schema")
}
