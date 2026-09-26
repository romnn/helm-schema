use std::collections::BTreeMap;

use helm_schema_core::ValuesPath;

use crate::emission_policy::{EmissionClass, EmissionClassKind};

/// The `helm lint` document a withdrawn constraint fails.
///
/// `helm lint` validates the root `values.yaml` coalesced with the user's
/// override files, never with dependency defaults (Helm v4.2.3
/// `pkg/chart/v2/lint/rules/values.go:62-68`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintDocument {
    /// The root `values.yaml` as written, linted without overrides; the
    /// whole constraint failed it.
    Root,
    /// The coalesced defaults restricted to the members the root declares,
    /// with the root's nulls kept; the constraint failed it once every
    /// guard an override can switch on was taken as switched on.
    Floor,
}

/// How a constraint a `helm lint` document fails was relaxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintOutcome {
    /// It applies only where every deciding path holds a non-null value;
    /// a pure presence requirement on those paths is lost.
    Conditioned,
    /// It was dropped: no deciding path could be encoded as a presence test.
    Withdrawn,
}

/// A conditional constraint relaxed because a `helm lint` document fails it
/// while the coalesced defaults satisfy it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintWithdrawal {
    /// Values path the constraint is anchored at.
    pub anchor: ValuesPath,
    /// First lint document that fails the constraint.
    pub document: LintDocument,
    /// Guard paths and constrained members of the constraint whose presence
    /// differs between a failing lint document and the coalesced defaults:
    /// only a dependency supplies them, or the root declares them null.
    pub deciding_paths: Vec<ValuesPath>,
    /// How the constraint was relaxed.
    pub outcome: LintOutcome,
}

/// Fact totals at one emission-selection boundary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FactCounts {
    /// Facts produced by lowering.
    pub lowered: usize,
    /// Facts retained by the selector.
    pub selected: usize,
    /// Facts removed by the selector.
    pub dropped: usize,
}

/// How selected mandatory facts reached the generated document.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MandatoryOutcomes {
    /// Facts emitted as distinct constraints.
    pub emitted: usize,
    /// Facts folded into validation-equivalent base structure.
    pub equivalent: usize,
    /// Facts already implied by emitted structure.
    pub redundant: usize,
    /// Facts preserved through the fallback emitter.
    pub fallback: usize,
}

impl MandatoryOutcomes {
    /// Returns the total number of accounted mandatory facts.
    #[must_use]
    pub const fn total(self) -> usize {
        self.emitted + self.equivalent + self.redundant + self.fallback
    }
}

/// Counts of conditional carriers in the completed generated schema.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CarrierCounts {
    /// Conditional carriers anchored at the document root.
    pub root: usize,
    /// Conditional carriers anchored below the document root.
    pub local: usize,
    /// JSON Schema `if` nodes in the completed document.
    pub condition_nodes: usize,
    /// Largest number of lowered facts grouped into one emitted carrier.
    pub grouping_fan_in: usize,
}

/// Outcomes reserved for canonical mandatory emission.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalizationCounts {
    /// Facts handled by canonical emission.
    pub applied: usize,
    /// Facts already represented by canonical structure.
    pub redundant: usize,
    /// Facts handled by the general fallback.
    pub fallback: usize,
    /// Default backfills skipped because object-union arms cannot expose an equivalent descendant.
    pub default_backfill_abstentions: usize,
}

/// Ambiguous-union insertion abstentions grouped by the phase that requested them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InsertionAbstentionCounts {
    /// Base path insertions skipped while materializing a projected document.
    pub base_document: usize,
    /// Member-descendant projections skipped while lowering conditional overlays.
    pub conditional_member_projection: usize,
    /// Nested requirement targets skipped while lowering requirement implications.
    pub requirement_target: usize,
}

/// Fact and carrier accounting produced alongside a generated schema.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EmissionReport {
    /// Accounting for the selector that produced the current document.
    pub facts: FactCounts,
    facts_by_class: BTreeMap<EmissionClassKind, FactCounts>,
    /// Outcomes for mandatory facts selected by the operative selector.
    pub mandatory_outcomes: MandatoryOutcomes,
    /// Completed-document carrier accounting.
    pub carriers: CarrierCounts,
    /// Canonical-emission accounting.
    pub canonicalization: CanonicalizationCounts,
    /// Ambiguous-union insertions that deliberately retained their original schema.
    pub insertion_abstentions: InsertionAbstentionCounts,
    /// Conditional constraints relaxed because a document `helm lint`
    /// validates fails them while the coalesced defaults satisfy them, in
    /// relaxation order.
    pub lint_withdrawals: Vec<LintWithdrawal>,
}

#[derive(Clone, Copy)]
pub(crate) struct FactRecord<'a> {
    pub(crate) class: &'a EmissionClass,
    pub(crate) selected: bool,
}

impl EmissionReport {
    pub(crate) fn record_fact(&mut self, fact: FactRecord<'_>) {
        Self::record_counts(
            &mut self.facts,
            &mut self.facts_by_class,
            fact.class.kind(),
            fact.selected,
        );
    }

    fn record_counts(
        totals: &mut FactCounts,
        by_class: &mut BTreeMap<EmissionClassKind, FactCounts>,
        class: EmissionClassKind,
        selected: bool,
    ) {
        totals.lowered += 1;
        let counts = by_class.entry(class).or_default();
        counts.lowered += 1;
        if selected {
            totals.selected += 1;
            counts.selected += 1;
        } else {
            totals.dropped += 1;
            counts.dropped += 1;
        }
    }

    /// Returns operative-selector accounting for one policy class.
    #[must_use]
    pub fn counts_for_class(&self, class: EmissionClassKind) -> FactCounts {
        self.facts_by_class.get(&class).copied().unwrap_or_default()
    }
}
