//! Persistent rendered-text sequences shared between execution forks.

use std::rc::Rc;

use super::OutputNode;

/// An append-only sequence of output parts.
///
/// Clones share every chunk, so forking a state never copies its rendered text. An append
/// extends the last chunk in place only while no clone holds it; otherwise it starts a chunk.
#[derive(Clone, Debug, Default)]
pub(super) struct TextParts {
    tail: Option<Rc<TextChunk>>,
}

#[derive(Debug)]
struct TextChunk {
    prev: Option<Rc<TextChunk>>,
    parts: Vec<OutputNode>,
    len: usize,
}

impl TextParts {
    pub(super) fn len(&self) -> usize {
        self.tail.as_ref().map_or(0, |chunk| chunk.len)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn push(&mut self, part: OutputNode) {
        let len = self.len() + 1;
        if let Some(chunk) = self.tail.as_mut().and_then(Rc::get_mut) {
            chunk.parts.push(part);
            chunk.len = len;
            return;
        }
        self.tail = Some(Rc::new(TextChunk {
            prev: self.tail.take(),
            parts: vec![part],
            len,
        }));
    }

    /// The chunks from first to last.
    fn chunks(&self) -> Vec<&[OutputNode]> {
        let mut chunks = Vec::new();
        let mut next = self.tail.as_deref();
        while let Some(chunk) = next {
            chunks.push(chunk.parts.as_slice());
            next = chunk.prev.as_deref();
        }
        chunks.reverse();
        chunks
    }

    pub(super) fn iter(&self) -> impl DoubleEndedIterator<Item = &OutputNode> + '_ {
        self.chunks().into_iter().flatten()
    }

    pub(super) fn get(&self, index: usize) -> Option<&OutputNode> {
        let mut next = self.tail.as_deref();
        while let Some(chunk) = next {
            let start = chunk.len - chunk.parts.len();
            if index >= start {
                return chunk.parts.get(index - start);
            }
            next = chunk.prev.as_deref();
        }
        None
    }

    /// The only part, when the sequence has exactly one.
    pub(super) fn single(&self) -> Option<&OutputNode> {
        (self.len() == 1).then(|| self.get(0)).flatten()
    }
}

impl From<Vec<OutputNode>> for TextParts {
    fn from(parts: Vec<OutputNode>) -> Self {
        let len = parts.len();
        Self {
            tail: (len != 0).then(|| {
                Rc::new(TextChunk {
                    prev: None,
                    parts,
                    len,
                })
            }),
        }
    }
}

impl FromIterator<OutputNode> for TextParts {
    fn from_iter<I: IntoIterator<Item = OutputNode>>(parts: I) -> Self {
        Self::from(parts.into_iter().collect::<Vec<_>>())
    }
}

impl<'a> IntoIterator for &'a TextParts {
    type Item = &'a OutputNode;
    type IntoIter = std::iter::Flatten<std::vec::IntoIter<&'a [OutputNode]>>;

    fn into_iter(self) -> Self::IntoIter {
        self.chunks().into_iter().flatten()
    }
}
