use std::cmp::Ordering;
use std::hash::Hash;

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

fn key_is_strict_subset_by<T: Ord>(subset: &[T], superset: &[T]) -> bool {
    subset.len() < superset.len()
        && subset
            .iter()
            .all(|item| superset.binary_search(item).is_ok())
}

fn resolve_complementary_keys_by<T: Clone + Ord>(
    left: &[T],
    right: &[T],
    are_complementary: impl Fn(&T, &T) -> bool,
) -> Option<Vec<T>> {
    if left.len() != right.len() {
        return None;
    }

    let mut left_extra = None;
    let mut right_extra = None;
    let mut left_items = left.iter().peekable();
    let mut right_items = right.iter().peekable();
    while let (Some(left), Some(right)) = (left_items.peek(), right_items.peek()) {
        match left.cmp(right) {
            Ordering::Equal => {
                left_items.next();
                right_items.next();
            }
            Ordering::Less => {
                if left_extra.replace(left_items.next()?).is_some() {
                    return None;
                }
            }
            Ordering::Greater => {
                if right_extra.replace(right_items.next()?).is_some() {
                    return None;
                }
            }
        }
    }
    for item in left_items {
        if left_extra.replace(item).is_some() {
            return None;
        }
    }
    for item in right_items {
        if right_extra.replace(item).is_some() {
            return None;
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

#[tracing::instrument(skip_all, level = "debug", fields(alternatives = keys.len(), ranks = tracing::field::Empty, masked = tracing::field::Empty))]
pub(crate) fn minimize_disjunction_by<T: Clone + Ord + Hash>(
    keys: Vec<Vec<T>>,
    are_complementary: impl Copy + Fn(&T, &T) -> bool,
) -> Vec<Vec<T>> {
    debug_assert!(keys.iter().all(|key| {
        key.windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right))
    }));
    if keys.len() <= 1 {
        return keys;
    }
    let mut distinct = keys
        .iter()
        .flatten()
        .collect::<ahash::AHashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    distinct.sort();
    let ranks = distinct
        .into_iter()
        .enumerate()
        .map(|(rank, value)| (value, rank))
        .collect::<ahash::AHashMap<_, _>>();
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::Span::current().record("ranks", ranks.len());
    }
    let ranked = keys
        .iter()
        .map(|key| {
            key.iter()
                .map(|value| {
                    Some(RankedLiteral {
                        rank: ranks.get(value).copied()?,
                        value,
                    })
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>();
    let Some(ranked) = ranked else {
        return minimize_ranked_disjunction(keys, are_complementary, |_| None);
    };
    minimize_ranked_disjunction(
        ranked,
        |left, right| are_complementary(left.value, right.value),
        rank_mask,
    )
    .into_iter()
    .map(|key| {
        key.into_iter()
            .map(|RankedLiteral { rank: _, value }| T::clone(value))
            .collect()
    })
    .collect()
}

// Ranks encode full literal order; values keep each original row's predicate nodes.
#[derive(Clone)]
struct RankedLiteral<T> {
    rank: usize,
    value: T,
}

impl<T> PartialEq for RankedLiteral<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rank == other.rank
    }
}

impl<T> Eq for RankedLiteral<T> {}

impl<T> PartialOrd for RankedLiteral<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for RankedLiteral<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}

fn rank_mask<T>(key: &[RankedLiteral<T>]) -> Option<u128> {
    let mut mask = 0_u128;
    for RankedLiteral { rank, value: _ } in key {
        let shift = u32::try_from(*rank).ok()?;
        mask |= 1_u128.checked_shl(shift)?;
    }
    Some(mask)
}

struct Resolution<T> {
    left_index: usize,
    right_index: usize,
    common: Vec<T>,
}

struct Disjunct<T> {
    literals: Vec<T>,
    mask: Option<u128>,
    retained: bool,
}

impl<T> Disjunct<T> {
    fn new(literals: Vec<T>, mask: impl Fn(&[T]) -> Option<u128>) -> Self {
        Self {
            mask: mask(&literals),
            literals,
            retained: true,
        }
    }
}

fn minimize_ranked_disjunction<T: Clone + Ord>(
    keys: Vec<Vec<T>>,
    are_complementary: impl Copy + Fn(&T, &T) -> bool,
    mask: impl Copy + Fn(&[T]) -> Option<u128>,
) -> Vec<Vec<T>> {
    debug_assert!(keys.iter().all(|key| {
        key.windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right))
    }));
    let mut keys = keys
        .into_iter()
        .map(|key| Disjunct::new(key, mask))
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| left.literals.cmp(&right.literals));
    keys.dedup_by(|left, right| left.literals == right.literals);
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::Span::current().record(
            "masked",
            keys.iter().filter(|row| row.mask.is_some()).count(),
        );
    }
    loop {
        let mut resolved = None;
        'search: for (index, left) in keys.iter().enumerate() {
            for (other_index, right) in keys.iter().enumerate().skip(index + 1) {
                // A complete rank mask can reject pairs before the original resolver runs.
                if left.literals.len() != right.literals.len()
                    || matches!((left.mask, right.mask), (Some(left), Some(right))
                        if (left ^ right).count_ones() != 2)
                {
                    continue;
                }
                if let Some(common) = resolve_complementary_keys_by(
                    &left.literals,
                    &right.literals,
                    are_complementary,
                ) {
                    resolved = Some(Resolution {
                        left_index: index,
                        right_index: other_index,
                        common,
                    });
                    break 'search;
                }
            }
        }
        let Some(Resolution {
            left_index,
            right_index,
            common,
        }) = resolved
        else {
            break;
        };
        keys.remove(right_index);
        keys.remove(left_index);
        if let Err(insert_at) = keys.binary_search_by(|row| row.literals.cmp(&common)) {
            keys.insert(insert_at, Disjunct::new(common, mask));
        }
    }

    for index in 0..keys.len() {
        let Some(candidate) = keys.get(index) else {
            continue;
        };
        let retained = !keys.iter().enumerate().any(|(other_index, other)| {
            index != other_index
                && match (other.mask, candidate.mask) {
                    (Some(subset), Some(superset)) => {
                        other.literals.len() < candidate.literals.len()
                            && subset & superset == subset
                    }
                    _ => key_is_strict_subset_by(&other.literals, &candidate.literals),
                }
        });
        if let Some(row) = keys.get_mut(index) {
            row.retained = retained;
        }
    }
    keys.into_iter()
        .filter_map(
            |Disjunct {
                 literals,
                 mask: _,
                 retained,
             }| { retained.then_some(literals) },
        )
        .collect()
}
