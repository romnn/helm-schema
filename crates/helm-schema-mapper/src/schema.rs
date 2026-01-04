use crate::{Role, ValueUse, YamlPath};
use crate::vyt::{ResourceRef, VYKind, VYUse, YPath};
use color_eyre::eyre;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_K8S_SCHEMA_VERSION_DIR: &str = "v1.29.0-standalone-strict";

pub trait VytSchemaProvider {
    fn schema_for_use(&self, u: &VYUse) -> Option<Value> {
        self.schema_for_ypath(&u.path)
    }
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value>;
}

#[derive(Debug, Clone)]
pub struct IngressV1Schema;

impl VytSchemaProvider for IngressV1Schema {
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        IngressV1Schema::schema_for_ypath(self, path)
    }
}

#[derive(Debug)]
pub struct UpstreamThenDefaultVytSchemaProvider {
    pub upstream: UpstreamK8sSchemaProvider,
    pub fallback: DefaultVytSchemaProvider,
}

impl Default for UpstreamThenDefaultVytSchemaProvider {
    fn default() -> Self {
        Self {
            upstream: UpstreamK8sSchemaProvider::new(DEFAULT_K8S_SCHEMA_VERSION_DIR),
            fallback: DefaultVytSchemaProvider::default(),
        }
    }
}

impl VytSchemaProvider for UpstreamThenDefaultVytSchemaProvider {
    fn schema_for_use(&self, u: &VYUse) -> Option<Value> {
        self.upstream
            .schema_for_use(u)
            .or_else(|| self.fallback.schema_for_use(u))
    }

    fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        self.fallback.schema_for_ypath(path)
    }
}

#[derive(Debug, Clone)]
pub struct CommonK8sSchema;

impl VytSchemaProvider for CommonK8sSchema {
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        let pat = ypath_pattern(path);
        match pat.as_str() {
            "apiVersion" | "kind" => Some(type_schema("string")),
            "metadata.name" | "metadata.namespace" => Some(type_schema("string")),
            "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),

            // Service
            "spec.type" => Some(type_schema("string")),
            "spec.clusterIP" => Some(type_schema("string")),
            "spec.ports[*].name" => Some(type_schema("string")),
            "spec.ports[*].protocol" => Some(type_schema("string")),
            "spec.ports[*].port" => Some(type_schema("integer")),
            "spec.ports[*].targetPort" => Some(type_schema("integer")),
            "spec.ports[*].nodePort" => Some(type_schema("integer")),

            // Workloads
            "spec.replicas" => Some(type_schema("integer")),
            "spec.selector.matchLabels" => Some(string_map_schema()),
            "spec.template.metadata.annotations" | "spec.template.metadata.labels" => {
                Some(string_map_schema())
            }
            "spec.template.spec.serviceAccountName" => Some(type_schema("string")),
            "spec.template.spec.nodeSelector" => Some(string_map_schema()),

            // PodSpec arrays and common leaves
            "spec.template.spec.tolerations[*].key" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].operator" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].value" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].effect" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].tolerationSeconds" => Some(type_schema("integer")),

            // Container
            "spec.template.spec.containers[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].image" => Some(type_schema("string")),
            "spec.template.spec.containers[*].imagePullPolicy" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].protocol" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].containerPort" => Some(type_schema("integer")),
            "spec.template.spec.containers[*].env[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].env[*].value" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.limits.cpu" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.limits.memory" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.requests.cpu" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.requests.memory" => Some(type_schema("string")),

            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DefaultVytSchemaProvider;

impl VytSchemaProvider for DefaultVytSchemaProvider {
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        let ingress = IngressV1Schema;
        if let Some(s) = ingress.schema_for_ypath(path) {
            return Some(s);
        }

        let common = CommonK8sSchema;
        if let Some(s) = common.schema_for_ypath(path) {
            return Some(s);
        }

        let pat = ypath_pattern(path);
        match pat.as_str() {
            _ => None,
        }
    }
}

impl IngressV1Schema {
    pub fn schema_for_yaml_path(&self, path: &YamlPath) -> Option<Value> {
        let pat = yaml_path_pattern(path);
        match pat.as_str() {
            "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),
            "spec.ingressClassName" => Some(type_schema("string")),
            "spec.rules[*].host" => Some(type_schema("string")),
            "spec.tls[*].hosts[*]" => Some(type_schema("string")),
            "spec.tls[*].secretName" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].path" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].pathType" => Some(Value::Object(
                [
                    ("type".to_string(), Value::String("string".to_string())),
                    (
                        "enum".to_string(),
                        Value::Array(
                            [
                                "ImplementationSpecific",
                                "Exact",
                                "Prefix",
                            ]
                            .into_iter()
                            .map(|s| Value::String(s.to_string()))
                            .collect(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
            _ => None,
        }
    }

    pub fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        let pat = ypath_pattern(path);
        match pat.as_str() {
            "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),
            "spec.ingressClassName" => Some(type_schema("string")),
            "spec.rules[*].host" => Some(type_schema("string")),
            "spec.tls[*].hosts[*]" => Some(type_schema("string")),
            "spec.tls[*].secretName" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].path" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].pathType" => Some(Value::Object(
                [
                    ("type".to_string(), Value::String("string".to_string())),
                    (
                        "enum".to_string(),
                        Value::Array(
                            [
                                "ImplementationSpecific",
                                "Exact",
                                "Prefix",
                            ]
                            .into_iter()
                            .map(|s| Value::String(s.to_string()))
                            .collect(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
            // Ingress backend service port number (stable API)
            "spec.rules[*].http.paths[*].backend.service.port.number" => Some(type_schema("integer")),
            // Legacy API uses servicePort directly under backend
            "spec.rules[*].http.paths[*].backend.servicePort" => Some(type_schema("integer")),
            _ => None,
        }
    }
}

pub fn generate_values_schema_for_ingress(uses: &[ValueUse]) -> Value {
    let provider = IngressV1Schema;
    generate_values_schema(uses, &provider)
}

pub fn generate_values_schema_for_ingress_vyt(uses: &[VYUse]) -> Value {
    let provider = IngressV1Schema;
    generate_values_schema_vyt(uses, &provider)
}

pub fn generate_values_schema(uses: &[ValueUse], provider: &IngressV1Schema) -> Value {
    let mut by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for u in uses {
        if u.value_path.trim().is_empty() {
            continue;
        }

        // For MVP, only use entries that can be mapped to a YAML path.
        // (Guards are useful later for conditionals.)
        let Some(yaml_path) = u.yaml_path.as_ref() else {
            continue;
        };

        let mut inferred = match u.role {
            Role::ScalarValue => provider.schema_for_yaml_path(yaml_path),
            Role::Fragment => provider
                .schema_for_yaml_path(yaml_path)
                .or_else(|| Some(type_schema("object"))),
            Role::Guard | Role::MappingKey | Role::Unknown => None,
        };

        // Fallback: if we could not map, skip.
        let Some(schema) = inferred.take() else {
            continue;
        };

        by_value_path
            .entry(u.value_path.clone())
            .or_default()
            .push(schema);
    }

    // Merge per-leaf schemas and build nested schema tree.
    let mut root_schema = object_schema(Map::new());
    for (vp, schemas) in by_value_path {
        let merged = merge_schema_list(schemas);
        insert_schema_at_value_path(&mut root_schema, &vp, merged);
    }

    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        Value::String("http://json-schema.org/draft-07/schema#".to_string()),
    );
    if let Value::Object(obj) = root_schema {
        if let Some(ty) = obj.get("type").cloned() {
            out.insert("type".to_string(), ty);
        }
        if let Some(props) = obj.get("properties").cloned() {
            out.insert("properties".to_string(), props);
        }
    } else {
        out.insert("type".to_string(), Value::String("object".to_string()));
        out.insert("properties".to_string(), Value::Object(Map::new()));
    }
    Value::Object(out)
}

fn infer_fallback_schema_vyt(u: &VYUse) -> Option<Value> {
    // Common Helm convention: boolean flags.
    if u.source_expr == "installCRDs"
        || u.source_expr.ends_with(".enabled")
        || u.source_expr.ends_with("Enabled")
    {
        return Some(type_schema("boolean"));
    }

    // Common Kubernetes scalar fields inferred from placement.
    let pat = ypath_pattern(&u.path);
    match pat.as_str() {
        // Metadata maps
        "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),

        // Very common scalar numbers
        "spec.replicas" => Some(type_schema("integer")),
        _ => {
            let last = u.path.0.last().map(|s| s.as_str()).unwrap_or("");
            if matches!(
                last,
                "replicas"
                    | "replicaCount"
                    | "revisionHistoryLimit"
                    | "terminationGracePeriodSeconds"
                    | "port"
                    | "targetPort"
                    | "nodePort"
                    | "containerPort"
                    | "hostPort"
                    | "number"
            ) {
                return Some(type_schema("integer"));
            }

            // Image strings in Pods/Deployments/etc.
            if last == "image" {
                return Some(type_schema("string"));
            }

            // If the value is used under a map-y key, treat as string map.
            if u.source_expr.ends_with("annotations") || u.source_expr.ends_with("labels") {
                return Some(string_map_schema());
            }

            // Last resort for scalars: string.
            Some(type_schema("string"))
        }
    }
}

pub fn generate_values_schema_vyt<P: VytSchemaProvider>(uses: &[VYUse], provider: &P) -> Value {
    let mut by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }

        let mut inferred = match u.kind {
            VYKind::Scalar => provider
                .schema_for_use(u)
                .or_else(|| infer_fallback_schema_vyt(u)),
            VYKind::Fragment => provider.schema_for_use(u).or_else(|| {
                if u.source_expr.ends_with("annotations") || u.source_expr.ends_with("labels") {
                    Some(string_map_schema())
                } else {
                    Some(type_schema("object"))
                }
            }),
        };

        let Some(schema) = inferred.take() else {
            continue;
        };

        by_value_path
            .entry(u.source_expr.clone())
            .or_default()
            .push(schema);
    }

    let mut root_schema = object_schema(Map::new());
    for (vp, schemas) in by_value_path {
        let merged = merge_schema_list(schemas);
        insert_schema_at_value_path(&mut root_schema, &vp, merged);
    }

    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        Value::String("http://json-schema.org/draft-07/schema#".to_string()),
    );
    if let Value::Object(obj) = root_schema {
        if let Some(ty) = obj.get("type").cloned() {
            out.insert("type".to_string(), ty);
        }
        if let Some(props) = obj.get("properties").cloned() {
            out.insert("properties".to_string(), props);
        }
    } else {
        out.insert("type".to_string(), Value::String("object".to_string()));
        out.insert("properties".to_string(), Value::Object(Map::new()));
    }
    Value::Object(out)
}

fn insert_schema_at_value_path(root: &mut Value, vp: &str, leaf: Value) {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    insert_schema_at_parts(root, &parts, leaf);
}

fn insert_schema_at_parts(node: &mut Value, parts: &[&str], leaf: Value) {
    if parts.is_empty() {
        return;
    }

    // Treat '*' as an array item marker.
    if parts[0] == "*" {
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if parts.len() == 1 {
            // Path ends at the array item itself.
            let existing = std::mem::replace(items, Value::Null);
            *items = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(items, &parts[1..], leaf);
        }
        return;
    }

    ensure_object_schema(node);
    let props = node
        .as_object_mut()
        .and_then(|o| o.get_mut("properties"))
        .and_then(|v| v.as_object_mut())
        .expect("object schema must have properties");

    if parts.len() == 1 {
        let key = parts[0].to_string();
        match props.remove(&key) {
            None => {
                props.insert(key, leaf);
            }
            Some(existing) => {
                props.insert(key, merge_two_schemas(existing, leaf));
            }
        }
        return;
    }

    let key = parts[0].to_string();
    let child = props.entry(key).or_insert(Value::Null);
    insert_schema_at_parts(child, &parts[1..], leaf);
}

fn object_schema(properties: Map<String, Value>) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(properties)),
        ]
        .into_iter()
        .collect(),
    )
}

fn ensure_object_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            obj.insert("type".to_string(), Value::String("object".to_string()));
            obj.entry("properties".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !obj
                .get("properties")
                .and_then(|p| p.as_object())
                .is_some()
            {
                obj.insert("properties".to_string(), Value::Object(Map::new()));
            }
        }
        _ => {
            *v = object_schema(Map::new());
        }
    }
}

fn ensure_array_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            obj.insert("type".to_string(), Value::String("array".to_string()));
            obj.entry("items".to_string()).or_insert(Value::Null);
        }
        _ => {
            *v = Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), Value::Null),
                ]
                .into_iter()
                .collect(),
            );
        }
    }
}

fn ensure_items_schema(array_schema: &mut Value) -> &mut Value {
    let items = array_schema
        .as_object_mut()
        .and_then(|o| o.get_mut("items"))
        .expect("array schema must have items");
    items
}

fn merge_schema_list(mut schemas: Vec<Value>) -> Value {
    schemas.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    schemas.dedup();
    let mut it = schemas.into_iter();
    let Some(first) = it.next() else {
        return Value::Object(Map::new());
    };
    it.fold(first, merge_two_schemas)
}

fn merge_two_schemas(a: Value, b: Value) -> Value {
    if a == b {
        return a;
    }

    if let Some(merged) = try_merge_compatible(&a, &b) {
        return merged;
    }

    // Fallback: anyOf union (flatten + dedup).
    let mut out: Vec<Value> = Vec::new();
    out.extend(flatten_anyof(a));
    out.extend(flatten_anyof(b));
    out.sort_by(|x, y| x.to_string().cmp(&y.to_string()));
    out.dedup();
    if out.len() == 1 {
        out.into_iter().next().expect("len == 1")
    } else {
        Value::Object([("anyOf".to_string(), Value::Array(out))].into_iter().collect())
    }
}

fn flatten_anyof(v: Value) -> Vec<Value> {
    if let Value::Object(obj) = &v {
        if let Some(arr) = obj.get("anyOf").and_then(|x| x.as_array()) {
            return arr.clone();
        }
    }
    vec![v]
}

fn schema_type<'a>(v: &'a Value) -> Option<&'a str> {
    v.as_object()?
        .get("type")
        .and_then(|t| t.as_str())
}

fn try_merge_compatible(a: &Value, b: &Value) -> Option<Value> {
    let ta = schema_type(a)?;
    let tb = schema_type(b)?;
    if ta != tb {
        return None;
    }

    match ta {
        "object" => merge_object_schemas(a, b),
        _ => merge_scalar_like_schemas(a, b),
    }
}

fn merge_scalar_like_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;

    // Handle enum intersection / strengthening.
    match (out.get("enum").and_then(|v| v.as_array()).cloned(), bobj.get("enum").and_then(|v| v.as_array()).cloned()) {
        (Some(ae), Some(be)) => {
            let mut inter: Vec<Value> = ae.into_iter().filter(|v| be.contains(v)).collect();
            inter.sort_by(|x, y| x.to_string().cmp(&y.to_string()));
            inter.dedup();
            if inter.is_empty() {
                return None;
            }
            out.insert("enum".to_string(), Value::Array(inter));
        }
        (None, Some(be)) => {
            out.insert("enum".to_string(), Value::Array(be));
        }
        _ => {}
    }

    // If there are other keys beyond type/enum and they conflict, we currently treat as incompatible.
    for (k, bv) in bobj {
        if k == "type" || k == "enum" {
            continue;
        }
        match out.get(k) {
            None => {
                out.insert(k.clone(), bv.clone());
            }
            Some(av) if av == bv => {}
            _ => {
                return None;
            }
        }
    }

    Some(Value::Object(out))
}

fn merge_object_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;

    // Merge additionalProperties (stricter wins; if both present, merge).
    match (out.get("additionalProperties").cloned(), bobj.get("additionalProperties").cloned()) {
        (Some(ap_a), Some(ap_b)) => {
            out.insert("additionalProperties".to_string(), merge_two_schemas(ap_a, ap_b));
        }
        (None, Some(ap_b)) => {
            out.insert("additionalProperties".to_string(), ap_b);
        }
        _ => {}
    }

    // Merge properties recursively.
    let mut props = out
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    let bprops = bobj
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    for (k, bv) in bprops {
        match props.remove(&k) {
            None => {
                props.insert(k, bv);
            }
            Some(av) => {
                props.insert(k, merge_two_schemas(av, bv));
            }
        }
    }
    out.insert("properties".to_string(), Value::Object(props));
    out.insert("type".to_string(), Value::String("object".to_string()));

    Some(Value::Object(out))
}

fn type_schema(ty: &str) -> Value {
    Value::Object(
        [("type".to_string(), Value::String(ty.to_string()))]
            .into_iter()
            .collect(),
    )
}

fn string_map_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "additionalProperties".to_string(),
                type_schema("string"),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn yaml_path_pattern(path: &YamlPath) -> String {
    use crate::yaml_path::PathElem;

    let mut out = String::new();
    for (i, elem) in path.0.iter().enumerate() {
        match elem {
            PathElem::Key(k) => {
                if i > 0 {
                    out.push('.');
                }
                out.push_str(k);
            }
            PathElem::Index(_) => {
                out.push_str("[*]");
            }
        }
    }
    out
}

fn ypath_pattern(path: &YPath) -> String {
    path.0.join(".")
}

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
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            base_url: "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master".to_string(),
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

    pub fn schema_for_resource(&self, resource: &ResourceRef) -> eyre::Result<Value> {
        let filename = self.filename_for_resource(resource);

        // Memoize by filename within the version dir.
        let key = format!("{}/{}", self.version_dir, filename);
        if let Some(v) = self.mem.lock().expect("poisoned mutex").get(&key).cloned() {
            return Ok(v);
        }

        let local = self.local_path_for(&filename);
        if !local.exists() {
            if !self.allow_download {
                return Err(eyre::eyre!(
                    "upstream k8s schema missing and downloads disabled: {}",
                    local.display()
                ));
            }
            self.download_to_cache(&filename, &local)?;
        }

        let bytes = fs::read(&local).map_err(|e| eyre::eyre!(e))?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| eyre::eyre!(e))?;
        self.mem
            .lock()
            .expect("poisoned mutex")
            .insert(key, v.clone());
        Ok(v)
    }

    pub fn schema_for_resource_ypath(
        &self,
        resource: &ResourceRef,
        path: &YPath,
    ) -> eyre::Result<Option<Value>> {
        let root = self.schema_for_resource(resource)?;
        let filename = filename_for_resource(resource);
        let mut ctx = ResolveCtx::new(self, filename.clone(), root.clone());
        let Some((leaf_filename, leaf)) = schema_at_ypath(&mut ctx, &filename, path) else {
            return Ok(None);
        };
        let (_, expanded) = expand_schema_node(&mut ctx, &leaf_filename, &leaf, 0);
        Ok(Some(expanded))
    }

    fn filename_for_resource(&self, resource: &ResourceRef) -> String {
        filename_for_resource(resource)
    }

    fn local_path_for(&self, filename: &str) -> PathBuf {
        self.cache_dir.join(&self.version_dir).join(filename)
    }

    fn download_to_cache(&self, filename: &str, local: &Path) -> eyre::Result<()> {
        let parent = local
            .parent()
            .ok_or_else(|| eyre::eyre!("no parent dir for {}", local.display()))?;
        fs::create_dir_all(parent).map_err(|e| eyre::eyre!(e))?;

        let url = format!("{}/{}/{}", self.base_url, self.version_dir, filename);
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| eyre::eyre!("failed to download {url}: {e}"))?;
        let mut reader = resp.into_reader();
        let tmp = local.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(|e| eyre::eyre!(e))?;
            std::io::copy(&mut reader, &mut f).map_err(|e| eyre::eyre!(e))?;
        }
        fs::rename(&tmp, local).map_err(|e| eyre::eyre!(e))?;
        Ok(())
    }
}

impl VytSchemaProvider for UpstreamK8sSchemaProvider {
    fn schema_for_use(&self, u: &VYUse) -> Option<Value> {
        let r = u.resource.as_ref()?;
        self.schema_for_resource_ypath(r, &u.path).ok().flatten()
    }

    fn schema_for_ypath(&self, _path: &YPath) -> Option<Value> {
        None
    }
}

fn default_k8s_schema_cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("HELM_SCHEMA_K8S_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("helm-schema").join("kubernetes-json-schema");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    PathBuf::from(".cache").join("helm-schema").join("kubernetes-json-schema")
}

fn filename_for_resource(resource: &ResourceRef) -> String {
    let kind = resource.kind.to_ascii_lowercase();
    let (group, version) = match resource.api_version.split_once('/') {
        Some((g, v)) => (g.to_ascii_lowercase(), v.to_ascii_lowercase()),
        None => ("".to_string(), resource.api_version.to_ascii_lowercase()),
    };

    if group.is_empty() {
        format!("{}-{}.json", kind, version)
    } else {
        let group = group.replace('.', "-");
        format!("{}-{}-{}.json", kind, group, version)
    }
}

struct ResolveCtx<'a> {
    provider: &'a UpstreamK8sSchemaProvider,
    docs: HashMap<String, Value>,
    stack: HashSet<(String, String)>,
}

impl<'a> ResolveCtx<'a> {
    fn new(provider: &'a UpstreamK8sSchemaProvider, root_filename: String, root_doc: Value) -> Self {
        let mut docs = HashMap::new();
        docs.insert(root_filename, root_doc);
        Self {
            provider,
            docs,
            stack: HashSet::new(),
        }
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
        let bytes = fs::read(&local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        self.docs.insert(filename.to_string(), doc);
        self.docs.get(filename)
    }

    fn resolve_ref(&mut self, current_filename: &str, r: &str) -> Option<(String, Value)> {
        // local reference: "#/..."
        if let Some(ptr) = r.strip_prefix('#') {
            let key = (current_filename.to_string(), format!("#{}", ptr));
            if !self.stack.insert(key) {
                return None;
            }
            let doc = self.doc(current_filename)?;
            return doc.pointer(ptr).cloned().map(|v| (current_filename.to_string(), v));
        }

        // maybe: "file.json#/..."
        let (file, ptr) = r.split_once('#').unwrap_or((r, ""));
        let filename = if file.is_empty() {
            current_filename.to_string()
        } else {
            file.to_string()
        };

        let key = (filename.clone(), format!("#{}", ptr));
        if !self.stack.insert(key) {
            return None;
        }

        let doc = self.load_doc(&filename)?.clone();
        if ptr.is_empty() {
            Some((filename, doc))
        } else {
            doc.pointer(ptr)
                .cloned()
                .map(|v| (filename, v))
        }
    }
}

fn schema_at_ypath(
    ctx: &mut ResolveCtx<'_>,
    root_filename: &str,
    path: &YPath,
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

    // anyOf/oneOf/allOf: try each branch.
    if let Some(arr) = schema.get("allOf").and_then(|v| v.as_array()) {
        for s in arr {
            if let Some(v) = descend_one(ctx, &schema_filename, s, seg) {
                return Some(v);
            }
        }
    }
    if let Some(arr) = schema.get("anyOf").and_then(|v| v.as_array()) {
        for s in arr {
            if let Some(v) = descend_one(ctx, &schema_filename, s, seg) {
                return Some(v);
            }
        }
    }
    if let Some(arr) = schema.get("oneOf").and_then(|v| v.as_array()) {
        for s in arr {
            if let Some(v) = descend_one(ctx, &schema_filename, s, seg) {
                return Some(v);
            }
        }
    }

    let (key, is_array_item) = if let Some(k) = seg.strip_suffix("[*]") {
        (k, true)
    } else {
        (seg, false)
    };

    // object property
    let mut next = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .and_then(|p| p.get(key))
        .cloned()
        .or_else(|| {
            // map-like
            schema
                .get("additionalProperties")
                .and_then(|ap| if ap.is_boolean() { None } else { Some(ap.clone()) })
        })?;

    if is_array_item {
        let (nf, ns) = resolve_refs(ctx, &schema_filename, &next)?;
        next = ns;
        let doc_key = nf;
        next = next
            .get("items")
            .cloned()
            .or_else(|| next.get("prefixItems").and_then(|v| v.as_array()).and_then(|a| a.first()).cloned())?;
        return Some((doc_key, next));
    }
    Some((schema_filename, next))
}

fn resolve_refs(
    ctx: &mut ResolveCtx<'_>,
    current_filename: &str,
    schema: &Value,
) -> Option<(String, Value)> {
    // A schema may itself be a $ref.
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        return ctx.resolve_ref(current_filename, r);
    }
    Some((current_filename.to_string(), schema.clone()))
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
        if let Some((nf, target)) = ctx.resolve_ref(current_filename, r) {
            return expand_schema_node(ctx, &nf, &target, depth + 1);
        }
        return (current_filename.to_string(), schema.clone());
    }

    if let Some(arr) = schema.get("allOf").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for s in arr {
            out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
        }
        let mut obj = schema.as_object().cloned().unwrap_or_default();
        obj.insert("allOf".to_string(), Value::Array(out));
        return (current_filename.to_string(), Value::Object(obj));
    }
    if let Some(arr) = schema.get("anyOf").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for s in arr {
            out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
        }
        let mut obj = schema.as_object().cloned().unwrap_or_default();
        obj.insert("anyOf".to_string(), Value::Array(out));
        return (current_filename.to_string(), Value::Object(obj));
    }
    if let Some(arr) = schema.get("oneOf").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for s in arr {
            out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
        }
        let mut obj = schema.as_object().cloned().unwrap_or_default();
        obj.insert("oneOf".to_string(), Value::Array(out));
        return (current_filename.to_string(), Value::Object(obj));
    }

    let mut obj = match schema.as_object() {
        Some(o) => o.clone(),
        None => return (current_filename.to_string(), schema.clone()),
    };

    if let Some(props) = obj.get("properties").and_then(|v| v.as_object()) {
        let mut new_props = Map::new();
        for (k, v) in props {
            new_props.insert(k.clone(), expand_schema_node(ctx, current_filename, v, depth + 1).1);
        }
        obj.insert("properties".to_string(), Value::Object(new_props));
    }

    if let Some(items) = obj.get("items") {
        obj.insert(
            "items".to_string(),
            expand_schema_node(ctx, current_filename, items, depth + 1).1,
        );
    }

    if let Some(ap) = obj.get("additionalProperties") {
        if !ap.is_boolean() {
            obj.insert(
                "additionalProperties".to_string(),
                expand_schema_node(ctx, current_filename, ap, depth + 1).1,
            );
        }
    }

    (current_filename.to_string(), Value::Object(obj))
}
