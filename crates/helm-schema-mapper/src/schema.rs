use crate::{Role, ValueUse, YamlPath};
use crate::vyt::{VYKind, VYUse, YPath};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub trait VytSchemaProvider {
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value>;
}

#[derive(Debug, Clone)]
pub struct IngressV1Schema;

impl VytSchemaProvider for IngressV1Schema {
    fn schema_for_ypath(&self, path: &YPath) -> Option<Value> {
        IngressV1Schema::schema_for_ypath(self, path)
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
                .schema_for_ypath(&u.path)
                .or_else(|| infer_fallback_schema_vyt(u)),
            VYKind::Fragment => provider.schema_for_ypath(&u.path).or_else(|| {
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
