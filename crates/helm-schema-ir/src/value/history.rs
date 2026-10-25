use std::rc::Rc;

// Only the tail may be partial; append copies at most one shared chunk plus the Rc directory.
const CHUNK_LENGTH: usize = 16;

#[derive(Debug)]
pub(super) struct History<T> {
    chunks: Rc<Vec<Rc<Vec<T>>>>,
    length: usize,
}

impl<T> Clone for History<T> {
    fn clone(&self) -> Self {
        Self {
            chunks: Rc::clone(&self.chunks),
            length: self.length,
        }
    }
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            chunks: Rc::new(Vec::new()),
            length: 0,
        }
    }
}

impl<T> History<T> {
    pub(super) fn len(&self) -> usize {
        self.length
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

    pub(super) fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|candidate| candidate == value)
    }
}

impl<T: Clone> History<T> {
    pub(super) fn push(&mut self, value: T) {
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
        HistoryIterMut {
            chunks: Rc::make_mut(&mut self.chunks).iter_mut(),
            current: None,
            remaining: self.length,
        }
    }
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
