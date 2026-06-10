use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::helper_analysis::insert_type_hint;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    pub(crate) reads: BTreeSet<String>,
    pub(crate) defaults: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) string_hints: BTreeSet<String>,
}

impl Effects {
    pub(crate) fn from_value(value: &AbstractValue) -> Self {
        Self {
            reads: value.paths(),
            ..Self::default()
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.reads.extend(other.reads);
        self.defaults.extend(other.defaults);
        self.string_hints.extend(other.string_hints);
        for (path, hints) in other.type_hints {
            for hint in hints {
                insert_type_hint(&mut self.type_hints, path.clone(), &hint);
            }
        }
    }

    pub(crate) fn add_default_paths(&mut self, paths: BTreeSet<String>) {
        self.defaults
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn add_type_hint(&mut self, path: String, schema_type: &str) {
        if path.trim().is_empty() {
            return;
        }
        insert_type_hint(&mut self.type_hints, path, schema_type);
    }

    pub(crate) fn add_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            self.add_type_hint(path, schema_type);
        }
    }

    pub(crate) fn add_string_hints(&mut self, paths: BTreeSet<String>) {
        self.string_hints
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalResult {
    pub(crate) value: Option<AbstractValue>,
    pub(crate) effects: Effects,
}

impl EvalResult {
    pub(crate) fn none() -> Self {
        Self::default()
    }

    pub(crate) fn from_value(value: AbstractValue) -> Self {
        Self {
            effects: Effects::from_value(&value),
            value: Some(value),
        }
    }

    pub(crate) fn with_effects(value: Option<AbstractValue>, effects: Effects) -> Self {
        Self { value, effects }
    }
}
