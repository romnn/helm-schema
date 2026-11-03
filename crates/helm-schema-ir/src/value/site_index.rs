use std::collections::BTreeMap;

use super::{ExecutedSite, ExecutedText, Exit, ObservationId};

pub(crate) enum Observation<'a> {
    Text(&'a ExecutedText),
    Value(&'a ExecutedSite),
}

#[derive(Clone, Copy, Debug)]
enum SiteLocation {
    Text { exit: usize, record: usize },
    Value { exit: usize, record: usize },
}

impl SiteLocation {
    fn resolve(self, exits: &[Exit]) -> Option<Observation<'_>> {
        match self {
            Self::Text { exit, record } => exits
                .get(exit)?
                .state
                .text_sites
                .get(record)
                .map(Observation::Text),
            Self::Value { exit, record } => exits
                .get(exit)?
                .state
                .sites
                .get(record)
                .map(Observation::Value),
        }
    }
}

// Equal occurrence identities retain every guarded record from the sealed histories.
#[derive(Debug, Default)]
pub(super) struct SiteIndex {
    observations: BTreeMap<ObservationId, Vec<SiteLocation>>,
}

impl SiteIndex {
    pub(super) fn new(exits: &[Exit]) -> Self {
        let mut index = Self::default();
        for (exit, execution) in exits.iter().enumerate() {
            for (record, site) in execution.state.text_sites.iter().enumerate() {
                let ExecutedText {
                    program,
                    coverage_exact: _,
                    rendered_unchanged: _,
                    ordinal,
                    bytes: _,
                    span,
                    source_path: _,
                    condition: _,
                } = site;
                index
                    .observations
                    .entry(ObservationId {
                        program: *program,
                        ordinal: *ordinal,
                        span: *span,
                    })
                    .or_default()
                    .push(SiteLocation::Text { exit, record });
            }
            for (record, site) in execution.state.sites.iter().enumerate() {
                let ExecutedSite {
                    program,
                    coverage_exact: _,
                    rendered_unchanged: _,
                    ordinal,
                    span,
                    source_path: _,
                    condition: _,
                    value: _,
                    consumption_binder: _,
                    plain_scalar_placement: _,
                    rendered: _,
                } = site;
                index
                    .observations
                    .entry(ObservationId {
                        program: *program,
                        ordinal: *ordinal,
                        span: *span,
                    })
                    .or_default()
                    .push(SiteLocation::Value { exit, record });
            }
        }
        index
    }

    pub(super) fn observations<'a>(
        &self,
        exits: &'a [Exit],
        id: ObservationId,
    ) -> impl Iterator<Item = Observation<'a>> {
        self.observations
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(move |location| location.resolve(exits))
    }
}

#[cfg(test)]
#[path = "../tests/runtime_site_index.rs"]
mod tests;
