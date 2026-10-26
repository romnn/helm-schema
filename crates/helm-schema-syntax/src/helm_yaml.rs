//! YAML documents decoded the way Helm hands them to Kubernetes.
//!
//! Documents are framed as k8s.io/apimachinery's `YAMLReader` frames them.
//! libyaml supplies syntax, scalar styles, expanded tags and errors.
//! Resolution and JSON conversion follow `go.yaml.in/yaml/v2` v2.4.3 and
//! `sigs.k8s.io/yaml` v1.6.0, including YAML 1.1 scalars, merge precedence,
//! alias limits and stringification of boolean and integer keys.
//! Finite float keys and stringified-key collisions remain explicitly unknown.
//!
//! Every libyaml error is unsupported, so the document projects nothing;
//! only exact post-parse Go rules produce a rejection.
//! Known P2: libyaml-safer rejects flow plain scalars such as `[a:]` and
//! `{a:}`, and a flow tag ended by `,` is declined, where Go decodes both.
//! A plain flow scalar containing `?` is also unsupported because Go
//! tokenizes it differently.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use base64::Engine as _;
use libyaml_safer::{EventData, MappingStyle, Parser, ScalarStyle, Scanner, SequenceStyle};
use serde_json::{Map, Number, Value};

use crate::yaml_dialect::{YAML_1_1_BOOLEANS, yaml_style_float};

/// The reader's collection bound; exceeding it widens rather than rejecting.
const MAX_DEPTH: usize = 128;

/// One YAML document as Helm decodes it.
#[derive(Clone, Debug, PartialEq)]
pub enum HelmYamlDocument {
    /// The JSON value, with `null` at each pointer listed in `unknown`.
    Decoded {
        /// The decoded value.
        value: Value,
        /// Pointers to values this reader cannot reproduce, in document order.
        unknown: Vec<String>,
    },
    /// No value is projected; the reason distinguishes Go rejection from reader limits.
    Failed(HelmYamlError),
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

/// Decodes the documents Kubernetes' `YAMLReader` frames, each on its own,
/// so a malformed document cannot hide later ones.
#[must_use]
#[tracing::instrument(skip_all, fields(bytes = source.len()))]
pub fn helm_yaml_documents(source: &str) -> Vec<HelmYamlDocument> {
    transport_documents(source)
        .into_iter()
        .map(
            |document| match document.and_then(|document| load(&document)) {
                Ok(Tree {
                    nodes,
                    root: Some(root),
                }) => decode_document(&nodes, root),
                Ok(Tree {
                    nodes: _,
                    root: None,
                }) => HelmYamlDocument::Decoded {
                    value: Value::Null,
                    unknown: Vec::new(),
                },
                Err(err) => HelmYamlDocument::Failed(err),
            },
        )
        .collect()
}

/// How Helm reads `source`, a single scalar, as a mapping key.
#[must_use]
pub fn helm_yaml_key(source: &str) -> HelmYamlKey {
    let Ok(Tree {
        nodes,
        root: Some(root),
    }) = load(source)
    else {
        return HelmYamlKey::Unknown;
    };
    let Some(YamlNode::Scalar(scalar)) = nodes.get(root) else {
        return HelmYamlKey::Unknown;
    };
    if is_merge_key(scalar) {
        return HelmYamlKey::Merge;
    }
    resolve_scalar(scalar)
        .and_then(|value| json_key(go_key(value, 0)))
        .ok()
        .flatten()
        .map_or(HelmYamlKey::Unknown, HelmYamlKey::Key)
}

/// The documents of k8s.io/apimachinery's `YAMLReader`, which Helm's
/// resource builder reads before `YAMLToJSON`.
///
/// Every line, the last included, ends in a line feed, and `\r\n` becomes
/// `\n`. A line beginning with `---` ends a non-empty document and is
/// otherwise kept; trailing text other than whitespace or a comment
/// discards the open document with an error.
fn transport_documents(source: &str) -> Vec<Result<String, HelmYamlError>> {
    let mut documents = Vec::new();
    let mut buffer = String::new();
    for line in source.split_inclusive('\n') {
        let line = line
            .strip_suffix('\n')
            .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line));
        if let Some(tail) = line.strip_prefix("---") {
            let tail = tail.trim();
            if !tail.is_empty() && !tail.starts_with('#') {
                buffer.clear();
                documents.push(Err(HelmYamlError::Rejected));
                continue;
            }
            if !buffer.is_empty() {
                documents.push(Ok(std::mem::take(&mut buffer)));
                continue;
            }
        }
        buffer.push_str(line);
        buffer.push('\n');
    }
    if !buffer.is_empty() {
        documents.push(Ok(buffer));
    }
    documents
}

/// `YAMLToJSON` decodes the first document of `source`.
///
/// libyaml-safer 0.3.0 panics when the input ends inside a block scalar
/// line or after a quoted backslash, so `source` is read line-terminated,
/// as the transport delivers it; it also panics on a flow tag that ends
/// at `,`, so a tag ending at `,` is declined first.
fn load(source: &str) -> Result<Tree, HelmYamlError> {
    let terminated;
    let source = if source.ends_with('\n') {
        source
    } else {
        terminated = format!("{source}\n");
        &terminated
    };
    if tag_ends_at_comma(source) {
        return Err(HelmYamlError::Unsupported);
    }
    let mut loader = Loader::default();
    let mut parser = Parser::new();
    parser.set_input(source.as_bytes());
    loop {
        let event = parser.parse().map_err(|_| HelmYamlError::Unsupported)?;
        match event.data {
            EventData::DocumentEnd { .. } | EventData::StreamEnd => {
                return Ok(Tree {
                    nodes: loader.nodes,
                    root: loader.root,
                });
            }
            EventData::DocumentStart {
                version_directive, ..
            } if version_directive
                .is_some_and(|version| version.major != 1 || version.minor != 1) =>
            {
                return Err(HelmYamlError::Rejected);
            }
            event @ (EventData::StreamStart { .. }
            | EventData::DocumentStart { .. }
            | EventData::Alias { .. }
            | EventData::Scalar { .. }
            | EventData::SequenceStart { .. }
            | EventData::SequenceEnd
            | EventData::MappingStart { .. }
            | EventData::MappingEnd) => loader.on_event(event)?,
        }
    }
}

/// Whether a tag ends at `,`: libyaml-safer's scanner panics on that in a
/// flow collection and errors outside one.
///
/// A tag starts at a `!` that begins a token, so after a blank, a line
/// break, a byte-order mark or an indicator. For each such `!` whose tag
/// characters run up to a `,`, the source before that `,` is scanned with
/// a non-blank terminator instead, which makes a tag at the `!` fail at
/// once. Candidates are checked in the order of their `,`, so every `,`
/// inside a later scan has already been cleared.
fn tag_ends_at_comma(source: &str) -> bool {
    // The reader consumes a leading byte-order mark before the scanner counts.
    let bom = if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut candidates = Vec::new();
    let mut previous = None;
    for (start, c) in source.char_indices() {
        let token_start = previous.is_none_or(|previous| {
            matches!(
                previous,
                ' ' | '\t'
                    | '\n'
                    | '\r'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{feff}'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | ':'
                    | '"'
                    | '\''
                    | '?'
                    | '-'
            )
        });
        previous = Some(c);
        if c != '!' || !token_start {
            continue;
        }
        let tail = source.get(start + 1..).unwrap_or_default();
        let length = if tail.starts_with('<') {
            // A verbatim tag `!<…>` ends at its first `>`.
            tail.find('>').map_or(tail.len(), |end| end + 1)
        } else {
            tail.find(|c: char| !(c.is_ascii_alphanumeric() || "-_;/?:@&=+$.%!~*'()".contains(c)))
                .unwrap_or(tail.len())
        };
        let end = start + 1 + length;
        if source.get(end..).is_some_and(|rest| rest.starts_with(',')) {
            candidates.push((end, start));
        }
    }
    candidates.sort_unstable();
    candidates
        .into_iter()
        .any(|(end, start)| tag_starts_at(source.get(..end).unwrap_or_default(), start - bom))
}

/// Whether scanning `prefix`, ended by a character that cannot end a tag,
/// fails on a tag that starts at `start`.
fn tag_starts_at(prefix: &str, start: usize) -> bool {
    let source = format!("{prefix}`\n");
    let mut scanner = Scanner::new();
    scanner.set_input(source.as_bytes());
    scanner
        .find_map(Result::err)
        .and_then(|err| err.context_mark())
        .is_some_and(|mark| mark.index == start as u64)
}

struct Tree {
    nodes: Vec<YamlNode>,
    root: Option<usize>,
}

/// Children and aliases refer to arena indices.
enum YamlNode {
    Scalar(Scalar),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
    Alias(usize),
}

struct Scalar {
    text: String,
    implicit: bool,
    tag: Option<String>,
}

enum Frame {
    Sequence {
        anchor: Option<String>,
        in_flow: bool,
        items: Vec<usize>,
    },
    Mapping {
        anchor: Option<String>,
        in_flow: bool,
        entries: Vec<(usize, usize)>,
        key: Option<usize>,
    },
}

/// Only completed anchors can be expanded; an open anchor masks an earlier definition.
#[derive(Default)]
struct Loader {
    nodes: Vec<YamlNode>,
    anchors: HashMap<String, usize>,
    frames: Vec<Frame>,
    root: Option<usize>,
}

impl Loader {
    fn in_flow(&self) -> bool {
        match self.frames.last() {
            Some(Frame::Sequence { in_flow, .. } | Frame::Mapping { in_flow, .. }) => *in_flow,
            None => false,
        }
    }

    fn anchor(&mut self, anchor: Option<String>, index: usize) {
        if let Some(anchor) = anchor {
            self.anchors.insert(anchor, index);
        }
    }

    fn complete(&mut self, node: YamlNode, anchor: Option<String>) {
        let index = self.nodes.len();
        self.nodes.push(node);
        self.anchor(anchor, index);
        match self.frames.last_mut() {
            Some(Frame::Sequence { items, .. }) => items.push(index),
            Some(Frame::Mapping { entries, key, .. }) => match key.take() {
                Some(key) => entries.push((key, index)),
                None => *key = Some(index),
            },
            None => self.root = Some(index),
        }
    }

    fn finish_collection(&mut self, node: YamlNode, anchor: Option<String>) {
        // A nested node may have rebound the same anchor after this collection opened.
        let anchor = anchor.filter(|anchor| !self.anchors.contains_key(anchor));
        self.complete(node, anchor);
    }

    fn on_event(&mut self, event: EventData) -> Result<(), HelmYamlError> {
        match event {
            EventData::Scalar {
                value,
                anchor,
                tag,
                plain_implicit,
                style,
                ..
            } => {
                // Go's scanner ends a plain flow scalar at `?`, unlike libyaml-safer.
                if self.in_flow() && style == ScalarStyle::Plain && value.contains('?') {
                    return Err(HelmYamlError::Unsupported);
                }
                self.complete(
                    YamlNode::Scalar(Scalar {
                        text: value,
                        implicit: plain_implicit,
                        tag,
                    }),
                    anchor,
                );
            }
            EventData::Alias { anchor } => {
                let target = self
                    .anchors
                    .get(&anchor)
                    .copied()
                    .ok_or(HelmYamlError::Rejected)?;
                self.complete(YamlNode::Alias(target), None);
            }
            EventData::SequenceStart { anchor, style, .. } => {
                let in_flow = self.in_flow() || style == SequenceStyle::Flow;
                if let Some(anchor) = &anchor {
                    self.anchors.remove(anchor);
                }
                self.frames.push(Frame::Sequence {
                    anchor,
                    in_flow,
                    items: Vec::new(),
                });
            }
            EventData::MappingStart { anchor, style, .. } => {
                let in_flow = self.in_flow() || style == MappingStyle::Flow;
                if let Some(anchor) = &anchor {
                    self.anchors.remove(anchor);
                }
                self.frames.push(Frame::Mapping {
                    anchor,
                    in_flow,
                    entries: Vec::new(),
                    key: None,
                });
            }
            EventData::SequenceEnd | EventData::MappingEnd => match self.frames.pop() {
                Some(Frame::Sequence {
                    anchor,
                    in_flow: _,
                    items,
                }) => self.finish_collection(YamlNode::Sequence(items), anchor),
                Some(Frame::Mapping {
                    anchor,
                    in_flow: _,
                    entries,
                    key: None,
                }) => self.finish_collection(YamlNode::Mapping(entries), anchor),
                Some(Frame::Mapping { key: Some(_), .. }) | None => {
                    return Err(HelmYamlError::Unsupported);
                }
            },
            EventData::DocumentStart { .. }
            | EventData::DocumentEnd { .. }
            | EventData::StreamStart { .. }
            | EventData::StreamEnd => {}
        }
        Ok(())
    }
}

/// Exact Go rejections are distinct from unsupported syntax and reader bounds.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HelmYamlError {
    /// An exact Go decoding or JSON-conversion rule rejects this document.
    #[error("helm rejects this YAML document")]
    Rejected,
    /// The syntax or a resource bound prevents this reader from reproducing Go.
    #[error("the reader cannot reproduce this YAML document")]
    Unsupported,
}

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
        Err(err) => HelmYamlDocument::Failed(err),
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
    fn count(&mut self) -> Result<(), HelmYamlError> {
        self.decode_count = self.decode_count.saturating_add(1);
        if self.alias_depth > 0 {
            self.alias_count = self.alias_count.saturating_add(1);
        }
        let excessive = self.alias_count > 100
            && self.decode_count > 1000
            && f64::from(self.alias_count) / f64::from(self.decode_count)
                > allowed_alias_ratio(self.decode_count);
        if excessive {
            Err(HelmYamlError::Rejected)
        } else {
            Ok(())
        }
    }

    fn enter(&mut self) -> Result<(), HelmYamlError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(HelmYamlError::Unsupported)
        } else {
            Ok(())
        }
    }

    fn unmarshal(&mut self, index: usize) -> Result<Decoded, HelmYamlError> {
        self.count()?;
        let nodes = self.nodes;
        match nodes.get(index).ok_or(HelmYamlError::Unsupported)? {
            YamlNode::Alias(target) => {
                self.alias_depth += 1;
                let decoded = self.unmarshal(*target);
                self.alias_depth -= 1;
                decoded
            }
            YamlNode::Scalar(scalar) => resolve_scalar(scalar),
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
    ) -> Result<(), HelmYamlError> {
        let nodes = self.nodes;
        for &(key, value) in entries {
            if let Some(YamlNode::Scalar(scalar)) = nodes.get(key)
                && is_merge_key(scalar)
            {
                self.merge(value, map)?;
                continue;
            }
            let key = match self.unmarshal(key)? {
                Decoded::Sequence(_) | Decoded::Mapping(_) => return Err(HelmYamlError::Rejected),
                key @ (Decoded::Null
                | Decoded::Bool(_)
                | Decoded::Int(_)
                | Decoded::Uint(_)
                | Decoded::Float(_)
                | Decoded::String(_)
                | Decoded::Unknown) => {
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
    fn merge(
        &mut self,
        value: usize,
        map: &mut BTreeMap<GoKey, Decoded>,
    ) -> Result<(), HelmYamlError> {
        let nodes = self.nodes;
        let is_mapping = |index: usize| {
            let node = match nodes.get(index) {
                Some(YamlNode::Alias(target)) => nodes.get(*target),
                node @ (Some(
                    YamlNode::Scalar(_) | YamlNode::Sequence(_) | YamlNode::Mapping(_),
                )
                | None) => node,
            };
            matches!(node, Some(YamlNode::Mapping(_)))
        };
        match nodes.get(value) {
            Some(YamlNode::Sequence(items)) => {
                for &item in items.iter().rev() {
                    if !is_mapping(item) {
                        return Err(HelmYamlError::Rejected);
                    }
                    self.unmarshal_into(item, map)?;
                }
                Ok(())
            }
            Some(YamlNode::Mapping(_) | YamlNode::Alias(_)) if is_mapping(value) => {
                self.unmarshal_into(value, map)
            }
            Some(YamlNode::Scalar(_) | YamlNode::Mapping(_) | YamlNode::Alias(_)) | None => {
                Err(HelmYamlError::Rejected)
            }
        }
    }

    fn unmarshal_into(
        &mut self,
        index: usize,
        map: &mut BTreeMap<GoKey, Decoded>,
    ) -> Result<(), HelmYamlError> {
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
            Some(YamlNode::Scalar(_) | YamlNode::Sequence(_)) | None => {
                Err(HelmYamlError::Rejected)
            }
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

/// go-yaml v2's `isMerge`, using the parser's expanded tag and implicit flag.
fn is_merge_key(scalar: &Scalar) -> bool {
    scalar.text == "<<"
        && (scalar.implicit || scalar.tag.as_deref() == Some("tag:yaml.org,2002:merge"))
}

fn resolve_scalar(scalar: &Scalar) -> Result<Decoded, HelmYamlError> {
    let tag = scalar.tag.as_deref();
    match tag {
        Some("tag:yaml.org,2002:binary") => {
            let text = scalar.text.replace(['\r', '\n'], "");
            let engine = base64::engine::GeneralPurpose::new(
                &base64::alphabet::STANDARD,
                base64::engine::GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
            );
            let bytes = engine.decode(text).map_err(|_| HelmYamlError::Rejected)?;
            return Ok(String::from_utf8(bytes).map_or(Decoded::Unknown, Decoded::String));
        }
        Some("tag:yaml.org,2002:timestamp") => {
            return if go_timestamp(&scalar.text) {
                Ok(Decoded::String(scalar.text.clone()))
            } else {
                Err(HelmYamlError::Rejected)
            };
        }
        Some(
            "tag:yaml.org,2002:null"
            | "tag:yaml.org,2002:bool"
            | "tag:yaml.org,2002:int"
            | "tag:yaml.org,2002:float",
        ) => {}
        None if scalar.implicit => {}
        Some(_) | None => return Ok(Decoded::String(scalar.text.clone())),
    }
    let decoded = resolve_plain(&scalar.text);
    match (tag, decoded) {
        (None, decoded)
        | (Some("tag:yaml.org,2002:null"), decoded @ Decoded::Null)
        | (Some("tag:yaml.org,2002:bool"), decoded @ Decoded::Bool(_))
        | (Some("tag:yaml.org,2002:int"), decoded @ (Decoded::Int(_) | Decoded::Uint(_)))
        | (Some("tag:yaml.org,2002:float"), decoded @ Decoded::Float(_)) => Ok(decoded),
        (Some("tag:yaml.org,2002:float"), Decoded::Int(value)) => {
            // Go's resolver converts signed integers to float64 for an explicit float tag.
            #[expect(
                clippy::cast_precision_loss,
                reason = "Go performs this same int64-to-float64 conversion"
            )]
            let value = value as f64;
            Ok(Decoded::Float(value))
        }
        (
            Some(_),
            Decoded::Null
            | Decoded::Bool(_)
            | Decoded::Int(_)
            | Decoded::Uint(_)
            | Decoded::Float(_)
            | Decoded::String(_)
            | Decoded::Sequence(_)
            | Decoded::Mapping(_)
            | Decoded::Unknown,
        ) => Err(HelmYamlError::Rejected),
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

/// The JSON key `sigs.k8s.io/yaml` makes of a Go key.
/// Finite float spellings stay unknown; null and uint64 keys are rejected
/// by the enclosing JSON conversion.
fn json_key(key: GoKey) -> Result<Option<String>, HelmYamlError> {
    Ok(match key {
        GoKey::String(key) => Some(key),
        GoKey::Bool(key) => Some(key.to_string()),
        GoKey::Int(key) => Some(key.to_string()),
        GoKey::Float(bits) if f64::from_bits(bits) == f64::INFINITY => Some(".inf".to_string()),
        GoKey::Float(bits) if f64::from_bits(bits) == f64::NEG_INFINITY => {
            Some("-.inf".to_string())
        }
        GoKey::Null | GoKey::Uint(_) => return Err(HelmYamlError::Rejected),
        GoKey::Float(_) | GoKey::Unknown(_) => None,
    })
}

/// Whether every Go key has a unique, reproduced JSON spelling.
fn json_keys_known(map: &BTreeMap<GoKey, Decoded>) -> Result<bool, HelmYamlError> {
    let mut spellings = BTreeSet::new();
    let mut known = true;
    for key in map.keys() {
        known &= json_key(key.clone())?.is_some_and(|key| spellings.insert(key));
    }
    Ok(known)
}

/// `convertToJSONableObject` and `json.Marshal`: keys are stringified, and
/// a non-finite float refuses the document.
fn to_json(
    decoded: Decoded,
    pointer: &mut String,
    unknown: &mut Vec<String>,
) -> Result<Value, HelmYamlError> {
    Ok(match decoded {
        Decoded::Null => Value::Null,
        Decoded::Bool(value) => Value::Bool(value),
        Decoded::Int(value) => Value::from(value),
        Decoded::Uint(value) => Value::from(value),
        Decoded::Float(value) => {
            Value::Number(Number::from_f64(value).ok_or(HelmYamlError::Rejected)?)
        }
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
            let keys_known = json_keys_known(&map)?;
            let mut object = Map::new();
            let mut nested_unknown = Vec::new();
            for (key, value) in map {
                let length = pointer.len();
                let key = json_key(key)?;
                if let Some(key) = &key {
                    pointer.push('/');
                    pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                }
                // A colliding key can overwrite a non-finite value before Go marshals JSON.
                let value = to_json(value, pointer, &mut nested_unknown).map_err(|err| {
                    if keys_known {
                        err
                    } else {
                        HelmYamlError::Unsupported
                    }
                })?;
                pointer.truncate(length);
                if let Some(key) = key {
                    object.insert(key, value);
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

/// The four timestamp layouts accepted by go-yaml v2's resolver.
fn go_timestamp(text: &str) -> bool {
    let (date, clock) = text.find(['T', 't', ' ']).map_or((text, None), |index| {
        let (date, rest) = text.split_at(index);
        (date, Some(rest))
    });
    let mut fields = date.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return false;
    };
    let (Some(year), Some(month), Some(day)) = (
        decimal_field(year, 4, 4),
        decimal_field(month, 1, 2),
        decimal_field(day, 1, 2),
    ) else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => return false,
    };
    if day == 0 || day > days {
        return false;
    }
    let Some(clock) = clock else {
        return true;
    };
    let zoned = !clock.starts_with(' ');
    let Some(clock) = clock.get(1..) else {
        return false;
    };
    let clock = if zoned {
        if let Some(clock) = clock.strip_suffix('Z') {
            clock
        } else if let Some(index) = clock.find(['+', '-']) {
            let (clock, offset) = clock.split_at(index);
            let Some((hour, minute)) = offset.get(1..).and_then(|offset| offset.split_once(':'))
            else {
                return false;
            };
            if decimal_field(hour, 2, 2).is_none_or(|hour| hour > 24)
                || decimal_field(minute, 2, 2).is_none_or(|minute| minute > 60)
            {
                return false;
            }
            clock
        } else {
            return false;
        }
    } else {
        clock.trim_start_matches(' ')
    };
    let mut fields = clock.split(':');
    let (Some(hour), Some(minute), Some(second), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return false;
    };
    let second = if let Some((second, fraction)) = second.split_once(['.', ',']) {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        second
    } else {
        second
    };
    decimal_field(hour, 1, 2).is_some_and(|hour| hour < 24)
        && decimal_field(minute, 1, 2).is_some_and(|minute| minute < 60)
        && decimal_field(second, 1, 2).is_some_and(|second| second < 60)
}

fn decimal_field(text: &str, min: usize, max: usize) -> Option<u32> {
    if !(min..=max).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
