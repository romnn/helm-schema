use std::collections::BTreeSet;

use crate::abstract_value::AbstractValue;

pub(super) fn value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value.as_ref().map(AbstractValue::paths).unwrap_or_default()
}

pub(super) fn value_strings(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value
        .as_ref()
        .map(AbstractValue::strings)
        .unwrap_or_default()
}

/// Paths whose value this abstract value may literally be. Widened influence
/// is dataflow through an unknown call, not value identity: defaulting or
/// type-hinting the call result (e.g. `required "..." .Values.x | quote`)
/// says nothing about the type or defaultedness of `.Values.x` itself.
pub(super) fn identity_value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    fn collect(value: &AbstractValue, paths: &mut BTreeSet<String>) {
        match value {
            AbstractValue::ValuesPath(path)
            | AbstractValue::JsonDecodedPath(path)
            | AbstractValue::OutputPath(path, _) => {
                paths.insert(path.clone());
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::Dict(_)
            | AbstractValue::List(_)
            | AbstractValue::Overlay { .. }
            | AbstractValue::SplitList { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut paths = BTreeSet::new();
    if let Some(value) = value {
        collect(value, &mut paths);
    }
    paths
}

/// Values identities contained in a payload serialized by `toJson` or
/// `toYaml`.
/// Constructed containers are not themselves aliases of their leaves, so
/// strict consumers use [`identity_value_paths`] and stop at those container
/// boundaries. Serialization is different: every structurally retained leaf
/// is part of the encoded payload and must be marked so a matching decoder
/// can recover its runtime identity.
pub(super) fn serialization_payload_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    fn collect(value: &AbstractValue, paths: &mut BTreeSet<String>) {
        match value {
            AbstractValue::ValuesPath(path)
            | AbstractValue::JsonDecodedPath(path)
            | AbstractValue::OutputPath(path, _) => {
                paths.insert(path.clone());
            }
            AbstractValue::Dict(entries) => {
                for value in entries.values() {
                    collect(value, paths);
                }
            }
            AbstractValue::List(items) => {
                for value in items {
                    collect(value, paths);
                }
            }
            AbstractValue::Overlay { entries, fallback } => {
                for value in entries.values() {
                    collect(value, paths);
                }
                collect(fallback, paths);
            }
            AbstractValue::Choice(choices) => {
                for value in choices {
                    collect(value, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::SplitList { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut paths = BTreeSet::new();
    if let Some(value) = value {
        collect(value, &mut paths);
    }
    paths
}

pub(super) fn identity_range_key_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value
        .as_ref()
        .map(AbstractValue::range_key_paths)
        .unwrap_or_default()
}

/// The exact element count of a statically known collection value.
pub(super) fn concrete_collection_len(value: &AbstractValue) -> Option<usize> {
    match value {
        AbstractValue::List(items) => Some(items.len()),
        AbstractValue::Dict(entries) => Some(entries.len()),
        _ => None,
    }
}

/// The exact integer a statically known scalar value denotes.
pub(super) fn concrete_integer(value: &AbstractValue) -> Option<i64> {
    let AbstractValue::StringSet(strings) = value else {
        return None;
    };
    let mut strings = strings.iter();
    let (Some(text), None) = (strings.next(), strings.next()) else {
        return None;
    };
    text.parse().ok()
}
