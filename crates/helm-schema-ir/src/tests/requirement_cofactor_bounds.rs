use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;
use tracing_subscriber::prelude::*;

use super::{
    BTreeMap, ExistingCoverage, FrozenConsumption, Projection, RawUnion,
    RequirementConditionBuilder, RequirementGuardInterner, RequirementIdentities,
};

#[derive(Clone, Default)]
struct RefinementBounds(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RefinementBounds {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() == "requirement_refinement_bound" {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[test]
fn shannon_refinement_counts_the_part_and_depth_cutoffs() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let occurrence = source
        .first()
        .ok_or_eyre("base occurrence")?
        .test_literal_true()
        .test_under(Predicate::truthy_path("flag"));
    let counts = RefinementBounds::default();
    let subscriber = tracing_subscriber::registry().with(counts.clone());
    tracing::subscriber::with_default(subscriber, || {
        for (parts, depth) in [(1, 1), (2, 0)] {
            let covers = ExistingCoverage::default();
            let mut raw = RawUnion::new(
                RequirementIdentities::default(),
                RequirementGuardInterner::default(),
            );
            let root = raw.occurrence(&occurrence).ok_or_eyre("raw occurrence")?;
            assert!(raw.node(root).first_variable.is_some());
            let mut projection = Projection {
                raw,
                graph: RequirementConditionBuilder::default(),
                covers: &covers,
                presence: None,
                work: 1,
                memo: BTreeMap::new(),
                fallback: BTreeMap::new(),
                alone: BTreeMap::new(),
            };
            let _ = projection.refine(root, parts, depth, &[0]);
            sim_assert_eq!(have: counts.0.swap(0, Ordering::Relaxed), want: 1);
        }
        Ok(())
    })
}
