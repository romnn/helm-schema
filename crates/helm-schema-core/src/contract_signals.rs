use std::collections::{BTreeMap, BTreeSet};

use crate::{GuardValue, ProviderSchemaUse};

/// Values-decidable guard expression that can be lowered into JSON Schema
/// conditionals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalGuard {
    Truthy {
        path: String,
    },
    With {
        path: String,
    },
    Eq {
        path: String,
        value: GuardValue,
    },
    NotEq {
        path: String,
        value: GuardValue,
    },
    Absent {
        path: String,
    },
    TypeIs {
        path: String,
        schema_type: String,
    },
    MatchesPattern {
        path: String,
        pattern: String,
    },
    /// The path's RAW value is a JSON integer strictly greater than `bound`
    /// — a sound SUBSET of the Sprig coercion (`gt (int64 x) bound`) it
    /// stands in for, valid only where firing less often is safe.
    IntGt {
        path: String,
        bound: i64,
    },
    /// The mapping at `path` contains the literal member `key`. The key is
    /// an OPAQUE property name (it may contain dots), so it rides beside
    /// the segmented path instead of being appended to it.
    HasKey {
        path: String,
        key: String,
    },
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
            | Self::TypeIs { path, .. }
            | Self::MatchesPattern { path, .. }
            | Self::IntGt { path, .. }
            | Self::HasKey { path, .. } => {
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
            guarded_type_hints: BTreeSet::new(),
            fallback_type_hints: BTreeSet::new(),
            provider_schema_uses: self.provider_schema_uses.clone(),
            requiredness: ContractRequirednessEvidence::default(),
            conditional_overlays: Vec::new(),
            fail_implications: Vec::new(),
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
    /// Hints observed only under branch predicates. At the path level these
    /// may only WIDEN (add accepted alternatives to an otherwise-typed
    /// base): `allOf` branches can narrow but never re-widen a base, so a
    /// branch-scoped domain alternative must surface here.
    pub guarded_type_hints: BTreeSet<String>,
    /// Hints from literal `default`/`coalesce` fallbacks. The selection call
    /// never consumes the raw value — every Helm-empty input takes the
    /// fallback — so these type only the truthy arm and base lowering must
    /// keep the whole Helm-falsy set open beside them.
    pub fallback_type_hints: BTreeSet<String>,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub requiredness: ContractRequirednessEvidence,
    pub conditional_overlays: Vec<ConditionalPathOverlay>,
    /// Requirements implied by explicit `fail` branches: the failing test's
    /// negation must hold wherever the outer guards do. Runtime-hard
    /// evidence — rendering genuinely aborts — so lowering must not let
    /// weaker streams suppress it.
    pub fail_implications: Vec<ContractFailImplication>,
}

/// A chart-wide default subtree merged into an effective `.Values` subtree.
///
/// The target remains user-overridable; the source supplies only keys absent
/// from the target, matching `mustMergeOverwrite SOURCE TARGET` before the
/// result replaces a root or nested `Values` object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValuesDefaultSource {
    /// Effective values subtree receiving defaults, with an empty path denoting `.Values`.
    pub target_path: String,
    /// Chart values subtree supplying defaults.
    pub source_path: String,
}

/// One `fail`-branch implication on a values path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractFailImplication {
    /// Conditions outside the failing test; empty means the requirement
    /// binds the path unconditionally.
    pub outer_guards: Vec<ConditionalGuard>,
    /// The runtime value affected by the requirement.
    pub target: ContractRequirementTarget,
    /// Conjunction of requirements the affected value must satisfy.
    pub requirements: Vec<FailValueRequirement>,
}

/// Runtime value within a values-path contract that must satisfy a requirement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractRequirementTarget {
    /// The values path itself.
    Value,
    /// Every value produced by ranging the path.
    ///
    /// `allow_integer` describes the range header's own integer lane. It is
    /// false for a two-variable range, even when the member requirement would
    /// otherwise accept integer values.
    Members { allow_integer: bool },
    /// Values of object entries whose keys start with the literal prefix.
    /// Empty arrays and null remain valid because they execute no range body.
    MembersMatchingPrefix { prefix: String },
    /// Each ranged member whose literal sibling equals `value` must satisfy
    /// the requirements at `target_path`, both relative to that member.
    MembersWhereEquals {
        guard_path: Vec<String>,
        value: GuardValue,
        target_path: Vec<String>,
    },
    /// Every ranged member must CONTAIN `target_path` and its value there
    /// must satisfy the requirements — an unconditional per-member field
    /// read by a strict consumer (`tpl $member.url` fails on a missing or
    /// non-string field). `allow_integer` mirrors [`Self::Members`].
    MembersAt {
        target_path: Vec<String>,
        allow_integer: bool,
    },
    /// Every key produced by ranging the path.
    Keys,
}

/// One requirement a `fail` branch imposes on an affected value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailValueRequirement {
    /// The value must be of this JSON Schema type.
    SchemaType(String),
    /// The value must be of this JSON Schema type IF present and non-null:
    /// Go's `eq`/`ne` compare `nil` against anything, so a missing or null
    /// comparison operand renders while a present value of a different
    /// basic kind aborts.
    ComparableKind(String),
    /// The value must NOT be of this JSON Schema type.
    NotSchemaType(String),
    /// The value must be an object containing this member.
    HasMember(String),
    /// The value must be a string matching this regular expression
    /// (`regexMatch` type-asserts a string subject, so string-ness rides
    /// along).
    MatchesPattern { pattern: String, templated: bool },
    /// The value HOSTS literal member reads: it must be an object — or one
    /// of the kinds the chart's own type dispatch provably handles before
    /// the reads run (nack converts the string image form with `set`).
    MemberHost { handled_kinds: Vec<String> },
    /// The value is iterated by `range`: collections and nil render, and
    /// integer counts iterate when the loop body has no member structure.
    Iterable { allow_integer: bool },
    /// A zero-based position must exist before `index` can project it.
    /// Arrays lower exactly; strings remain conservative because Go indexes
    /// bytes while JSON Schema `minLength` counts Unicode code points.
    IndexableAt(usize),
    /// Splitting the textual form must produce at least `segments` entries.
    /// When the input was first passed through a total text conversion,
    /// non-string inputs remain conservatively accepted.
    SplitSegmentsAtLeast {
        separator: String,
        segments: usize,
        allow_non_string: bool,
    },
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
    direct_ranged_value_paths: BTreeSet<String>,
    values_default_sources: BTreeSet<ValuesDefaultSource>,
    /// Terminating validator formulas spanning several paths: rendering
    /// aborts whenever ALL guards of one clause hold, so no valid values
    /// document may satisfy them (`fail`/`required` under fully lowerable
    /// cross-path conditions).
    terminal_clauses: Vec<Vec<ConditionalGuard>>,
}

impl ContractSchemaSignals {
    #[must_use]
    pub fn new(
        schema_evidence_by_value_path: BTreeMap<String, ContractPathSchemaEvidence>,
        terminal_clauses: Vec<Vec<ConditionalGuard>>,
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
        let direct_ranged_value_paths = schema_evidence_by_value_path
            .iter()
            .filter(|(_, evidence)| evidence.facts.is_direct_ranged_source)
            .map(|(path, _)| path.clone())
            .collect();
        Self {
            schema_evidence_by_value_path,
            referenced_value_paths,
            pruned_parent_value_paths,
            direct_ranged_value_paths,
            values_default_sources: BTreeSet::new(),
            terminal_clauses,
        }
    }

    /// Attaches chart subtrees that supply runtime defaults to effective values paths.
    #[must_use]
    pub fn with_values_default_sources(
        mut self,
        sources: impl IntoIterator<Item = ValuesDefaultSource>,
    ) -> Self {
        self.values_default_sources.extend(sources);
        self
    }

    /// Default subtrees applied to effective values before templates consume them.
    #[must_use]
    pub fn values_default_sources(&self) -> &BTreeSet<ValuesDefaultSource> {
        &self.values_default_sources
    }

    /// Paths the chart ranges DIRECTLY: their runtime iterable domain is
    /// wider than any declared shape, so ancestor subtree schemas must not
    /// shadow their own resolutions.
    #[must_use]
    pub fn direct_ranged_value_paths(&self) -> &BTreeSet<String> {
        &self.direct_ranged_value_paths
    }

    /// Terminating validator formulas: no valid values document satisfies
    /// all guards of one clause.
    #[must_use]
    pub fn terminal_clauses(&self) -> &[Vec<ConditionalGuard>] {
        &self.terminal_clauses
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
    /// Descendant rows that continue through a `*` item segment. Item rows
    /// describe a ranged collection's element shape; a literal member read
    /// (e.g. a guard probing one key of a user-populated map) does not.
    pub has_item_descendants: bool,
    /// Item descendants that continue INTO element structure (`p.*.field`).
    /// A bare `p.*` value row proves no LIST shape: `range` iterates maps
    /// too, so declared-empty maps with only bare member-value rows stay
    /// user-populated.
    pub has_structured_item_descendants: bool,
    pub used_as_fragment: bool,
    /// The path renders through a serializing or total-stringification sink
    /// (`tpl (toYaml …)`, `quote`, `toString`, `join`): any input type
    /// renders, so the use exposes provenance but no input shape.
    pub used_as_serialized: bool,
    /// The path is rendered through `toYaml`. The input kind is unrestricted,
    /// while the resulting YAML fragment still obeys structural placement.
    pub used_as_yaml_serialized: bool,
    /// A string-consuming transform (`trunc`, `b64enc`, `fromYaml`, a
    /// dynamic `printf` format) bound a real runtime string contract on the
    /// path: rendering fails for non-string values, so this typing survives
    /// even when another use stringifies the path.
    pub has_string_contract: bool,
    /// Some `path.*` member row carries a runtime string contract (`tpl`
    /// over each ranged member): integer iteration yields int members the
    /// contract rejects, so the integer lane closes.
    pub has_string_contract_items: bool,
    pub used_as_pathless_fragment: bool,
    pub accepted_values_root_fragment: bool,
    pub accepted_dependency_values_root_fragment: bool,
    pub is_ranged_source: bool,
    /// The chart ranges this path DIRECTLY (`range .Values.x`), so the
    /// runtime iterable domain applies to the path's own value.
    pub is_direct_ranged_source: bool,
    /// Some direct range over this path uses TWO variables
    /// (`range $k, $v := …`): integers iterate single-variable ranges only
    /// ("can't use 2 to iterate over more than one variable").
    pub has_destructured_range_use: bool,
    /// Some direct range sees the path after JSON decoding, where numbers are
    /// `float64` values rather than Helm's integer iteration counts.
    pub has_json_decoded_range_use: bool,
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
    pub(crate) fn has_non_self_guarded_render_use(self) -> bool {
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
