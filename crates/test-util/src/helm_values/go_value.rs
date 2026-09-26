//! Go's `map[string]any` values, including their reference semantics.
//!
//! A Go map is a reference: Helm's coalescing shares nested tables between
//! documents and later mutates them in place, and that sharing is visible in
//! what Helm validates (a subchart's `global` table leaks into its parent's,
//! and `helm lint`'s values rule rewrites the override tables its template
//! rule reads). Tables are therefore shared `Rc<RefCell<..>>` handles, and
//! every place where Helm deep-copies calls [`deep_copy`] explicitly.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Map, Value};

/// A Go `map[string]any`, shared by reference like the Go map it models.
pub(super) type Table = Rc<RefCell<BTreeMap<String, Node>>>;

/// One Go `any` value as Helm's coalescing sees it.
#[derive(Clone, Debug)]
pub(super) enum Node {
    Nil,
    /// A scalar or a list. Coalescing replaces both wholesale and never
    /// looks inside a list, so a list is an opaque leaf.
    Leaf(Value),
    Table(Table),
}

impl Node {
    pub(super) fn from_json(value: Value) -> Self {
        match value {
            Value::Null => Self::Nil,
            Value::Object(map) => Self::Table(table_from_json(map)),
            leaf => Self::Leaf(leaf),
        }
    }

    pub(super) fn to_json(&self) -> Value {
        match self {
            Self::Nil => Value::Null,
            Self::Leaf(value) => value.clone(),
            Self::Table(table) => Value::Object(table_to_json(table)),
        }
    }

    pub(super) const fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }
}

pub(super) fn new_table() -> Table {
    Rc::new(RefCell::new(BTreeMap::new()))
}

pub(super) fn table_from_json(map: Map<String, Value>) -> Table {
    let mut entries = BTreeMap::new();
    for (key, value) in map {
        entries.insert(key, Node::from_json(value));
    }
    Rc::new(RefCell::new(entries))
}

pub(super) fn table_to_json(table: &Table) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in table.borrow().iter() {
        map.insert(key.clone(), value.to_json());
    }
    map
}

/// Helm's `copystructure.Copy`: a deep copy that shares nothing.
pub(super) fn deep_copy(table: &Table) -> Table {
    let mut entries = BTreeMap::new();
    for (key, value) in table.borrow().iter() {
        let copied = match value {
            Node::Table(nested) => Node::Table(deep_copy(nested)),
            Node::Nil | Node::Leaf(_) => value.clone(),
        };
        entries.insert(key.clone(), copied);
    }
    Rc::new(RefCell::new(entries))
}

/// The entries of `table` at this moment, so a caller can mutate tables
/// while walking them the way Go ranges over a map.
pub(super) fn entries(table: &Table) -> Vec<(String, Node)> {
    table
        .borrow()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(super) fn get(table: &Table, key: &str) -> Option<Node> {
    table.borrow().get(key).cloned()
}

pub(super) fn set(table: &Table, key: &str, value: Node) {
    table.borrow_mut().insert(key.to_string(), value);
}

pub(super) fn remove(table: &Table, key: &str) {
    table.borrow_mut().remove(key);
}
