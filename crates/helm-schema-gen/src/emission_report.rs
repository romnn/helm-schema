use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;
use sha2::{Digest, Sha256};

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
}

/// Direction of a disagreement between the legacy lean gate and the policy projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionDifferenceDirection {
    /// The decision-table projection retains a fact that legacy lean drops.
    ProjectionOnly,
    /// Legacy lean retains a fact that the decision-table projection drops.
    LegacyOnly,
}

/// Stable identity and direction for one selection disagreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionDifference {
    /// Stable lowering-order position within this generated artifact.
    pub fact_index: usize,
    /// Policy class assigned during lowering.
    pub class: EmissionClassKind,
    /// Producer category used only for diagnostics.
    pub origin: EmissionOrigin,
    /// Values path carried by the fact, empty for document-level termination.
    pub target_value_path: String,
    /// SHA-256 of the complete policy class, including guards and anchor.
    pub class_sha256: String,
    /// SHA-256 of the fact's schema payload.
    pub schema_sha256: String,
    /// Which selector alone retains the fact.
    pub direction: SelectionDifferenceDirection,
}

/// Fact and carrier accounting produced alongside a generated schema.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EmissionReport {
    /// Accounting for the selector that produced the current document.
    pub facts: FactCounts,
    /// Accounting for the decision-table projection, including shadow mode.
    pub projected_facts: FactCounts,
    facts_by_class_and_origin: BTreeMap<(EmissionClassKind, EmissionOrigin), FactCounts>,
    projected_facts_by_class_and_origin: BTreeMap<(EmissionClassKind, EmissionOrigin), FactCounts>,
    /// Outcomes for mandatory facts selected by the operative selector.
    pub mandatory_outcomes: MandatoryOutcomes,
    /// Completed-document carrier accounting.
    pub carriers: CarrierCounts,
    /// Canonical-emission accounting.
    pub canonicalization: CanonicalizationCounts,
    /// Fact-level disagreements between legacy lean and the projected policy.
    pub selection_differences: Vec<SelectionDifference>,
}

#[derive(Clone, Copy)]
pub(crate) struct FactRecord<'a> {
    pub(crate) fact_index: usize,
    pub(crate) class: &'a EmissionClass,
    pub(crate) origin: EmissionOrigin,
    pub(crate) target_value_path: &'a str,
    pub(crate) schema: &'a Value,
    pub(crate) selected: bool,
    pub(crate) projected_selected: bool,
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
        Self::record_counts(
            &mut self.projected_facts,
            &mut self.projected_facts_by_class_and_origin,
            fact.class.kind(),
            fact.origin,
            fact.projected_selected,
        );
        let direction = match (fact.selected, fact.projected_selected) {
            (false, true) => SelectionDifferenceDirection::ProjectionOnly,
            (true, false) => SelectionDifferenceDirection::LegacyOnly,
            (false, false) | (true, true) => return,
        };
        self.selection_differences.push(SelectionDifference {
            fact_index: fact.fact_index,
            class: fact.class.kind(),
            origin: fact.origin,
            target_value_path: fact.target_value_path.to_string(),
            class_sha256: sha256_text(&format!("{:?}", fact.class)),
            schema_sha256: sha256_text(&fact.schema.to_string()),
            direction,
        });
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

    /// Returns decision-table projection accounting for one policy class.
    #[must_use]
    pub fn projected_counts_for_class(&self, class: EmissionClassKind) -> FactCounts {
        Self::counts_for(&self.projected_facts_by_class_and_origin, class)
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

    /// Returns projected accounting for one class and producer pair.
    #[must_use]
    pub fn projected_counts_for_class_and_origin(
        &self,
        class: EmissionClassKind,
        origin: EmissionOrigin,
    ) -> FactCounts {
        self.projected_facts_by_class_and_origin
            .get(&(class, origin))
            .copied()
            .unwrap_or_default()
    }

    /// Returns a stable SHA-256 for the ordered fact-level selection diff.
    #[must_use]
    pub fn selection_differences_sha256(&self) -> String {
        let mut hasher = Sha256::new();
        for difference in &self.selection_differences {
            hasher.update(
                format!(
                    "{}\t{:?}\t{:?}\t{}\t{}\t{}\t{:?}\n",
                    difference.fact_index,
                    difference.class,
                    difference.origin,
                    difference.target_value_path,
                    difference.class_sha256,
                    difference.schema_sha256,
                    difference.direction,
                )
                .as_bytes(),
            );
        }
        let digest = hasher.finalize();
        let mut output = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(output, "{byte:02x}");
        }
        output
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

fn sha256_text(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
