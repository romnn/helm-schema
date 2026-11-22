use std::collections::hash_map::Entry;
use std::hash::{BuildHasher as _, BuildHasherDefault, DefaultHasher, Hash};
use std::rc::Rc;

use ahash::AHashMap;

// Only the tail may be partial; append copies at most one shared chunk plus the Rc directory.
const CHUNK_LENGTH: usize = 16;

#[derive(Debug)]
pub(super) struct History<T> {
    chunks: Rc<Vec<Rc<Vec<T>>>>,
    length: usize,
    prefix: Rc<SharedPrefix>,
}

#[derive(Debug, Default)]
struct SharedPrefix {
    parent: Option<Rc<Self>>,
    length: usize,
    depth: usize,
}

impl Drop for SharedPrefix {
    fn drop(&mut self) {
        let mut parent = self.parent.take();
        while let Some(mut owned) = parent.and_then(|shared| Rc::try_unwrap(shared).ok()) {
            parent = owned.parent.take();
        }
    }
}

impl<T> Clone for History<T> {
    fn clone(&self) -> Self {
        Self {
            chunks: Rc::clone(&self.chunks),
            length: self.length,
            prefix: Rc::clone(&self.prefix),
        }
    }
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            chunks: Rc::new(Vec::new()),
            length: 0,
            prefix: Rc::default(),
        }
    }
}

impl<T> History<T> {
    pub(super) fn len(&self) -> usize {
        self.length
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        self.chunks
            .get(index / CHUNK_LENGTH)?
            .get(index % CHUNK_LENGTH)
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn iter(&self) -> HistoryIter<'_, T> {
        HistoryIter {
            chunks: self.chunks.iter(),
            current: None,
            remaining: self.length,
        }
    }

    pub(super) fn since(&self, prefix: &Self) -> impl Iterator<Item = &T> {
        self.suffix(self.shared_prefix(prefix))
    }

    fn shared_prefix(&self, other: &Self) -> usize {
        let (mut left, mut right) = (&self.prefix, &other.prefix);
        let mut length = self.length.min(other.length);
        while !Rc::ptr_eq(left, right) {
            if left.depth >= right.depth {
                length = length.min(left.length);
                let Some(parent) = &left.parent else {
                    return 0;
                };
                left = parent;
            } else {
                length = length.min(right.length);
                let Some(parent) = &right.parent else {
                    return 0;
                };
                right = parent;
            }
        }
        length
    }

    fn suffix(&self, start: usize) -> impl Iterator<Item = &T> {
        self.chunks
            .iter()
            .skip(start / CHUNK_LENGTH)
            .flat_map(|chunk| chunk.iter())
            .skip(start % CHUNK_LENGTH)
    }
}

impl<T: Clone> History<T> {
    /// Appends incoming observations absent under `same`.
    ///
    /// Equivalent observations must have equal fingerprints.
    #[tracing::instrument(skip_all, fields(left = self.len(), right = other.len(), comparisons = tracing::field::Empty, shared_prefix = tracing::field::Empty))]
    pub(super) fn union_by_hashed(
        &self,
        other: &Self,
        mut same: impl FnMut(&T, &T) -> bool,
        mut fingerprint: impl FnMut(&T) -> u64,
    ) -> Self {
        let prefix = self.shared_prefix(other);
        let mut result = self.clone();
        let mut comparisons = 0_u64;
        let mut compare = |left: &T, right: &T| {
            comparisons += 1;
            same(left, right)
        };
        if other.length - prefix <= 1 {
            for value in other.suffix(prefix) {
                if !self.iter().any(|existing| compare(existing, value)) {
                    result.push(value.clone());
                }
            }
        } else {
            let mut members = AHashMap::<u64, FingerprintBucket<'_, T>>::with_capacity(self.len());
            for value in self {
                members
                    .entry(fingerprint(value))
                    .and_modify(|bucket| bucket.collisions.push(value))
                    .or_insert_with(|| FingerprintBucket::new(value));
            }
            for value in other.suffix(prefix) {
                match members.entry(fingerprint(value)) {
                    Entry::Occupied(mut entry) => {
                        if entry.get().iter().any(|existing| compare(existing, value)) {
                            continue;
                        }
                        entry.get_mut().collisions.push(value);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(FingerprintBucket::new(value));
                    }
                }
                result.push(value.clone());
            }
        }
        tracing::Span::current().record("comparisons", comparisons);
        tracing::Span::current().record("shared_prefix", prefix);
        result
    }

    pub(super) fn push(&mut self, value: T) {
        if Rc::strong_count(&self.chunks) > 1 {
            self.prefix = Rc::new(SharedPrefix {
                parent: Some(Rc::clone(&self.prefix)),
                length: self.length,
                depth: self.prefix.depth + 1,
            });
        }
        let chunks = Rc::make_mut(&mut self.chunks);
        if let Some(last) = chunks.last_mut().filter(|last| last.len() < CHUNK_LENGTH) {
            Rc::make_mut(last).push(value);
        } else {
            let mut chunk = Vec::with_capacity(CHUNK_LENGTH);
            chunk.push(value);
            chunks.push(Rc::new(chunk));
        }
        self.length += 1;
    }

    pub(super) fn iter_mut(&mut self) -> HistoryIterMut<'_, T> {
        // A rewritten observation is no longer part of the inherited append-only prefix.
        self.prefix = Rc::default();
        HistoryIterMut {
            chunks: Rc::make_mut(&mut self.chunks).iter_mut(),
            current: None,
            remaining: self.length,
        }
    }
}

struct FingerprintBucket<'a, T> {
    first: &'a T,
    collisions: Vec<&'a T>,
}

impl<'a, T> FingerprintBucket<'a, T> {
    fn new(first: &'a T) -> Self {
        Self {
            first,
            collisions: Vec::new(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = &'a T> + '_ {
        std::iter::once(self.first).chain(self.collisions.iter().copied())
    }
}

pub(super) fn fingerprint<T: Hash + ?Sized>(key: &T) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(key)
}

impl<T: Clone> FromIterator<T> for History<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut history = Self::default();
        for value in iter {
            history.push(value);
        }
        history
    }
}

impl<'a, T> IntoIterator for &'a History<T> {
    type Item = &'a T;
    type IntoIter = HistoryIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a mut History<T> {
    type Item = &'a mut T;
    type IntoIter = HistoryIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

pub(super) struct HistoryIter<'a, T> {
    chunks: std::slice::Iter<'a, Rc<Vec<T>>>,
    current: Option<std::slice::Iter<'a, T>>,
    remaining: usize,
}

impl<'a, T> Iterator for HistoryIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(value) = self.current.as_mut().and_then(Iterator::next) {
                self.remaining -= 1;
                return Some(value);
            }
            self.current = Some(self.chunks.next()?.iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> ExactSizeIterator for HistoryIter<'_, T> {}

pub(super) struct HistoryIterMut<'a, T> {
    chunks: std::slice::IterMut<'a, Rc<Vec<T>>>,
    current: Option<std::slice::IterMut<'a, T>>,
    remaining: usize,
}

impl<'a, T: Clone> Iterator for HistoryIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(value) = self.current.as_mut().and_then(Iterator::next) {
                self.remaining -= 1;
                return Some(value);
            }
            self.current = Some(Rc::make_mut(self.chunks.next()?).iter_mut());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T: Clone> ExactSizeIterator for HistoryIterMut<'_, T> {}

#[cfg(test)]
#[path = "../tests/persistent_history.rs"]
mod tests;
