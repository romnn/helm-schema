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
            AbstractValue::MergedLayers(layers) => {
                for layer in layers {
                    collect(layer, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::Dict(_)
            | AbstractValue::List(_)
            | AbstractValue::Overlay { .. }
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
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
            AbstractValue::MergedLayers(layers) => {
                for value in layers {
                    collect(value, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
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

/// Wraps a raw-identity value with a lexical escape token: the enclosing
/// transform (`replace TOKEN …`, `(split TOKEN …)._0`) is the identity on
/// raw strings that do not contain `token`, so the output keeps the raw
/// path qualified by the token instead of degrading to derived text.
/// Values that are not a clean raw identity (already derived, serialized,
/// or shape-erased) return `None` — callers fall back to their legacy
/// lowering.
pub(super) fn escape_wrapped_identity(
    value: &AbstractValue,
    effects: &crate::eval_effect::Effects,
    token: &str,
) -> Option<AbstractValue> {
    match value {
        AbstractValue::ValuesPath(path) => {
            if effects.shape_erased_paths.contains(path)
                || effects.derived_text_paths.contains(path)
                || effects
                    .local_output_meta
                    .get(path)
                    .is_some_and(|meta| meta.shape_erased || meta.derived_text)
            {
                return None;
            }
            let mut meta = crate::helper_meta::HelperOutputMeta::default();
            meta.lexical_escapes.insert(token.to_string());
            Some(AbstractValue::OutputPath(path.clone(), meta))
        }
        AbstractValue::OutputPath(path, meta) => {
            if meta.shape_erased
                || meta.derived_text
                || meta.yaml_serialized
                || meta.json_serialized
            {
                return None;
            }
            let mut meta = meta.clone();
            meta.lexical_escapes.insert(token.to_string());
            Some(AbstractValue::OutputPath(path.clone(), meta))
        }
        _ => None,
    }
}

/// The transformed value of `replace OLD NEW subject` when OLD is one
/// static literal: static string arms replace textually, raw-identity arms
/// keep their path qualified by OLD as a lexical escape. `None` when any
/// arm has neither meaning — the caller falls back to the legacy
/// derived-text lowering.
pub(super) fn replace_transformed_value(
    value: &AbstractValue,
    effects: &crate::eval_effect::Effects,
    old: &str,
    new_values: &BTreeSet<String>,
) -> Option<AbstractValue> {
    match value {
        AbstractValue::StringSet(strings) => {
            if new_values.is_empty() {
                return None;
            }
            let mut rendered = BTreeSet::new();
            for subject in strings {
                for new in new_values {
                    rendered.insert(subject.replace(old, new));
                }
            }
            Some(AbstractValue::StringSet(rendered))
        }
        AbstractValue::Choice(choices) => {
            let mapped = choices
                .iter()
                .map(|choice| replace_transformed_value(choice, effects, old, new_values))
                .collect::<Option<Vec<_>>>()?;
            AbstractValue::choice(mapped)
        }
        other => escape_wrapped_identity(other, effects, old),
    }
}

/// The map value of `split SEP subject` when SEP is a static literal:
/// static string arms split into their exact `_N` member maps, raw-identity
/// arms become a map whose `_0` member keeps the raw path qualified by SEP
/// as a lexical escape (`_0` IS the whole raw string exactly when SEP does
/// not occur in it). `None` when any arm has neither meaning.
pub(super) fn split_transformed_value(
    value: &AbstractValue,
    effects: &crate::eval_effect::Effects,
    separator: &str,
) -> Option<AbstractValue> {
    match value {
        AbstractValue::StringSet(strings) => {
            let members = strings
                .iter()
                .map(|text| {
                    AbstractValue::Dict(
                        text.split(separator)
                            .enumerate()
                            .map(|(index, part)| {
                                (
                                    format!("_{index}"),
                                    AbstractValue::StringSet(
                                        [part.to_string()].into_iter().collect(),
                                    ),
                                )
                            })
                            .collect(),
                    )
                })
                .collect();
            AbstractValue::choice(members)
        }
        AbstractValue::Choice(choices) => {
            let mapped = choices
                .iter()
                .map(|choice| split_transformed_value(choice, effects, separator))
                .collect::<Option<Vec<_>>>()?;
            AbstractValue::choice(mapped)
        }
        other => {
            let prefix = escape_wrapped_identity(other, effects, separator)?;
            Some(AbstractValue::Overlay {
                entries: std::collections::BTreeMap::from([("_0".to_string(), prefix)]),
                fallback: Box::new(AbstractValue::Unknown),
            })
        }
    }
}

/// Marks a passthrough value's `OutputPath` arms as derived text: the
/// enclosing transform produced NEW text from them (a dynamic `replace`, a
/// truncation, a numeric cast), so an escape-qualified identity they still
/// spell no longer holds. `ValuesPath` arms need no marking — the
/// expression-level derived flags already cover them.
/// Marks `toString`'s identity-bearing operand arms as `stringified`: the
/// result IS the exact `%v` rendering of that path, so total-string-preimage
/// consumers (strict parsers, equality preimages) may keep binding the raw
/// path through the rendered text. An arm that was already text-derived
/// stringifies its derivation instead and keeps its flags alone.
pub(super) fn mark_stringified_identities(value: Option<AbstractValue>) -> Option<AbstractValue> {
    fn mark(value: AbstractValue) -> AbstractValue {
        match value {
            AbstractValue::OutputPath(path, mut meta) => {
                if !meta.derived_text
                    && !meta.shape_erased
                    && !meta.yaml_serialized
                    && !meta.json_serialized
                {
                    meta.stringified = true;
                }
                AbstractValue::OutputPath(path, meta)
            }
            AbstractValue::Choice(choices) => {
                AbstractValue::Choice(choices.into_iter().map(mark).collect())
            }
            other => other,
        }
    }
    value.map(mark)
}

pub(super) fn derive_value_text(value: Option<AbstractValue>) -> Option<AbstractValue> {
    fn mark(value: AbstractValue) -> AbstractValue {
        match value {
            AbstractValue::OutputPath(path, mut meta) => {
                meta.derived_text = true;
                AbstractValue::OutputPath(path, meta)
            }
            AbstractValue::Choice(choices) => {
                AbstractValue::Choice(choices.into_iter().map(mark).collect())
            }
            other => other,
        }
    }
    value.map(mark)
}

/// The transformed value of `trimPrefix`/`trimSuffix` with one static
/// nonempty affix: static string arms trim exactly, raw-identity arms keep
/// their path qualified by the affix as a lexical escape (trimming is the
/// identity on strings that do not contain it — a superset of the
/// starts-with/ends-with strings it actually touches, so the exemption
/// only widens). `None` when any arm has neither meaning.
pub(super) fn trim_affix_transformed_value(
    value: &AbstractValue,
    effects: &crate::eval_effect::Effects,
    token: &str,
    prefix: bool,
) -> Option<AbstractValue> {
    match value {
        AbstractValue::StringSet(strings) => Some(AbstractValue::StringSet(
            strings
                .iter()
                .map(|text| {
                    let trimmed = if prefix {
                        text.strip_prefix(token)
                    } else {
                        text.strip_suffix(token)
                    };
                    trimmed.unwrap_or(text).to_string()
                })
                .collect(),
        )),
        AbstractValue::Choice(choices) => {
            let mapped = choices
                .iter()
                .map(|choice| trim_affix_transformed_value(choice, effects, token, prefix))
                .collect::<Option<Vec<_>>>()?;
            AbstractValue::choice(mapped)
        }
        other => escape_wrapped_identity(other, effects, token),
    }
}

/// The transformed value of a `regexReplaceAll`-family call whose pattern
/// has a mandatory literal: the replacement is the identity on strings not
/// containing that literal, so raw-identity arms keep their path qualified
/// by it as a lexical escape. `None` when the subject is not a clean raw
/// identity.
pub(super) fn regex_replace_transformed_value(
    value: &AbstractValue,
    effects: &crate::eval_effect::Effects,
    token: &str,
) -> Option<AbstractValue> {
    match value {
        AbstractValue::Choice(choices) => {
            let mapped = choices
                .iter()
                .map(|choice| regex_replace_transformed_value(choice, effects, token))
                .collect::<Option<Vec<_>>>()?;
            AbstractValue::choice(mapped)
        }
        other => escape_wrapped_identity(other, effects, token),
    }
}

/// A literal substring every match of `pattern` must contain: the literal
/// run at the pattern start (after an optional `^` anchor), minus a final
/// character an immediately following quantifier would make optional.
/// `None` when the pattern opens with a metacharacter.
pub(super) fn regex_mandatory_literal(pattern: &str) -> Option<String> {
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let mut literal = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(character) = chars.peek().copied() {
        if matches!(
            character,
            '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            break;
        }
        literal.push(character);
        chars.next();
    }
    if matches!(chars.peek(), Some('?' | '*' | '{')) {
        literal.pop();
    }
    (!literal.is_empty()).then_some(literal)
}
