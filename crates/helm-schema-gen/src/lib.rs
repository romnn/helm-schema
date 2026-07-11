mod foreign_schema;
mod merge;
mod path_resolver;
mod path_schema;
mod provider_definitions;
mod provider_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_node;
mod schema_tree;
mod values_yaml;

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ResourceSchemaOracle;
use helm_schema_core::guard_algebra::minimize_key_disjunction;
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, GuardValue};

use merge::{merge_schema_list, union_schema_list};
use path_resolver::PathSchemaResolver;
use provider_definitions::{extract_provider_definitions, insert_definitions_into_root};
use schema_model::{
    guard_value_to_json, is_fixed_object_schema, is_scalar_like_schema, schema_allows_type,
};
use schema_node::{JsonSchemaType, SchemaNode, is_placeholder_fragment_object_schema};
use schema_tree::{SchemaDocument, draft07_root_document};
use values_yaml::yaml_value_at_path;

/// Inputs for JSON Schema generation from the current contract schema signals.
///
/// The generated schema is derived from the contract-layer signal bundle plus
/// optional structural signals collected by earlier analysis phases.
/// Values-file descriptions are metadata only: they are applied only to schema
/// nodes that already exist from template or values evidence.
pub struct ValuesSchemaInput<'a> {
    pub contract_schema_signals: &'a ContractSchemaSignals,
    pub provider: &'a dyn ResourceSchemaOracle,
    pub values_yaml: Option<&'a str>,
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ValuesSchemaInput<'a> {
    pub fn new(
        contract_schema_signals: &'a ContractSchemaSignals,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        Self {
            contract_schema_signals,
            provider,
            values_yaml: None,
            values_descriptions: None,
        }
    }

    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
        self
    }

    pub fn with_values_descriptions(
        mut self,
        values_descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        self.values_descriptions = Some(values_descriptions);
        self
    }
}

/// Generate a JSON Schema with chart-authored values-file descriptions.
///
/// The output schema has no `required` arrays inferred by helm-schema; callers
/// that want that behaviour layer [`required_inference::apply_required_inference`]
/// on top of the returned schema. Keeping required-inference outside this
/// function isolates a heuristic feature from the core schema-generation
/// pipeline.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema(input: ValuesSchemaInput<'_>) -> Value {
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        input.contract_schema_signals,
        &values_yaml_doc,
        values_descriptions,
        input.provider,
    );

    draft07_root_document(root_schema)
}

#[tracing::instrument(skip_all)]
fn build_root_schema(
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    values_descriptions: &BTreeMap<String, String>,
    provider: &dyn ResourceSchemaOracle,
) -> Value {
    let mut root_schema = SchemaDocument::new_root_object();
    let path_resolver = PathSchemaResolver::new(contract_schema_signals, values_yaml_doc, provider);
    let mut resolved_paths = path_resolver.resolve_all();
    let provider_definitions =
        extract_provider_definitions(&mut resolved_paths, values_descriptions);

    let conditional_schemas = collect_conditional_schemas(
        &resolved_paths,
        contract_schema_signals,
        values_yaml_doc,
        provider,
    );
    let conditional_targets = ConditionalTargetIndex::from_conditionals(&conditional_schemas);
    let accepted_values_root_paths = contract_schema_signals
        .schema_evidence_by_value_path()
        .values()
        .filter(|evidence| evidence.facts.accepted_values_root_fragment)
        .map(|evidence| split_value_path(&evidence.value_path))
        .collect::<Vec<_>>();
    let mut delayed_replacements = Vec::new();
    for resolved_path in &resolved_paths {
        match base_insertion_decision(resolved_path, &conditional_targets) {
            BaseInsertionDecision::Insert(schema) => {
                root_schema.insert_path_schema(&resolved_path.path_segments, schema);
            }
            BaseInsertionDecision::Replace(schema) => {
                delayed_replacements.push((resolved_path.path_segments.clone(), schema));
            }
        }
    }
    // A replaced target under a replaced ancestor adds nothing (the
    // ancestor's base already owns the subtree) and descending into the
    // ancestor's non-object base would coerce it into a closed map.
    let replaced_paths: BTreeSet<Vec<String>> = delayed_replacements
        .iter()
        .map(|(path_segments, _)| path_segments.clone())
        .collect();
    for (path_segments, schema) in delayed_replacements {
        let has_replaced_ancestor = (1..path_segments.len())
            .any(|length| replaced_paths.contains(&path_segments[..length]));
        if !has_replaced_ancestor {
            root_schema.replace_path_schema(&path_segments, schema);
        }
    }

    append_conditional_schemas(&mut root_schema, conditional_schemas, values_yaml_doc);
    root_schema.merge_missing_values_yaml_defaults_under_roots(
        values_yaml_doc,
        &accepted_values_root_paths,
        &conditional_targets.target_paths,
    );

    let mut root_schema = root_schema.into_value();
    let mut provider_definitions = provider_definitions;
    if value_references_helm_truthy(&root_schema) {
        provider_definitions.insert(
            HELM_TRUTHY_DEFINITION_NAME.to_string(),
            helm_truthy_definition_schema(),
        );
    }
    insert_definitions_into_root(&mut root_schema, provider_definitions);
    schema_tree::apply_values_descriptions(&mut root_schema, values_descriptions);
    root_schema
}

enum BaseInsertionDecision {
    Insert(SchemaNode),
    Replace(SchemaNode),
}

fn base_insertion_decision(
    resolved_path: &path_resolver::ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
) -> BaseInsertionDecision {
    if is_pathless_dependency_root_with_guarded_descendant(resolved_path, conditional_targets) {
        return BaseInsertionDecision::Insert(SchemaNode::unknown_object());
    }

    let Some(target) = conditional_targets
        .targets
        .get(resolved_path.value_path.as_str())
    else {
        return BaseInsertionDecision::Insert(SchemaNode::foreign(resolved_path.schema.clone()));
    };

    if target.preserve_base_schema {
        BaseInsertionDecision::Insert(SchemaNode::foreign(unclose_fixed_objects(
            resolved_path.schema.clone(),
        )))
    } else {
        BaseInsertionDecision::Replace(guarded_only_target_base_schema(resolved_path, target))
    }
}

/// Unclose fixed objects (top level or union arms) in a conditional target's
/// base: overlays own keys the resolved shape does not know about, so a
/// closed base would reject values the guarded renders accept.
fn unclose_fixed_objects(mut schema: Value) -> Value {
    if is_fixed_object_schema(&schema) {
        if let Some(object) = schema.as_object_mut() {
            object.insert("additionalProperties".to_string(), serde_json::json!({}));
        }
        return schema;
    }
    if let Some(object) = schema.as_object_mut() {
        for key in ["anyOf", "oneOf"] {
            if let Some(Value::Array(arms)) = object.get_mut(key) {
                for arm in arms {
                    *arm = unclose_fixed_objects(std::mem::take(arm));
                }
            }
        }
    }
    schema
}

fn is_pathless_dependency_root_with_guarded_descendant(
    resolved_path: &path_resolver::ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
) -> bool {
    resolved_path.accepted_dependency_values_root_fragment
        && resolved_path.used_as_pathless_fragment
        && conditional_targets.has_guarded_only_descendant(&resolved_path.path_segments)
}

fn guarded_only_target_base_schema(
    resolved_path: &path_resolver::ResolvedPathSchema,
    target: &ConditionalTargetSummary,
) -> SchemaNode {
    let schema = if target.open_fragment_base_schema {
        if resolved_path.provider_schema_candidate.is_some() {
            resolved_path.schema.clone()
        } else if is_fixed_object_schema(&resolved_path.schema) {
            // Keep the resolved children but unclose the object: a closed
            // shape at the base would reject keys whose typing now lives
            // under this target's overlays.
            unclose_fixed_objects(resolved_path.schema.clone())
        } else {
            open_fragment_base_schema(&resolved_path.schema)
        }
    } else {
        crate::schema_model::empty_schema()
    };
    SchemaNode::foreign(schema)
}

#[derive(Debug, Clone, Copy)]
struct ConditionalTargetSummary {
    preserve_base_schema: bool,
    open_fragment_base_schema: bool,
}

struct ConditionalTargetIndex {
    targets: BTreeMap<String, ConditionalTargetSummary>,
    guarded_only_paths: BTreeSet<Vec<String>>,
    /// Every conditional target path, preserved or not: the values-defaults
    /// merge must not reshape these subtrees (their shape is overlay-owned,
    /// and inserting into a non-object base would coerce it to a closed map).
    target_paths: BTreeSet<Vec<String>>,
}

impl ConditionalTargetIndex {
    fn from_conditionals(conditionals: &[ConditionalResolvedSchema]) -> Self {
        let mut targets = BTreeMap::new();
        for conditional in conditionals {
            let entry = targets
                .entry(conditional.target_value_path.clone())
                .or_insert(ConditionalTargetSummary {
                    preserve_base_schema: false,
                    open_fragment_base_schema: true,
                });
            entry.preserve_base_schema |= conditional.preserve_base_schema;
            if !conditional.preserve_base_schema {
                entry.open_fragment_base_schema &= conditional.target_is_fragment;
            }
        }

        let guarded_only_paths = targets
            .iter()
            .filter(|(_, target)| !target.preserve_base_schema)
            .map(|(path, _)| split_value_path(path))
            .collect();
        let target_paths = targets.keys().map(|path| split_value_path(path)).collect();

        Self {
            targets,
            guarded_only_paths,
            target_paths,
        }
    }

    fn has_guarded_only_descendant(&self, path_segments: &[String]) -> bool {
        self.guarded_only_paths.iter().any(|target_path| {
            target_path.len() > path_segments.len() && target_path.starts_with(path_segments)
        })
    }
}

fn open_fragment_base_schema(resolved_schema: &Value) -> Value {
    let mut schemas = Vec::new();
    if schema_allows_type(resolved_schema, "object") {
        schemas.push(SchemaNode::unknown_object().into_value());
    }
    if schema_allows_type(resolved_schema, "array") {
        schemas.push(SchemaNode::array().items(SchemaNode::empty()).into_value());
    }
    // tpl-rendered fragments accept a string template alternative alongside
    // the structured form; the opened base must keep accepting it when the
    // structured typing moves under a conditional overlay.
    if schema_allows_type(resolved_schema, "string") {
        schemas.push(SchemaNode::typed(JsonSchemaType::String).into_value());
    }
    if schema_allows_type(resolved_schema, "null") {
        schemas.push(SchemaNode::typed(JsonSchemaType::Null).into_value());
    }

    match schemas.len() {
        0 => crate::schema_model::empty_schema(),
        1 => schemas
            .pop()
            .expect("single open fragment schema should be present"),
        _ => union_schema_list(schemas),
    }
}

struct ConditionalResolvedSchema {
    target_value_path: String,
    ancestor_segments: Vec<String>,
    relative_target_segments: Vec<String>,
    guards: Vec<ConditionalGuard>,
    target_schema: Value,
    preserve_base_schema: bool,
    target_is_fragment: bool,
}

#[tracing::instrument(skip_all)]
fn collect_conditional_schemas(
    resolved_paths: &[path_resolver::ResolvedPathSchema],
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> Vec<ConditionalResolvedSchema> {
    let resolved_by_path = resolved_paths
        .iter()
        .map(|resolved| (resolved.value_path.as_str(), resolved))
        .collect::<BTreeMap<_, _>>();
    let mut conditionals = Vec::new();

    for (target_value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let Some(resolved_target) = resolved_by_path.get(target_value_path.as_str()) else {
            continue;
        };

        for overlay in &evidence.conditional_overlays {
            if !guards_supported_for_conditional_lowering(
                &overlay.guards,
                &resolved_by_path,
                values_yaml_doc,
            ) {
                continue;
            }

            let target_segments = split_value_path(target_value_path);
            let ancestor_segments =
                conditional_ancestor_segments(&target_segments, &overlay.guards);
            let active_by_defaults = evaluate_guard_set_on_values(&overlay.guards, values_yaml_doc);
            let target_schema = conditional_target_schema(
                target_value_path,
                overlay,
                values_yaml_doc,
                provider,
                resolved_target.values_yaml_schema.clone(),
                resolved_target.schema.clone(),
                active_by_defaults,
            );
            if crate::schema_model::is_empty_schema(&target_schema) {
                continue;
            }

            conditionals.push(ConditionalResolvedSchema {
                target_value_path: target_value_path.clone(),
                relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                ancestor_segments,
                guards: overlay.guards.clone(),
                target_schema,
                preserve_base_schema: overlay.preserve_base_schema,
                target_is_fragment: overlay.evidence.facts.used_as_fragment,
            });
        }
    }

    conditionals
}

fn conditional_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
    values_yaml_schema: Value,
    resolved_fallback: Value,
    active_by_defaults: Option<bool>,
) -> Value {
    let branch_schema = resolve_overlay_target_schema(target_value_path, overlay, provider);

    let declared_default = yaml_value_at_path(values_yaml_doc, target_value_path)
        .and_then(|value| serde_json::to_value(value).ok());
    // A branch that rejects the path's own declared default narrows values
    // the chart itself ships.
    let rejects_declared_default = |schema: &Value| {
        declared_default
            .as_ref()
            .is_some_and(|default_value| !schema_accepts_json_value(schema, default_value))
    };

    let branch_schema = if active_by_defaults.is_some()
        && should_merge_values_yaml_into_conditional_branch(&branch_schema, &values_yaml_schema)
    {
        merge_schema_list(vec![branch_schema, values_yaml_schema])
    } else {
        branch_schema
    };
    // Guards inactive by defaults or undecidable on the values doc can still
    // be activated by a user who keeps the chart's other defaults.
    if active_by_defaults != Some(true) {
        if is_placeholder_fragment_object_schema(&branch_schema)
            && !is_placeholder_fragment_object_schema(&resolved_fallback)
        {
            // The swap gives a vacuous placeholder branch the resolved
            // content, but never a shape that rejects the shipped default.
            return if rejects_declared_default(&resolved_fallback) {
                branch_schema
            } else {
                resolved_fallback
            };
        }
        // A branch whose renders all sit behind their own truthiness only
        // fires for truthy values, so it must keep accepting the shipped
        // (possibly falsy) default. A branch read unconditionally under its
        // guard may legitimately narrow the default away.
        if !overlay.evidence.facts.is_nullable {
            return branch_schema;
        }
    }

    if rejects_declared_default(&branch_schema) {
        resolved_fallback
    } else {
        branch_schema
    }
}

fn should_merge_values_yaml_into_conditional_branch(
    branch_schema: &Value,
    values_yaml_schema: &Value,
) -> bool {
    crate::schema_model::is_empty_schema(branch_schema)
        || (is_scalar_like_schema(branch_schema) && is_scalar_like_schema(values_yaml_schema))
}

fn resolve_overlay_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> Value {
    let evidence = overlay.evidence.as_path_evidence(target_value_path);
    PathSchemaResolver::resolve_single_path_evidence(&evidence, provider).schema
}

fn conditional_ancestor_segments(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Vec<String> {
    let mut shared_prefix = target_segments.to_vec();
    for guard in guards {
        for guard_path in guard.value_paths() {
            let guard_path = split_value_path(&guard_path);
            shared_prefix.truncate(common_prefix_len(&shared_prefix, &guard_path));
        }
    }
    shared_prefix
}

fn guards_supported_for_conditional_lowering(
    guards: &[ConditionalGuard],
    resolved_by_path: &BTreeMap<&str, &path_resolver::ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            // The truthiness condition encoding is type-generic (const true,
            // non-zero number, non-empty string/array/object), so a guard
            // path declared by the chart lowers whatever its type. Undeclared
            // paths still lower only as boolean-like flags: every guard path
            // gets an accumulator entry, so mere resolution would also admit
            // paths fabricated by imprecise lookups (`index $vals $a $b`).
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                yaml_value_at_path(values_yaml_doc, path).is_some()
                    || resolved_by_path
                        .get(path.as_str())
                        .is_some_and(|resolved| schema_is_boolean_like(&resolved.schema))
            }
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_for_conditional_lowering(
                std::slice::from_ref(inner),
                resolved_by_path,
                values_yaml_doc,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                guards_supported_for_conditional_lowering(guards, resolved_by_path, values_yaml_doc)
            }
        })
}

fn schema_is_boolean_like(schema: &Value) -> bool {
    crate::schema_model::schema_allows_type(schema, "boolean")
        && !crate::schema_model::schema_allows_type(schema, "string")
        && !crate::schema_model::schema_allows_type(schema, "integer")
        && !crate::schema_model::schema_allows_type(schema, "number")
        && !crate::schema_model::schema_allows_type(schema, "object")
        && !crate::schema_model::schema_allows_type(schema, "array")
}

#[tracing::instrument(skip_all)]
fn append_conditional_schemas(
    root_schema: &mut SchemaDocument,
    conditionals: Vec<ConditionalResolvedSchema>,
    values_yaml_doc: &YamlValue,
) {
    // Conditionals sharing one guard set and scope conjoin into one if/then:
    // `allOf [{if G then A}, {if G then B}]` is `{if G then A ∧ B}`, and the
    // repeated `if` blocks dominate emitted size on charts with many guarded
    // blocks. Distinct targets merge disjointly; a leaf collision falls back
    // to its own conditional.
    let mut grouped: BTreeMap<
        (Vec<String>, Vec<ConditionalGuard>),
        Vec<ConditionalResolvedSchema>,
    > = BTreeMap::new();
    for conditional in conditionals {
        grouped
            .entry((
                conditional.ancestor_segments.clone(),
                conditional.guards.clone(),
            ))
            .or_default()
            .push(conditional);
    }
    struct ContentGroup {
        fragment: Value,
        guard_sets: Vec<Vec<ConditionalGuard>>,
    }
    let mut by_content: BTreeMap<(Vec<String>, String), ContentGroup> = BTreeMap::new();
    for ((ancestor_segments, guards), group) in grouped {
        let mut merged: Option<Value> = None;
        let mut separate = Vec::new();
        for conditional in group {
            let fragment = build_target_fragment(
                &conditional.relative_target_segments,
                SchemaNode::foreign(conditional.target_schema),
            )
            .into_value();
            match &mut merged {
                None => merged = Some(fragment),
                Some(target) => {
                    if !merge_disjoint_property_fragment(target, fragment.clone()) {
                        separate.push(fragment);
                    }
                }
            }
        }
        for fragment in merged.into_iter().chain(separate) {
            // Conditionals with identical content under one scope disjoin
            // their guards: `if G1 then X` and `if G2 then X` is
            // `if anyOf [G1, G2] then X`, and X (often a repeated provider
            // schema) is the dominant emitted size.
            by_content
                .entry((ancestor_segments.clone(), fragment.to_string()))
                .or_insert_with(|| ContentGroup {
                    fragment,
                    guard_sets: Vec::new(),
                })
                .guard_sets
                .push(guards.clone());
        }
    }
    for ((ancestor_segments, _), group) in by_content {
        let mut conditions: Vec<SchemaNode> = minimize_key_disjunction(group.guard_sets)
            .into_iter()
            .map(|guards| {
                SchemaNode::all_of(build_condition_clauses(
                    &guards,
                    &ancestor_segments,
                    values_yaml_doc,
                ))
            })
            .collect();
        let condition = if conditions.len() == 1 {
            conditions.remove(0)
        } else {
            SchemaNode::any_of(conditions)
        };
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(group.fragment),
        );
    }
}

/// Merge `incoming` into `target` when both are plain `properties` object
/// fragments whose leaves do not collide; returns false (leaving `target`
/// unchanged) when they do.
fn merge_disjoint_property_fragment(target: &mut Value, incoming: Value) -> bool {
    fn mergeable(target: &Value, incoming: &Value) -> bool {
        let (Some(target), Some(incoming)) = (target.as_object(), incoming.as_object()) else {
            return false;
        };
        let plain_object = |node: &serde_json::Map<String, Value>| {
            node.keys().all(|key| key == "properties" || key == "type")
                && node.get("type").and_then(Value::as_str) == Some("object")
        };
        if !plain_object(target) || !plain_object(incoming) {
            return false;
        }
        let (Some(Value::Object(target_props)), Some(Value::Object(incoming_props))) =
            (target.get("properties"), incoming.get("properties"))
        else {
            return false;
        };
        incoming_props.iter().all(|(key, value)| {
            target_props
                .get(key)
                .is_none_or(|existing| mergeable(existing, value))
        })
    }
    fn merge(target: &mut Value, incoming: Value) {
        let Value::Object(mut incoming_object) = incoming else {
            return;
        };
        let Some(Value::Object(incoming_props)) = incoming_object.remove("properties") else {
            return;
        };
        let Some(target_props) = target
            .as_object_mut()
            .and_then(|object| object.get_mut("properties"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        for (key, value) in incoming_props {
            match target_props.get_mut(&key) {
                Some(existing) => merge(existing, value),
                None => {
                    target_props.insert(key, value);
                }
            }
        }
    }
    if !mergeable(target, &incoming) {
        return false;
    }
    merge(target, incoming);
    true
}

fn build_condition_clauses(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
        })
        .collect()
}

fn build_single_condition_fragment(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Option<SchemaNode> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                helm_truthy_condition_schema(),
                yaml_value_at_path(values_yaml_doc, path).is_some_and(yaml_value_is_truthy),
            )
        }
        ConditionalGuard::Eq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value)?,
            guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::NotEq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value).map(SchemaNode::not)?,
            !guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::Absent { path } => {
            let segments = split_value_path(path);
            let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
            if relative_segments.is_empty() {
                None
            } else {
                build_required_condition_fragment(&relative_segments, SchemaNode::empty())
                    .map(SchemaNode::not)
            }
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::type_named(schema_type),
                yaml_value_at_path(values_yaml_doc, path)
                    .is_some_and(|value| matches_yaml_schema_type(value, schema_type)),
            )
        }
        ConditionalGuard::Not(inner) => Some(SchemaNode::not(build_single_condition_fragment(
            inner,
            ancestor_segments,
            values_yaml_doc,
        )?)),
        ConditionalGuard::AllOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::all_of(clauses))
        }
        ConditionalGuard::AnyOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::any_of(clauses))
        }
    }
}

fn guard_value_enum_schema(value: &GuardValue) -> Option<SchemaNode> {
    guard_value_to_json(value).map(|value| SchemaNode::enum_values(vec![value]))
}

fn build_default_aware_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: SchemaNode,
    default_matches: bool,
) -> Option<SchemaNode> {
    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        return Some(leaf_schema);
    }
    let explicit = build_required_condition_fragment(&relative_segments, leaf_schema)?;
    if !default_matches {
        return Some(explicit);
    }
    let absent = build_required_condition_fragment(&relative_segments, SchemaNode::empty())
        .map(SchemaNode::not)?;
    Some(SchemaNode::any_of(vec![absent, explicit]))
}

fn value_references_helm_truthy(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            (key == "$ref"
                && child.as_str()
                    == Some(&format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}") as &str))
                || value_references_helm_truthy(child)
        }),
        Value::Array(items) => items.iter().any(value_references_helm_truthy),
        _ => false,
    }
}

/// Helm truthiness as one shared definition: every truthy/with condition
/// references it, which keeps the emitted `if` blocks small on charts with
/// many guarded renders.
pub(crate) const HELM_TRUTHY_DEFINITION_NAME: &str = "helm-truthy";

fn helm_truthy_condition_schema() -> SchemaNode {
    SchemaNode::foreign(serde_json::json!({
        "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}")
    }))
}

pub(crate) fn helm_truthy_definition_schema() -> Value {
    SchemaNode::any_of(vec![
        SchemaNode::const_value(Value::Bool(true)),
        SchemaNode::typed(JsonSchemaType::Number).typed_keyword(
            "not",
            SchemaNode::const_value(Value::Number(0.into())).into_value(),
        ),
        SchemaNode::typed(JsonSchemaType::String)
            .typed_keyword("minLength", Value::Number(1.into())),
        SchemaNode::array().min_items(1),
        SchemaNode::object().min_properties(1),
    ])
    .into_value()
}

fn build_required_condition_fragment(
    path_segments: &[String],
    leaf_schema: SchemaNode,
) -> Option<SchemaNode> {
    let (head, tail) = path_segments.split_first()?;
    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_required_condition_fragment(tail, leaf_schema)?
    };
    Some(
        SchemaNode::object()
            .require(head.clone())
            .property(head.clone(), child),
    )
}

fn build_target_fragment(path_segments: &[String], leaf_schema: SchemaNode) -> SchemaNode {
    let Some((head, tail)) = path_segments.split_first() else {
        return leaf_schema;
    };

    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_target_fragment(tail, leaf_schema)
    };
    SchemaNode::object().property(head.clone(), child)
}

pub(crate) fn split_value_path(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn evaluate_guard_set_on_values(
    guards: &[ConditionalGuard],
    values_yaml_doc: &YamlValue,
) -> Option<bool> {
    guards
        .iter()
        .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
        .collect::<Option<Vec<_>>>()
        .map(|results| results.into_iter().all(|result| result))
}

fn evaluate_guard_on_values(guard: &ConditionalGuard, values_yaml_doc: &YamlValue) -> Option<bool> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_some_and(yaml_value_is_truthy))
        }
        ConditionalGuard::Eq { path, value } => Some(guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::NotEq { path, value } => Some(!guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::Absent { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_none())
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            let Some(value) = yaml_value_at_path(values_yaml_doc, path) else {
                return Some(false);
            };
            Some(matches_yaml_schema_type(value, schema_type))
        }
        ConditionalGuard::Not(inner) => {
            evaluate_guard_on_values(inner, values_yaml_doc).map(|v| !v)
        }
        ConditionalGuard::AllOf(guards) => evaluate_guard_set_on_values(guards, values_yaml_doc),
        ConditionalGuard::AnyOf(guards) => guards
            .iter()
            .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
            .collect::<Option<Vec<_>>>()
            .map(|results| results.into_iter().any(|result| result)),
    }
}

fn guard_value_matches_optional_yaml(value: &GuardValue, yaml: Option<&YamlValue>) -> bool {
    let Some(yaml) = yaml else {
        return matches!(value, GuardValue::Null);
    };
    match value {
        GuardValue::String(expected) => yaml.as_str() == Some(expected.as_str()),
        GuardValue::Bool(expected) => yaml.as_bool() == Some(*expected),
        GuardValue::Int(expected) => {
            yaml.as_i64() == Some(*expected)
                || (*expected >= 0 && yaml.as_u64() == Some(*expected as u64))
        }
        GuardValue::Float(expected) => {
            let Some(expected) = expected.parse::<f64>().ok() else {
                return false;
            };
            yaml.as_f64() == Some(expected)
        }
        GuardValue::Null => matches!(yaml, YamlValue::Null),
    }
}

fn yaml_value_is_truthy(value: &YamlValue) -> bool {
    match value {
        YamlValue::Null => false,
        YamlValue::Bool(value) => *value,
        YamlValue::Number(value) => {
            value.as_i64().is_some_and(|value| value != 0)
                || value.as_u64().is_some_and(|value| value != 0)
                || value.as_f64().is_some_and(|value| value != 0.0)
        }
        YamlValue::String(value) => !value.is_empty(),
        YamlValue::Sequence(value) => !value.is_empty(),
        YamlValue::Mapping(value) => !value.is_empty(),
        YamlValue::Tagged(_) => false,
    }
}

fn matches_yaml_schema_type(value: &YamlValue, schema_type: &str) -> bool {
    match schema_type {
        "array" => matches!(value, YamlValue::Sequence(_)),
        "boolean" => value.as_bool().is_some(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.as_f64().is_some(),
        "object" => matches!(value, YamlValue::Mapping(_)),
        "string" => value.as_str().is_some(),
        _ => false,
    }
}

fn schema_accepts_json_value(schema: &Value, instance: &Value) -> bool {
    jsonschema::validator_for(schema)
        .map(|validator| validator.is_valid(instance))
        .unwrap_or(false)
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn strip_ancestor_prefix(
    path_segments: &[String],
    ancestor_segments: &[String],
) -> Option<Vec<String>> {
    path_segments
        .starts_with(ancestor_segments)
        .then(|| path_segments[ancestor_segments.len()..].to_vec())
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
