//! Bounded regular-language preimages of an explicit text relation.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use regex_automata::{
    Anchored, Input,
    dfa::{Automaton as _, dense},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// An ASCII character homomorphism surrounded by literal text.
///
/// Non-ASCII sources remain unknown rather than using a guessed Unicode mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TextPreimage {
    /// Text emitted before the source.
    pub before: String,
    /// Text emitted after the source.
    pub after: String,
    /// Changed ASCII characters; absent entries preserve the character.
    pub characters: BTreeMap<u8, String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PatternContext {
    relation: TextPreimage,
    pattern: String,
}

thread_local! {
    static PATTERN_PREIMAGES: RefCell<BTreeMap<PatternContext, Option<String>>> = RefCell::default();
}

struct SchemaBound {
    value: Value,
    exact: bool,
}

impl SchemaBound {
    fn unknown() -> Self {
        Self {
            value: Value::Bool(true),
            exact: false,
        }
    }
}

impl TextPreimage {
    /// Applies the relation to an ASCII source within the output byte bound.
    #[must_use]
    pub fn apply(&self, text: &str, limit: usize) -> Option<String> {
        if !text.is_ascii() || self.before.len().saturating_add(self.after.len()) > limit {
            return None;
        }
        let mut output = self.before.clone();
        for byte in text.bytes() {
            if let Some(mapped) = self.characters.get(&byte) {
                if output.len().saturating_add(mapped.len()) > limit {
                    return None;
                }
                output.push_str(mapped);
            } else {
                if output.len() == limit {
                    return None;
                }
                output.push(char::from(byte));
            }
        }
        if output.len().saturating_add(self.after.len()) > limit {
            return None;
        }
        output.push_str(&self.after);
        (output.len() <= limit).then_some(output)
    }

    /// Composes this relation followed by `outer`, preserving the byte bound.
    #[must_use]
    pub fn then(&self, outer: &Self) -> Option<Self> {
        let mut result = Self {
            before: outer.apply(&self.before, 4096)?,
            after: String::new(),
            characters: BTreeMap::new(),
        };
        let mapping = Self {
            before: String::new(),
            after: String::new(),
            characters: outer.characters.clone(),
        };
        result
            .before
            .truncate(result.before.len().checked_sub(outer.after.len())?);
        result.after = mapping.apply(&self.after, 4096)? + &outer.after;
        for byte in 0..=127 {
            let source = char::from(byte).to_string();
            let inner = self.characters.get(&byte).unwrap_or(&source);
            let mapped = mapping.apply(inner, 4096)?;
            if mapped != source {
                result.characters.insert(byte, mapped);
            }
        }
        Some(result)
    }

    /// Pulls a Draft7 JSON schema over strings back through the relation.
    ///
    /// Unsupported schemas, exhausted automata and non-ASCII sources widen to unknown.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn schema_preimage(&self, schema: &Value) -> Value {
        let bound = self.schema(schema, &mut 128);
        if bound.value == Value::Bool(true) {
            return json!({});
        }
        json!({"anyOf": [bound.value, {"type": "string", "pattern": r"[^\x00-\x7f]"}, {"not": {"type": "string"}}]})
    }

    fn schema(&self, schema: &Value, remaining: &mut usize) -> SchemaBound {
        let Some(rest) = remaining.checked_sub(1) else {
            return SchemaBound::unknown();
        };
        *remaining = rest;
        let object = match schema {
            Value::Bool(_) => {
                return SchemaBound {
                    value: schema.clone(),
                    exact: true,
                };
            }
            Value::Object(object) => object,
            _ => return SchemaBound::unknown(),
        };
        // Draft7 ignores reference siblings; resolving the reference belongs to the caller.
        if object.contains_key("$ref") {
            return SchemaBound::unknown();
        }
        if let Some(kind) = object.get("type") {
            let string = match kind {
                Value::String(kind) => kind == "string",
                Value::Array(kinds) => kinds.iter().any(|kind| kind.as_str() == Some("string")),
                _ => return SchemaBound::unknown(),
            };
            if !string {
                return SchemaBound {
                    value: Value::Bool(false),
                    exact: true,
                };
            }
        }
        let mut parts = Vec::new();
        let mut exact = true;
        for (keyword, value) in object {
            let part = match keyword.as_str() {
                "$dynamicRef" | "$recursiveRef" | "if" | "then" | "else" | "format" => None,
                "pattern" => value.as_str().and_then(|pattern|self.pattern(pattern)).map(|pattern|json!({"pattern":pattern})),
                "const" => self.finite(std::iter::once(value)),
                "enum" => value.as_array().and_then(|values|self.finite(values.iter())),
                "minLength" | "maxLength" => value.as_u64().and_then(|width| {
                    let pattern = if keyword == "minLength" { format!(r"^[\s\S]{{{width},}}$") } else { format!(r"^[\s\S]{{0,{width}}}$") };
                    self.pattern(&pattern).map(|pattern|json!({"pattern":pattern}))
                }),
                "not" => {
                    // Negation reverses the error polarity and therefore requires exactness.
                    let inner = self.schema(value, remaining);
                    inner.exact.then(||json!({"not":inner.value}))
                }
                "allOf" | "anyOf" | "oneOf" => value.as_array().and_then(|values| {
                    let children = values.iter().map(|value|self.schema(value, remaining)).collect::<Vec<_>>();
                    let complete = children.iter().all(|child|child.exact);
                    if keyword == "oneOf" && !complete { return None; }
                    exact &= complete;
                    Some(json!({keyword: children.into_iter().map(|child|child.value).collect::<Vec<_>>()}))
                }),
                _ => continue,
            };
            match part {
                Some(part) => parts.push(part),
                None => exact = false,
            }
        }
        SchemaBound {
            value: match parts.as_slice() {
                [] => Value::Bool(true),
                [part] => part.clone(),
                _ => json!({"allOf":parts}),
            },
            exact,
        }
    }

    fn finite<'a>(&self, values: impl Iterator<Item = &'a Value>) -> Option<Value> {
        let patterns = values.filter_map(Value::as_str).map(|text| {
            Some(json!({"pattern": self.pattern(&format!("^{}$",crate::escape_regex_literal(text)))?}))
        }).collect::<Option<Vec<_>>>()?;
        Some(if patterns.is_empty() {
            Value::Bool(false)
        } else {
            json!({"anyOf": patterns})
        })
    }

    fn pattern(&self, pattern: &str) -> Option<String> {
        if pattern.len() > 16 * 1024
            || self.before.len() + self.after.len() > 4096
            || self.characters.len() > 128
            || !self.before.is_ascii()
            || !self.after.is_ascii()
            || self
                .characters
                .values()
                .any(|text| text.len() > 4096 || !text.is_ascii())
        {
            return None;
        }
        let key = PatternContext {
            relation: self.clone(),
            pattern: pattern.to_owned(),
        };
        if let Some(result) = PATTERN_PREIMAGES.with_borrow(|cache| cache.get(&key).cloned()) {
            return result;
        }
        let result = self.compile_pattern(pattern);
        PATTERN_PREIMAGES.with_borrow_mut(|cache| {
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(key, result.clone());
        });
        result
    }

    #[tracing::instrument(skip_all)]
    fn compile_pattern(&self, pattern: &str) -> Option<String> {
        let dfa = pattern_automaton(pattern)?;
        let mut start = dfa
            .start_state_forward(&Input::new(b"").anchored(Anchored::Yes))
            .ok()?;
        for byte in self.before.bytes() {
            start = dfa.next_state(start, byte);
        }
        let mut states = vec![start];
        let mut edges = Vec::new();
        let mut index = 0;
        while let Some(state) = states.get(index).copied() {
            if states.len() > 64 || dfa.is_quit_state(state) {
                return None;
            }
            let mut targets = BTreeMap::<usize, Vec<u8>>::new();
            for byte in 0..=127 {
                let mut next = state;
                if let Some(mapped) = self.characters.get(&byte) {
                    for byte in mapped.bytes() {
                        next = dfa.next_state(next, byte);
                    }
                } else {
                    next = dfa.next_state(next, byte);
                }
                let target = if let Some(target) = states.iter().position(|state| *state == next) {
                    target
                } else {
                    states.push(next);
                    states.len() - 1
                };
                targets.entry(target).or_default().push(byte);
            }
            for (target, bytes) in targets {
                let class = regex_character_class(bytes.into_iter().map(char::from))?;
                edges.push(RegularTransition {
                    from: index,
                    to: target,
                    pattern: class,
                });
            }
            index += 1;
        }
        let entry = states.len();
        let exit = entry + 1;
        edges.push(RegularTransition {
            from: entry,
            to: 0,
            pattern: String::new(),
        });
        for (index, mut state) in states.iter().copied().enumerate() {
            for byte in self.after.bytes() {
                state = dfa.next_state(state, byte);
            }
            if dfa.is_match_state(dfa.next_eoi_state(state)) {
                edges.push(RegularTransition {
                    from: index,
                    to: exit,
                    pattern: String::new(),
                });
            }
        }
        Some(format!(
            "^(?:{})$",
            RegularLanguage {
                transitions: edges,
                start: entry,
                end: exit,
                states: states.len()
            }
            .eliminate()?
        ))
    }
}

fn pattern_automaton(pattern: &str) -> Option<dense::DFA<Vec<u32>>> {
    if !crate::pattern_dialect::portable_ascii_pattern(pattern) {
        return None;
    }
    dense::Builder::new()
        .syntax(
            regex_automata::util::syntax::Config::new()
                .unicode(false)
                .utf8(false)
                .crlf(true),
        )
        .configure(
            dense::Config::new()
                .dfa_size_limit(Some(1024 * 1024))
                .determinize_size_limit(Some(4 * 1024 * 1024)),
        )
        .build(&format!(r"\A[\s\S]*(?:{pattern})[\s\S]*\z"))
        .ok()
}

/// A regular expression labelling one directed edge.
pub struct RegularTransition {
    /// Source vertex.
    pub from: usize,
    /// Destination vertex.
    pub to: usize,
    /// Edge language; an empty string is epsilon.
    pub pattern: String,
}

/// A finite labelled graph with two retained boundary vertices.
pub struct RegularLanguage {
    /// Directed edges, including epsilon transitions.
    pub transitions: Vec<RegularTransition>,
    /// Entry vertex outside the eliminated range.
    pub start: usize,
    /// Exit vertex outside the eliminated range.
    pub end: usize,
    /// Vertices `0..states` are eliminated in reverse order.
    pub states: usize,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Vertices {
    from: usize,
    to: usize,
}

impl RegularLanguage {
    /// Eliminates intermediate vertices, retaining the entry-to-exit language.
    ///
    /// Expressions exceeding 16 KiB abstain; absent paths have no language.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn eliminate(self) -> Option<String> {
        let Self {
            transitions,
            start,
            end,
            states,
        } = self;
        let mut edges = BTreeMap::new();
        for RegularTransition { from, to, pattern } in transitions {
            connect(&mut edges, Vertices { from, to }, pattern)?;
        }
        for removed in (0..states).rev() {
            let repeated = edges
                .get(&Vertices {
                    from: removed,
                    to: removed,
                })
                .map_or_else(String::new, |text| format!("(?:{text})*"));
            let vertices = (0..removed).chain([start, end]);
            for from in vertices.clone() {
                let Some(before) = edges.get(&Vertices { from, to: removed }).cloned() else {
                    continue;
                };
                for to in vertices.clone() {
                    if let Some(after) = edges.get(&Vertices { from: removed, to }) {
                        let pattern = format!("{before}{repeated}{after}");
                        connect(&mut edges, Vertices { from, to }, pattern)?;
                    }
                }
            }
            edges.retain(|edge, _| edge.from != removed && edge.to != removed);
        }
        edges.remove(&Vertices {
            from: start,
            to: end,
        })
    }
}

fn connect(edges: &mut BTreeMap<Vertices, String>, edge: Vertices, pattern: String) -> Option<()> {
    let pattern = match edges.remove(&edge) {
        Some(previous) if previous != pattern => format!("(?:{previous}|{pattern})"),
        Some(previous) => previous,
        None => pattern,
    };
    if pattern.len() > 16 * 1024 {
        return None;
    }
    edges.insert(edge, pattern);
    Some(())
}

/// Encodes a finite character set in the regex dialect shared by schema consumers.
/// Empty sets abstain rather than emitting an invalid character class.
#[must_use]
pub fn regex_character_class(characters: impl IntoIterator<Item = char>) -> Option<String> {
    let mut characters = characters.into_iter().peekable();
    characters.peek()?;
    let mut class = String::from("[");
    while let Some(first) = characters.next() {
        let mut last = first;
        while characters
            .peek()
            .is_some_and(|next| u32::from(*next) == u32::from(last) + 1)
        {
            last = characters.next()?;
        }
        class_character(&mut class, first)?;
        if last != first {
            class.push('-');
            class_character(&mut class, last)?;
        }
    }
    class.push(']');
    Some(class)
}

fn class_character(output: &mut String, character: char) -> Option<()> {
    if character <= '\u{ff}' {
        write!(output, r"\x{:02x}", u32::from(character)).ok()?;
    } else {
        output.push(character);
    }
    Some(())
}

#[cfg(test)]
#[path = "tests/text_preimage.rs"]
mod tests;
