//! Typed records of the decisions one emitter run takes for each values path.
//!
//! Each record is written where its decision executes, from the operands that
//! decision already computed. The records never feed back into generation:
//! they take no part in comparison, selection, caching or definition naming.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ValuesPath;
use serde::Serialize;

use crate::base_schema::BaseOwner;
use crate::emission_policy::EmissionOrigin;
use crate::overlay_lowering::ConditionalBaseEffect;
use crate::schema_model::is_empty_schema;
use crate::schema_node::JsonSchemaType;

/// Generation decisions for every values path one emitter run resolved,
/// lowered or materialized.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenerationDecisions {
    paths: BTreeMap<ValuesPath, PathGenerationDecision>,
}

impl GenerationDecisions {
    /// The decisions recorded for `path`, if generation reached it.
    #[must_use]
    pub fn path(&self, path: &ValuesPath) -> Option<&PathGenerationDecision> {
        self.paths.get(path)
    }

    /// Every path with recorded decisions, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&ValuesPath, &PathGenerationDecision)> {
        self.paths.iter()
    }

    pub(crate) fn path_mut(&mut self, path: &ValuesPath) -> &mut PathGenerationDecision {
        self.paths.entry(path.clone()).or_default()
    }
}

/// The decisions recorded for one values path, grouped by evaluation context.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PathGenerationDecision {
    /// Resolution of the path's unconditional evidence, when it was resolved.
    pub base: Option<PathResolution>,
    /// Resolutions of the path's guarded overlays, in overlay order.
    pub overlays: Vec<OverlayResolution>,
    /// Checks deciding whether a guarded requirement owns the path's base.
    pub containment_checks: Vec<ContainmentDecision>,
    /// The owner selected when the unconditional base was materialized.
    pub base_owner: Option<BaseOwnerDecision>,
}

/// The policy evaluations of one path's unconditional evidence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathResolution {
    /// The evaluation with declared defaults: the path's resolved schema.
    pub effective: PolicyEvaluation,
    /// The evaluation without declared defaults, which conditional
    /// ownership consults.
    pub structural: PolicyEvaluation,
    /// Qualification of the path's independent literal-path contract.
    pub independent_contract: IndependentQualification,
}

/// The policy evaluation of one guarded overlay of a path.
///
/// Overlay lowering consumes only the effective evaluation of the overlay's
/// evidence, so only that evaluation is recorded.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayResolution {
    /// Position of the overlay in the path's `conditional_overlays`.
    pub overlay: usize,
    /// The policy evaluation of the overlay's evidence.
    pub evaluation: PolicyEvaluation,
    /// Base effect of the lowered conjunct; `None` when the overlay lowered
    /// to no conjunct.
    pub base_effect: Option<ConditionalBaseEffect>,
}

/// How one run of the resolve policy treated its input channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyEvaluation {
    /// Provider schemas of rendered resource slots.
    pub provider: ChannelDisposition,
    /// The declared `values.yaml` default.
    pub declared_default: ChannelDisposition,
    /// Domains of guards that test the path.
    pub guard_domain: ChannelDisposition,
    /// Type hints from strict consumers.
    pub type_hints: ChannelDisposition,
    /// Branch-scoped type hints.
    pub guarded_type_hints: ChannelDisposition,
    /// Type hints from literal `default`/`coalesce` fallbacks.
    pub fallback_type_hints: ChannelDisposition,
    /// How the merge chose its base from the provider and declared schemas.
    pub merge_base: MergeBase,
    /// Policy rules that fired, in execution order.
    pub rules: Vec<PolicyRule>,
}

/// What the resolve policy did with one input channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum ChannelDisposition {
    /// The channel carried no schema.
    Absent,
    /// The channel entered the merge, after the listed adjustments.
    Applied {
        /// Adjustments applied before the merge, in order.
        adjustments: Vec<ChannelAdjustment>,
    },
    /// The channel was removed before the merge.
    Dropped {
        /// Why the channel was removed.
        reason: DropReason,
    },
    /// The channel may only widen an otherwise-typed result.
    WideningOnly {
        /// Whether the widening union was applied.
        applied: bool,
    },
}

impl ChannelDisposition {
    pub(crate) fn input(schema: &serde_json::Value) -> Self {
        if is_empty_schema(schema) {
            Self::Absent
        } else {
            Self::Applied {
                adjustments: Vec::new(),
            }
        }
    }

    pub(crate) fn widening_input(schema: &serde_json::Value) -> Self {
        if is_empty_schema(schema) {
            Self::Absent
        } else {
            Self::WideningOnly { applied: false }
        }
    }

    pub(crate) fn adjust(&mut self, adjustment: ChannelAdjustment) {
        if let Self::Applied { adjustments } = self {
            adjustments.push(adjustment);
        }
    }

    pub(crate) fn discard(&mut self, reason: DropReason) {
        if matches!(self, Self::Applied { .. }) {
            *self = Self::Dropped { reason };
        }
    }

    pub(crate) fn defer_to_widening(&mut self) {
        if matches!(self, Self::Applied { .. }) {
            *self = Self::WideningOnly { applied: false };
        }
    }

    pub(crate) fn settle_widening(&mut self, widened: bool) {
        if let Self::WideningOnly { applied } = self {
            *applied = widened;
        }
    }
}

/// A transformation of one input channel before the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAdjustment {
    /// A provider schema restricted to its scalar lanes for a scalar splice.
    RestrictedToScalar,
    /// A scalar default spliced into a partial string slot, widened to
    /// every scalar.
    WidenedToScalarUnion,
    /// A fragment default opened for passthrough members.
    OpenedFragment,
    /// A ranged fixed-object default generalized to an open map.
    GeneralizedToOpenMap,
}

/// Why the resolve policy removed an input channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    /// A serializing render accepts every input at this path.
    SerializedRender,
    /// Only control flow reads the path and no channel types it.
    ControlOnlyWithoutContract,
    /// An unconditional render's contract applies to every input.
    UnconditionalRenderUse,
    /// A declared `{}` placeholder whose structural object use owns the shape.
    StructuralObjectPlaceholder,
}

/// How the merge chose its base from the provider and declared schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeBase {
    /// Neither a provider nor a declared schema reached the merge.
    NoEvidence,
    /// The provider schema alone.
    Provider,
    /// The declared schema alone.
    DeclaredDefault,
    /// A declared object with referenced descendants over a scalar provider.
    DeclaredObjectOverScalarProvider,
    /// An open string-map provider over a declared object fragment.
    ProviderStringMapOverDeclaredObject,
    /// A declared scalar fragment over a structured provider.
    DeclaredScalarOverStructuredProvider,
    /// The provider schema, which admits the declared scalar's type.
    ProviderAdmitsDeclaredScalar,
    /// The provider schema plus the declared empty-string fallback.
    ProviderWithEmptyStringFallback,
    /// The provider and declared schemas merged.
    ProviderMergedWithDeclared,
    /// A fragment without shape evidence, left unconstrained.
    UnconstrainedFragment,
}

/// A resolve-policy rule that changed the merged schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum PolicyRule {
    /// A partial scalar splice without other typing contributed every scalar.
    PartialScalarDomain,
    /// A provider's plain-string lane was kept beside the merged schema.
    PlainStringPreserved,
    /// The Helm-falsy set was kept open beside the merged schema.
    HelmFalsyEscape {
        /// The first condition that held.
        reason: FalsyEscapeReason,
    },
    /// `null` was admitted beside the merged schema.
    NullAdmitted {
        /// The first condition that held.
        reason: NullAdmissionReason,
    },
    /// An explicit `null` default without other typing left the path open.
    ExplicitNullUnconstrained,
    /// A declared `{}` placeholder with structural object use.
    EmptyMapPlaceholder {
        /// Whether a merge-layered render kept an open-map lane.
        merge_layered_open_map: bool,
    },
    /// An undeclared map the chart iterates was stamped open.
    OpenIterableMap,
    /// An unconstrained serialized slot kept open for descendant rows.
    SerializedDescendantHost,
    /// A direct `range` contributed its runtime iterable domain.
    RuntimeIterableDomain {
        /// Whether the range was the path's only evidence.
        sole_evidence: bool,
    },
}

/// Why the Helm-falsy set stayed open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FalsyEscapeReason {
    /// Every render is guarded by the path's own truthiness.
    SelfGuardedRenders,
    /// Every render tolerates falsy input and the path has no descendants.
    FalsyTolerantRenders,
    /// A literal fallback is the path's only typing.
    FallbackHintOnly,
}

/// Why `null` was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NullAdmissionReason {
    /// The declared default is an explicit `null` the contract accepts.
    ExplicitNullDefault,
    /// A nullable scalar without a strict raw consumer.
    NullableScalar,
    /// A structure whose every render is self-guarded.
    SelfGuardedStructure,
    /// A render-time default merge refills the path.
    RuntimeDefault,
}

/// Qualification of a path's independent literal-path contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum IndependentQualification {
    /// The path has a wildcard segment.
    WildcardPath,
    /// Neither a strict string consumer nor a provider slot reads the path.
    NoIndependentConsumer,
    /// The independent channels resolved to an unconstrained schema.
    RejectedEmpty {
        /// The independent consumer channels present.
        channels: IndependentChannels,
        /// The policy evaluation of those channels.
        evaluation: PolicyEvaluation,
    },
    /// The independent channels resolved to a contract.
    Qualified {
        /// The independent consumer channels present.
        channels: IndependentChannels,
        /// The policy evaluation of those channels.
        evaluation: PolicyEvaluation,
    },
}

/// Independent consumer channels of a literal path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IndependentChannels {
    /// A strict raw-string consumer outside the path's own guards.
    pub strict_string: bool,
    /// A provider slot or metadata role.
    pub provider: bool,
}

/// One containment check of a guarded requirement against the path's
/// structural domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainmentDecision {
    /// The requirement implication checked.
    pub implication: ImplicationRef,
    /// The check's operands and result, or why it was not evaluated.
    pub check: ContainmentCheck,
    /// The base effect the lowered requirement received.
    pub base_effect: ConditionalBaseEffect,
}

/// A requirement implication of one path, by origin and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ImplicationRef {
    /// `requirement_implication` for the path's own implications,
    /// `backprojection` for implications synthesized from rendered sinks.
    pub origin: EmissionOrigin,
    /// Position within the implications of that origin.
    pub index: usize,
}

/// Whether the structural domain contains a requirement's runtime types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum ContainmentCheck {
    /// The containment test ran.
    Evaluated {
        /// Runtime types of the structural schema.
        structural_types: BTreeSet<JsonSchemaType>,
        /// Runtime types the requirement admits.
        requirement_types: BTreeSet<JsonSchemaType>,
        /// Whether the structural types contain the requirement types.
        contains: bool,
    },
    /// An earlier condition decided the base effect.
    NotEvaluated {
        /// The condition that decided.
        short_circuit_reason: ContainmentShortCircuit,
    },
}

/// The condition that decided a base effect before containment ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentShortCircuit {
    /// An incomplete member-host requirement keeps the base.
    IncompleteMemberHost,
    /// An unguarded requirement keeps the base.
    UnguardedRequirement,
    /// A guard on the path's own truthiness owns the base.
    SelfTruthyGuard,
    /// A type requirement scoped by the path's own presence owns the base.
    SelfPresenceTypeArm,
    /// The structural schema is unconstrained.
    EmptyStructuralDomain,
}

/// The owner selected when a path's unconditional base was materialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BaseOwnerDecision {
    /// The selected owner.
    pub owner: BaseOwner,
    /// The ancestor or target rule that selected it.
    pub rule: BaseOwnerRule,
    /// What became of the path's qualified independent contract.
    pub qualified_contract: QualifiedContract,
}

/// The ancestor or target rule that selected a base owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseOwnerRule {
    /// A wildcard member of a guarded-only collection.
    GuardedCollectionMember,
    /// A strict ancestor owns the subtree.
    OwningAncestor,
    /// A pathless dependency root with guarded-only descendants.
    PathlessDependencyRoot,
    /// A conditional target beneath a preserving ancestor.
    PreservingAncestorTarget,
    /// A serializing render owns the path.
    SerializedRender,
    /// The path is a conditional target.
    ConditionalTarget,
    /// No ancestor or target rule applies.
    Unconditional,
}

/// What became of a path's qualified independent contract at base
/// materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualifiedContract {
    /// No independent contract qualified.
    NotQualified,
    /// The contract was conjoined as the base.
    Retained,
    /// Another owner materialized the base.
    Discarded,
}
