//! YAML documents decoded the way Helm hands them to Kubernetes.
//!
//! Helm builds resources with `sigs.k8s.io/yaml`: go-yaml v2 decodes each
//! document with YAML 1.1 rules into Go values, which are then converted to
//! JSON. [`helm_yaml_documents`] replays that pipeline over yaml-rust's event
//! stream, following `go.yaml.in/yaml/v2` v2.4.3 (`resolve.go`, `decode.go`)
//! and `sigs.k8s.io/yaml` v1.6.0 (`convertToJSONableObject`):
//! - a plain scalar resolves by go-yaml v2's table: `y`, `yes` and `on` (and
//!   their case variants) are booleans, `012` is octal, `_` is dropped from
//!   numbers, and timestamps and sexagesimals stay strings;
//! - `<<` merges a mapping, or a sequence of mappings of which earlier ones
//!   win, into the enclosing mapping where it stands, so it overrides the
//!   keys before it and the keys after it override it;
//! - boolean and integer keys are stringified.
//!
//! What this reader cannot reproduce is reported, never guessed. An
//! explicitly tagged scalar (bar the `!!merge` tag of a merge key), a float
//! key and keys whose stringified forms collide (Go's map order then
//! decides) are unknown values. A document Helm
//! refuses (`.inf` or `.nan`, a null or out-of-range key, a collection key,
//! a malformed merge, excessive aliasing), one nested deeper than
//! [`MAX_DEPTH`] and one yaml-rust cannot parse are refused.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Number, Value};
use yaml_rust::parser::{Event, Parser};
use yaml_rust::scanner::{TScalarStyle, TokenType};

use crate::yaml_dialect::{YAML_1_1_BOOLEANS, yaml_style_float};

/// The deepest collection nesting decoded, as `serde_yaml` allows.
const MAX_DEPTH: usize = 128;

/// One YAML document as Helm decodes it.
#[derive(Clone, Debug, PartialEq)]
pub enum HelmYamlDocument {
    /// The document's JSON value. Each JSON pointer in `unknown` names a
    /// value this reader cannot reproduce; it is `null` in `value`.
    Decoded {
        /// The decoded value.
        value: Value,
        /// Pointers to the values that stay unknown, in document order.
        unknown: Vec<String>,
    },
    /// Helm refuses the document, or this reader cannot follow it.
    Refused,
}

/// How Helm reads a mapping key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelmYamlKey {
    /// The JSON object key.
    Key(String),
    /// The merge key `<<`.
    Merge,
    /// A key this reader cannot reproduce, or Helm refuses.
    Unknown,
}

/// Every document of `source` as Helm decodes it. A parse error refuses the
/// document it occurs in and ends the stream.
#[must_use]
#[tracing::instrument(skip_all, fields(bytes = source.len()))]
pub fn helm_yaml_documents(source: &str) -> Vec<HelmYamlDocument> {
    load(source)
        .into_iter()
        .map(|tree| match tree {
            Some(Tree {
                nodes,
                root: Some(root),
            }) => decode_document(&nodes, root),
            Some(Tree { root: None, .. }) => HelmYamlDocument::Decoded {
                value: Value::Null,
                unknown: Vec::new(),
            },
            None => HelmYamlDocument::Refused,
        })
        .collect()
}

/// How Helm reads `source`, a single scalar, as a mapping key.
#[must_use]
pub fn helm_yaml_key(source: &str) -> HelmYamlKey {
    let trees = load(source);
    let [
        Some(Tree {
            nodes,
            root: Some(root),
        }),
    ] = trees.as_slice()
    else {
        return HelmYamlKey::Unknown;
    };
    let Some(YamlNode::Scalar(scalar)) = nodes.get(*root) else {
        return HelmYamlKey::Unknown;
    };
    if is_merge_key(scalar) {
        return HelmYamlKey::Merge;
    }
    match json_key(go_key(resolve_scalar(scalar), 0)) {
        Some(key) => HelmYamlKey::Key(key),
        None => HelmYamlKey::Unknown,
    }
}

/// Each document's tree, `None` for one that cannot be built. yaml-rust's
/// `load` would stop the stream at an alias of an earlier document's
/// anchor; Helm splits documents first, so only that document fails.
fn load(source: &str) -> Vec<Option<Tree>> {
    let mut loader = Loader::default();
    let mut parser = Parser::new(source.chars());
    loop {
        match parser.next() {
            Ok((Event::StreamEnd, _)) => break,
            Ok((event, _)) => loader.on_event(event),
            Err(_) => {
                loader.trees.push(None);
                break;
            }
        }
    }
    loader.trees
}

/// One document's YAML tree.
struct Tree {
    nodes: Vec<YamlNode>,
    root: Option<usize>,
}

/// A node of one document's YAML tree; children are arena indices.
enum YamlNode {
    Scalar(Scalar),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
    /// An alias of the completed node it names.
    Alias(usize),
}

struct Scalar {
    text: String,
    plain: bool,
    tag: ScalarTag,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarTag {
    None,
    /// `!!merge`, which makes `<<` a merge key whatever its style.
    Merge,
    Other,
}

enum Frame {
    Sequence {
        anchor: usize,
        items: Vec<usize>,
    },
    Mapping {
        anchor: usize,
        entries: Vec<(usize, usize)>,
        key: Option<usize>,
    },
}

/// Builds each document's tree from yaml-rust's events.
#[derive(Default)]
struct Loader {
    nodes: Vec<YamlNode>,
    /// yaml-rust anchor ids of completed nodes. An alias of an anchor whose
    /// node is still open is the self-reference go-yaml refuses.
    anchors: HashMap<usize, usize>,
    frames: Vec<Frame>,
    root: Option<usize>,
    refused: bool,
    trees: Vec<Option<Tree>>,
}

impl Loader {
    fn complete(&mut self, node: YamlNode, anchor: usize) {
        let index = self.nodes.len();
        self.nodes.push(node);
        if anchor != 0 {
            self.anchors.insert(anchor, index);
        }
        match self.frames.last_mut() {
            Some(Frame::Sequence { items, .. }) => items.push(index),
            Some(Frame::Mapping { entries, key, .. }) => match key.take() {
                Some(key) => entries.push((key, index)),
                None => *key = Some(index),
            },
            None => self.root = Some(index),
        }
    }

    fn finish_document(&mut self) {
        let tree = Tree {
            nodes: std::mem::take(&mut self.nodes),
            root: self.root.take(),
        };
        self.trees.push((!self.refused).then_some(tree));
        self.anchors.clear();
        self.frames.clear();
        self.refused = false;
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Scalar(text, style, anchor, tag) => {
                let scalar = Scalar {
                    text,
                    plain: style == TScalarStyle::Plain,
                    tag: match tag {
                        None => ScalarTag::None,
                        Some(TokenType::Tag(handle, suffix))
                            if (handle == "!!" && suffix == "merge")
                                || (handle.is_empty() && suffix == "tag:yaml.org,2002:merge") =>
                        {
                            ScalarTag::Merge
                        }
                        Some(_) => ScalarTag::Other,
                    },
                };
                self.complete(YamlNode::Scalar(scalar), anchor);
            }
            Event::Alias(anchor) => {
                let target = self.anchors.get(&anchor).copied();
                self.refused |= target.is_none();
                let node = target.map_or_else(|| YamlNode::Sequence(Vec::new()), YamlNode::Alias);
                self.complete(node, 0);
            }
            Event::SequenceStart(anchor) => self.frames.push(Frame::Sequence {
                anchor,
                items: Vec::new(),
            }),
            Event::MappingStart(anchor) => self.frames.push(Frame::Mapping {
                anchor,
                entries: Vec::new(),
                key: None,
            }),
            Event::SequenceEnd | Event::MappingEnd => match self.frames.pop() {
                Some(Frame::Sequence { anchor, items }) => {
                    self.complete(YamlNode::Sequence(items), anchor);
                }
                Some(Frame::Mapping {
                    anchor,
                    entries,
                    key,
                }) => {
                    self.refused |= key.is_some();
                    self.complete(YamlNode::Mapping(entries), anchor);
                }
                None => self.refused = true,
            },
            Event::DocumentEnd => self.finish_document(),
            Event::Nothing | Event::StreamStart | Event::StreamEnd | Event::DocumentStart => {}
        }
    }
}

/// Helm refuses the document.
struct Refused;

/// A decoded Go value, before its JSON conversion.
enum Decoded {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),
    String(String),
    Sequence(Vec<Decoded>),
    Mapping(BTreeMap<GoKey, Decoded>),
    Unknown,
}

/// A Go map key: equal keys overwrite each other.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GoKey {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    /// The bits of a float that is not NaN, with `-0` as `0`.
    Float(u64),
    String(String),
    /// A key this reader cannot reproduce, distinct from every other key.
    Unknown(usize),
}

fn decode_document(nodes: &[YamlNode], root: usize) -> HelmYamlDocument {
    let mut decoder = Decoder {
        nodes,
        decode_count: 1,
        alias_count: 0,
        alias_depth: 0,
        depth: 0,
        unknown_keys: 0,
    };
    let mut unknown = Vec::new();
    let value = decoder
        .unmarshal(root)
        .and_then(|decoded| to_json(decoded, &mut String::new(), &mut unknown));
    match value {
        Ok(value) => HelmYamlDocument::Decoded { value, unknown },
        Err(Refused) => HelmYamlDocument::Refused,
    }
}

/// go-yaml v2's decoder into `interface{}`, alias accounting included.
struct Decoder<'nodes> {
    nodes: &'nodes [YamlNode],
    /// go-yaml v2's counters; they saturate far beyond its aliasing limit.
    decode_count: u32,
    alias_count: u32,
    alias_depth: usize,
    depth: usize,
    unknown_keys: usize,
}

impl Decoder<'_> {
    /// go-yaml v2's `unmarshal` prologue: it refuses excessive aliasing.
    fn count(&mut self) -> Result<(), Refused> {
        self.decode_count = self.decode_count.saturating_add(1);
        if self.alias_depth > 0 {
            self.alias_count = self.alias_count.saturating_add(1);
        }
        let excessive = self.alias_count > 100
            && self.decode_count > 1000
            && f64::from(self.alias_count) / f64::from(self.decode_count)
                > allowed_alias_ratio(self.decode_count);
        if excessive { Err(Refused) } else { Ok(()) }
    }

    fn enter(&mut self) -> Result<(), Refused> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(Refused)
        } else {
            Ok(())
        }
    }

    fn unmarshal(&mut self, index: usize) -> Result<Decoded, Refused> {
        self.count()?;
        let nodes = self.nodes;
        match nodes.get(index).ok_or(Refused)? {
            YamlNode::Alias(target) => {
                self.alias_depth += 1;
                let decoded = self.unmarshal(*target);
                self.alias_depth -= 1;
                decoded
            }
            YamlNode::Scalar(scalar) => Ok(resolve_scalar(scalar)),
            YamlNode::Sequence(items) => {
                self.enter()?;
                let mut decoded = Vec::with_capacity(items.len());
                for &item in items {
                    decoded.push(self.unmarshal(item)?);
                }
                self.depth -= 1;
                Ok(Decoded::Sequence(decoded))
            }
            YamlNode::Mapping(entries) => {
                self.enter()?;
                let mut map = BTreeMap::new();
                self.fill(entries, &mut map)?;
                self.depth -= 1;
                Ok(Decoded::Mapping(map))
            }
        }
    }

    /// Decode a mapping's entries into `map`, applying merges in place.
    fn fill(
        &mut self,
        entries: &[(usize, usize)],
        map: &mut BTreeMap<GoKey, Decoded>,
    ) -> Result<(), Refused> {
        let nodes = self.nodes;
        for &(key, value) in entries {
            if let Some(YamlNode::Scalar(scalar)) = nodes.get(key)
                && is_merge_key(scalar)
            {
                self.merge(value, map)?;
                continue;
            }
            let key = match self.unmarshal(key)? {
                Decoded::Sequence(_) | Decoded::Mapping(_) => return Err(Refused),
                key => {
                    self.unknown_keys += 1;
                    go_key(key, self.unknown_keys)
                }
            };
            let value = self.unmarshal(value)?;
            map.insert(key, value);
        }
        Ok(())
    }

    /// go-yaml v2's `merge`: a mapping, or each mapping of a sequence from
    /// the last to the first, decoded into `map`.
    fn merge(&mut self, value: usize, map: &mut BTreeMap<GoKey, Decoded>) -> Result<(), Refused> {
        let nodes = self.nodes;
        let is_mapping = |index: usize| {
            let node = match nodes.get(index) {
                Some(YamlNode::Alias(target)) => nodes.get(*target),
                node => node,
            };
            matches!(node, Some(YamlNode::Mapping(_)))
        };
        match nodes.get(value) {
            Some(YamlNode::Sequence(items)) => {
                for &item in items.iter().rev() {
                    if !is_mapping(item) {
                        return Err(Refused);
                    }
                    self.unmarshal_into(item, map)?;
                }
                Ok(())
            }
            _ if is_mapping(value) => self.unmarshal_into(value, map),
            _ => Err(Refused),
        }
    }

    fn unmarshal_into(
        &mut self,
        index: usize,
        map: &mut BTreeMap<GoKey, Decoded>,
    ) -> Result<(), Refused> {
        self.count()?;
        let nodes = self.nodes;
        match nodes.get(index) {
            Some(YamlNode::Alias(target)) => {
                self.alias_depth += 1;
                let merged = self.unmarshal_into(*target, map);
                self.alias_depth -= 1;
                merged
            }
            Some(YamlNode::Mapping(entries)) => {
                self.enter()?;
                self.fill(entries, map)?;
                self.depth -= 1;
                Ok(())
            }
            Some(YamlNode::Scalar(_) | YamlNode::Sequence(_)) | None => Err(Refused),
        }
    }
}

/// go-yaml v2's `allowedAliasRatio`.
fn allowed_alias_ratio(decode_count: u32) -> f64 {
    const LOW: u32 = 400_000;
    const HIGH: u32 = 4_000_000;
    match decode_count {
        ..=LOW => 0.99,
        HIGH.. => 0.10,
        _ => 0.99 - 0.89 * (f64::from(decode_count - LOW) / f64::from(HIGH - LOW)),
    }
}

/// go-yaml v2's `isMerge`: `<<`, plain and untagged or tagged `!!merge`.
fn is_merge_key(scalar: &Scalar) -> bool {
    scalar.text == "<<"
        && match scalar.tag {
            ScalarTag::None => scalar.plain,
            ScalarTag::Merge => true,
            ScalarTag::Other => false,
        }
}

fn resolve_scalar(scalar: &Scalar) -> Decoded {
    if scalar.tag != ScalarTag::None {
        Decoded::Unknown
    } else if scalar.plain {
        resolve_plain(&scalar.text)
    } else {
        Decoded::String(scalar.text.clone())
    }
}

/// go-yaml v2's `resolve` of an untagged plain scalar.
fn resolve_plain(text: &str) -> Decoded {
    let string = || Decoded::String(text.to_string());
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Decoded::Null,
        "true" | "True" | "TRUE" => return Decoded::Bool(true),
        "false" | "False" | "FALSE" => return Decoded::Bool(false),
        ".nan" | ".NaN" | ".NAN" => return Decoded::Float(f64::NAN),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => {
            return Decoded::Float(f64::INFINITY);
        }
        "-.inf" | "-.Inf" | "-.INF" => return Decoded::Float(f64::NEG_INFINITY),
        _ if YAML_1_1_BOOLEANS.contains(&text) => {
            let truthy = text.starts_with(['y', 'Y']) || text.eq_ignore_ascii_case("on");
            return Decoded::Bool(truthy);
        }
        _ => {}
    }
    match text.as_bytes().first() {
        Some(b'.') => go_parse_float(text).map_or_else(string, Decoded::Float),
        Some(b'0'..=b'9' | b'+' | b'-') => {
            let plain: String = text.chars().filter(|&c| c != '_').collect();
            if let Some(value) = go_parse_int(&plain, 0) {
                return Decoded::Int(value);
            }
            if let Some(value) = go_parse_uint(&plain, 0) {
                return Decoded::Uint(value);
            }
            if yaml_style_float(&plain)
                && let Some(value) = finite(&plain)
            {
                return Decoded::Float(value);
            }
            // go-yaml v2's explicit binary fallbacks, reached when the
            // base-prefixed reading fails.
            if let Some(digits) = plain.strip_prefix("0b") {
                if let Some(value) = go_parse_int(digits, 2) {
                    return Decoded::Int(value);
                }
                if let Some(value) = go_parse_uint(digits, 2) {
                    return Decoded::Uint(value);
                }
            } else if let Some(digits) = plain.strip_prefix("-0b")
                && let Some(value) = go_parse_int(&format!("-{digits}"), 2)
            {
                return Decoded::Int(value);
            }
            string()
        }
        _ => string(),
    }
}

/// Go's `strconv.ParseFloat` of a scalar starting with `.`: underscores may
/// only separate digits; an out-of-range value is an error.
fn go_parse_float(text: &str) -> Option<f64> {
    if !go_underscores_ok(text) {
        return None;
    }
    let plain: String = text.chars().filter(|&c| c != '_').collect();
    if !yaml_style_float(&plain) {
        return None;
    }
    finite(&plain)
}

/// Go's `underscoreOK` for a decimal number: every `_` sits between digits.
fn go_underscores_ok(text: &str) -> bool {
    let mut previous = None;
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte == b'_'
            && !(previous.is_some_and(|previous: u8| previous.is_ascii_digit())
                && bytes.peek().is_some_and(u8::is_ascii_digit))
        {
            return false;
        }
        previous = Some(byte);
    }
    true
}

/// A decimal float that Go parses in range, as Rust parses it: both round
/// correctly, so the values agree.
fn finite(plain: &str) -> Option<f64> {
    plain.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Go's `strconv.ParseInt(s, base, 64)`.
fn go_parse_int(text: &str, base: u32) -> Option<i64> {
    let (negative, magnitude) = match text.as_bytes().first()? {
        b'+' => (false, text.get(1..)?),
        b'-' => (true, text.get(1..)?),
        _ => (false, text),
    };
    let magnitude = go_parse_uint(magnitude, base)?;
    if negative {
        (magnitude <= 1 << 63).then(|| 0_i64.wrapping_sub_unsigned(magnitude))
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// Go's `strconv.ParseUint(s, base, 64)` for `s` without underscores; base
/// 0 reads a `0b`, `0o` or `0x` prefix, and a leading `0` as octal.
fn go_parse_uint(text: &str, base: u32) -> Option<u64> {
    if text.is_empty() {
        return None;
    }
    let (digits, radix) = match (base, text.as_bytes()) {
        (0, [b'0', prefix, _, ..]) if prefix.eq_ignore_ascii_case(&b'b') => (text.get(2..)?, 2),
        (0, [b'0', prefix, _, ..]) if prefix.eq_ignore_ascii_case(&b'o') => (text.get(2..)?, 8),
        (0, [b'0', prefix, _, ..]) if prefix.eq_ignore_ascii_case(&b'x') => (text.get(2..)?, 16),
        (0, [b'0', ..]) => (text.get(1..)?, 8),
        (0, _) => (text, 10),
        (base, _) => (text, base),
    };
    let mut value: u64 = 0;
    for c in digits.chars() {
        let digit = c.to_digit(36).filter(|&digit| digit < radix)?;
        value = value
            .checked_mul(u64::from(radix))?
            .checked_add(u64::from(digit))?;
    }
    Some(value)
}

fn go_key(decoded: Decoded, unknown: usize) -> GoKey {
    match decoded {
        Decoded::Null => GoKey::Null,
        Decoded::Bool(value) => GoKey::Bool(value),
        Decoded::Int(value) => GoKey::Int(value),
        Decoded::Uint(value) => GoKey::Uint(value),
        Decoded::Float(value) if !value.is_nan() => GoKey::Float((value + 0.0).to_bits()),
        Decoded::String(value) => GoKey::String(value),
        Decoded::Float(_) | Decoded::Sequence(_) | Decoded::Mapping(_) | Decoded::Unknown => {
            GoKey::Unknown(unknown)
        }
    }
}

/// The JSON key `sigs.k8s.io/yaml` makes of a Go key; `None` for a float
/// key, whose `%g` float32 spelling is not reproduced, and for the null and
/// out-of-range keys it refuses.
fn json_key(key: GoKey) -> Option<String> {
    match key {
        GoKey::String(key) => Some(key),
        GoKey::Bool(key) => Some(key.to_string()),
        GoKey::Int(key) => Some(key.to_string()),
        GoKey::Null | GoKey::Uint(_) | GoKey::Float(_) | GoKey::Unknown(_) => None,
    }
}

/// `convertToJSONableObject` and `json.Marshal`: keys are stringified, and
/// a non-finite float refuses the document.
fn to_json(
    decoded: Decoded,
    pointer: &mut String,
    unknown: &mut Vec<String>,
) -> Result<Value, Refused> {
    Ok(match decoded {
        Decoded::Null => Value::Null,
        Decoded::Bool(value) => Value::Bool(value),
        Decoded::Int(value) => Value::from(value),
        Decoded::Uint(value) => Value::from(value),
        Decoded::Float(value) => Value::Number(Number::from_f64(value).ok_or(Refused)?),
        Decoded::String(value) => Value::String(value),
        Decoded::Unknown => {
            unknown.push(pointer.clone());
            Value::Null
        }
        Decoded::Sequence(items) => {
            let mut array = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&index.to_string());
                array.push(to_json(item, pointer, unknown)?);
                pointer.truncate(length);
            }
            Value::Array(array)
        }
        Decoded::Mapping(map) => {
            let mut object = Map::new();
            let mut nested_unknown = Vec::new();
            let mut keys_known = true;
            for (key, value) in map {
                if matches!(key, GoKey::Null | GoKey::Uint(_)) {
                    return Err(Refused);
                }
                let length = pointer.len();
                let key = json_key(key);
                if let Some(key) = &key {
                    pointer.push('/');
                    pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                }
                let value = to_json(value, pointer, &mut nested_unknown)?;
                pointer.truncate(length);
                match key {
                    Some(key) if !object.contains_key(&key) => {
                        object.insert(key, value);
                    }
                    // A float key, or Go's map order picks among colliding keys.
                    Some(_) | None => keys_known = false,
                }
            }
            if keys_known {
                unknown.append(&mut nested_unknown);
                Value::Object(object)
            } else {
                unknown.push(pointer.clone());
                Value::Null
            }
        }
    })
}
