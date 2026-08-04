use std::collections::BTreeMap;

use crate::emission_policy::{EmissionClass, EmissionClassKind, EmissionOrigin};

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
    /// Nested requirement targets skipped while lowering fail implications.
    pub requirement_target: usize,
}

/// Fact and carrier accounting produced alongside a generated schema.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EmissionReport {
    /// Accounting for the selector that produced the current document.
    pub facts: FactCounts,
    facts_by_class_and_origin: BTreeMap<(EmissionClassKind, EmissionOrigin), FactCounts>,
    /// Outcomes for mandatory facts selected by the operative selector.
    pub mandatory_outcomes: MandatoryOutcomes,
    /// Completed-document carrier accounting.
    pub carriers: CarrierCounts,
    /// Canonical-emission accounting.
    pub canonicalization: CanonicalizationCounts,
    /// Ambiguous-union insertions that deliberately retained their original schema.
    pub insertion_abstentions: InsertionAbstentionCounts,
}

#[derive(Clone, Copy)]
pub(crate) struct FactRecord<'a> {
    pub(crate) class: &'a EmissionClass,
    pub(crate) origin: EmissionOrigin,
    pub(crate) selected: bool,
}

impl EmissionReport {
    pub(crate) fn record_fact(&mut self, fact: FactRecord<'_>) {
        Self::record_counts(
            &mut self.facts,
            &mut self.facts_by_class_and_origin,
            fact.class.kind(),
            fact.origin,
            fact.selected,
        );
    }

    fn record_counts(
        totals: &mut FactCounts,
        by_class_and_origin: &mut BTreeMap<(EmissionClassKind, EmissionOrigin), FactCounts>,
        class: EmissionClassKind,
        origin: EmissionOrigin,
        selected: bool,
    ) {
        totals.lowered += 1;
        let counts = by_class_and_origin.entry((class, origin)).or_default();
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
        Self::counts_for(&self.facts_by_class_and_origin, class)
    }

    /// Returns operative-selector accounting for one class and producer pair.
    #[must_use]
    pub fn counts_for_class_and_origin(
        &self,
        class: EmissionClassKind,
        origin: EmissionOrigin,
    ) -> FactCounts {
        self.facts_by_class_and_origin
            .get(&(class, origin))
            .copied()
            .unwrap_or_default()
    }

    fn counts_for(
        counts: &BTreeMap<(EmissionClassKind, EmissionOrigin), FactCounts>,
        class: EmissionClassKind,
    ) -> FactCounts {
        counts
            .iter()
            .filter(|((candidate, _), _)| *candidate == class)
            .fold(FactCounts::default(), |mut total, (_, counts)| {
                total.lowered += counts.lowered;
                total.selected += counts.selected;
                total.dropped += counts.dropped;
                total
            })
    }
}
