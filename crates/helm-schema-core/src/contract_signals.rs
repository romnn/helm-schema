use std::collections::{BTreeMap, BTreeSet};

use crate::{GuardValue, ProviderSchemaUse};

/// Values-decidable guard expression that can be lowered into JSON Schema
/// conditionals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalGuard {
    Truthy { path: String },
    With { path: String },
    Eq { path: String, value: GuardValue },
    NotEq { path: String, value: GuardValue },
    Absent { path: String },
    TypeIs { path: String, schema_type: String },
    Not(Box<ConditionalGuard>),
    AllOf(Vec<ConditionalGuard>),
    AnyOf(Vec<ConditionalGuard>),
}

impl ConditionalGuard {
    #[must_use]
    pub fn value_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_value_paths(&mut paths);
        paths
    }

    fn collect_value_paths(&self, paths: &mut BTreeSet<String>) {
        match self {
            Self::Truthy { path }
            | Self::With { path }
            | Self::Eq { path, .. }
            | Self::NotEq { path, .. }
            | Self::Absent { path }
            | Self::TypeIs { path, .. } => {
                paths.insert(path.clone());
            }
            Self::Not(inner) => inner.collect_value_paths(paths),
            Self::AllOf(guards) | Self::AnyOf(guards) => {
                for guard in guards {
                    guard.collect_value_paths(paths);
                }
            }
        }
    }
}

/// Conditionally-scoped values path whose schema can be lowered under a
/// values-decidable guard set.
///
/// Multiple entries in `guards` mean conjunction: all guards in the set must
/// hold for the overlay to apply.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConditionalPathOverlay {
    pub guards: Vec<ConditionalGuard>,
    pub evidence: ConditionalOverlayEvidence,
    /// Keep the unconditional/base schema for this path alongside the guarded
    /// overlay because the contract also observed an unguarded use.
    pub preserve_base_schema: bool,
}

/// Branch-local evidence for one conditional schema overlay.
///
/// The target path is implicit from the enclosing [`ContractPathSchemaEvidence`]
/// entry that owns the overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConditionalOverlayEvidence {
    pub facts: ContractValuePathFacts,
    pub metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    pub type_hints: BTreeSet<String>,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
}

impl ConditionalOverlayEvidence {
    #[must_use]
    pub fn as_path_evidence(&self, value_path: &str) -> ContractPathSchemaEvidence {
        ContractPathSchemaEvidence {
            value_path: value_path.to_string(),
            is_referenced_value_path: true,
            facts: self.facts,
            guard_predicates: Vec::new(),
            metadata_field_kinds: self.metadata_field_kinds.clone(),
            type_hints: self.type_hints.clone(),
            provider_schema_uses: self.provider_schema_uses.clone(),
            requiredness: ContractRequirednessEvidence::default(),
            conditional_overlays: Vec::new(),
        }
    }
}

/// Kubernetes `metadata.*` field shape referenced by a values path.
///
/// The contract layer records the field category structurally from the
/// rendered document path. JSON Schema lowering remains a generator policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetadataFieldKind {
    /// `metadata.labels` and `metadata.annotations`.
    StringMap,
    /// `metadata.name`.
    Name,
    /// `metadata.namespace`.
    Namespace,
}

/// All schema-lowering evidence for one values path.
///
/// The contract layer owns this view so downstream generation can consume one
/// path-local static-analysis fact instead of reassembling meaning from
/// several parallel maps.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractPathSchemaEvidence {
    pub value_path: String,
    pub is_referenced_value_path: bool,
    pub facts: ContractValuePathFacts,
    pub guard_predicates: Vec<ConditionalGuard>,
    pub metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    pub type_hints: BTreeSet<String>,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub requiredness: ContractRequirednessEvidence,
    pub conditional_overlays: Vec<ConditionalPathOverlay>,
}

impl ContractPathSchemaEvidence {
    #[must_use]
    pub fn is_required_inference_candidate(&self) -> bool {
        self.requiredness.is_positive_header
            && !self.requiredness.has_default_fallback
            && !self.requiredness.is_conditionally_optional
            && self.facts.has_non_self_guarded_render_use()
    }
}

/// Contract-derived facts consumed by core values-schema generation.
///
/// This is the typed boundary between static template interpretation and JSON
/// Schema lowering. Optional post-passes can ask for their own projections,
/// but core schema generation should consume this artifact rather than
/// re-reading raw contract claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractSchemaSignals {
    schema_evidence_by_value_path: BTreeMap<String, ContractPathSchemaEvidence>,
    referenced_value_paths: BTreeSet<String>,
    pruned_parent_value_paths: BTreeSet<String>,
}

impl ContractSchemaSignals {
    #[must_use]
    pub fn new(
        schema_evidence_by_value_path: BTreeMap<String, ContractPathSchemaEvidence>,
    ) -> Self {
        let referenced_value_paths = schema_evidence_by_value_path
            .iter()
            .filter(|(_, evidence)| evidence.is_referenced_value_path)
            .map(|(path, _)| path.clone())
            .collect();
        let pruned_parent_value_paths = schema_evidence_by_value_path
            .iter()
            .filter(|(_, evidence)| {
                evidence.facts.has_referenced_descendants && !evidence.facts.used_as_fragment
            })
            .map(|(path, _)| path.clone())
            .collect();
        Self {
            schema_evidence_by_value_path,
            referenced_value_paths,
            pruned_parent_value_paths,
        }
    }

    #[must_use]
    pub fn schema_evidence_by_value_path(&self) -> &BTreeMap<String, ContractPathSchemaEvidence> {
        &self.schema_evidence_by_value_path
    }

    /// Values paths the contract directly referenced, in stable order.
    #[must_use]
    pub fn referenced_value_paths(&self) -> &BTreeSet<String> {
        &self.referenced_value_paths
    }

    /// Non-fragment parent paths whose referenced descendants own their own
    /// schema evidence, so parent-level defaults must not restate them.
    #[must_use]
    pub fn pruned_parent_value_paths(&self) -> &BTreeSet<String> {
        &self.pruned_parent_value_paths
    }

    #[must_use]
    pub fn evidence_for(&self, value_path: &str) -> Option<&ContractPathSchemaEvidence> {
        self.schema_evidence_by_value_path.get(value_path)
    }
}

/// Schema-generation facts for one input values path.
///
/// This bundles the contract-owned path state that schema lowering needs, so
/// generator code does not have to reconstruct semantic facts from multiple
/// lower-level projections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractValuePathFacts {
    pub has_referenced_descendants: bool,
    pub used_as_fragment: bool,
    pub used_as_pathless_fragment: bool,
    pub accepted_values_root_fragment: bool,
    pub accepted_dependency_values_root_fragment: bool,
    pub is_ranged_source: bool,
    pub is_partial_scalar_value_path: bool,
    pub has_render_use: bool,
    pub has_unconditional_render_use: bool,
    pub has_self_guarded_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_self_range_guard_render_use: bool,
    pub is_nullable: bool,
}

impl ContractValuePathFacts {
    pub fn record_render_use(&mut self, range_guarded: bool, self_guarded: Option<bool>) {
        if !self.has_render_use {
            self.all_render_uses_self_guarded = true;
        }
        self.has_render_use = true;
        self.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            self.has_self_guarded_render_use |= self_guarded;
            self.all_render_uses_self_guarded &= self_guarded;
        }
    }

    pub fn merge_render_use_facts(&mut self, other: Self) {
        if !other.has_render_use {
            return;
        }
        if !self.has_render_use {
            self.all_render_uses_self_guarded = true;
        }
        self.has_render_use = true;
        self.has_unconditional_render_use |= other.has_unconditional_render_use;
        self.has_self_guarded_render_use |= other.has_self_guarded_render_use;
        self.has_self_range_guard_render_use |= other.has_self_range_guard_render_use;
        self.all_render_uses_self_guarded &= other.all_render_uses_self_guarded;
    }

    #[must_use]
    pub fn has_non_self_guarded_render_use(self) -> bool {
        self.has_render_use
            && !self.has_self_guarded_render_use
            && !self.all_render_uses_self_guarded
    }
}

/// Path-local evidence consumed by the optional `--infer-required` post-pass.
///
/// These are still static-analysis facts, not a decision that the path must be
/// required. The generator combines them with render-use facts and chart
/// defaults before mutating the JSON Schema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractRequirednessEvidence {
    pub is_positive_header: bool,
    pub is_conditionally_optional: bool,
    pub has_default_fallback: bool,
}
