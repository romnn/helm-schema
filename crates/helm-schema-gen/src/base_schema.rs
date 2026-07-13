use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::merge::union_schema_list;
use crate::overlay_lowering::ConditionalResolvedSchema;
use crate::path_resolver::ResolvedPathSchema;
use crate::schema_model::{is_fixed_object_schema, schema_allows_type};
use crate::schema_node::{JsonSchemaType, SchemaNode};
use crate::split_value_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseOwner {
    /// Not a conditional target: the resolved schema verbatim.
    Resolved,
    /// A serialization transform owns the subtree because its sink exposes
    /// provenance without exposing input shape.
    Serialized,
    /// Preserved conditional target: the resolved schema with fixed objects
    /// unclosed.
    ResolvedUnclosed,
    /// Guarded-only fragment target: the open fragment union.
    OpenFragment,
    /// Guarded-only non-fragment target: an unconstrained schema.
    Empty,
    /// Pathless dependency root with guarded-only descendants.
    UnknownObject,
    /// A strict ancestor owns this subtree through replacement.
    OwnedByAncestor,
}

impl BaseOwner {
    pub(crate) fn schema(self, resolved_path: &ResolvedPathSchema) -> Option<SchemaNode> {
        match self {
            Self::Resolved | Self::Serialized => {
                Some(SchemaNode::foreign(resolved_path.schema.clone()))
            }
            Self::ResolvedUnclosed => Some(SchemaNode::foreign(unclose_fixed_objects(
                resolved_path.schema.clone(),
            ))),
            Self::OpenFragment => Some(open_fragment_target_base_schema(resolved_path)),
            Self::Empty => Some(SchemaNode::foreign(crate::schema_model::empty_schema())),
            Self::UnknownObject => Some(SchemaNode::unknown_object()),
            Self::OwnedByAncestor => None,
        }
    }

    pub(crate) const fn replaces(self) -> bool {
        matches!(
            self,
            Self::Serialized | Self::ResolvedUnclosed | Self::OpenFragment | Self::Empty
        )
    }
}

pub(crate) fn classify_base(
    resolved_path: &ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
    replaced_ancestors: &BTreeSet<Vec<String>>,
) -> BaseOwner {
    let has_replaced_ancestor = (1..resolved_path.path_segments.len())
        .any(|length| replaced_ancestors.contains(&resolved_path.path_segments[..length]));
    if has_replaced_ancestor {
        return BaseOwner::OwnedByAncestor;
    }

    if resolved_path.used_as_serialized {
        return BaseOwner::Serialized;
    }

    if is_pathless_dependency_root_with_guarded_descendant(resolved_path, conditional_targets) {
        return BaseOwner::UnknownObject;
    }

    let Some(target) = conditional_targets
        .targets
        .get(resolved_path.value_path.as_str())
    else {
        return BaseOwner::Resolved;
    };

    if target.preserve_base_schema {
        BaseOwner::ResolvedUnclosed
    } else if target.open_fragment_base_schema {
        BaseOwner::OpenFragment
    } else {
        BaseOwner::Empty
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
    resolved_path: &ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
) -> bool {
    resolved_path.accepted_dependency_values_root_fragment
        && resolved_path.used_as_pathless_fragment
        && conditional_targets.has_guarded_only_descendant(&resolved_path.path_segments)
}

fn open_fragment_target_base_schema(resolved_path: &ResolvedPathSchema) -> SchemaNode {
    let schema = if resolved_path.provider_schema_candidate.is_some() {
        resolved_path.schema.clone()
    } else if is_fixed_object_schema(&resolved_path.schema) {
        // Keep the resolved children but unclose the object: a closed
        // shape at the base would reject keys whose typing now lives
        // under this target's overlays.
        unclose_fixed_objects(resolved_path.schema.clone())
    } else {
        open_fragment_base_schema(&resolved_path.schema)
    };
    SchemaNode::foreign(schema)
}

#[derive(Debug, Clone, Copy)]
struct ConditionalTargetSummary {
    preserve_base_schema: bool,
    open_fragment_base_schema: bool,
}

pub(crate) struct ConditionalTargetIndex {
    targets: BTreeMap<String, ConditionalTargetSummary>,
    guarded_only_paths: BTreeSet<Vec<String>>,
    /// Every conditional target path, preserved or not: the values-defaults
    /// merge must not reshape these subtrees (their shape is overlay-owned,
    /// and inserting into a non-object base would coerce it to a closed map).
    pub(crate) target_paths: BTreeSet<Vec<String>>,
}

impl ConditionalTargetIndex {
    pub(crate) fn from_conditionals(conditionals: &[ConditionalResolvedSchema]) -> Self {
        let mut targets = BTreeMap::new();
        for conditional in conditionals {
            if conditional.arm_only {
                continue;
            }
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
