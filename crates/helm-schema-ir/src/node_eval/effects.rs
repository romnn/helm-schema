use crate::Guard;
use helm_schema_core::Predicate;

pub(crate) trait NodeActionEffectSink {
    fn push_predicate_if_absent(&mut self, predicate: Predicate);
}

/// Push each contract guard of `predicate`; when the predicate decodes to no
/// contract guards, push the raw predicate so the branch fact is not lost.
/// Returns the decoded guards for callers that also observe guard-path uses.
pub(crate) fn push_predicate_contract_guards(
    sink: &mut impl NodeActionEffectSink,
    predicate: &Predicate,
) -> Vec<Guard> {
    let guards = predicate.contract_guards();
    for guard in &guards {
        sink.push_predicate_if_absent(Predicate::from(guard.clone()));
    }
    if guards.is_empty() {
        sink.push_predicate_if_absent(predicate.clone());
    }
    guards
}
