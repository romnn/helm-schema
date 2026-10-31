//! Conservative ancestry hints for skipping unrelated heap transfers.

use std::rc::Rc;

use super::{HeapNode, HeapVersion, ObjectId};

#[derive(Clone, Copy, Debug, Default)]
enum Changes {
    #[default]
    Empty,
    Objects {
        first: u64,
        last: u64,
    },
    All,
}

impl Changes {
    fn object(object: &ObjectId) -> Self {
        Self::Objects {
            first: object.ordinal,
            last: object.ordinal,
        }
    }

    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::All, Self::Empty | Self::Objects { first: _, last: _ } | Self::All)
            | (Self::Empty | Self::Objects { first: _, last: _ }, Self::All) => Self::All,
            (Self::Empty, other) | (other, Self::Empty) => other,
            (
                Self::Objects {
                    first: yes_first,
                    last: yes_last,
                },
                Self::Objects {
                    first: no_first,
                    last: no_last,
                },
            ) => Self::Objects {
                first: yes_first.min(no_first),
                last: yes_last.max(no_last),
            },
        }
    }

    fn contains(self, object: &ObjectId) -> bool {
        match self {
            Self::Empty => false,
            Self::Objects { first, last } => (first..=last).contains(&object.ordinal),
            Self::All => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct HeapIndex {
    depth: usize,
    parent: Option<Rc<HeapVersion>>,
    jump: Option<Rc<HeapVersion>>,
    changes: Changes,
    jump_changes: Changes,
}

impl HeapIndex {
    pub(super) fn for_node(node: &HeapNode) -> Self {
        let (parent, changes) = match node {
            HeapNode::Empty => return Self::default(),
            HeapNode::Input { parent } => (Rc::clone(parent), Changes::Empty),
            HeapNode::Install {
                parent,
                object,
                storage: _,
            }
            | HeapNode::Write {
                parent,
                object,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent,
                object,
                index: _,
                value: _,
            } => (Rc::clone(parent), Changes::object(object)),
            HeapNode::UnknownWrite {
                parent,
                objects: _,
                inputs: _,
            } => (Rc::clone(parent), Changes::All),
            HeapNode::Join {
                decision: _,
                yes,
                no,
            } => {
                let Some((parent, changes)) = common_prefix(yes, no) else {
                    return Self {
                        changes: Changes::All,
                        jump_changes: Changes::All,
                        ..Self::default()
                    };
                };
                (Rc::clone(parent), changes)
            }
        };
        let depth = parent.index.depth + 1;
        let mut jump = Rc::clone(&parent);
        let mut jump_changes = changes;
        if let Some(first) = &parent.index.jump
            && let Some(second) = &first.index.jump
            && parent.index.depth - first.index.depth == first.index.depth - second.index.depth
        {
            jump = Rc::clone(second);
            jump_changes = jump_changes
                .union(parent.index.jump_changes)
                .union(first.index.jump_changes);
        }
        Self {
            depth,
            parent: Some(parent),
            jump: Some(jump),
            changes,
            jump_changes,
        }
    }

    pub(super) fn skip(&self, object: &ObjectId) -> Option<&HeapVersion> {
        self.skip_root(object).map(Rc::as_ref)
    }

    pub(super) fn skip_root(&self, object: &ObjectId) -> Option<&Rc<HeapVersion>> {
        if !self.jump_changes.contains(object) {
            self.jump.as_ref()
        } else if !self.changes.contains(object) {
            self.parent.as_ref()
        } else {
            None
        }
    }

    pub(super) fn detach(&mut self, pending: &mut Vec<Rc<HeapVersion>>) {
        pending.extend(self.parent.take());
        pending.extend(self.jump.take());
    }
}

fn suffix(mut version: &Rc<HeapVersion>, depth: usize) -> Option<(&Rc<HeapVersion>, Changes)> {
    let mut changes = Changes::Empty;
    while version.index.depth > depth {
        if let Some(jump) = &version.index.jump
            && jump.index.depth >= depth
        {
            changes = changes.union(version.index.jump_changes);
            version = jump;
        } else {
            changes = changes.union(version.index.changes);
            version = version.index.parent.as_ref()?;
        }
    }
    Some((version, changes))
}

fn common_prefix<'version>(
    mut yes: &'version Rc<HeapVersion>,
    mut no: &'version Rc<HeapVersion>,
) -> Option<(&'version Rc<HeapVersion>, Changes)> {
    let depth = yes.index.depth.min(no.index.depth);
    let yes_changes;
    let no_changes;
    (yes, yes_changes) = suffix(yes, depth)?;
    (no, no_changes) = suffix(no, depth)?;
    let mut changes = yes_changes.union(no_changes);
    while yes.id != no.id {
        match (&yes.index.jump, &no.index.jump) {
            (Some(yes_jump), Some(no_jump)) if yes_jump.id != no_jump.id => {
                changes = changes
                    .union(yes.index.jump_changes)
                    .union(no.index.jump_changes);
                yes = yes_jump;
                no = no_jump;
            }
            (Some(_) | None, Some(_) | None) => {
                changes = changes.union(yes.index.changes).union(no.index.changes);
                yes = yes.index.parent.as_ref()?;
                no = no.index.parent.as_ref()?;
            }
        }
    }
    Some((yes, changes))
}
