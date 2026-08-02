use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::overlay_lowering::{ConditionalBaseEffect, LoweredConjunct};
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
        matches!(self, Self::Serialized)
    }

    pub(crate) const fn preserves_descendants(self) -> bool {
        matches!(self, Self::ResolvedUnclosed)
    }
}

pub(crate) fn classify_base(
    resolved_path: &ResolvedPathSchema,
    conditional_targets: &ConditionalTargetIndex,
    owning_ancestors: &BTreeSet<Vec<String>>,
    preserving_ancestors: &BTreeSet<Vec<String>>,
) -> BaseOwner {
    let has_owning_ancestor = (1..resolved_path.path_segments.len()).any(|length| {
        resolved_path
            .path_segments
            .get(..length)
            .is_some_and(|path| owning_ancestors.contains(path))
    });
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

    let has_preserving_ancestor = (1..resolved_path.path_segments.len()).any(|length| {
        resolved_path
            .path_segments
            .get(..length)
            .is_some_and(|path| preserving_ancestors.contains(path))
    });

    if is_pathless_dependency_root_with_guarded_descendant(resolved_path, conditional_targets) {
        return BaseOwner::UnknownObject;
    }

    let target = conditional_targets
        .targets
        .get(resolved_path.value_path.as_str());
    if has_preserving_ancestor {
        if let Some(target) = target {
            return if target.preserve_base_schema {
                BaseOwner::ResolvedUnclosed
            } else {
                BaseOwner::Empty
            };
        }
    } else if resolved_path.used_as_serialized {
        return BaseOwner::Serialized;
    }

    if let Some(target) = target {
        return if target.preserve_base_schema {
            BaseOwner::ResolvedUnclosed
        } else {
            BaseOwner::Empty
        };
    }

    if resolved_path.used_as_serialized {
        return BaseOwner::Serialized;
    }

    BaseOwner::Resolved
}

/// Unclose fixed objects (top level or union arms) in a conditional target's
/// base: overlays own keys the resolved shape does not know about, so a
/// closed base would reject values the guarded renders accept.
fn unclose_fixed_objects(schema: Value) -> Value {
    unclose_fixed_objects_in_union(schema, false)
}

fn unclose_fixed_objects_in_union(mut schema: Value, in_union: bool) -> Value {
    let fixed_object = is_fixed_object_schema(&schema);
    // The declared-`{}` off-state placeholder only uncloses when it IS the
    // base: inside a union its siblings already carry every lane the
    // guarded renders reach, and opening it to any object would erase them
    // (trivy's provider-typed `nodeSelector` sits beside the placeholder).
    let exact_empty_object = !in_union
        && schema.get("maxProperties").and_then(Value::as_u64) == Some(0)
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
                    *arm = unclose_fixed_objects_in_union(std::mem::take(arm), true);
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

#[derive(Clone)]
pub(crate) struct ConditionalTargetIndex {
    targets: BTreeMap<String, ConditionalTargetSummary>,
    /// Targets whose base is wholly owned by guarded overlays. Declared
    /// defaults must not rebuild these paths or anything beneath them.
    pub(crate) guarded_only_paths: BTreeSet<Vec<String>>,
}

impl ConditionalTargetIndex {
    pub(crate) fn from_conditionals(conditionals: &[LoweredConjunct]) -> Self {
        let mut targets = BTreeMap::new();
        let mut required_bases = BTreeSet::new();
        for conditional in conditionals {
            let preserve_base_schema = match conditional.carrier.base_effect {
                ConditionalBaseEffect::None => continue,
                ConditionalBaseEffect::Own => false,
                ConditionalBaseEffect::Preserve => true,
                ConditionalBaseEffect::Require => {
                    required_bases.insert(conditional.carrier.target_value_path.clone());
                    continue;
                }
            };
            let entry = targets
                .entry(conditional.carrier.target_value_path.clone())
                .or_insert(ConditionalTargetSummary {
                    preserve_base_schema: false,
                });
            entry.preserve_base_schema |= preserve_base_schema;
        }
        for path in required_bases {
            if let Some(target) = targets.get_mut(&path) {
                target.preserve_base_schema = true;
            }
        }
        let guarded_only_paths = targets
            .iter()
            .filter(|(path, target)| !path.is_empty() && !target.preserve_base_schema)
            .map(|(path, _)| split_value_path(path))
            .collect();
        Self {
            targets,
            guarded_only_paths,
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
