mod merge;
pub mod required_inference;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{Guard, ValueKind, ValueUse};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use merge::{merge_schema_list, merge_two_schemas};

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Generates a JSON Schema for Helm `values.yaml` from IR and a K8s schema provider.
pub trait ValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Default values schema generator.
///
/// Collects all `.Values.*` uses, infers their types from the K8s schema
/// provider and heuristics, merges conflicting schemas, and builds a nested
/// JSON Schema tree.
pub struct DefaultValuesSchemaGenerator;

impl ValuesSchemaGenerator for DefaultValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
        generate_values_schema(uses, provider)
    }
}

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

pub fn generate_values_schema(uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
    generate_values_schema_with_values_yaml(uses, provider, None)
}

/// Generate a JSON Schema for Helm values.
///
/// If `values_yaml` is provided, it is used as an additional type signal:
/// scalar types found in `values.yaml` may be preferred over provider-derived
/// schemas in some cases (e.g. when values are scalar presets expanded into
/// full objects by templates).
pub fn generate_values_schema_with_values_yaml(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
) -> Value {
    generate_values_schema_full(uses, provider, values_yaml, &BTreeMap::new())
}

/// Generate a JSON Schema with all available signals.
///
/// Extends [`generate_values_schema_with_values_yaml`] with one extra
/// input:
///
/// - `type_hints`: per-path JSON Schema fragments inferred from `default
///   <literal> .Values.X` patterns in templates (see
///   [`helm_schema_ir::extract_default_type_hints`]). When values.yaml ships a
///   null default for a hinted path, the null is preserved alongside the
///   hint, producing a nullable union. Literal-only because we need the
///   type to build the schema fragment.
///
/// The output schema has no `required` arrays inferred by helm-schema;
/// callers that want that behaviour layer
/// [`required_inference::apply_required_inference`] on top of the
/// returned schema. Keeping required-inference outside this function
/// isolates a heuristic feature from the core schema-generation
/// pipeline.
pub fn generate_values_schema_full(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
    type_hints: &BTreeMap<String, Vec<Value>>,
) -> Value {
    let mut referenced_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut ranged_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut value_paths_used_as_fragment: BTreeSet<String> = BTreeSet::new();
    let mut provider_schemas_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut metadata_schemas_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut guard_boolish_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut guard_constraints_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    // When a value is rendered inside the body guarded by its own truthiness,
    // that body use is stronger evidence than the guard itself. The guard only
    // says "non-empty", while the body proves the value participates in a
    // scalar rendering position and should not be forced to boolean from the
    // control flow alone.
    let scalar_paths_with_self_truthy_output_use: BTreeSet<String> = uses
        .iter()
        .filter(|u| {
            u.kind == ValueKind::Scalar
                && u.guards
                    .iter()
                    .any(|g| matches!(g, Guard::Truthy { path } if path == &u.source_expr))
        })
        .map(|u| u.source_expr.clone())
        .collect();

    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }

        referenced_value_paths.insert(u.source_expr.clone());
        if u.kind == ValueKind::Fragment {
            value_paths_used_as_fragment.insert(u.source_expr.clone());
        }
        for g in &u.guards {
            for path in g.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                referenced_value_paths.insert(path.to_string());
                if matches!(g, Guard::Range { .. }) {
                    ranged_value_paths.insert(path.to_string());
                }

                if scalar_paths_with_self_truthy_output_use.contains(path) {
                    continue;
                }

                if let Some(schema) = infer_guard_boolish_schema(g) {
                    guard_boolish_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
                if let Some(schema) = infer_guard_constraint_schema(g) {
                    guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
            }
        }

        if !u.path.0.is_empty()
            && let Some(schema) = provider.schema_for_use(u)
        {
            provider_schemas_by_value_path
                .entry(u.source_expr.clone())
                .or_default()
                .push(schema);
        }

        if let Some(schema) = infer_metadata_path_schema(&u.path.0) {
            metadata_schemas_by_value_path
                .entry(u.source_expr.clone())
                .or_default()
                .push(schema);
        }
    }

    let nullable_paths = collect_nullable_value_paths(uses);
    let default_fallback_paths = collect_default_fallback_value_paths(uses);
    let paths_with_descendants = collect_paths_with_descendants(&referenced_value_paths);

    let values_yaml_doc = values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let mut root_schema = object_schema(Map::new());
    for vp in referenced_value_paths {
        let used_as_fragment = value_paths_used_as_fragment.contains(&vp);
        let provider_schemas = provider_schemas_by_value_path
            .remove(&vp)
            .unwrap_or_default();
        let provider_schema =
            if provider_schemas.len() > 1 && provider_schemas.iter().all(is_string_like_schema) {
                type_schema("string")
            } else {
                merge_schema_list(provider_schemas)
            };
        let metadata_schema = metadata_schemas_by_value_path
            .remove(&vp)
            .map_or_else(empty_schema, merge_schema_list);
        let provider_schema = merge_schema_list(vec![provider_schema, metadata_schema]);

        // Preserve an explicit `null` default when the template proves the
        // path is optional and type hints or provider evidence can still
        // describe the non-null branch. This keeps `null` as an accepted input
        // value without letting a null placeholder erase the richer schema we
        // inferred from rendered usage.
        let path_is_nullable = nullable_paths.contains(&vp)
            || default_fallback_paths.contains(&vp)
            || type_hints.contains_key(&vp);
        let preserve_explicit_null_default =
            path_is_nullable && is_null_at_value_path(&values_yaml_doc, &vp);
        let values_yaml_schema = if used_as_fragment
            && schema_type(&provider_schema) == Some("object")
            && is_empty_map_at_value_path(&values_yaml_doc, &vp)
        {
            // An empty mapping default (`foo: {}`) tells us that the chart value exists and is
            // object-shaped, but it contributes no key/value contract of its own. When static
            // analysis already proves the value is spliced as a YAML fragment into an object
            // field, keep the richer provider object schema instead of degrading it to a generic
            // open object.
            empty_schema()
        } else {
            lookup_values_yaml_schema(&values_yaml_doc, &vp).unwrap_or_else(empty_schema)
        };
        let values_yaml_schema = if ranged_value_paths.contains(&vp)
            && is_mapping_at_value_path(&values_yaml_doc, &vp)
        {
            generalize_fixed_object_schema_to_open_map(values_yaml_schema)
        } else {
            values_yaml_schema
        };

        let guard_boolish_schema = guard_boolish_by_value_path
            .remove(&vp)
            .map_or_else(empty_schema, merge_schema_list);

        let guard_constraint_schema = guard_constraints_by_value_path
            .remove(&vp)
            .map_or_else(empty_schema, merge_schema_list);

        let type_hint_schema = type_hints
            .get(&vp)
            .cloned()
            .map_or_else(empty_schema, merge_schema_list);

        let merged = resolve_schema_for_value_path(
            paths_with_descendants.contains(&vp),
            used_as_fragment,
            provider_schema,
            values_yaml_schema,
            guard_boolish_schema,
            guard_constraint_schema,
            type_hint_schema,
        );
        let merged = if is_scalar_like_schema(&merged)
            && (preserve_explicit_null_default
                || default_fallback_paths.contains(&vp)
                || nullable_paths.contains(&vp))
            && !is_empty_schema(&merged)
        {
            merge_two_schemas(merged, type_schema("null"))
        } else {
            merged
        };

        insert_schema_at_value_path(&mut root_schema, &vp, merged);
    }

    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        Value::String("http://json-schema.org/draft-07/schema#".to_string()),
    );

    if let Value::Object(obj) = root_schema {
        for (k, v) in obj {
            out.insert(k, v);
        }
    } else {
        out.insert("type".to_string(), Value::String("object".to_string()));
        out.insert("properties".to_string(), Value::Object(Map::new()));
        out.insert("additionalProperties".to_string(), Value::Bool(false));
    }
    Value::Object(out)
}

// ---------------------------------------------------------------------------
// Inference helpers
// ---------------------------------------------------------------------------

fn schema_type(v: &Value) -> Option<&str> {
    v.as_object()?.get("type")?.as_str()
}

fn is_scalar_schema(v: &Value) -> bool {
    matches!(
        schema_type(v),
        Some("string" | "integer" | "number" | "boolean")
    )
}

fn is_string_like_schema(v: &Value) -> bool {
    if schema_type(v) == Some("string") {
        return true;
    }

    let Some(obj) = v.as_object() else {
        return false;
    };

    if let Some(Value::Array(types)) = obj.get("type") {
        return types
            .iter()
            .all(|value| matches!(value.as_str(), Some("string" | "null")));
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get(key) {
            return !variants.is_empty() && variants.iter().all(is_string_like_schema);
        }
    }

    false
}

fn infer_metadata_path_schema(path: &[String]) -> Option<Value> {
    let last = path.last()?.as_str();
    let prev = path.get(path.len().checked_sub(2)?)?.as_str();
    if prev != "metadata" {
        return None;
    }

    match last {
        "labels" | "annotations" => Some(string_map_schema()),
        "name" | "namespace" => Some(type_schema("string")),
        _ => None,
    }
}

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}

fn is_scalar_like_schema(v: &Value) -> bool {
    if is_scalar_schema(v) {
        return true;
    }

    let Some(obj) = v.as_object() else {
        return false;
    };

    if let Some(Value::Array(types)) = obj.get("type") {
        return types.iter().all(|value| {
            matches!(
                value.as_str(),
                Some("string" | "number" | "integer" | "boolean" | "null")
            )
        });
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get(key) {
            return !variants.is_empty() && variants.iter().all(is_scalar_like_schema);
        }
    }

    false
}

fn is_object_or_array_schema(v: &Value) -> bool {
    matches!(schema_type(v), Some("object" | "array"))
}

fn is_fixed_object_schema(v: &Value) -> bool {
    if schema_type(v) != Some("object") {
        return false;
    }
    let Some(obj) = v.as_object() else {
        return false;
    };
    obj.get("properties")
        .and_then(Value::as_object)
        .is_some_and(|props| !props.is_empty())
        && obj.get("additionalProperties") == Some(&Value::Bool(false))
}

fn is_open_string_map_schema(v: &Value) -> bool {
    if schema_type(v) != Some("object") {
        return false;
    }
    let Some(obj) = v.as_object() else {
        return false;
    };
    matches!(
        obj.get("additionalProperties"),
        Some(Value::Object(map))
            if map.get("type").and_then(Value::as_str) == Some("string")
    )
}

fn schema_allows_scalar_type(schema: &Value, scalar_ty: &str) -> bool {
    if let Some(ty) = schema_type(schema) {
        return ty == scalar_ty;
    }

    let Some(obj) = schema.as_object() else {
        return false;
    };

    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(variants)) = obj.get(key)
            && variants
                .iter()
                .any(|v| schema_allows_scalar_type(v, scalar_ty))
        {
            return true;
        }
    }

    false
}

fn resolve_schema_for_value_path(
    has_referenced_descendants: bool,
    used_as_fragment: bool,
    provider_schema: Value,
    values_yaml_schema: Value,
    guard_boolish_schema: Value,
    guard_constraint_schema: Value,
    type_hint_schema: Value,
) -> Value {
    let base = if !is_empty_schema(&provider_schema) {
        if is_empty_schema(&values_yaml_schema) {
            provider_schema
        } else {
            // Some charts use scalar "preset" values that are fed into helpers which
            // expand into full K8s objects in the rendered manifest (e.g. affinity presets).
            // In these cases the *input* type in values.yaml is the scalar, not the output
            // object type, so prefer the values.yaml scalar schema.
            if has_referenced_descendants
                && is_fixed_object_schema(&values_yaml_schema)
                && is_scalar_schema(&provider_schema)
            {
                values_yaml_schema
            } else if used_as_fragment
                && is_fixed_object_schema(&values_yaml_schema)
                && is_open_string_map_schema(&provider_schema)
            {
                provider_schema
            } else if used_as_fragment
                && is_scalar_schema(&values_yaml_schema)
                && is_object_or_array_schema(&provider_schema)
            {
                values_yaml_schema
            } else if let Some(values_yaml_ty) = schema_type(&values_yaml_schema)
                && is_scalar_schema(&values_yaml_schema)
                && schema_allows_scalar_type(&provider_schema, values_yaml_ty)
            {
                provider_schema
            } else {
                merge_two_schemas(provider_schema, values_yaml_schema)
            }
        }
    } else if !is_empty_schema(&values_yaml_schema) {
        values_yaml_schema
    } else if used_as_fragment {
        unknown_object_schema()
    } else {
        empty_schema()
    };

    let base = if is_empty_schema(&guard_boolish_schema) {
        base
    } else if is_empty_schema(&base) {
        guard_boolish_schema
    } else {
        base
    };

    let base = if is_empty_schema(&type_hint_schema) {
        base
    } else if is_empty_schema(&base) {
        type_hint_schema
    } else {
        merge_two_schemas(base, type_hint_schema)
    };

    if is_empty_schema(&guard_constraint_schema) {
        base
    } else if is_empty_schema(&base) {
        guard_constraint_schema
    } else {
        merge_two_schemas(base, guard_constraint_schema)
    }
}

fn is_empty_schema(v: &Value) -> bool {
    v.as_object().is_some_and(serde_json::Map::is_empty)
}

fn empty_schema() -> Value {
    Value::Object(Map::new())
}

fn infer_guard_boolish_schema(guard: &Guard) -> Option<Value> {
    match guard {
        // `with .Values.X` accepts any non-empty value (string, list, map,
        // number, bool, …), not just booleans, so it must not contribute a
        // boolean type hint. `eq` constrains the value but the constraint is
        // emitted separately by `infer_guard_constraint_schema`. `default`
        // guards express fallback/nullability, not booleans.
        Guard::Eq { .. } | Guard::Range { .. } | Guard::With { .. } | Guard::Default { .. } => None,
        _ => Some(type_schema("boolean")),
    }
}

fn infer_guard_constraint_schema(guard: &Guard) -> Option<Value> {
    let Guard::Eq { value, .. } = guard else {
        return None;
    };
    Some(Value::Object(
        [(
            "anyOf".to_string(),
            Value::Array(vec![
                Value::Object(
                    [(
                        "enum".to_string(),
                        Value::Array(vec![Value::String(value.clone())]),
                    )]
                    .into_iter()
                    .collect(),
                ),
                type_schema("string"),
            ]),
        )]
        .into_iter()
        .collect(),
    ))
}

fn lookup_values_yaml_schema(doc: &YamlValue, vp: &str) -> Option<Value> {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }

    let values = lookup_values_yaml_values(doc, &parts)?;
    if values.is_empty() {
        return None;
    }

    let schemas: Vec<Value> = values.into_iter().map(schema_from_yaml_value).collect();
    Some(merge_schema_list(schemas))
}

/// True when the value at `vp` resolves to YAML null in the document.
///
/// Returns false for missing keys or non-mapping intermediate nodes — only an
/// explicit `null` at the leaf qualifies.
fn is_null_at_value_path(doc: &YamlValue, vp: &str) -> bool {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }
    is_null_at_parts(doc, &parts)
}

fn is_empty_map_at_value_path(doc: &YamlValue, vp: &str) -> bool {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }
    let Some(values) = lookup_values_yaml_values(doc, &parts) else {
        return false;
    };
    !values.is_empty()
        && values
            .into_iter()
            .all(|value| matches!(value, YamlValue::Mapping(map) if map.is_empty()))
}

fn is_mapping_at_value_path(doc: &YamlValue, vp: &str) -> bool {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }
    let Some(values) = lookup_values_yaml_values(doc, &parts) else {
        return false;
    };
    !values.is_empty()
        && values
            .into_iter()
            .all(|value| matches!(value, YamlValue::Mapping(_)))
}

fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
}

fn generalize_fixed_object_schema_to_open_map(schema: Value) -> Value {
    if !is_fixed_object_schema(&schema) {
        return schema;
    }
    let Some(obj) = schema.as_object() else {
        return schema;
    };
    let Some(properties) = obj.get("properties").and_then(Value::as_object) else {
        return schema;
    };

    let merged_value_schema = merge_schema_list(properties.values().cloned().collect());
    let mut out = obj.clone();
    out.insert("additionalProperties".to_string(), merged_value_schema);
    Value::Object(out)
}

fn is_null_at_parts(doc: &YamlValue, parts: &[&str]) -> bool {
    if parts.is_empty() {
        return matches!(doc, YamlValue::Null);
    }
    let head = parts[0];
    let tail = &parts[1..];
    match doc {
        YamlValue::Mapping(m) => {
            let k = YamlValue::String(head.to_string());
            m.get(&k).is_some_and(|next| is_null_at_parts(next, tail))
        }
        _ => false,
    }
}

/// Identify value paths for which an explicit `null` default in values.yaml is
/// contractually valid according to the template control flow.
///
/// A path qualifies when every observed use is null-tolerant and at least one
/// rendered use provides non-null type evidence:
///
/// - header-only guard/binding uses (`if` / `with` / `range` conditions) are
///   always null-tolerant because Helm evaluates them against `nil` without
///   crashing;
/// - rendered uses must sit under a self-guard that only renders the body when
///   the same value path is non-empty (`if .Values.X`, `with .Values.X`,
///   `range .Values.X`, `if eq .Values.X "literal"`, and similar composed
///   conditions that retain the per-path guard).
///
/// We intentionally do not widen pure control-flow paths on the strength of
/// guard heuristics alone. Without a rendered use, a truthy/or/not condition
/// still does not tell us whether the underlying values type is boolean,
/// string, list, or object.
///
/// This keeps generated schemas aligned with the chart's actual acceptance
/// surface: if the template explicitly treats a value as optional and
/// values.yaml ships `null`, the schema should preserve that `null`.
fn collect_nullable_value_paths(uses: &[ValueUse]) -> BTreeSet<String> {
    struct PathInfo {
        has_render_use: bool,
        all_uses_nullable: bool,
    }

    impl Default for PathInfo {
        fn default() -> Self {
            Self {
                has_render_use: false,
                all_uses_nullable: true,
            }
        }
    }

    fn use_is_null_tolerant(use_: &ValueUse) -> bool {
        if use_.path.0.is_empty() {
            return true;
        }

        use_.guards.iter().any(|guard| match guard {
            Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path } => path == &use_.source_expr,
            Guard::Not { .. } | Guard::Or { .. } => false,
        })
    }

    let mut by_path: BTreeMap<&str, PathInfo> = BTreeMap::new();
    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }
        let info = by_path.entry(u.source_expr.as_str()).or_default();
        if !u.path.0.is_empty() {
            info.has_render_use = true;
        }
        info.all_uses_nullable &= use_is_null_tolerant(u);
    }
    by_path
        .into_iter()
        .filter_map(|(path, info)| {
            (info.has_render_use && info.all_uses_nullable).then(|| path.to_string())
        })
        .collect()
}

fn collect_default_fallback_value_paths(uses: &[ValueUse]) -> BTreeSet<String> {
    uses.iter()
        .filter(|use_| !use_.path.0.is_empty())
        .filter_map(|use_| {
            use_.guards
                .iter()
                .any(|guard| matches!(guard, Guard::Default { path } if path == &use_.source_expr))
                .then(|| use_.source_expr.clone())
        })
        .collect()
}

fn lookup_values_yaml_values<'a>(doc: &'a YamlValue, parts: &[&str]) -> Option<Vec<&'a YamlValue>> {
    if parts.is_empty() {
        return Some(vec![doc]);
    }

    let head = parts[0];
    let tail = &parts[1..];

    match doc {
        YamlValue::Mapping(m) => {
            let k = YamlValue::String(head.to_string());
            let next = m.get(&k)?;
            lookup_values_yaml_values(next, tail)
        }
        YamlValue::Sequence(seq) if head == "*" => {
            let mut out: Vec<&'a YamlValue> = Vec::new();
            for it in seq {
                if let Some(mut child) = lookup_values_yaml_values(it, tail) {
                    out.append(&mut child);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn schema_from_yaml_value(v: &YamlValue) -> Value {
    match v {
        YamlValue::Null | YamlValue::Tagged(_) => empty_schema(),
        YamlValue::Bool(_) => type_schema("boolean"),
        YamlValue::Number(n) => {
            if n.as_i64().is_some() || n.as_u64().is_some() {
                type_schema("integer")
            } else {
                type_schema("number")
            }
        }
        YamlValue::String(_) => type_schema("string"),
        YamlValue::Sequence(seq) => {
            let items = if seq.is_empty() {
                empty_schema()
            } else {
                merge_schema_list(seq.iter().map(schema_from_yaml_value).collect())
            };
            Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), items),
                ]
                .into_iter()
                .collect(),
            )
        }
        YamlValue::Mapping(m) => {
            if m.is_empty() {
                return unknown_object_schema();
            }
            let mut props = Map::new();
            for (k, v) in m {
                let Some(key) = k.as_str() else {
                    continue;
                };
                props.insert(key.to_string(), schema_from_yaml_value(v));
            }
            object_schema(props)
        }
    }
}

// ---------------------------------------------------------------------------
// Schema tree construction
// ---------------------------------------------------------------------------

fn object_schema(properties: Map<String, Value>) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(properties)),
            ("additionalProperties".to_string(), Value::Bool(false)),
        ]
        .into_iter()
        .collect(),
    )
}

fn unknown_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "additionalProperties".to_string(),
                Value::Object(Map::new()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn insert_schema_at_value_path(root: &mut Value, vp: &str, leaf: Value) {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    insert_schema_at_parts(root, &parts, leaf);
}

fn ensure_object_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            obj.insert("type".to_string(), Value::String("object".to_string()));
            obj.entry("properties".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if obj.get("properties").and_then(|p| p.as_object()).is_none() {
                obj.insert("properties".to_string(), Value::Object(Map::new()));
            }
            obj.entry("additionalProperties".to_string())
                .or_insert(Value::Bool(false));

            let has_structure = obj
                .get("properties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| !m.is_empty())
                || obj
                    .get("patternProperties")
                    .and_then(|v| v.as_object())
                    .is_some_and(|m| !m.is_empty())
                || obj
                    .get("required")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());

            let ap_is_empty_schema = obj
                .get("additionalProperties")
                .and_then(|v| v.as_object())
                .is_some_and(serde_json::Map::is_empty);

            if has_structure && ap_is_empty_schema {
                obj.insert("additionalProperties".to_string(), Value::Bool(false));
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
    array_schema
        .as_object_mut()
        .and_then(|o| o.get_mut("items"))
        .expect("array schema must have items")
}

const MAP_WILDCARD_SEGMENT: &str = "__any__";

fn is_object_like_schema(v: &Value) -> bool {
    match schema_type(v) {
        Some("object") => true,
        Some(_) => false,
        None => v.as_object().is_some_and(|o| {
            o.contains_key("properties")
                || o.contains_key("additionalProperties")
                || o.contains_key("patternProperties")
                || o.contains_key("required")
        }),
    }
}

fn is_array_like_schema(v: &Value) -> bool {
    match schema_type(v) {
        Some("array") => true,
        Some(_) => false,
        None => v.as_object().is_some_and(|o| o.contains_key("items")),
    }
}

fn union_key(obj: &Map<String, Value>) -> Option<&'static str> {
    if obj.get("anyOf").and_then(Value::as_array).is_some() {
        Some("anyOf")
    } else if obj.get("oneOf").and_then(Value::as_array).is_some() {
        Some("oneOf")
    } else {
        None
    }
}

fn take_union_variants(obj: &mut Map<String, Value>, key: &str) -> Option<Vec<Value>> {
    let Value::Array(variants) = obj.remove(key).unwrap_or_else(|| Value::Array(Vec::new())) else {
        return None;
    };
    Some(variants)
}

fn push_union_structural_constraints_down(obj: &mut Map<String, Value>, variants: &mut [Value]) {
    let structural_keys = [
        "type",
        "properties",
        "additionalProperties",
        "patternProperties",
        "required",
        "items",
    ];
    let mut structural = Map::new();
    for k in structural_keys {
        if let Some(v) = obj.remove(k) {
            structural.insert(k.to_string(), v);
        }
    }

    if structural.is_empty() {
        return;
    }

    let structural_schema = Value::Object(structural);
    for v in variants {
        let compatible = if is_array_like_schema(&structural_schema) {
            is_array_like_schema(v)
        } else {
            is_object_like_schema(v)
        };

        if compatible {
            let existing = std::mem::replace(v, Value::Null);
            *v = merge_two_schemas(existing, structural_schema.clone());
        }
    }
}

fn insert_schema_into_union_variants(variants: &mut [Value], parts: &[&str], leaf: &Value) -> bool {
    let head = parts[0];
    let mut touched = false;
    for v in variants {
        let compatible = if head == "*" {
            is_array_like_schema(v)
        } else {
            is_object_like_schema(v)
        };

        if compatible {
            insert_schema_at_parts(v, parts, leaf.clone());
            touched = true;
        }
    }
    touched
}

fn new_union_variant_for_head(head: &str) -> Value {
    if head == "*" {
        Value::Object(
            [
                ("type".to_string(), Value::String("array".to_string())),
                ("items".to_string(), Value::Null),
            ]
            .into_iter()
            .collect(),
        )
    } else {
        object_schema(Map::new())
    }
}

fn insert_schema_at_union(
    obj: &mut Map<String, Value>,
    key: &'static str,
    parts: &[&str],
    leaf: Value,
) {
    let Some(mut variants) = take_union_variants(obj, key) else {
        return;
    };

    push_union_structural_constraints_down(obj, &mut variants);

    let touched = insert_schema_into_union_variants(&mut variants, parts, &leaf);
    if !touched {
        let mut new_variant = new_union_variant_for_head(parts[0]);
        insert_schema_at_parts(&mut new_variant, parts, leaf);
        variants.push(new_variant);
    }

    obj.insert(key.to_string(), Value::Array(variants));
}

fn insert_schema_at_parts(node: &mut Value, parts: &[&str], leaf: Value) {
    if parts.is_empty() {
        return;
    }

    // Union-aware insertion: when a value path is a union (e.g. `anyOf: [object, string]`),
    // inserting nested schemas must update the appropriate variant(s) rather than forcing the
    // union node itself into an object/array schema.
    if let Value::Object(obj) = node
        && let Some(key) = union_key(obj)
    {
        insert_schema_at_union(obj, key, parts, leaf);
        return;
    }

    if parts[0] == MAP_WILDCARD_SEGMENT {
        ensure_object_schema(node);
        let obj = node.as_object_mut().expect("object schema");
        let ap = obj
            .entry("additionalProperties")
            .or_insert_with(|| Value::Object(Map::new()));
        if ap.as_bool() == Some(false) {
            *ap = Value::Object(Map::new());
        }
        if parts.len() == 1 {
            let existing = std::mem::replace(ap, Value::Null);
            *ap = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(ap, &parts[1..], leaf);
        }
        return;
    }

    if parts[0] == "*" {
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if parts.len() == 1 {
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
    let child = props
        .entry(key)
        .or_insert_with(|| object_schema(Map::new()));
    insert_schema_at_parts(child, &parts[1..], leaf);
}

#[cfg(test)]
mod tests;
