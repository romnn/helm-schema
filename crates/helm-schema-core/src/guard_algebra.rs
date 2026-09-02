use crate::{ConditionalGuard, ValuesPath};

/// Reports whether two conditional guards are exact logical complements.
#[must_use]
pub(crate) fn guards_are_complementary(left: &ConditionalGuard, right: &ConditionalGuard) -> bool {
    fn negated_truthy_path(guard: &ConditionalGuard) -> Option<&ValuesPath> {
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

    let mut left_extra = None;
    let mut right_extra = None;
    for item in left {
        if !right.contains(item) {
            if left_extra.is_some() {
                return None;
            }
            left_extra = Some(item);
        }
    }
    for item in right {
        if !left.contains(item) {
            if right_extra.is_some() {
                return None;
            }
            right_extra = Some(item);
        }
    }

    let (Some(left_extra), Some(right_extra)) = (left_extra, right_extra) else {
        return None;
    };
    if !are_complementary(left_extra, right_extra) {
        return None;
    }
    Some(
        left.iter()
            .filter(|item| *item != left_extra)
            .cloned()
            .collect(),
    )
}

pub(crate) fn minimize_disjunction_by<T: Clone + Ord>(
    mut keys: Vec<Vec<T>>,
    are_complementary: impl Copy + Fn(&T, &T) -> bool,
) -> Vec<Vec<T>> {
    debug_assert!(keys.iter().all(|key| {
        key.windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right))
    }));
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
            let insert_at = keys.partition_point(|key| key < &common);
            keys.insert(insert_at, common);
        }
    }

    let retained = keys
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            !keys.iter().enumerate().any(|(other_index, other)| {
                candidate_index != other_index && key_is_strict_subset_by(other, candidate)
            })
        })
        .collect::<Vec<_>>();
    let mut retained = retained.into_iter();
    keys.retain(|_| retained.next().unwrap_or_default());
    keys
}
