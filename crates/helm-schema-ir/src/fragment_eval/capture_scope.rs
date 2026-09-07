use helm_schema_core::{
    Conjunction, Guard, GuardValue, Predicate, PredicateKind, Segment, ValuesPath,
};

use crate::eval_effect::{CaptureKind, FailCapture};
use crate::range_modes::RangeModes;

/// Adds caller scope without repeating facts proved by a concrete selected value.
/// A concrete nonempty string witnesses its mapping ancestors and their matching
/// iterations, but cannot discharge an unbound member-dependent body or caller test.
pub(super) fn scope_capture_conditions(
    body: &FailCapture,
    mut ambient: Conjunction,
    mut ranged: RangeModes,
) -> (Conjunction, RangeModes) {
    for predicate in &body.conjunction {
        let PredicateKind::Guard(Guard::Eq {
            path: selected,
            value: GuardValue::String(value),
        }) = predicate.kind()
        else {
            continue;
        };
        if value.is_empty()
            || selected.segments().len() == 0
            || selected.segments().any(Segment::is_each_member)
            || body_uses_iteration(body, selected)
        {
            continue;
        }
        let blocked = ambient.iter().any(|predicate| {
            let related = predicate
                .value_paths()
                .iter()
                .any(|path| prefix_matches(path, selected) || uses_iteration(path, selected));
            related && known_truth(predicate, selected, value, &ranged) != Some(true)
        });
        if blocked {
            continue;
        }
        ambient.retain(|predicate| known_truth(predicate, selected, value, &ranged) != Some(true));
        let redundant_ranges = ranged
            .iter()
            .filter_map(|(path, mode)| {
                (path.segments().len() < selected.segments().len()
                    && prefix_matches(path, selected)
                    && mode.input_identity
                    && !mode.json_decoded)
                    .then(|| path.clone())
            })
            .collect::<Vec<_>>();
        for path in redundant_ranges {
            ranged.remove(&path);
        }
    }
    ambient.extend(body.conjunction.iter().cloned());
    ranged.merge(&body.ranged);
    (ambient, ranged)
}

fn body_uses_iteration(body: &FailCapture, selected: &ValuesPath) -> bool {
    if body
        .conjunction
        .iter()
        .any(|predicate| capture_predicate_uses_iteration(predicate, selected))
    {
        return true;
    }
    match &body.kind {
        CaptureKind::Fail | CaptureKind::MemberAccess { .. } => false,
        CaptureKind::StringRequirement {
            path, selection, ..
        } => {
            uses_iteration(path, selected)
                || selection
                    .iter()
                    .any(|predicate| capture_predicate_uses_iteration(predicate, selected))
        }
        CaptureKind::RangeSelection { path, chain, .. } => {
            uses_iteration(path, selected)
                || chain.iter().any(|path| uses_iteration(path, selected))
        }
        CaptureKind::RangeKeyStrings { paths } | CaptureKind::RangeKeyPlainSlot { paths } => paths
            .iter()
            .any(|path| prefix_matches(path, selected) || uses_iteration(path, selected)),
        CaptureKind::CollectionItems { paths, .. }
        | CaptureKind::SplitIndexAccess { paths, .. } => {
            paths.iter().any(|path| uses_iteration(path, selected))
        }
        CaptureKind::IndexAccess { path, .. }
        | CaptureKind::ValueType { path, .. }
        | CaptureKind::RangeInput { path, .. }
        | CaptureKind::DigSubject { path }
        | CaptureKind::RequiredPresence { path }
        | CaptureKind::AbsenceAborts { path }
        | CaptureKind::ComparableKind { path, .. }
        | CaptureKind::ValuePattern { path, .. }
        | CaptureKind::QuotedSerialization { path, .. }
        | CaptureKind::PrintfStringOperand { path }
        | CaptureKind::PlainSlotText { path, .. } => uses_iteration(path, selected),
    }
}

fn capture_predicate_uses_iteration(predicate: &Predicate, selected: &ValuesPath) -> bool {
    if matches!(predicate.kind(), PredicateKind::Guard(Guard::Eq { path, .. }) if path == selected)
    {
        return false;
    }
    predicate
        .value_paths()
        .iter()
        .any(|path| uses_iteration(path, selected) || prefix_matches(path, selected))
}

fn prefix_matches(candidate: &ValuesPath, selected: &ValuesPath) -> bool {
    candidate.segments().len() <= selected.segments().len()
        && candidate
            .segments()
            .zip(selected.segments())
            .all(|(candidate, selected)| candidate.is_each_member() || candidate == selected)
}

fn uses_iteration(path: &ValuesPath, selected: &ValuesPath) -> bool {
    let Some(wildcard) = path.segments().position(Segment::is_each_member) else {
        return false;
    };
    wildcard < selected.segments().len()
        && path
            .segments()
            .take(wildcard)
            .zip(selected.segments())
            .all(|(candidate, selected)| candidate == selected)
}

fn known_truth(
    predicate: &Predicate,
    selected: &ValuesPath,
    value: &str,
    ranged: &RangeModes,
) -> Option<bool> {
    match predicate.kind() {
        PredicateKind::True => Some(true),
        PredicateKind::False => Some(false),
        PredicateKind::Approximate { .. } => None,
        PredicateKind::Not(inner) => {
            known_truth(inner, selected, value, ranged).map(|truth| !truth)
        }
        PredicateKind::And(items) => items
            .iter()
            .map(|item| known_truth(item, selected, value, ranged))
            .collect::<Option<Vec<_>>>()
            .map(|truths| truths.into_iter().all(|truth| truth)),
        PredicateKind::Or(items) => items
            .iter()
            .map(|item| known_truth(item, selected, value, ranged))
            .collect::<Option<Vec<_>>>()
            .map(|truths| truths.into_iter().any(|truth| truth)),
        PredicateKind::Guard(guard) => known_guard_truth(guard, selected, value, ranged),
    }
}

fn known_guard_truth(
    guard: &Guard,
    selected: &ValuesPath,
    value: &str,
    ranged: &RangeModes,
) -> Option<bool> {
    let path = match guard {
        Guard::Truthy { path }
        | Guard::With { path }
        | Guard::Not { path }
        | Guard::Absent { path }
        | Guard::Range { path }
        | Guard::TypeIs { path, .. }
        | Guard::NotTypeIs { path, .. }
        | Guard::Eq { path, .. }
        | Guard::NotEq { path, .. } => path,
        Guard::MatchesPattern { .. }
        | Guard::NotMatchesPattern { .. }
        | Guard::RangeKeyPrefix { .. }
        | Guard::RangeKeyEquals { .. }
        | Guard::RangeKeyMatches { .. }
        | Guard::Or { .. }
        | Guard::AnyOf { .. }
        | Guard::Default { .. }
        | Guard::IntGt { .. }
        | Guard::IntLt { .. }
        | Guard::AtMostOneMember { .. }
        | Guard::MinMembers { .. }
        | Guard::HasKey { .. }
        | Guard::NotHasKey { .. }
        | Guard::ContainsEquals { .. }
        | Guard::ContainsMemberEquals { .. }
        | Guard::ContainsTruthyMember { .. } => return None,
    };
    if !prefix_matches(path, selected) {
        return None;
    }
    let ancestor = path.segments().len() < selected.segments().len();
    match guard {
        Guard::Truthy { .. } | Guard::With { .. } => Some(true),
        Guard::Not { .. } | Guard::Absent { .. } => Some(false),
        Guard::Range { .. } => {
            let mode = ranged.mode(path);
            (ancestor && mode.input_identity && !mode.json_decoded).then_some(true)
        }
        Guard::TypeIs { schema_type, .. } => {
            Some(schema_type == if ancestor { "object" } else { "string" })
        }
        Guard::NotTypeIs { schema_type, .. } => {
            Some(schema_type != if ancestor { "object" } else { "string" })
        }
        Guard::Eq {
            value: GuardValue::Null,
            ..
        } => Some(false),
        Guard::NotEq {
            value: GuardValue::Null,
            ..
        } => Some(true),
        Guard::Eq {
            value: GuardValue::String(expected),
            ..
        } if !ancestor => Some(expected == value),
        Guard::NotEq {
            value: GuardValue::String(expected),
            ..
        } if !ancestor => Some(expected != value),
        Guard::Eq { .. }
        | Guard::NotEq { .. }
        | Guard::MatchesPattern { .. }
        | Guard::NotMatchesPattern { .. }
        | Guard::RangeKeyPrefix { .. }
        | Guard::RangeKeyEquals { .. }
        | Guard::RangeKeyMatches { .. }
        | Guard::Or { .. }
        | Guard::AnyOf { .. }
        | Guard::Default { .. }
        | Guard::IntGt { .. }
        | Guard::IntLt { .. }
        | Guard::AtMostOneMember { .. }
        | Guard::MinMembers { .. }
        | Guard::HasKey { .. }
        | Guard::NotHasKey { .. }
        | Guard::ContainsEquals { .. }
        | Guard::ContainsMemberEquals { .. }
        | Guard::ContainsTruthyMember { .. } => None,
    }
}

#[cfg(test)]
#[path = "../tests/capture_scope.rs"]
mod tests;
