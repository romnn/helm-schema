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
        EmissionPolicy::for_profile(self).resolved()
    }
}

/// Read-only resolved emission policy used in final output metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedEmissionPolicy {
    root_anchored_conditionals: bool,
    local_conditionals: bool,
    terminal_clauses: bool,
    kind_partitions: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmissionPolicy {
    knobs: EmissionKnobs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmissionKnobs {
    pub(crate) root_anchored_conditionals: bool,
    pub(crate) local_conditionals: bool,
    pub(crate) terminal_clauses: bool,
    pub(crate) kind_partitions: bool,
}

impl EmissionPolicy {
    pub(crate) const fn new(knobs: EmissionKnobs) -> Self {
        Self { knobs }
    }

    pub(crate) const fn for_profile(profile: SchemaProfile) -> Self {
        match profile {
            SchemaProfile::Full => Self::new(EmissionKnobs {
                root_anchored_conditionals: true,
                local_conditionals: true,
                terminal_clauses: true,
                kind_partitions: true,
            }),
            SchemaProfile::Lean => Self::new(EmissionKnobs {
                root_anchored_conditionals: false,
                local_conditionals: true,
                terminal_clauses: false,
                kind_partitions: false,
            }),
        }
    }

    pub(crate) const fn is_valid(self) -> bool {
        !self.knobs.kind_partitions
            || self.knobs.root_anchored_conditionals
            || self.knobs.local_conditionals
    }

    const fn resolved(self) -> ResolvedEmissionPolicy {
        ResolvedEmissionPolicy {
            root_anchored_conditionals: self.knobs.root_anchored_conditionals,
            local_conditionals: self.knobs.local_conditionals,
            terminal_clauses: self.knobs.terminal_clauses,
            kind_partitions: self.knobs.kind_partitions,
        }
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
                    self.knobs.root_anchored_conditionals
                } else {
                    self.knobs.local_conditionals
                }
            }
            EmissionClass::Conditional {
                anchor,
                flavor: ConditionalFlavor::KindPartition,
                ..
            } => {
                self.knobs.kind_partitions
                    && if anchor.is_root() {
                        self.knobs.root_anchored_conditionals
                    } else {
                        self.knobs.local_conditionals
                    }
            }
            EmissionClass::Terminal { .. } => self.knobs.terminal_clauses,
        }
    }
}
