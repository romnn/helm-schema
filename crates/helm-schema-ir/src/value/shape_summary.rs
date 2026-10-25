use super::{CertifiedSourcePlan, ParsedMapListMergePlan, SourceSummaryPlan};

pub(super) fn certified_plan(_source: &str) -> Option<CertifiedSourcePlan> {
    None
}

impl CertifiedSourcePlan {
    pub(super) fn applicable(&self, operands: usize) -> bool {
        match &self.plan {
            SourceSummaryPlan::NilScrub(plan) => plan.operand < operands && self.proof.complete,
            SourceSummaryPlan::ParsedMapListMerge(ParsedMapListMergePlan {
                operands: roles,
                renderer,
                overwrite: _,
            }) => {
                roles.iter().all(|index| *index < operands)
                    && renderer.proof.complete
                    && self.proof.complete
            }
        }
    }
}

pub(super) fn executed_wrapper_metadata(
    _source: &str,
    _program: u64,
) -> Option<super::ProgramWrapperMetadata> {
    None
}
