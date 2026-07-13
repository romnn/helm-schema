use std::collections::BTreeMap;

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, ResourceSchemaOracle,
};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::condition_encoding::{
    build_condition_clauses, evaluate_guard_set_on_values, guard_encodes_fully,
};
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::conditional_target_schema;
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::values_yaml::yaml_value_at_path;
use crate::{common_prefix_len, split_value_path};

pub(crate) struct ConditionalResolvedSchema {
    pub(crate) target_value_path: String,
    ancestor_segments: Vec<String>,
    relative_target_segments: Vec<String>,
    guards: Vec<ConditionalGuard>,
    pub(crate) target_schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
    pub(crate) preserve_base_schema: bool,
    pub(crate) target_is_fragment: bool,
    /// The conditional is a pure `allOf` arm (fail implication): it adds a
    /// requirement without owning the path's shape, so base classification
    /// must ignore it entirely — an implication must never flip an
    /// overlay-owned base to the resolved schema nor empty a resolved one.
    pub(crate) arm_only: bool,
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_conditional_schemas(
    resolved_paths: &[ResolvedPathSchema],
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

        // `fail` implications: wherever the outer guards hold, the failing
        // test's negation must hold. Runtime-hard, so the requirement
        // rides an `allOf` arm — property-level union lanes (declared
        // defaults, range alternatives, carrier variants) must never
        // bypass it. An empty guard set means the requirement is
        // unconditional and the arm's condition is trivially true.
        for implication in &evidence.fail_implications {
            if !implication.outer_guards.is_empty()
                && !implication_guards_supported(
                    &implication.outer_guards,
                    target_value_path,
                    &resolved_by_path,
                )
            {
                continue;
            }
            let target_schema =
                crate::path_resolver::fail_requirement_schema(std::iter::once(implication));
            if crate::schema_model::is_empty_schema(&target_schema) {
                continue;
            }
            let target_segments = split_value_path(target_value_path);
            let mut ancestor_segments =
                conditional_ancestor_segments(&target_segments, &implication.outer_guards);
            // Anchor at a STRICT ancestor: an arm appended at the target
            // node itself lands inside one union alternative, letting the
            // other alternatives bypass the requirement.
            if ancestor_segments.len() == target_segments.len() {
                ancestor_segments.pop();
            }
            conditionals.push(ConditionalResolvedSchema {
                target_value_path: target_value_path.clone(),
                relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                ancestor_segments,
                guards: implication.outer_guards.clone(),
                target_schema,
                provider_schema_candidate: None,
                preserve_base_schema: true,
                target_is_fragment: false,
                arm_only: true,
            });
        }

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
            let resolved_overlay =
                resolve_overlay_target_schema(target_value_path, overlay, provider);
            let target_schema = conditional_target_schema(
                target_value_path,
                overlay,
                values_yaml_doc,
                resolved_overlay.schema,
                resolved_target.values_yaml_schema.clone(),
                resolved_target.schema.clone(),
                active_by_defaults,
            );
            // A branch that RANGES the path binds the runtime iterable
            // domain on top of whatever else it claims; integer counts
            // drop out when the loop body reads member structure the
            // int64 iteration values cannot provide.
            let target_schema = if overlay.evidence.facts.is_ranged_source {
                crate::merge::merge_schema_list(vec![
                    target_schema,
                    crate::runtime_iterable_schema(
                        !overlay.evidence.facts.has_structured_item_descendants
                            && !overlay.evidence.facts.has_destructured_range_use,
                    ),
                ])
            } else {
                target_schema
            };
            if crate::schema_model::is_empty_schema(&target_schema) {
                // A branch whose renders are all serialized proves the wider
                // contract inside that branch, so it carries no schema; it
                // stays a conditional TARGET so base classification still
                // uncloses/opens the base the way the guarded renders
                // demand. Mixed branches resolve their own evidence above,
                // so a stringified occurrence never erases an independent
                // stricter sibling.
                if overlay.evidence.facts.used_as_serialized {
                    conditionals.push(ConditionalResolvedSchema {
                        target_value_path: target_value_path.clone(),
                        relative_target_segments: target_segments[ancestor_segments.len()..]
                            .to_vec(),
                        ancestor_segments,
                        guards: overlay.guards.clone(),
                        target_schema,
                        provider_schema_candidate: None,
                        preserve_base_schema: overlay.preserve_base_schema,
                        target_is_fragment: overlay.evidence.facts.used_as_fragment,
                        arm_only: false,
                    });
                }
                continue;
            }
            let provider_schema_candidate = resolved_overlay
                .provider_schema_candidate
                .filter(|candidate| candidate.survives_as(&target_schema));

            conditionals.push(ConditionalResolvedSchema {
                target_value_path: target_value_path.clone(),
                relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                ancestor_segments,
                guards: overlay.guards.clone(),
                target_schema,
                provider_schema_candidate,
                preserve_base_schema: overlay.preserve_base_schema,
                target_is_fragment: overlay.evidence.facts.used_as_fragment,
                arm_only: false,
            });
        }
    }

    conditionals
}

pub(crate) fn resolve_overlay_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> ResolvedPathSchema {
    let evidence = overlay.evidence.as_path_evidence(target_value_path);
    PathSchemaResolver::resolve_single_path_evidence(&evidence, provider)
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
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    guards_supported_with_self_path(guards, None, resolved_by_path, values_yaml_doc)
}

/// Fail-implication guard support is more permissive than overlay guard
/// support on TWO axes, both bounded by the arm-only shape (an implication
/// adds an `if guards then requirement` arm and never contributes rows or
/// base structure, so a guard that never fires costs nothing):
/// - a truthy guard over the implication's OWN target path is the
///   capture's structurally derived test subject (`if truthy(x) then x is
///   a string`), not a decoded ambient condition, so the fabricated-path
///   concern does not apply to it even when the chart never declares it;
/// - truthy guards over other undeclared-but-resolved paths lower
///   type-generically: the requirement is a hard render failure, and a
///   fabricated guard path merely leaves the arm inactive.
fn implication_guards_supported(
    guards: &[ConditionalGuard],
    target_value_path: &str,
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                path == target_value_path || resolved_by_path.contains_key(path.as_str())
            }
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. } => true,
            ConditionalGuard::Not(inner) => implication_guards_supported(
                std::slice::from_ref(inner),
                target_value_path,
                resolved_by_path,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                implication_guards_supported(guards, target_value_path, resolved_by_path)
            }
        })
}

fn guards_supported_with_self_path(
    guards: &[ConditionalGuard],
    self_path: Option<&str>,
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
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
                self_path == Some(path.as_str())
                    || yaml_value_at_path(values_yaml_doc, path).is_some()
                    || resolved_by_path
                        .get(path.as_str())
                        .is_some_and(|resolved| schema_is_boolean_like(&resolved.schema))
            }
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_with_self_path(
                std::slice::from_ref(inner),
                self_path,
                resolved_by_path,
                values_yaml_doc,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                guards_supported_with_self_path(
                    guards,
                    self_path,
                    resolved_by_path,
                    values_yaml_doc,
                )
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
/// Lower terminating validator formulas: for each clause, no valid values
/// document satisfies ALL its guards, so the document gets
/// `if <guards> then false` at the guards' shared ancestor. Clauses with
/// any unencodable guard are skipped whole — a partially encoded `if`
/// would reject documents the validator never terminates.
pub(crate) fn append_terminal_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[Vec<ConditionalGuard>],
    values_yaml_doc: &YamlValue,
) {
    for guards in clauses {
        let ancestor_segments = shared_guard_ancestor_segments(guards);
        if !guards
            .iter()
            .all(|guard| guard_encodes_fully(guard, &ancestor_segments, values_yaml_doc))
        {
            continue;
        }
        let condition = SchemaNode::all_of(build_condition_clauses(
            guards,
            &ancestor_segments,
            values_yaml_doc,
        ));
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(Value::Bool(false)),
        );
    }
}

/// The longest common prefix of the PARENTS of every path the guards
/// reference. Presence tests (`required`/`Absent` encodings) need the
/// tested segment to stay relative, so a single-path clause anchors at the
/// path's parent rather than the path itself.
fn shared_guard_ancestor_segments(guards: &[ConditionalGuard]) -> Vec<String> {
    let mut shared: Option<Vec<String>> = None;
    for guard in guards {
        for guard_path in guard.value_paths() {
            let mut segments = split_value_path(&guard_path);
            segments.pop();
            shared = Some(match shared {
                None => segments,
                Some(prefix) => {
                    let len = common_prefix_len(&prefix, &segments);
                    prefix[..len].to_vec()
                }
            });
        }
    }
    shared.unwrap_or_default()
}

pub(crate) fn append_conditional_schemas(
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
        // Schema-less conditionals exist only to mark a serialized-use
        // target for base classification; nothing to emit.
        if crate::schema_model::is_empty_schema(&conditional.target_schema) {
            continue;
        }
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
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded fail implication).
        if group.guard_sets.iter().any(Vec::is_empty) {
            root_schema.append_conditional(
                &ancestor_segments,
                SchemaNode::empty(),
                SchemaNode::foreign(group.fragment),
            );
            continue;
        }
        let mut conditions: Vec<SchemaNode> =
            helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(group.guard_sets)
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
