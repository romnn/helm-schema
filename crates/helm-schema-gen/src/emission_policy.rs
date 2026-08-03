//! Schema emission policy.

use helm_schema_core::ConditionalGuard;
use serde::Serialize;

/// Version of the emission-policy vocabulary used in output annotations.
pub const POLICY_VOCABULARY_VERSION: u64 = 1;

/// Selects how much analyzed contract evidence is emitted as JSON Schema.
///
/// Profiles change only emission. They do not change chart analysis or the
/// recovered contract. A reduced profile may remove constraints and therefore
/// widen acceptance, but must never introduce a rejection that the full
/// profile does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaProfile {
    /// Emits every constraint supported by the schema backend.
    #[default]
    Full,
    /// Omits document-level conditional validation while preserving base
    /// path and provider constraints.
    ///
    /// This profile exists for Helm's validator, whose schema compilation
    /// cost grows superlinearly on large conditional documents.
    Lean,
}

impl SchemaProfile {
    /// Stable profile spelling used in serialized policy metadata.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Lean => "lean",
        }
    }

    /// Resolves this preset into the complete version-1 policy vocabulary.
    #[must_use]
    pub const fn resolved_policy(self) -> ResolvedEmissionPolicy {
        ResolvedEmissionPolicy {
            requested_profile: Some(self),
            policy: EmissionPolicy::for_profile(self),
        }
    }
}

/// A complete, valid selection over the version-1 emission vocabulary.
///
/// Construction is checked so callers cannot enable kind partitions while
/// disabling every anchor capable of carrying them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EmissionPolicy {
    root_anchored_conditionals: bool,
    local_conditionals: bool,
    terminal_clauses: bool,
    kind_partitions: bool,
}

/// Conditional anchor lanes selected by a complete emission policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalAnchors {
    /// No conditional anchor lane is selected.
    None,
    /// Only document-root conditionals are selected.
    Root,
    /// Only locally anchored conditionals are selected.
    Local,
    /// Both root and local conditional anchors are selected.
    RootAndLocal,
}

impl ConditionalAnchors {
    /// Creates the exhaustive anchor selection from its two public knobs.
    #[must_use]
    pub const fn new(root_anchored_conditionals: bool, local_conditionals: bool) -> Self {
        match (root_anchored_conditionals, local_conditionals) {
            (false, false) => Self::None,
            (true, false) => Self::Root,
            (false, true) => Self::Local,
            (true, true) => Self::RootAndLocal,
        }
    }

    const fn root_selected(self) -> bool {
        matches!(self, Self::Root | Self::RootAndLocal)
    }

    const fn local_selected(self) -> bool {
        matches!(self, Self::Local | Self::RootAndLocal)
    }
}

impl EmissionPolicy {
    /// Creates an emission policy after checking the complete knob matrix.
    ///
    /// # Errors
    ///
    /// Returns an error when kind partitions are enabled while both root and
    /// local conditional anchors are disabled.
    pub const fn new(
        conditional_anchors: ConditionalAnchors,
        terminal_clauses: bool,
        kind_partitions: bool,
    ) -> Result<Self, InvalidEmissionPolicy> {
        let policy = Self {
            root_anchored_conditionals: conditional_anchors.root_selected(),
            local_conditionals: conditional_anchors.local_selected(),
            terminal_clauses,
            kind_partitions,
        };
        if policy.is_valid() {
            Ok(policy)
        } else {
            Err(InvalidEmissionPolicy)
        }
    }

    pub(crate) const fn for_profile(profile: SchemaProfile) -> Self {
        match profile {
            SchemaProfile::Full => Self {
                root_anchored_conditionals: true,
                local_conditionals: true,
                terminal_clauses: true,
                kind_partitions: true,
            },
            SchemaProfile::Lean => Self {
                root_anchored_conditionals: false,
                local_conditionals: true,
                terminal_clauses: false,
                kind_partitions: false,
            },
        }
    }

    pub(crate) const fn is_valid(self) -> bool {
        !self.kind_partitions || self.root_anchored_conditionals || self.local_conditionals
    }

    const fn apply_delta(self, delta: EmissionPolicyDelta) -> Result<Self, InvalidEmissionPolicy> {
        let root_anchored_conditionals = match delta.root_anchored_conditionals {
            Some(value) => value,
            None => self.root_anchored_conditionals,
        };
        let local_conditionals = match delta.local_conditionals {
            Some(value) => value,
            None => self.local_conditionals,
        };
        Self::new(
            ConditionalAnchors::new(root_anchored_conditionals, local_conditionals),
            match delta.terminal_clauses {
                Some(value) => value,
                None => self.terminal_clauses,
            },
            match delta.kind_partitions {
                Some(value) => value,
                None => self.kind_partitions,
            },
        )
    }

    /// Whether root-anchored ordinary conditionals are selected.
    #[must_use]
    pub const fn root_anchored_conditionals(self) -> bool {
        self.root_anchored_conditionals
    }

    /// Whether locally anchored ordinary conditionals are selected.
    #[must_use]
    pub const fn local_conditionals(self) -> bool {
        self.local_conditionals
    }

    /// Whether unconditional and guarded terminal clauses are selected.
    #[must_use]
    pub const fn terminal_clauses(self) -> bool {
        self.terminal_clauses
    }

    /// Whether kind-partition refinements are selected at enabled anchors.
    #[must_use]
    pub const fn kind_partitions(self) -> bool {
        self.kind_partitions
    }

    pub(crate) fn selects(self, class: &EmissionClass) -> bool {
        match class {
            EmissionClass::Mandatory => true,
            EmissionClass::Conditional {
                anchor,
                flavor: ConditionalFlavor::Ordinary,
                ..
            } => {
                if anchor.is_root() {
                    self.root_anchored_conditionals
                } else {
                    self.local_conditionals
                }
            }
            EmissionClass::Conditional {
                anchor,
                flavor: ConditionalFlavor::KindPartition,
                ..
            } => {
                self.kind_partitions
                    && if anchor.is_root() {
                        self.root_anchored_conditionals
                    } else {
                        self.local_conditionals
                    }
            }
            EmissionClass::Terminal { .. } => self.terminal_clauses,
        }
    }
}

/// Error returned for a contradictory emission knob matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("kind partitions require root-anchored-conditionals or local-conditionals to be enabled")]
pub struct InvalidEmissionPolicy;

/// Optional version-1 W-class knob changes applied over a profile preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmissionPolicyDelta {
    root_anchored_conditionals: Option<bool>,
    local_conditionals: Option<bool>,
    terminal_clauses: Option<bool>,
    kind_partitions: Option<bool>,
}

impl EmissionPolicyDelta {
    /// Creates a delta covering every W-class knob.
    #[must_use]
    pub const fn new(
        root_anchored_conditionals: Option<bool>,
        local_conditionals: Option<bool>,
        terminal_clauses: Option<bool>,
        kind_partitions: Option<bool>,
    ) -> Self {
        Self {
            root_anchored_conditionals,
            local_conditionals,
            terminal_clauses,
            kind_partitions,
        }
    }

    /// Optional root-anchored conditional override.
    #[must_use]
    pub const fn root_anchored_conditionals(self) -> Option<bool> {
        self.root_anchored_conditionals
    }

    /// Optional local conditional override.
    #[must_use]
    pub const fn local_conditionals(self) -> Option<bool> {
        self.local_conditionals
    }

    /// Optional terminal-clause override.
    #[must_use]
    pub const fn terminal_clauses(self) -> Option<bool> {
        self.terminal_clauses
    }

    /// Optional kind-partition override.
    #[must_use]
    pub const fn kind_partitions(self) -> Option<bool> {
        self.kind_partitions
    }
}

/// Caller selection retaining either preset provenance or an explicit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmissionSelection {
    /// A stable profile plus optional W-class knob changes.
    Preset {
        /// Requested stable profile.
        profile: SchemaProfile,
        /// W-class changes applied over the profile.
        delta: EmissionPolicyDelta,
    },
    /// An explicitly constructed complete policy.
    Explicit(EmissionPolicy),
}

impl EmissionSelection {
    /// Resolves the selection once into its complete policy and provenance.
    ///
    /// # Errors
    ///
    /// Returns an error when preset deltas produce a contradictory knob matrix.
    pub const fn resolve(self) -> Result<ResolvedEmissionPolicy, InvalidEmissionPolicy> {
        match self {
            Self::Preset { profile, delta } => {
                let policy = match EmissionPolicy::for_profile(profile).apply_delta(delta) {
                    Ok(policy) => policy,
                    Err(error) => return Err(error),
                };
                Ok(ResolvedEmissionPolicy {
                    requested_profile: Some(profile),
                    policy,
                })
            }
            Self::Explicit(policy) => Ok(ResolvedEmissionPolicy {
                requested_profile: None,
                policy,
            }),
        }
    }
}

impl Default for EmissionSelection {
    fn default() -> Self {
        SchemaProfile::Full.into()
    }
}

impl From<SchemaProfile> for EmissionSelection {
    fn from(profile: SchemaProfile) -> Self {
        Self::Preset {
            profile,
            delta: EmissionPolicyDelta::default(),
        }
    }
}

impl From<EmissionPolicy> for EmissionSelection {
    fn from(policy: EmissionPolicy) -> Self {
        Self::Explicit(policy)
    }
}

/// One resolved emission source used by generation and final annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedEmissionPolicy {
    requested_profile: Option<SchemaProfile>,
    policy: EmissionPolicy,
}

impl ResolvedEmissionPolicy {
    /// Profile provenance retained for the final annotation.
    #[must_use]
    pub const fn requested_profile(self) -> Option<SchemaProfile> {
        self.requested_profile
    }

    /// Complete policy consumed by generation.
    #[must_use]
    pub const fn policy(self) -> EmissionPolicy {
        self.policy
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NestedGuardScope {
    pub(crate) ancestor_segments: Vec<String>,
    pub(crate) guards: Vec<ConditionalGuard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GuardScopes {
    pub(crate) outer: Vec<ConditionalGuard>,
    pub(crate) nested: Vec<NestedGuardScope>,
}

impl GuardScopes {
    pub(crate) fn new(outer: Vec<ConditionalGuard>, nested: Vec<NestedGuardScope>) -> Self {
        Self { outer, nested }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.outer.is_empty() && self.nested.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NonEmptyGuardScopes(GuardScopes);

impl NonEmptyGuardScopes {
    pub(crate) fn new(scopes: GuardScopes) -> Option<Self> {
        (!scopes.is_empty()).then_some(Self(scopes))
    }

    pub(crate) fn scopes(&self) -> &GuardScopes {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmissionAnchor {
    Root,
    Local(Vec<String>),
}

impl EmissionAnchor {
    pub(crate) fn from_segments(segments: &[String]) -> Self {
        if segments.is_empty() {
            Self::Root
        } else {
            Self::Local(segments.to_vec())
        }
    }

    const fn is_root(&self) -> bool {
        matches!(self, Self::Root)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConditionalFlavor {
    Ordinary,
    KindPartition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalWhen {
    Always,
    Guarded(NonEmptyGuardScopes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmissionClass {
    Mandatory,
    Conditional {
        guards: GuardScopes,
        anchor: EmissionAnchor,
        flavor: ConditionalFlavor,
    },
    Terminal {
        when: TerminalWhen,
    },
}

impl EmissionClass {
    pub(crate) fn conditional(
        guards: GuardScopes,
        anchor_segments: &[String],
        flavor: ConditionalFlavor,
    ) -> Self {
        if guards.is_empty() {
            Self::Mandatory
        } else {
            Self::Conditional {
                guards,
                anchor: EmissionAnchor::from_segments(anchor_segments),
                flavor,
            }
        }
    }

    pub(crate) fn terminal_guarded(guards: Vec<ConditionalGuard>) -> Option<Self> {
        let scopes = NonEmptyGuardScopes::new(GuardScopes::new(guards, Vec::new()))?;
        Some(Self::Terminal {
            when: TerminalWhen::Guarded(scopes),
        })
    }

    pub(crate) const fn terminal_always() -> Self {
        Self::Terminal {
            when: TerminalWhen::Always,
        }
    }

    pub(crate) const fn kind(&self) -> EmissionClassKind {
        match self {
            Self::Mandatory => EmissionClassKind::Mandatory,
            Self::Conditional {
                anchor: EmissionAnchor::Root,
                flavor: ConditionalFlavor::Ordinary,
                ..
            } => EmissionClassKind::OrdinaryRoot,
            Self::Conditional {
                anchor: EmissionAnchor::Local(_),
                flavor: ConditionalFlavor::Ordinary,
                ..
            } => EmissionClassKind::OrdinaryLocal,
            Self::Conditional {
                anchor: EmissionAnchor::Root,
                flavor: ConditionalFlavor::KindPartition,
                ..
            } => EmissionClassKind::KindPartitionRoot,
            Self::Conditional {
                anchor: EmissionAnchor::Local(_),
                flavor: ConditionalFlavor::KindPartition,
                ..
            } => EmissionClassKind::KindPartitionLocal,
            Self::Terminal {
                when: TerminalWhen::Always,
            } => EmissionClassKind::TerminalAlways,
            Self::Terminal {
                when: TerminalWhen::Guarded(_),
            } => EmissionClassKind::TerminalGuarded,
        }
    }
}

/// Policy-relevant class without its guard and anchor payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmissionClassKind {
    /// Constraint retained by every decision-table policy.
    Mandatory,
    /// Ordinary conditional anchored at the document root.
    OrdinaryRoot,
    /// Ordinary conditional anchored below the document root.
    OrdinaryLocal,
    /// Kind partition anchored at the document root.
    KindPartitionRoot,
    /// Kind partition anchored below the document root.
    KindPartitionLocal,
    /// Unconditional terminating behavior.
    TerminalAlways,
    /// Guarded terminating behavior.
    TerminalGuarded,
}

/// Producer category used for emission diagnostics and accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmissionOrigin {
    /// Guarded path evidence.
    Overlay,
    /// Requirement implied by a failing render path.
    FailImplication,
    /// Constraint for a lower-precedence merge layer.
    MergeShadow,
    /// Provider member conditionally retained by omission logic.
    OmittedMember,
    /// Requirement projected back from a rendered sink.
    Backprojection,
}
