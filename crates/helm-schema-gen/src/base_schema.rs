use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::overlay_lowering::ConditionalResolvedSchema;
use crate::path_resolver::ResolvedPathSchema;
use crate::schema_model::is_fixed_object_schema;
use crate::schema_node::SchemaNode;
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
    /// Guarded-only target: an unconstrained schema.
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
            Self::Empty => Some(SchemaNode::foreign(crate::schema_model::empty_schema())),
            Self::UnknownObject => Some(SchemaNode::unknown_object()),
            Self::OwnedByAncestor => None,
        }
    }

    pub(crate) const fn replaces(self) -> bool {
        matches!(
            self,
            Self::Serialized | Self::ResolvedUnclosed | Self::Empty
        )
    }

    pub(crate) const fn owns_descendants(self) -> bool {
        matches!(self, Self::Serialized | Self::ResolvedUnclosed)
    }
}

pub(crate) fn classify_base(
    resolved_path: &ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
    owning_ancestors: &BTreeSet<Vec<String>>,
) -> BaseOwner {
    let has_owning_ancestor = (1..resolved_path.path_segments.len())
        .any(|length| owning_ancestors.contains(&resolved_path.path_segments[..length]));
    if has_owning_ancestor {
        return BaseOwner::OwnedByAncestor;
    }
    if conditional_targets.has_guarded_only_item_ancestor(&resolved_path.path_segments) {
        // Wildcard descendants of a guarded collection belong to that
        // collection's conditional lane. Inserting them into the base would
        // rebuild an unconditional object carrier and eliminate valid array
        // lanes. Literal descendants remain independent base evidence.
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
    } else {
        BaseOwner::Empty
    }
}

/// Unclose fixed objects (top level or union arms) in a conditional target's
/// base: overlays own keys the resolved shape does not know about, so a
/// closed base would reject values the guarded renders accept.
fn unclose_fixed_objects(mut schema: Value) -> Value {
    let fixed_object = is_fixed_object_schema(&schema);
    let exact_empty_object = schema.get("maxProperties").and_then(Value::as_u64) == Some(0)
        && schema.get("type").and_then(Value::as_str) == Some("object");
    if (fixed_object || exact_empty_object)
        && let Some(object) = schema.as_object_mut()
    {
        if exact_empty_object {
            object.remove("maxProperties");
        }
        object.insert("additionalProperties".to_string(), serde_json::json!({}));
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

#[derive(Debug, Clone, Copy)]
struct ConditionalTargetSummary {
    preserve_base_schema: bool,
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
            if conditional.arm_only && conditional.preserve_base_schema {
                continue;
            }
            let entry = targets
                .entry(conditional.target_value_path.clone())
                .or_insert(ConditionalTargetSummary {
                    preserve_base_schema: false,
                });
            entry.preserve_base_schema |= conditional.preserve_base_schema;
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

    fn has_guarded_only_item_ancestor(&self, path_segments: &[String]) -> bool {
        self.guarded_only_paths.iter().any(|target_path| {
            path_segments.len() > target_path.len()
                && path_segments.starts_with(target_path)
                && path_segments
                    .get(target_path.len())
                    .is_some_and(|segment| segment == "*")
        })
    }
}
