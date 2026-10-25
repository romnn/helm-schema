use std::cmp::Ordering;
use std::rc::Rc;

use super::RuntimeRead;

type Link = Option<Rc<Node>>;

#[derive(Debug)]
struct Node {
    entry: Rc<RuntimeRead>,
    left: Link,
    right: Link,
    height: usize,
    length: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ReadSet {
    root: Link,
}

impl ReadSet {
    pub(super) fn len(&self) -> usize {
        length(&self.root)
    }

    pub(super) fn clear(&mut self) {
        self.root = None;
    }

    pub(super) fn insert(&mut self, entry: RuntimeRead) -> bool {
        let before = self.len();
        self.root = Some(insert(&self.root, Rc::new(entry)));
        self.len() != before
    }

    pub(super) fn union(&self, other: &Self) -> Self {
        Self {
            root: union(&self.root, &other.root),
        }
    }

    pub(super) fn iter(&self) -> ReadIter<'_> {
        let mut result = ReadIter {
            pending: Vec::new(),
            remaining: self.len(),
        };
        result.descend(self.root.as_deref());
        result
    }
}

impl FromIterator<RuntimeRead> for ReadSet {
    fn from_iter<T: IntoIterator<Item = RuntimeRead>>(iter: T) -> Self {
        let mut result = Self::default();
        for entry in iter {
            result.insert(entry);
        }
        result
    }
}

impl PartialEq for ReadSet {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for ReadSet {}

impl<'a> IntoIterator for &'a ReadSet {
    type Item = &'a RuntimeRead;
    type IntoIter = ReadIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(super) struct ReadIter<'a> {
    pending: Vec<&'a Node>,
    remaining: usize,
}

impl<'a> ReadIter<'a> {
    fn descend(&mut self, mut current: Option<&'a Node>) {
        while let Some(node) = current {
            self.pending.push(node);
            current = node.left.as_deref();
        }
    }
}

impl<'a> Iterator for ReadIter<'a> {
    type Item = &'a RuntimeRead;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.pending.pop()?;
        self.remaining -= 1;
        self.descend(node.right.as_deref());
        Some(&node.entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for ReadIter<'_> {}

fn height(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.height)
}

fn length(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.length)
}

fn node(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    Rc::new(Node {
        height: 1 + height(&left).max(height(&right)),
        length: 1 + length(&left) + length(&right),
        entry,
        left,
        right,
    })
}

fn balance(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    if height(&left) > height(&right) + 1
        && let Some(left) = &left
    {
        if height(&left.right) > height(&left.left)
            && let Some(pivot) = &left.right
        {
            return node(
                Rc::clone(&pivot.entry),
                Some(node(
                    Rc::clone(&left.entry),
                    left.left.clone(),
                    pivot.left.clone(),
                )),
                Some(node(entry, pivot.right.clone(), right)),
            );
        }
        return node(
            Rc::clone(&left.entry),
            left.left.clone(),
            Some(node(entry, left.right.clone(), right)),
        );
    }
    if height(&right) > height(&left) + 1
        && let Some(right) = &right
    {
        if height(&right.left) > height(&right.right)
            && let Some(pivot) = &right.left
        {
            return node(
                Rc::clone(&pivot.entry),
                Some(node(entry, left, pivot.left.clone())),
                Some(node(
                    Rc::clone(&right.entry),
                    pivot.right.clone(),
                    right.right.clone(),
                )),
            );
        }
        return node(
            Rc::clone(&right.entry),
            Some(node(entry, left, right.left.clone())),
            right.right.clone(),
        );
    }
    node(entry, left, right)
}

fn insert(root: &Link, entry: Rc<RuntimeRead>) -> Rc<Node> {
    let Some(current) = root else {
        return node(entry, None, None);
    };
    let (left, right) = match entry.cmp(&current.entry) {
        Ordering::Equal => return Rc::clone(current),
        Ordering::Less => {
            let left = insert(&current.left, entry);
            if current
                .left
                .as_ref()
                .is_some_and(|old| Rc::ptr_eq(old, &left))
            {
                return Rc::clone(current);
            }
            (Some(left), current.right.clone())
        }
        Ordering::Greater => {
            let right = insert(&current.right, entry);
            if current
                .right
                .as_ref()
                .is_some_and(|old| Rc::ptr_eq(old, &right))
            {
                return Rc::clone(current);
            }
            (current.left.clone(), Some(right))
        }
    };
    balance(Rc::clone(&current.entry), left, right)
}

// All left entries precede entry, and all right entries follow it; subtree heights may differ.
fn join(entry: Rc<RuntimeRead>, left: Link, right: Link) -> Rc<Node> {
    if height(&left) > height(&right) + 1
        && let Some(root) = &left
    {
        let tail = join(entry, root.right.clone(), right);
        return balance(Rc::clone(&root.entry), root.left.clone(), Some(tail));
    }
    if height(&right) > height(&left) + 1
        && let Some(root) = &right
    {
        let head = join(entry, left, root.left.clone());
        return balance(Rc::clone(&root.entry), Some(head), root.right.clone());
    }
    node(entry, left, right)
}

// Partition around entry, omitting an equal entry; join restores AVL balance after each cut.
fn split(root: &Link, entry: &RuntimeRead) -> (Link, Link) {
    let Some(root) = root else {
        return (None, None);
    };
    match entry.cmp(&root.entry) {
        Ordering::Equal => (root.left.clone(), root.right.clone()),
        Ordering::Less => {
            let (lower, upper) = split(&root.left, entry);
            (
                lower,
                Some(join(Rc::clone(&root.entry), upper, root.right.clone())),
            )
        }
        Ordering::Greater => {
            let (lower, upper) = split(&root.right, entry);
            (
                Some(join(Rc::clone(&root.entry), root.left.clone(), lower)),
                upper,
            )
        }
    }
}

fn union(left: &Link, right: &Link) -> Link {
    let (Some(left), Some(right)) = (left, right) else {
        return left.clone().or_else(|| right.clone());
    };
    if Rc::ptr_eq(left, right) {
        return Some(Rc::clone(left));
    }
    let (lower, upper) = split(&Some(Rc::clone(right)), &left.entry);
    Some(join(
        Rc::clone(&left.entry),
        union(&left.left, &lower),
        union(&left.right, &upper),
    ))
}

#[cfg(test)]
#[path = "../tests/persistent_read_set.rs"]
mod tests;
