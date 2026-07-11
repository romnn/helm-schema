use crate::ConditionalGuard;

#[must_use]
pub fn guards_are_complementary(left: &ConditionalGuard, right: &ConditionalGuard) -> bool {
    fn negated_truthy_path(guard: &ConditionalGuard) -> Option<&str> {
        let ConditionalGuard::Not(inner) = guard else {
            return None;
        };
        match inner.as_ref() {
            ConditionalGuard::Truthy { path } => Some(path),
            _ => None,
        }
    }
    match (left, right) {
        (ConditionalGuard::Truthy { path }, negated)
        | (negated, ConditionalGuard::Truthy { path }) => {
            negated_truthy_path(negated) == Some(path)
        }
        _ => false,
    }
}

#[must_use]
pub fn key_is_strict_subset(subset: &[ConditionalGuard], superset: &[ConditionalGuard]) -> bool {
    key_is_strict_subset_by(subset, superset)
}

/// Keys differing in exactly one complementary member resolve to their shared key.
#[must_use]
pub fn resolve_complementary_keys(
    left: &[ConditionalGuard],
    right: &[ConditionalGuard],
) -> Option<Vec<ConditionalGuard>> {
    resolve_complementary_keys_by(left, right, guards_are_complementary)
}

fn key_is_strict_subset_by<T: PartialEq>(subset: &[T], superset: &[T]) -> bool {
    subset.len() < superset.len() && subset.iter().all(|item| superset.contains(item))
}

fn resolve_complementary_keys_by<T: Clone + PartialEq>(
    left: &[T],
    right: &[T],
    are_complementary: impl Fn(&T, &T) -> bool,
) -> Option<Vec<T>> {
    if left.len() != right.len() {
        return None;
    }
    let left_only: Vec<&T> = left.iter().filter(|guard| !right.contains(guard)).collect();
    let right_only: Vec<&T> = right.iter().filter(|guard| !left.contains(guard)).collect();
    let ([left_extra], [right_extra]) = (left_only.as_slice(), right_only.as_slice()) else {
        return None;
    };
    if !are_complementary(left_extra, right_extra) {
        return None;
    }
    Some(
        left.iter()
            .filter(|guard| *guard != *left_extra)
            .cloned()
            .collect(),
    )
}

/// Minimize a disjunction of conjunctive guard keys by exact resolution,
/// absorption, and deduplication.
#[must_use]
pub fn minimize_key_disjunction(keys: Vec<Vec<ConditionalGuard>>) -> Vec<Vec<ConditionalGuard>> {
    minimize_disjunction_by(keys, guards_are_complementary)
}

pub(crate) fn minimize_disjunction_by<T: Clone + Ord>(
    mut keys: Vec<Vec<T>>,
    are_complementary: impl Copy + Fn(&T, &T) -> bool,
) -> Vec<Vec<T>> {
    keys.sort();
    keys.dedup();
    loop {
        let mut resolved = None;
        'search: for (index, left) in keys.iter().enumerate() {
            for (other_index, right) in keys.iter().enumerate().skip(index + 1) {
                if let Some(common) = resolve_complementary_keys_by(left, right, are_complementary)
                {
                    resolved = Some((index, other_index, common));
                    break 'search;
                }
            }
        }
        let Some((index, other_index, common)) = resolved else {
            break;
        };
        keys.remove(other_index);
        keys.remove(index);
        if !keys.contains(&common) {
            keys.push(common);
        }
        keys.sort();
    }
    let sets = keys.clone();
    keys.retain(|candidate| {
        !sets
            .iter()
            .any(|other| other != candidate && key_is_strict_subset_by(other, candidate))
    });
    keys
}
