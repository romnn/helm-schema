//! Helm's `values.Options.MergeValues` (`pkg/cli/values/options.go`) and
//! the `strvals` parser (`pkg/strvals/parser.go`) behind `--set`,
//! `--set-string` and `--set-json`.
//!
//! The groups apply in Helm's fixed order whatever their order on the
//! command line: `-f` files, then `--set-json`, `--set`, `--set-string`.
//!
//! `--set-file` and `--set-literal` are not modelled: [`ValuesOptions`] has
//! no field for them, so it cannot accept them. A command-line adapter built
//! on it must reject those flags, never drop them.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::ValuesError;
use super::chart::merge_maps;
use super::yaml;

/// `strvals.MaxIndex` and `strvals.MaxNestedNameLevel`.
const MAX_INDEX: usize = 65_536;
const MAX_NESTED_NAME_LEVEL: usize = 30;

/// The largest integer a float64 holds exactly; `--set-json` decodes every
/// number to float64.
const MAX_EXACT_INTEGER: u64 = 1 << 53;

/// The value flags of a Helm command line this port parses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValuesOptions {
    /// `-f`/`--values` files, merged in order; each may hold several
    /// YAML documents.
    pub value_files: Vec<PathBuf>,
    /// `--set-json` arguments.
    pub json_values: Vec<String>,
    /// `--set` arguments.
    pub values: Vec<String>,
    /// `--set-string` arguments.
    pub string_values: Vec<String>,
}

impl ValuesOptions {
    /// `Options.MergeValues`: the override map Helm hands the coalescer,
    /// nulls retained.
    ///
    /// # Errors
    ///
    /// Returns [`ValuesError::NotValidated`] where Helm rejects a flag,
    /// [`ValuesError::Unmodelled`] for inputs Helm decodes differently from
    /// this port, and [`ValuesError::Io`] when a values file is unreadable.
    pub fn merge_values(&self) -> Result<Value, ValuesError> {
        let mut base = Map::new();
        for path in &self.value_files {
            let bytes = std::fs::read(path).map_err(|source| ValuesError::Io {
                path: path.display().to_string(),
                source,
            })?;
            for document in yaml::documents(&bytes, &path.display().to_string())? {
                match document {
                    Value::Null => {}
                    Value::Object(document) => merge_maps(&mut base, document),
                    _ => {
                        return Err(ValuesError::NotValidated(format!(
                            "failed to parse {}: not a map",
                            path.display()
                        )));
                    }
                }
            }
        }
        for value in &self.json_values {
            let trimmed = value.trim();
            if trimmed.starts_with('{') {
                let Ok(Value::Object(object)) = serde_json::from_str::<Value>(trimmed) else {
                    return Err(ValuesError::NotValidated(format!(
                        "failed parsing --set-json data JSON: {value}"
                    )));
                };
                merge_maps(&mut base, object);
            } else {
                Parser::new(value, Mode::Json)
                    .parse(&mut base)
                    .map_err(|_| {
                        ValuesError::NotValidated(format!("failed parsing --set-json data {value}"))
                    })?;
            }
        }
        refuse_inexact_numbers(&Value::Object(base.clone()))?;
        for value in &self.values {
            Parser::new(value, Mode::Typed)
                .parse(&mut base)
                .map_err(|error| {
                    ValuesError::NotValidated(format!("failed parsing --set data: {error}"))
                })?;
        }
        for value in &self.string_values {
            Parser::new(value, Mode::String)
                .parse(&mut base)
                .map_err(|error| {
                    ValuesError::NotValidated(format!("failed parsing --set-string data: {error}"))
                })?;
        }
        Ok(Value::Object(base))
    }
}

/// `-f` files and `--set-json` decode numbers to float64; an integer they
/// would round is refused rather than carried exactly.
fn refuse_inexact_numbers(value: &Value) -> Result<(), ValuesError> {
    match value {
        Value::Number(number) => {
            let exact = match (number.as_u64(), number.as_i64()) {
                (Some(unsigned), _) => unsigned <= MAX_EXACT_INTEGER,
                (None, Some(signed)) => signed.unsigned_abs() <= MAX_EXACT_INTEGER,
                (None, None) => true,
            };
            if exact {
                Ok(())
            } else {
                Err(ValuesError::Unmodelled(format!(
                    "the number {number}, beyond float64 precision"
                )))
            }
        }
        Value::Array(items) => items.iter().try_for_each(refuse_inexact_numbers),
        Value::Object(entries) => entries.values().try_for_each(refuse_inexact_numbers),
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `--set`: `typedVal` coerces booleans, null and integers.
    Typed,
    /// `--set-string`: every value is a string.
    String,
    /// `--set-json`: every value is JSON.
    Json,
}

/// How a strvals step stopped: the input ended (Go's `io.EOF`) or failed.
enum Stop {
    End,
    Failed(String),
}

enum ListStop {
    End,
    NotList,
    Failed(String),
}

struct Parser {
    chars: Vec<char>,
    position: usize,
    mode: Mode,
}

impl Parser {
    fn new(text: &str, mode: Mode) -> Self {
        Self {
            chars: text.chars().collect(),
            position: 0,
            mode,
        }
    }

    fn parse(&mut self, data: &mut Map<String, Value>) -> Result<(), String> {
        loop {
            match self.key(data, 0) {
                Ok(()) => {}
                Err(Stop::End) => return Ok(()),
                Err(Stop::Failed(error)) => return Err(error),
            }
        }
    }

    fn read(&mut self) -> Option<char> {
        let character = self.chars.get(self.position).copied()?;
        self.position += 1;
        Some(character)
    }

    /// `runesUntil`: the unescaped text up to a stop character, and that
    /// character; `None` at the end of input.
    fn until(&mut self, stop: &[char]) -> (String, Option<char>) {
        let mut text = String::new();
        loop {
            match self.read() {
                None => return (text, None),
                Some(character) if stop.contains(&character) => return (text, Some(character)),
                Some('\\') => match self.read() {
                    None => return (text, None),
                    Some(escaped) => text.push(escaped),
                },
                Some(character) => text.push(character),
            }
        }
    }

    fn key(&mut self, data: &mut Map<String, Value>, level: usize) -> Result<(), Stop> {
        let (key, terminator) = self.until(&['=', '[', ',', '.']);
        match terminator {
            None if key.is_empty() => Err(Stop::End),
            None => Err(Stop::Failed(format!("key {key:?} has no value"))),
            Some('[') => {
                let index = self.key_index()?;
                let list = match data.get(&key) {
                    None => Vec::new(),
                    Some(Value::Array(list)) => list.clone(),
                    Some(_) => {
                        return Err(Stop::Failed(format!(
                            "unable to parse key: {key:?} is not a list"
                        )));
                    }
                };
                let (list, result) = self.list_item(list, index, level);
                set(data, &key, Value::Array(list));
                result
            }
            Some('=') => self.key_value(data, &key),
            Some(',') => {
                set(data, &key, Value::String(String::new()));
                Err(Stop::Failed(format!(
                    "key {key:?} has no value (cannot end with ,)"
                )))
            }
            Some(_) => {
                let level = level + 1;
                if level > MAX_NESTED_NAME_LEVEL {
                    return Err(Stop::Failed(format!(
                        "value name nested level is greater than maximum supported nested level of {MAX_NESTED_NAME_LEVEL}"
                    )));
                }
                let mut inner = match data.get(&key) {
                    None => Map::new(),
                    Some(Value::Object(inner)) => inner.clone(),
                    Some(_) => {
                        return Err(Stop::Failed(format!(
                            "unable to parse key: {key:?} is not a map"
                        )));
                    }
                };
                let result = self.key(&mut inner, level);
                if result.is_ok() && inner.is_empty() {
                    return Err(Stop::Failed(format!("key map {key:?} has no value")));
                }
                if !inner.is_empty() {
                    set(data, &key, Value::Object(inner));
                }
                result
            }
        }
    }

    fn key_value(&mut self, data: &mut Map<String, Value>, key: &str) -> Result<(), Stop> {
        if self.mode == Mode::Json {
            let value = self.json_value()?;
            set(data, key, value);
            return Ok(());
        }
        match self.value_list() {
            Ok(list) => {
                set(data, key, Value::Array(list));
                Ok(())
            }
            Err(ListStop::End) => {
                set(data, key, Value::String(String::new()));
                Err(Stop::End)
            }
            Err(ListStop::NotList) => {
                let (text, _) = self.until(&[',']);
                set(data, key, self.typed(text));
                Ok(())
            }
            Err(ListStop::Failed(error)) => Err(Stop::Failed(error)),
        }
    }

    /// A `--set-json` value: empty is null, otherwise one JSON value.
    fn json_value(&mut self) -> Result<Value, Stop> {
        if self.empty_value() {
            return Ok(Value::Null);
        }
        let rest: String = self
            .chars
            .get(self.position..)
            .unwrap_or_default()
            .iter()
            .collect();
        let mut values = serde_json::Deserializer::from_str(&rest).into_iter::<Value>();
        let value = match values.next() {
            Some(Ok(value)) => value,
            Some(Err(error)) => return Err(Stop::Failed(error.to_string())),
            None => return Err(Stop::Failed("missing JSON value".to_string())),
        };
        let consumed = rest
            .get(..values.byte_offset())
            .unwrap_or_default()
            .chars()
            .count();
        self.position += consumed;
        self.empty_value();
        Ok(value)
    }

    fn key_index(&mut self) -> Result<usize, Stop> {
        let (index, terminator) = self.until(&[']']);
        if terminator.is_none() {
            return Err(Stop::Failed("error parsing index: EOF".to_string()));
        }
        index
            .parse::<i64>()
            .map_err(|error| Stop::Failed(format!("error parsing index: {error}")))
            .and_then(|index| {
                usize::try_from(index)
                    .map_err(|_| Stop::Failed(format!("negative {index} index not allowed")))
            })
    }

    fn list_item(
        &mut self,
        mut list: Vec<Value>,
        index: usize,
        level: usize,
    ) -> (Vec<Value>, Result<(), Stop>) {
        let (rest, terminator) = self.until(&['[', '.', '=']);
        if !rest.is_empty() {
            return (
                list,
                Err(Stop::Failed(format!(
                    "unexpected data at end of array index: {rest:?}"
                ))),
            );
        }
        match terminator {
            None => (list, Err(Stop::End)),
            Some('=') => {
                let value = if self.mode == Mode::Json {
                    match self.json_value() {
                        Ok(value) => value,
                        Err(stop) => return (list, Err(stop)),
                    }
                } else {
                    match self.value_list() {
                        Ok(items) => Value::Array(items),
                        Err(ListStop::End) => Value::String(String::new()),
                        Err(ListStop::NotList) => {
                            let (text, _) = self.until(&[',']);
                            self.typed(text)
                        }
                        Err(ListStop::Failed(error)) => return (list, Err(Stop::Failed(error))),
                    }
                };
                set_index(list, index, value)
            }
            Some('[') => {
                let next = match self.key_index() {
                    Ok(next) => next,
                    Err(stop) => return (list, Err(stop)),
                };
                let nested = match list.get(index) {
                    None | Some(Value::Null) => Vec::new(),
                    Some(Value::Array(nested)) => nested.clone(),
                    Some(_) => {
                        return (
                            list,
                            Err(Stop::Failed("unable to parse key: not a list".to_string())),
                        );
                    }
                };
                let (nested, result) = self.list_item(nested, next, level);
                if result.is_err() {
                    return (list, result);
                }
                set_index(list, index, Value::Array(nested))
            }
            Some(_) => {
                let mut inner = match list.get_mut(index) {
                    Some(Value::Object(inner)) => inner.clone(),
                    Some(slot) => {
                        *slot = Value::Object(Map::new());
                        Map::new()
                    }
                    None => Map::new(),
                };
                let result = self.key(&mut inner, level);
                if result.is_err() {
                    return (list, result);
                }
                set_index(list, index, Value::Object(inner))
            }
        }
    }

    /// `emptyVal`: skips blanks; true at a comma or the end of input.
    fn empty_value(&mut self) -> bool {
        loop {
            match self.read() {
                None | Some(',') => return true,
                Some(character) if character.is_whitespace() => {}
                Some(_) => {
                    self.position -= 1;
                    return false;
                }
            }
        }
    }

    /// `valList`: a `{a,b}` list, or why there is none.
    fn value_list(&mut self) -> Result<Vec<Value>, ListStop> {
        match self.read() {
            None => return Err(ListStop::End),
            Some('{') => {}
            Some(_) => {
                self.position -= 1;
                return Err(ListStop::NotList);
            }
        }
        let mut list = Vec::new();
        loop {
            let (text, terminator) = self.until(&[',', '}']);
            match terminator {
                None => {
                    return Err(ListStop::Failed("list must terminate with '}'".to_string()));
                }
                Some('}') => {
                    if let Some(next) = self.read()
                        && next != ','
                    {
                        self.position -= 1;
                    }
                    list.push(self.typed(text));
                    return Ok(list);
                }
                Some(_) => list.push(self.typed(text)),
            }
        }
    }

    /// `typedVal`.
    fn typed(&self, text: String) -> Value {
        if self.mode == Mode::String {
            return Value::String(text);
        }
        if text.eq_ignore_ascii_case("true") {
            return Value::Bool(true);
        }
        if text.eq_ignore_ascii_case("false") {
            return Value::Bool(false);
        }
        if text.eq_ignore_ascii_case("null") {
            return Value::Null;
        }
        if text == "0" {
            return Value::from(0_i64);
        }
        if !text.is_empty()
            && !text.starts_with('0')
            && let Ok(integer) = text.parse::<i64>()
        {
            return Value::from(integer);
        }
        Value::String(text)
    }
}

fn set(data: &mut Map<String, Value>, key: &str, value: Value) {
    if !key.is_empty() {
        data.insert(key.to_string(), value);
    }
}

fn set_index(mut list: Vec<Value>, index: usize, value: Value) -> (Vec<Value>, Result<(), Stop>) {
    if index > MAX_INDEX {
        return (
            list,
            Err(Stop::Failed(format!(
                "index of {index} is greater than maximum supported index of {MAX_INDEX}"
            ))),
        );
    }
    if list.len() <= index {
        list.resize(index + 1, Value::Null);
    }
    if let Some(slot) = list.get_mut(index) {
        *slot = value;
    }
    (list, Ok(()))
}
