use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use serde::{Serialize, Serializer};

use crate::ConditionalGuard;

/// An immutable branch prefix shared by its exported conjunctions.
#[derive(Clone)]
pub struct GuardPrefix(Arc<PrefixData>);

struct PrefixData {
    guards: Arc<Vec<ConditionalGuard>>,
    canonical: OnceLock<Arc<Vec<ConditionalGuard>>>,
}

impl GuardPrefix {
    /// Owns the prefix in its original traversal order.
    #[must_use]
    pub fn new(guards: Vec<ConditionalGuard>) -> Self {
        Self(Arc::new(PrefixData {
            guards: Arc::new(guards),
            canonical: OnceLock::new(),
        }))
    }

    /// Returns the original flat prefix.
    #[must_use]
    pub fn as_slice(&self) -> &[ConditionalGuard] {
        let PrefixData {
            guards,
            canonical: _canonical,
        } = self.0.as_ref();
        guards
    }

    /// Returns the number of guards in the original prefix.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Whether the prefix has no guards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Iterates the original flat prefix.
    pub fn iter(&self) -> std::slice::Iter<'_, ConditionalGuard> {
        self.as_slice().iter()
    }

    /// Creates one shared child prefix, retaining every existing guard.
    #[must_use]
    pub fn extended(&self, guard: ConditionalGuard) -> Self {
        let mut guards = Vec::with_capacity(self.len() + 1);
        guards.extend_from_slice(self.as_slice());
        guards.push(guard);
        Self::new(guards)
    }

    /// Shares the canonical prefix without replacing the original traversal order.
    #[must_use]
    pub fn canonical(&self) -> Arc<Vec<ConditionalGuard>> {
        Arc::clone(self.canonical_ref())
    }

    fn canonical_ref(&self) -> &Arc<Vec<ConditionalGuard>> {
        self.0.canonical.get_or_init(|| {
            let mut canonical = self.as_slice().to_vec();
            ConditionalGuard::canonicalize_conjunction(&mut canonical);
            if canonical == *self.0.guards {
                Arc::clone(&self.0.guards)
            } else {
                Arc::new(canonical)
            }
        })
    }

    /// Whether two prefixes retain the same immutable storage.
    #[must_use]
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Default for GuardPrefix {
    fn default() -> Self {
        static EMPTY: OnceLock<GuardPrefix> = OnceLock::new();
        EMPTY.get_or_init(|| Self::new(Vec::new())).clone()
    }
}

impl<'a> IntoIterator for &'a GuardPrefix {
    type Item = &'a ConditionalGuard;
    type IntoIter = std::slice::Iter<'a, ConditionalGuard>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Debug for GuardPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl PartialEq for GuardPrefix {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for GuardPrefix {}
impl PartialOrd for GuardPrefix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GuardPrefix {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.shares_storage(other) {
            Ordering::Equal
        } else {
            self.as_slice().cmp(other.as_slice())
        }
    }
}
impl Hash for GuardPrefix {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

/// The canonical shared head of a conjunction and its ordered appended guards.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalGuardComponents<'a> {
    /// Immutable guards merged into the canonical head.
    pub prefix: &'a GuardPrefix,
    /// Sorted guards interleaved with the prefix.
    pub extras: &'a [ConditionalGuard],
    /// Guards appended after the complete head, in their original order.
    pub tail: &'a [ConditionalGuard],
}

/// A flat guard conjunction whose branch prefix may use shared immutable storage.
///
/// Iteration, ordering and serialization expose the same guard sequence as a vector.
/// Canonicalization sorts and deduplicates that sequence without copying a shared prefix
/// into every row.
#[derive(Clone)]
pub struct GuardConjunction(Arc<ConjunctionData>);

#[derive(Clone)]
struct ConjunctionData {
    before: Vec<ConditionalGuard>,
    prefix: GuardPrefix,
    after: Vec<ConditionalGuard>,
    // The leading guards merge with the canonical prefix before any appended guards.
    merge_prefix: bool,
}

impl GuardConjunction {
    /// Retains the exact sequence `before`, `prefix`, `after`.
    #[must_use]
    pub fn prefixed(
        before: Vec<ConditionalGuard>,
        prefix: GuardPrefix,
        after: Vec<ConditionalGuard>,
    ) -> Self {
        Self(Arc::new(ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix: false,
        }))
    }

    /// Sorts and deduplicates every guard, preserving immutable prefix sharing.
    pub fn canonicalize(&mut self) {
        if self.0.merge_prefix && self.0.after.is_empty() {
            return;
        }
        let prefix = self.0.prefix.canonical();
        let ConjunctionData {
            before,
            prefix: _prefix,
            after,
            merge_prefix,
        } = Arc::make_mut(&mut self.0);
        before.reserve_exact(after.len());
        before.extend(std::mem::take(after));
        ConditionalGuard::canonicalize_conjunction(before);
        before.retain(|guard| prefix.binary_search(guard).is_err());
        before.shrink_to_fit();
        *merge_prefix = true;
    }

    /// Returns the same conjunction in canonical order.
    #[must_use]
    pub fn canonicalized(mut self) -> Self {
        self.canonicalize();
        self
    }

    /// Iterates the complete flat sequence without materializing its shared prefix.
    #[must_use]
    pub fn iter(&self) -> GuardIter<'_> {
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = self.0.as_ref();
        if *merge_prefix {
            let canonical_prefix = prefix.canonical_ref();
            GuardIter {
                before,
                prefix: canonical_prefix,
                after,
                sorted: true,
            }
        } else {
            GuardIter {
                before,
                prefix: prefix.as_slice(),
                after,
                sorted: false,
            }
        }
    }

    /// Returns the complete number of logical guards.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().len()
    }

    /// Whether the conjunction has no logical guards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the first guard in the current flat order.
    #[must_use]
    pub fn first(&self) -> Option<&ConditionalGuard> {
        self.iter().next()
    }

    /// Returns the last guard in the current flat order.
    #[must_use]
    pub fn last(&self) -> Option<&ConditionalGuard> {
        self.iter().next_back()
    }

    /// Materializes one flat row for a consumer requiring contiguous storage.
    #[must_use]
    pub fn to_vec(&self) -> Vec<ConditionalGuard> {
        self.iter().cloned().collect()
    }

    /// Appends a guard to the existing flat sequence.
    pub fn push(&mut self, guard: ConditionalGuard) {
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = Arc::make_mut(&mut self.0);
        if prefix.is_empty() && after.is_empty() {
            before.push(guard);
            *merge_prefix = false;
        } else {
            after.reserve_exact(1);
            after.push(guard);
        }
    }

    /// Removes every occurrence of a guard while preserving the order of the others.
    #[must_use]
    pub fn without(&self, guard: &ConditionalGuard) -> Self {
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = self.0.as_ref();
        let (prefix_guards, remove_prefix_guard) = if *merge_prefix {
            let guards = prefix.canonical_ref().as_slice();
            (guards, guards.binary_search(guard).is_ok())
        } else {
            let guards = prefix.as_slice();
            (guards, guards.contains(guard))
        };
        let mut result = self.clone();
        if !remove_prefix_guard && !before.contains(guard) && !after.contains(guard) {
            return result;
        }
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix: _,
        } = Arc::make_mut(&mut result.0);
        for guards in [before, after] {
            guards.retain(|candidate| candidate != guard);
            if guards.is_empty() {
                *guards = Vec::new();
            }
        }
        if remove_prefix_guard {
            *prefix = GuardPrefix::new(
                prefix_guards
                    .iter()
                    .filter(|candidate| *candidate != guard)
                    .cloned()
                    .collect(),
            );
        }
        result
    }

    /// Reports whether every canonical guard in this row also occurs in `other`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        let subset = self.clone().canonicalized();
        let superset = other.clone().canonicalized();
        if let (Some((prefix, extras)), Some((other_prefix, other_extras))) =
            (subset.canonical_parts(), superset.canonical_parts())
            && prefix.shares_storage(other_prefix)
        {
            return sorted_subset(extras.iter(), other_extras.iter());
        }
        sorted_subset(subset.iter(), superset.iter())
    }

    /// Returns the canonical shared prefix and its disjoint additional guards.
    #[must_use]
    pub fn canonical_parts(&self) -> Option<(&GuardPrefix, &[ConditionalGuard])> {
        self.canonical_components()
            .filter(|(prefix, _)| !prefix.is_empty())
    }

    /// Returns the prefix and disjoint extras of a fully canonical row.
    ///
    /// A flat canonical row has an empty prefix and its guards appear as extras.
    #[must_use]
    pub fn canonical_components(&self) -> Option<(&GuardPrefix, &[ConditionalGuard])> {
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = self.0.as_ref();
        (*merge_prefix && after.is_empty()).then_some((prefix, before.as_slice()))
    }

    /// Returns a shared canonical head without reordering guards appended after it.
    #[must_use]
    pub fn canonical_prefix_components(&self) -> Option<CanonicalGuardComponents<'_>> {
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = self.0.as_ref();
        (*merge_prefix && !prefix.is_empty()).then_some(CanonicalGuardComponents {
            prefix,
            extras: before,
            tail: after,
        })
    }

    /// The guard slots owned by this row, excluding the shared branch prefix.
    #[must_use]
    pub fn owned_capacity(&self) -> usize {
        self.0.before.capacity() + self.0.after.capacity()
    }
}

fn sorted_subset<'a>(
    mut subset: impl Iterator<Item = &'a ConditionalGuard>,
    mut superset: impl Iterator<Item = &'a ConditionalGuard>,
) -> bool {
    subset.all(|guard| superset.by_ref().find(|candidate| *candidate >= guard) == Some(guard))
}

impl Default for GuardConjunction {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl From<Vec<ConditionalGuard>> for GuardConjunction {
    fn from(guards: Vec<ConditionalGuard>) -> Self {
        Self::prefixed(guards, GuardPrefix::default(), Vec::new())
    }
}
impl From<&GuardConjunction> for GuardConjunction {
    fn from(guards: &GuardConjunction) -> Self {
        guards.clone()
    }
}
impl FromIterator<ConditionalGuard> for GuardConjunction {
    fn from_iter<T: IntoIterator<Item = ConditionalGuard>>(iter: T) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}
impl<'a> IntoIterator for &'a GuardConjunction {
    type Item = &'a ConditionalGuard;
    type IntoIter = GuardIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl fmt::Debug for GuardConjunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl PartialEq for GuardConjunction {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for GuardConjunction {}
impl PartialOrd for GuardConjunction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GuardConjunction {
    fn cmp(&self, other: &Self) -> Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            return Ordering::Equal;
        }
        let ConjunctionData {
            before,
            prefix,
            after,
            merge_prefix,
        } = self.0.as_ref();
        let ConjunctionData {
            before: other_before,
            prefix: other_prefix,
            after: other_after,
            merge_prefix: other_merge_prefix,
        } = other.0.as_ref();
        if !*merge_prefix
            && !*other_merge_prefix
            && before.len() == other_before.len()
            && prefix.shares_storage(other_prefix)
        {
            return before
                .cmp(other_before)
                .then_with(|| after.cmp(other_after));
        }
        self.iter().cmp_flat(other.iter())
    }
}
impl Hash for GuardConjunction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for guard in self {
            guard.hash(state);
        }
    }
}
impl Serialize for GuardConjunction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

/// A borrowed flat traversal of one guard conjunction.
pub struct GuardIter<'a> {
    before: &'a [ConditionalGuard],
    prefix: &'a [ConditionalGuard],
    after: &'a [ConditionalGuard],
    sorted: bool,
}

impl GuardIter<'_> {
    fn cmp_flat(mut self, mut other: Self) -> Ordering {
        while self.sorted
            && other.sorted
            && !self.prefix.is_empty()
            && std::ptr::eq(self.prefix, other.prefix)
        {
            // Only skip shared guards that precede the next extra in both rows.
            let next_extra = self
                .before
                .first()
                .into_iter()
                .chain(other.before.first())
                .min();
            let skip = next_extra.map_or(self.prefix.len(), |extra| {
                self.prefix.partition_point(|guard| guard < extra)
            });
            let Some(remaining_prefix) = self.prefix.get(skip..) else {
                break;
            };
            self.prefix = remaining_prefix;
            other.prefix = remaining_prefix;
            let order = self.next().cmp(&other.next());
            if !order.is_eq() {
                return order;
            }
        }
        self.cmp(other)
    }
}

impl<'a> Iterator for GuardIter<'a> {
    type Item = &'a ConditionalGuard;
    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            before,
            prefix,
            after,
            sorted,
        } = self;
        let slice = if *sorted {
            match (before.first(), prefix.first()) {
                (Some(left), Some(right)) if left <= right => before,
                (Some(_) | None, Some(_)) => prefix,
                (Some(_), None) => before,
                (None, None) => after,
            }
        } else if !before.is_empty() {
            before
        } else if !prefix.is_empty() {
            prefix
        } else {
            after
        };
        let (guard, rest) = slice.split_first()?;
        *slice = rest;
        Some(guard)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.before.len() + self.prefix.len() + self.after.len();
        (count, Some(count))
    }
}
impl DoubleEndedIterator for GuardIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let Self {
            before,
            prefix,
            after,
            sorted,
        } = self;
        let slice = if !after.is_empty() {
            after
        } else if *sorted {
            match (before.last(), prefix.last()) {
                (Some(left), Some(right)) if left >= right => before,
                (Some(_) | None, Some(_)) => prefix,
                (Some(_), None) => before,
                (None, None) => return None,
            }
        } else if !prefix.is_empty() {
            prefix
        } else {
            before
        };
        let (guard, rest) = slice.split_last()?;
        *slice = rest;
        Some(guard)
    }
}
impl ExactSizeIterator for GuardIter<'_> {}

impl PartialEq<Vec<ConditionalGuard>> for GuardConjunction {
    fn eq(&self, other: &Vec<ConditionalGuard>) -> bool {
        self.iter().eq(other.iter())
    }
}
impl PartialEq<GuardConjunction> for Vec<ConditionalGuard> {
    fn eq(&self, other: &GuardConjunction) -> bool {
        self.iter().eq(other.iter())
    }
}

#[cfg(test)]
#[path = "tests/guard_conjunction.rs"]
mod tests;
