use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;

use crate::{ConditionalGuard, GuardConjunction, GuardPrefix, ValuesPath};

/// Reports whether two conditional guards are exact logical complements.
#[must_use]
pub(crate) fn guards_are_complementary(left: &ConditionalGuard, right: &ConditionalGuard) -> bool {
    match (left, right) {
        (ConditionalGuard::Truthy { path }, negated)
        | (negated, ConditionalGuard::Truthy { path }) => {
            negated_truthy_path(negated) == Some(path)
        }
        _ => false,
    }
}

fn negated_truthy_path(guard: &ConditionalGuard) -> Option<&ValuesPath> {
    let ConditionalGuard::Not(inner) = guard else {
        return None;
    };
    match inner.as_ref() {
        ConditionalGuard::Truthy { path } => Some(path),
        _ => None,
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

fn truthy_literal(guard: &ConditionalGuard) -> Option<(&ValuesPath, bool)> {
    if let ConditionalGuard::Truthy { path } = guard {
        Some((path, true))
    } else {
        negated_truthy_path(guard).map(|path| (path, false))
    }
}

struct SharedResolution {
    left: usize,
    right: usize,
    common: GuardConjunction,
}

#[expect(
    clippy::mutable_key_type,
    reason = "the prefix cache only memoizes a canonical view of immutable conjunction guards"
)]
fn find_shared_resolution(
    rows: &[GuardConjunction],
    eligible: &BTreeSet<GuardConjunction>,
    resolvable: &BTreeSet<ValuesPath>,
) -> Option<SharedResolution> {
    for row in eligible {
        let Ok(left) = rows.binary_search(row) else {
            continue;
        };
        let mut first = None;
        for guard in row {
            let Some((path, positive)) = truthy_literal(guard) else {
                continue;
            };
            if !resolvable.contains(path) {
                continue;
            }
            let positive_guard = ConditionalGuard::Truthy { path: path.clone() };
            let complement = if positive {
                ConditionalGuard::Not(std::sync::Arc::new(positive_guard))
            } else {
                positive_guard
            };
            let mut candidate = row.without(guard);
            candidate.push(complement);
            candidate.canonicalize();
            if candidate.len() != row.len() {
                continue;
            }
            let Ok(right) = rows.binary_search(&candidate) else {
                continue;
            };
            if right > left
                && first
                    .as_ref()
                    .is_none_or(|old: &SharedResolution| right < old.right)
            {
                first = Some(SharedResolution {
                    left,
                    right,
                    common: row.without(guard),
                });
            }
        }
        if first.is_some() {
            return first;
        }
    }
    None
}

/// Preserves the existing first complementary pair while keeping branch prefixes shared.
#[tracing::instrument(skip_all, fields(alternatives = rows.len(), retained = tracing::field::Empty))]
pub(crate) fn minimize_shared_guard_disjunction(
    mut rows: Vec<GuardConjunction>,
) -> Vec<GuardConjunction> {
    for row in &mut rows {
        row.canonicalize();
    }
    rows.sort();
    rows.dedup();
    let mut positive = BTreeSet::new();
    let mut negative = BTreeSet::new();
    for guard in rows.iter().flat_map(GuardConjunction::iter) {
        if let Some((path, taken)) = truthy_literal(guard) {
            if taken {
                positive.insert(path.clone());
            } else {
                negative.insert(path.clone());
            }
        }
    }
    // Resolution only removes literals, so a missing complement cannot appear later.
    let resolvable = positive
        .intersection(&negative)
        .cloned()
        .collect::<BTreeSet<_>>();
    let eligible_row = |row: &GuardConjunction| {
        row.iter()
            .any(|guard| truthy_literal(guard).is_some_and(|(path, _)| resolvable.contains(path)))
    };
    #[expect(
        clippy::mutable_key_type,
        reason = "the prefix cache only memoizes a canonical view of immutable conjunction guards"
    )]
    let mut eligible = if resolvable.is_empty() {
        BTreeSet::new()
    } else {
        rows.iter()
            .filter(|row| eligible_row(row))
            .cloned()
            .collect()
    };
    while let Some(SharedResolution {
        left,
        right,
        common,
    }) = find_shared_resolution(&rows, &eligible, &resolvable)
    {
        if let Some(row) = rows.get(right) {
            eligible.remove(row);
        }
        rows.remove(right);
        if let Some(row) = rows.get(left) {
            eligible.remove(row);
        }
        rows.remove(left);
        if let Err(index) = rows.binary_search(&common) {
            if eligible_row(&common) {
                eligible.insert(common.clone());
            }
            rows.insert(index, common);
        }
    }
    let retained = shared_absorption(&rows).unwrap_or_else(|| {
        rows.iter()
            .map(|row| {
                !rows
                    .iter()
                    .any(|subset| subset.len() < row.len() && subset.is_subset_of(row))
            })
            .collect()
    });
    let rows = rows
        .into_iter()
        .zip(retained)
        .filter_map(|(row, keep)| keep.then_some(row))
        .collect::<Vec<_>>();
    tracing::Span::current().record("retained", rows.len());
    rows
}

#[derive(Default)]
struct ExtraGuardNode<'a> {
    terminal: bool,
    children: BTreeMap<&'a ConditionalGuard, usize>,
}

struct ExtraGuardIndex<'a> {
    nodes: Vec<ExtraGuardNode<'a>>,
}

impl<'a> ExtraGuardIndex<'a> {
    fn new() -> Self {
        Self {
            nodes: vec![ExtraGuardNode::default()],
        }
    }

    fn insert(&mut self, guards: &'a [ConditionalGuard]) -> Option<()> {
        let mut current = 0;
        for guard in guards {
            let child = self.nodes.get(current)?.children.get(guard).copied();
            current = if let Some(child) = child {
                child
            } else {
                let child = self.nodes.len();
                self.nodes.push(ExtraGuardNode::default());
                self.nodes.get_mut(current)?.children.insert(guard, child);
                child
            };
        }
        self.nodes.get_mut(current)?.terminal = true;
        Some(())
    }

    fn has_strict_subset(&self, guards: &[&ConditionalGuard]) -> Option<bool> {
        struct Search {
            node: usize,
            start: usize,
            chosen: usize,
        }
        let mut pending = vec![Search {
            node: 0,
            start: 0,
            chosen: 0,
        }];
        while let Some(Search {
            node,
            start,
            chosen,
        }) = pending.pop()
        {
            let node = self.nodes.get(node)?;
            if node.terminal && chosen < guards.len() {
                return Some(true);
            }
            if chosen + 1 >= guards.len() {
                continue;
            }
            for (index, guard) in guards.iter().enumerate().skip(start) {
                if let Some(child) = node.children.get(guard) {
                    pending.push(Search {
                        node: *child,
                        start: index + 1,
                        chosen: chosen + 1,
                    });
                }
            }
        }
        Some(false)
    }
}

struct PrefixRows<'a> {
    prefix: std::sync::Arc<Vec<ConditionalGuard>>,
    rows: Vec<usize>,
    extras: ExtraGuardIndex<'a>,
    min_extra: usize,
    max_extra: usize,
}

// Every row is its prefix union its extras, so only extras need a subset index.
// Prefix differences are shared by all rows in a group rather than recomputed per pair.
fn shared_absorption(rows: &[GuardConjunction]) -> Option<Vec<bool>> {
    let width = rows.first()?.len();
    if rows.iter().all(|row| row.len() == width) {
        return Some(vec![true; rows.len()]);
    }
    #[expect(
        clippy::mutable_key_type,
        reason = "prefix ordering reads immutable guards and ignores the cached canonical view"
    )]
    let mut groups = BTreeMap::<GuardPrefix, PrefixRows<'_>>::new();
    for (index, row) in rows.iter().enumerate() {
        let (prefix, extras) = row.canonical_components()?;
        let group = groups.entry(prefix.clone()).or_insert_with(|| PrefixRows {
            prefix: prefix.canonical(),
            rows: Vec::new(),
            extras: ExtraGuardIndex::new(),
            min_extra: usize::MAX,
            max_extra: 0,
        });
        group.rows.push(index);
        group.extras.insert(extras)?;
        group.min_extra = group.min_extra.min(extras.len());
        group.max_extra = group.max_extra.max(extras.len());
    }
    let mut by_width = BTreeMap::<usize, Vec<&PrefixRows<'_>>>::new();
    for group in groups.values() {
        by_width
            .entry(group.prefix.len() + group.min_extra)
            .or_default()
            .push(group);
    }
    let mut retained = vec![true; rows.len()];
    for target in groups.values() {
        let sources = by_width
            .range(..target.prefix.len() + target.max_extra)
            .flat_map(|(_, groups)| groups.iter().copied())
            .filter_map(|source| {
                let missing = source
                    .prefix
                    .iter()
                    .filter(|guard| target.prefix.binary_search(guard).is_err())
                    .collect::<Vec<_>>();
                (missing.len() <= target.max_extra).then_some((source, missing))
            })
            .collect::<Vec<_>>();
        for index in &target.rows {
            let row = rows.get(*index)?;
            let (_, target_extras) = row.canonical_components()?;
            for (source, missing) in &sources {
                let Some(available_len) = row.len().checked_sub(source.prefix.len()) else {
                    continue;
                };
                if source.min_extra >= available_len
                    || missing
                        .iter()
                        .any(|guard| target_extras.binary_search(guard).is_err())
                {
                    continue;
                }
                let available = if std::ptr::eq(*source, target) {
                    target_extras.iter().collect::<Vec<_>>()
                } else {
                    row.iter()
                        .filter(|guard| source.prefix.binary_search(guard).is_err())
                        .collect::<Vec<_>>()
                };
                if source.extras.has_strict_subset(&available)? {
                    *retained.get_mut(*index)? = false;
                    break;
                }
            }
        }
    }
    Some(retained)
}
