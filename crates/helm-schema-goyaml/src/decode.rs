//! Generic node decoding from go-yaml v2.4.3 `decode.go`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::resolve::{decode_error, resolve};
use crate::{Error, ErrorKind, Event, EventKind, Mark, Reader};

/// A value in Go's generic YAML target (`decode.go:221-225,405-481,594-673`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// A nil interface.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed Go integer.
    Int(i64),
    /// An unsigned integer above the signed range.
    Uint(u64),
    /// IEEE-754 bits, retaining NaN, infinity and negative zero.
    Float(u64),
    /// A Go string, represented as decoded Unicode.
    String(String),
    /// A Go string with ill-formed UTF-8, retaining byte identity until JSON encoding.
    RawString(Vec<u8>),
    /// A generic slice.
    Sequence(Vec<Value>),
    /// A generic map, retaining the concrete type of each key.
    Mapping(Vec<Entry>),
}

/// An entry of a generic Go map.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entry {
    /// A comparable scalar key.
    pub key: Value,
    /// The decoded value.
    pub value: Value,
    /// Whether the key and value were supplied directly, without alias or merge expansion.
    pub authored: bool,
}

impl Value {
    /// Retains a floating-point result without imposing JSON restrictions.
    #[must_use]
    pub fn float(value: f64) -> Self {
        Self::Float(value.to_bits())
    }

    /// Produces the oracle protocol's type-preserving canonical tree.
    #[must_use]
    pub fn tagged(&self) -> serde_json::Value {
        match self {
            Self::Null => json!({"kind":"null"}),
            Self::Bool(value) => json!({"kind":"bool","value":value}),
            Self::Int(value) => json!({"kind":"int","value":value.to_string()}),
            Self::Uint(value) => json!({"kind":"uint","value":value.to_string()}),
            Self::Float(bits) => json!({"kind":"float","value":format!("{bits:016x}")}),
            Self::String(value) => json!({"kind":"string","value":value}),
            Self::RawString(value) => {
                use base64::Engine as _;
                json!({"kind":"string","value":go_json_string(value),"bytes":base64::engine::general_purpose::STANDARD.encode(value)})
            }
            Self::Sequence(items) => {
                json!({"kind":"sequence","items":items.iter().map(Self::tagged).collect::<Vec<_>>()})
            }
            Self::Mapping(entries) => {
                let mut entries: Vec<_> = entries
                    .iter()
                    .map(
                        |Entry {
                             key,
                             value,
                             authored: _,
                         }| vec![key.tagged(), value.tagged()],
                    )
                    .collect();
                entries.sort_by_cached_key(|entry| {
                    serde_json::Value::Array(entry.clone()).to_string()
                });
                json!({"kind":"mapping","entries":entries})
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
}

#[derive(Clone, Debug)]
enum NodeKind {
    Scalar {
        tag: String,
        value: String,
        implicit: bool,
    },
    Sequence(Vec<usize>),
    Mapping(Vec<usize>),
    Alias {
        name: String,
        target: usize,
    },
}

#[derive(Default)]
struct Tree {
    nodes: Vec<Node>,
    anchors: BTreeMap<String, usize>,
    stack: Vec<usize>,
    root: Option<usize>,
}

impl Tree {
    /// Builds the node graph and resolves aliases (`decode.go:129-212`).
    fn event(&mut self, event: &Event) -> Result<(), Error> {
        let Event {
            kind,
            start: _,
            end: _,
        } = event;
        let (kind, anchor) = match kind {
            EventKind::Scalar {
                anchor,
                tag,
                value,
                style: _,
                implicit,
            } => (
                NodeKind::Scalar {
                    tag: tag.clone(),
                    value: value.clone(),
                    implicit: *implicit,
                },
                anchor.as_str(),
            ),
            EventKind::SequenceStart {
                anchor,
                tag: _,
                implicit: _,
            } => (NodeKind::Sequence(Vec::new()), anchor.as_str()),
            EventKind::MappingStart {
                anchor,
                tag: _,
                implicit: _,
            } => (NodeKind::Mapping(Vec::new()), anchor.as_str()),
            EventKind::Alias(name) => {
                let target = *self
                    .anchors
                    .get(name)
                    .ok_or_else(|| decode_error(format!("unknown anchor '{name}' referenced")))?;
                (
                    NodeKind::Alias {
                        name: name.clone(),
                        target,
                    },
                    "",
                )
            }
            EventKind::SequenceEnd | EventKind::MappingEnd => {
                self.stack.pop();
                return Ok(());
            }
            EventKind::StreamStart
            | EventKind::StreamEnd
            | EventKind::DocumentStart { implicit: _ }
            | EventKind::DocumentEnd { implicit: _ } => return Ok(()),
        };
        let collection = matches!(kind, NodeKind::Sequence(_) | NodeKind::Mapping(_));
        let id = self.nodes.len();
        self.nodes.push(Node { kind });
        if !anchor.is_empty() {
            self.anchors.insert(anchor.to_owned(), id);
        }
        if let Some(parent) = self.stack.last().and_then(|id| self.nodes.get_mut(*id)) {
            match &mut parent.kind {
                NodeKind::Sequence(children) | NodeKind::Mapping(children) => children.push(id),
                NodeKind::Scalar { .. } | NodeKind::Alias { .. } => {}
            }
        } else {
            self.root = Some(id);
        }
        if collection {
            self.stack.push(id);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Decoder {
    aliases: Vec<usize>,
    decode_count: u64,
    alias_count: u64,
    merging: bool,
}

impl Decoder {
    /// Decodes generic interfaces, maps and slices (`decode.go:333-393,594-673`).
    fn node(&mut self, tree: &Tree, id: usize) -> Result<Value, Error> {
        self.decode_count += 1;
        if !self.aliases.is_empty() {
            self.alias_count += 1;
        }
        if self.alias_count > 100
            && self.decode_count > 1000
            && go_unsigned_float(self.alias_count) / go_unsigned_float(self.decode_count)
                > allowed_alias_ratio(self.decode_count)
        {
            return Err(decode_error("document contains excessive aliasing"));
        }
        let node = tree
            .nodes
            .get(id)
            .ok_or_else(|| decode_error("missing parser node"))?;
        let Node { kind } = node;
        match kind {
            NodeKind::Scalar {
                tag,
                value,
                implicit,
            } => resolve(tag, value, *implicit),
            NodeKind::Alias { name, target } => {
                if self.aliases.contains(target) {
                    return Err(decode_error(format!(
                        "anchor '{name}' value contains itself"
                    )));
                }
                self.aliases.push(*target);
                let value = self.node(tree, *target);
                self.aliases.pop();
                value
            }
            NodeKind::Sequence(children) => children
                .iter()
                .map(|id| self.node(tree, *id))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Sequence),
            NodeKind::Mapping(children) => {
                let mut entries = Vec::new();
                self.mapping(tree, children, &mut entries)?;
                Ok(Value::Mapping(entries))
            }
        }
    }

    /// Applies entries in Go's order, including merge overwrites (`decode.go:649-673,788-815`).
    fn mapping(
        &mut self,
        tree: &Tree,
        children: &[usize],
        entries: &mut Vec<Entry>,
    ) -> Result<(), Error> {
        let (pairs, _) = children.as_chunks::<2>();
        for [key_id, value_id] in pairs {
            if tree.nodes.get(*key_id).is_some_and(|node| {
                matches!(&node.kind,
                NodeKind::Scalar { tag, value, implicit } if is_merge_key(value, tag, *implicit))
            }) {
                self.merge(tree, *value_id, entries)?;
                continue;
            }
            let key = self.node(tree, *key_id)?;
            if matches!(key, Value::Sequence(_) | Value::Mapping(_)) {
                return Err(decode_error("invalid map key"));
            }
            let value = self.node(tree, *value_id)?;
            let authored = !self.merging
                && self.aliases.is_empty()
                && tree
                    .nodes
                    .get(*key_id)
                    .is_some_and(|node| matches!(node.kind, NodeKind::Scalar { .. }))
                && tree
                    .nodes
                    .get(*value_id)
                    .is_some_and(|node| match &node.kind {
                        NodeKind::Alias { .. } => false,
                        NodeKind::Sequence(items) => items.iter().all(|id| {
                            tree.nodes
                                .get(*id)
                                .is_some_and(|item| !matches!(item.kind, NodeKind::Alias { .. }))
                        }),
                        NodeKind::Scalar { .. } | NodeKind::Mapping(_) => true,
                    });
            set_entry(
                entries,
                Entry {
                    key,
                    value,
                    authored,
                },
            );
        }
        Ok(())
    }

    /// Merges maps and reverses sequence precedence (`decode.go:766-812`).
    fn merge(&mut self, tree: &Tree, id: usize, entries: &mut Vec<Entry>) -> Result<(), Error> {
        let node = tree
            .nodes
            .get(id)
            .ok_or_else(|| decode_error("missing merge node"))?;
        match &node.kind {
            NodeKind::Mapping(_) | NodeKind::Alias { .. } => {
                let was_merging = std::mem::replace(&mut self.merging, true);
                let merged = self.node(tree, id);
                self.merging = was_merging;
                let Value::Mapping(merged) = merged? else {
                    return Err(decode_error(
                        "map merge requires map or sequence of maps as the value",
                    ));
                };
                for entry in merged {
                    set_entry(entries, entry);
                }
                Ok(())
            }
            NodeKind::Sequence(children) => {
                for id in children.iter().rev() {
                    if tree
                        .nodes
                        .get(*id)
                        .is_some_and(|node| matches!(node.kind, NodeKind::Sequence(_)))
                    {
                        return Err(decode_error(
                            "map merge requires map or sequence of maps as the value",
                        ));
                    }
                    self.merge(tree, *id, entries)?;
                }
                Ok(())
            }
            NodeKind::Scalar { .. } => Err(decode_error(
                "map merge requires map or sequence of maps as the value",
            )),
        }
    }
}

/// Computes the pinned alias-expansion threshold (`decode.go:309-329`).
fn allowed_alias_ratio(count: u64) -> f64 {
    if count <= 400_000 {
        0.99
    } else if count >= 4_000_000 {
        0.10
    } else {
        0.99 - 0.89 * (go_unsigned_float(count - 400_000) / 3_600_000.0)
    }
}

/// Assigns a Go map entry, preserving type identity and IEEE key equality (`decode.go:675-682`).
fn set_entry(entries: &mut Vec<Entry>, incoming: Entry) {
    let existing = entries
        .iter_mut()
        .find(|entry| match (&entry.key, &incoming.key) {
            (Value::Float(left), Value::Float(right)) => {
                #[expect(
                    clippy::float_cmp,
                    reason = "Go map keys use exact IEEE equality, including NaN and signed zero"
                )]
                let equal = f64::from_bits(*left) == f64::from_bits(*right);
                equal
            }
            (left, right) => left == right,
        });
    if let Some(entry) = existing {
        *entry = incoming;
    } else {
        entries.push(incoming);
    }
}

/// Whether Go treats a scalar mapping key as a YAML merge.
#[must_use]
pub fn is_merge_key(value: &str, tag: &str, implicit: bool) -> bool {
    value == "<<" && (implicit || tag == "tag:yaml.org,2002:merge")
}

/// Decodes the first document's event stream (`decode.go:170-178,333-377`).
///
/// # Errors
///
/// Returns invalid aliases, invalid map keys or scalar resolution errors.
pub fn decode_events(events: &[Event]) -> Result<Value, Error> {
    let mut tree = Tree::default();
    for event in events {
        tree.event(event)?;
    }
    if let Some(root) = tree.root {
        Decoder {
            decode_count: 1,
            ..Decoder::default()
        }
        .node(&tree, root)
    } else {
        Ok(Value::Null)
    }
}

/// The concrete JSON target used by Helm (`sigs.k8s.io/yaml/yaml.go:58-98`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// An untyped interface.
    Generic,
    /// A map with string keys and untyped values.
    Map,
    /// A slice of untyped elements.
    Array,
    /// Helm's manifest header.
    Head,
}

/// Converts generic YAML to sigs' JSON target (`yaml.go:181-365`).
///
/// For distinct YAML keys that coerce to the same JSON key, Go chooses an unspecified
/// map iteration order.
/// This concrete conversion keeps the last YAML entry, one of Go's permitted outcomes,
/// and fails whenever any candidate fails to marshal.
///
/// # Errors
///
/// Returns unsupported JSON keys, nonfinite numbers or incompatible target types.
pub fn to_json(value: &Value, target: Target) -> Result<serde_json::Value, Error> {
    let shape = if target == Target::Head {
        Shape::Head
    } else {
        Shape::Any
    };
    let json = convert(value, shape)?.concrete()?;
    match target {
        Target::Generic => Ok(json),
        Target::Map if json.is_null() || json.is_object() => Ok(json),
        Target::Array if json.is_null() || json.is_array() => Ok(json),
        Target::Head => decode_head(&json),
        Target::Map | Target::Array => Err(json_error("cannot unmarshal JSON into target")),
    }
}

/// The Go destination type that steers sigs' conversion and `encoding/json`'s assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `interface{}`.
    Any,
    /// `map[string]interface{}`, Helm's values target.
    Values,
    /// `string`.
    Text,
    /// `bool`.
    Bool,
    /// `[]string`.
    Texts,
    /// `[]interface{}`.
    List,
    /// `map[string]string`.
    TextMap,
    /// `chart.Metadata` (`pkg/chart/v2/metadata.go:30-82`).
    Chart,
    /// `[]*chart.Dependency`.
    Dependencies,
    /// `chart.Dependency` (`pkg/chart/v2/dependency.go:26-58`).
    Dependency,
    /// `[]*chart.Maintainer`.
    Maintainers,
    /// `chart.Maintainer`.
    Maintainer,
    /// `releaseutil.SimpleHead` (`pkg/release/v1/util/manifest_sorter.go:37-45`).
    Head,
    /// `SimpleHead.Metadata`.
    HeadMetadata,
}

impl Shape {
    /// JSON field names and types of a struct destination, in Go field order.
    pub(crate) fn fields(self) -> &'static [(&'static str, Self)] {
        use Shape::{Bool, Dependencies, HeadMetadata, List, Maintainers, Text, TextMap, Texts};
        match self {
            Self::Chart => &[
                ("name", Text),
                ("home", Text),
                ("sources", Texts),
                ("version", Text),
                ("description", Text),
                ("keywords", Texts),
                ("maintainers", Maintainers),
                ("icon", Text),
                ("apiVersion", Text),
                ("condition", Text),
                ("tags", Text),
                ("appVersion", Text),
                ("deprecated", Bool),
                ("annotations", TextMap),
                ("kubeVersion", Text),
                ("dependencies", Dependencies),
                ("type", Text),
            ],
            Self::Dependency => &[
                ("name", Text),
                ("version", Text),
                ("repository", Text),
                ("condition", Text),
                ("tags", Texts),
                ("enabled", Bool),
                ("import-values", List),
                ("alias", Text),
            ],
            Self::Maintainer => &[("name", Text), ("email", Text), ("url", Text)],
            Self::Head => &[
                ("apiVersion", Text),
                ("kind", Text),
                ("metadata", HeadMetadata),
            ],
            Self::HeadMetadata => &[("name", Text), ("annotations", TextMap)],
            Self::Any
            | Self::Values
            | Self::Text
            | Self::Bool
            | Self::Texts
            | Self::List
            | Self::TextMap
            | Self::Dependencies
            | Self::Maintainers => &[],
        }
    }

    /// The struct field a JSON key decodes into: exact name first, then Go's simple
    /// fold (`encoding/json/decode.go:690-707`, `sigs.k8s.io/yaml/yaml.go:258-276`).
    pub(crate) fn field(self, key: &str) -> Option<(&'static str, Self)> {
        let fields = self.fields();
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .or_else(|| fields.iter().find(|(name, _)| equal_fold_ascii(key, name)))
            .copied()
    }

    /// Whether a JSON object decodes into this destination.
    pub(crate) fn is_object(self) -> bool {
        matches!(self, Self::Any | Self::Values | Self::TextMap) || !self.fields().is_empty()
    }

    /// The destination of one member of a JSON object.
    fn member(self, key: &str) -> Self {
        match self {
            Self::Values => Self::Any,
            Self::TextMap => Self::Text,
            _ => self.field(key).map_or(Self::Any, |(_, field)| field),
        }
    }

    /// The destination of one element of a JSON array.
    pub(crate) fn element(self) -> Self {
        match self {
            Self::Texts => Self::Text,
            Self::Dependencies => Self::Dependency,
            Self::Maintainers => Self::Maintainer,
            _ => Self::Any,
        }
    }
}

/// sigs' JSON-able object before `json.Marshal`, with numbers still in their YAML types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    /// IEEE-754 bits; a surviving nonfinite value fails marshaling.
    Float(u64),
    /// A YAML string.
    String(String),
    /// A number or Boolean sigs converted for a string destination.
    Coerced(String),
    Array(Vec<Json>),
    /// Members in marshaled order, sorted by their raw key bytes.
    Object(Vec<Member>),
}

/// One converted map key and every YAML entry that converted to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) raw: Vec<u8>,
    /// Distinct Go keys, such as `1` and `"1"`, compete in Go's unspecified map iteration
    /// order (`sigs.k8s.io/yaml/yaml.go:204-300`); each one is a possible winner.
    pub(crate) candidates: Vec<Candidate>,
}

/// One possible winner of a converted key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) value: Json,
    /// Whether the entry was spelled directly, without alias or merge expansion.
    pub(crate) direct: bool,
}

/// Converts every YAML entry for its destination, as sigs' `convertToJSONableObject` does
/// (`sigs.k8s.io/yaml/yaml.go:181-365`).
///
/// Every candidate converts, including ignored fields and collision losers, so a key
/// conversion failure anywhere fails the document before a winner is chosen.
///
/// # Errors
///
/// Returns an unsupported map key.
pub(crate) fn convert(value: &Value, shape: Shape) -> Result<Json, Error> {
    Ok(match value {
        Value::Null => Json::Null,
        Value::Bool(value) if shape == Shape::Text => Json::Coerced(value.to_string()),
        Value::Int(value) if shape == Shape::Text => Json::Coerced(value.to_string()),
        Value::Uint(value) if shape == Shape::Text => Json::Coerced(value.to_string()),
        Value::Float(bits) if shape == Shape::Text => {
            Json::Coerced(go_float(f64::from_bits(*bits), true))
        }
        Value::Bool(value) => Json::Bool(*value),
        Value::Int(value) => Json::Int(*value),
        Value::Uint(value) => Json::Uint(*value),
        Value::Float(bits) => Json::Float(*bits),
        Value::String(value) => Json::String(value.clone()),
        Value::RawString(value) => Json::String(go_json_string(value)),
        Value::Sequence(items) => Json::Array(
            items
                .iter()
                .map(|item| convert(item, shape.element()))
                .collect::<Result<_, _>>()?,
        ),
        Value::Mapping(entries) => {
            let mut members = BTreeMap::<_, Vec<_>>::new();
            for Entry {
                key,
                value,
                authored,
            } in entries
            {
                let raw = json_key(key)?;
                let value = convert(value, shape.member(&go_json_string(&raw)))?;
                members.entry(raw).or_default().push(Candidate {
                    value,
                    direct: *authored,
                });
            }
            Json::Object(
                members
                    .into_iter()
                    .map(|(raw, candidates)| Member { raw, candidates })
                    .collect(),
            )
        }
    })
}

impl Json {
    /// Succeeds when some choice of collision winners marshals (`encoding/json/encode.go:575-585`).
    ///
    /// # Errors
    ///
    /// Returns the first nonfinite number that every outcome keeps.
    pub(crate) fn marshal(&self) -> Result<(), Error> {
        match self {
            Self::Float(bits) if !f64::from_bits(*bits).is_finite() => Err(json_error(format!(
                "unsupported value: {}",
                go_float(f64::from_bits(*bits), false)
            ))),
            Self::Array(items) => items.iter().try_for_each(Self::marshal),
            Self::Object(members) => {
                members
                    .iter()
                    .try_for_each(|Member { raw: _, candidates }| {
                        let mut failure = None;
                        for Candidate { value, direct: _ } in candidates {
                            match value.marshal() {
                                Ok(()) => return Ok(()),
                                Err(error) => {
                                    failure.get_or_insert(error);
                                }
                            }
                        }
                        failure.map_or(Ok(()), Err)
                    })
            }
            Self::Null
            | Self::Bool(_)
            | Self::Int(_)
            | Self::Uint(_)
            | Self::Float(_)
            | Self::String(_)
            | Self::Coerced(_) => Ok(()),
        }
    }

    /// One concrete outcome: the last YAML entry wins each converted key, and every
    /// candidate must marshal.
    fn concrete(&self) -> Result<serde_json::Value, Error> {
        Ok(match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => json!(*value),
            Self::Int(value) => json!(go_signed_float(*value)),
            Self::Uint(value) => json!(go_unsigned_float(*value)),
            Self::Float(bits) => {
                self.marshal()?;
                json!(f64::from_bits(*bits))
            }
            Self::String(value) | Self::Coerced(value) => json!(value),
            Self::Array(items) => serde_json::Value::Array(
                items.iter().map(Self::concrete).collect::<Result<_, _>>()?,
            ),
            Self::Object(members) => {
                let mut object = serde_json::Map::new();
                for Member { raw, candidates } in members {
                    let mut winner = serde_json::Value::Null;
                    for Candidate { value, direct: _ } in candidates {
                        winner = value.concrete()?;
                    }
                    object.insert(go_json_string(raw), winner);
                }
                serde_json::Value::Object(object)
            }
        })
    }
}

/// Converts a Go scalar map key before JSON marshaling (`sigs.k8s.io/yaml/yaml.go:219-252`).
fn json_key(key: &Value) -> Result<Vec<u8>, Error> {
    let text = match key {
        Value::String(value) => return Ok(value.as_bytes().to_vec()),
        Value::RawString(value) => return Ok(value.clone()),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(bits) => match go_float(f64::from_bits(*bits), true).as_str() {
            "+Inf" => ".inf".to_owned(),
            "-Inf" => "-.inf".to_owned(),
            "NaN" => ".nan".to_owned(),
            text => text.to_owned(),
        },
        Value::Null | Value::Uint(_) | Value::Sequence(_) | Value::Mapping(_) => {
            return Err(json_error("unsupported map key"));
        }
    };
    Ok(text.into_bytes())
}

/// Whether distinct decoded YAML keys can collapse to one JSON key anywhere in `value`.
/// Such collisions can select different values under Go's unspecified map iteration order.
/// Exact duplicate YAML keys have already been resolved by the YAML decoder.
#[must_use]
pub fn has_json_key_collisions(value: &Value) -> bool {
    match value {
        Value::Mapping(entries) => {
            let mut keys = BTreeSet::new();
            entries.iter().any(
                |Entry {
                     key,
                     value,
                     authored: _,
                 }| {
                    json_key(key).is_ok_and(|key| !keys.insert(go_json_string(&key)))
                        || has_json_key_collisions(value)
                },
            )
        }
        Value::Sequence(items) => items.iter().any(has_json_key_collisions),
        Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Uint(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::RawString(_) => false,
    }
}

/// Replaces each invalid byte separately, as `encoding/json/encode.go:1019-1048` does.
pub(crate) fn go_json_string(input: &[u8]) -> String {
    let mut output = String::new();
    let mut remaining = input;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                output.push_str(text);
                break;
            }
            Err(err) => {
                let prefix = remaining.get(..err.valid_up_to()).unwrap_or_default();
                if let Ok(prefix) = std::str::from_utf8(prefix) {
                    output.push_str(prefix);
                }
                output.push(char::REPLACEMENT_CHARACTER);
                remaining = remaining.get(err.valid_up_to() + 1..).unwrap_or_default();
            }
        }
    }
    output
}

/// Compares a field against ASCII using Go's Unicode simple-fold classes (`encoding/json/fold.go:20-48`).
///
/// The only non-ASCII members of ASCII fold classes in the pinned Go tables are long s and Kelvin sign.
fn equal_fold_ascii(input: &str, field: &str) -> bool {
    let mut input = input.chars();
    for byte in field.bytes() {
        let Some(character) = input.next() else {
            return false;
        };
        let folded = match character {
            '\u{017f}' => 's',
            '\u{212a}' => 'k',
            character => character,
        };
        if !folded.eq_ignore_ascii_case(&char::from(byte)) {
            return false;
        }
    }
    input.next().is_none()
}

#[derive(Default)]
struct Head {
    version: String,
    kind: String,
    metadata: Option<Metadata>,
}

#[derive(Default)]
struct Metadata {
    name: String,
    annotations: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Decodes JSON into existing pointer/struct fields (`encoding/json/decode.go:597-808`).
fn decode_head(json: &serde_json::Value) -> Result<serde_json::Value, Error> {
    let mut head = Head::default();
    if !json.is_null() {
        let fields = json
            .as_object()
            .ok_or_else(|| json_error("cannot unmarshal JSON into releaseutil.SimpleHead"))?;
        let mut fields: Vec<_> = fields.iter().collect();
        fields.sort_by_key(|(left, _)| *left);
        for (key, value) in fields {
            if equal_fold_ascii(key, "apiVersion") {
                decode_string(value, &mut head.version)?;
            } else if equal_fold_ascii(key, "kind") {
                decode_string(value, &mut head.kind)?;
            } else if equal_fold_ascii(key, "metadata") {
                if value.is_null() {
                    head.metadata = None;
                } else {
                    decode_metadata(value, head.metadata.get_or_insert_with(Metadata::default))?;
                }
            }
        }
    }
    let Head {
        version,
        kind,
        metadata,
    } = head;
    let mut json = serde_json::Map::new();
    json.insert("apiVersion".to_owned(), json!(version));
    if !kind.is_empty() {
        json.insert("kind".to_owned(), json!(kind));
    }
    if let Some(Metadata { name, annotations }) = metadata {
        json.insert(
            "metadata".to_owned(),
            json!({"name":name,"annotations":annotations}),
        );
    }
    Ok(serde_json::Value::Object(json))
}

/// Preserves existing strings when JSON supplies null (`encoding/json/decode.go:904-960`).
fn decode_string(value: &serde_json::Value, target: &mut String) -> Result<(), Error> {
    match value {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::String(value) => {
            target.clone_from(value);
            Ok(())
        }
        _ => Err(json_error("cannot unmarshal JSON into string")),
    }
}

/// Reuses the metadata struct and annotation map across folded keys (`encoding/json/decode.go:597-808`).
fn decode_metadata(value: &serde_json::Value, metadata: &mut Metadata) -> Result<(), Error> {
    let fields = value
        .as_object()
        .ok_or_else(|| json_error("cannot unmarshal JSON into metadata"))?;
    let mut fields: Vec<_> = fields.iter().collect();
    fields.sort_by_key(|(left, _)| *left);
    for (key, value) in fields {
        if equal_fold_ascii(key, "name") {
            decode_string(value, &mut metadata.name)?;
        } else if equal_fold_ascii(key, "annotations") {
            if value.is_null() {
                metadata.annotations = None;
            } else {
                let map = value
                    .as_object()
                    .ok_or_else(|| json_error("cannot unmarshal JSON into annotations"))?;
                let target = metadata
                    .annotations
                    .get_or_insert_with(serde_json::Map::new);
                for (key, value) in map {
                    let mut decoded = String::new();
                    decode_string(value, &mut decoded)?;
                    target.insert(key.clone(), json!(decoded));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn json_error(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Json,
        line: 0,
        message: message.into(),
    }
}

/// Formats Go's shortest `%g` float spelling (`strconv/ftoa.go:25-56,199-258`).
pub(crate) fn go_float(value: f64, bits32: bool) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == f64::INFINITY {
        return "+Inf".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-Inf".to_owned();
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "sigs yaml explicitly formats float64 through strconv bitSize 32"
    )]
    let narrow = value as f32;
    if bits32 && narrow.is_infinite() {
        return if narrow.is_sign_positive() {
            "+Inf".to_owned()
        } else {
            "-Inf".to_owned()
        };
    }
    let scientific = if bits32 {
        format!("{narrow:e}")
    } else {
        format!("{value:e}")
    };
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific;
    };
    let exponent = exponent.parse::<i32>().unwrap_or_default();
    if !(-4..6).contains(&exponent) {
        format!("{mantissa}e{exponent:+03}")
    } else if bits32 {
        narrow.to_string()
    } else {
        value.to_string()
    }
}

/// Applies Go's generic JSON number conversion (`sigs.k8s.io/yaml/yaml.go:50-51`).
pub(crate) fn go_signed_float(value: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Go json.Unmarshal decodes generic numbers as float64 and deliberately loses integer precision"
    )]
    let value = value as f64;
    value
}

/// Applies Go's uint64-to-float64 JSON conversion (`sigs.k8s.io/yaml/yaml.go:50-51`).
pub(crate) fn go_unsigned_float(value: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Go float64 conversion is deliberately rounded, including alias-ratio arithmetic"
    )]
    let value = value as f64;
    value
}

/// The first document's decoded value and parser evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Generic go-yaml value.
    pub value: Value,
    /// Parser events consumed before the first document completes.
    pub events: Vec<Event>,
    /// Completion mark, including stream end for an empty input.
    pub end: Option<Mark>,
}

/// Builds the node graph in Go's event-consumption order (`decode.go:129-212`).
///
/// # Errors
///
/// Returns byte, scanner, parser, alias or generic decode errors in Go's order.
pub fn read_document(input: &[u8]) -> Result<Document, Error> {
    let mut reader = Reader::default();
    let mut tree = Tree::default();
    let mut events = Vec::new();
    let mut sink = |event: &Event| {
        tree.event(event)?;
        events.push(event.clone());
        Ok(())
    };
    reader.push_bytes(input, &mut sink)?;
    reader.finish(&mut sink)?;
    let value = if let Some(root) = tree.root {
        Decoder {
            decode_count: 1,
            ..Decoder::default()
        }
        .node(&tree, root)?
    } else {
        Value::Null
    };
    Ok(Document {
        value,
        events,
        end: reader.document_end(),
    })
}
