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

use std::collections::BTreeSet;

use helm_schema_goyaml::{Entry, Value as GoValue};

use libyaml_safer::{
    EventData, MappingStyle, Parser, ScalarStyle, Scanner, SequenceStyle, TokenData,
};
use serde_json::{Map, Number, Value};

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
                Ok(doc) => decode_document(doc.value),
                Err(err) => HelmYamlDocument::Failed(err),
            },
        )
        .collect()
}

/// How Helm reads `source`, a single scalar, as a mapping key.
#[must_use]
pub fn helm_yaml_key(source: &str) -> HelmYamlKey {
    let Ok(document) = load(source) else {
        return HelmYamlKey::Unknown;
    };
    let Some(helm_schema_goyaml::EventKind::Scalar {
        implicit,
        tag,
        value,
        ..
    }) = document
        .events
        .iter()
        .map(|event| &event.kind)
        .find(|event| matches!(event, helm_schema_goyaml::EventKind::Scalar { .. }))
    else {
        return HelmYamlKey::Unknown;
    };
    if !matches!(
        document.value,
        helm_schema_goyaml::Value::Mapping(_) | helm_schema_goyaml::Value::Sequence(_)
    ) && helm_schema_goyaml::is_merge_key(value, tag, *implicit)
    {
        return HelmYamlKey::Merge;
    }
    json_key(&document.value)
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
#[must_use]
pub fn transport_documents(source: &str) -> Vec<Result<String, HelmYamlError>> {
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
fn load(source: &str) -> Result<helm_schema_goyaml::Document, HelmYamlError> {
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
    let mut flow = Vec::new();
    let mut parser = Parser::new();
    parser.set_input(source.as_bytes());
    loop {
        match parser.parse().map_err(|_| HelmYamlError::Unsupported)?.data {
            EventData::DocumentEnd { .. } | EventData::StreamEnd => break,
            EventData::DocumentStart {
                version_directive, ..
            } if version_directive
                .is_some_and(|version| version.major != 1 || version.minor != 1) =>
            {
                return Err(HelmYamlError::Rejected);
            }
            EventData::Scalar {
                style: ScalarStyle::Plain,
                value,
                ..
            } if flow.last() == Some(&true) && value.contains('?') => {
                return Err(HelmYamlError::Unsupported);
            }
            EventData::SequenceStart { style, .. } => {
                flow.push(flow.last() == Some(&true) || style == SequenceStyle::Flow);
            }
            EventData::MappingStart { style, .. } => {
                flow.push(flow.last() == Some(&true) || style == MappingStyle::Flow);
            }
            EventData::SequenceEnd | EventData::MappingEnd => {
                flow.pop();
            }
            _ => {}
        }
        if flow.len() > MAX_DEPTH {
            return Err(HelmYamlError::Unsupported);
        }
    }
    helm_schema_goyaml::read_document(source.as_bytes()).map_err(|error| {
        if error.kind() == helm_schema_goyaml::ErrorKind::Decode {
            HelmYamlError::Rejected
        } else {
            HelmYamlError::Unsupported
        }
    })
}

/// Whether a tag ends at `,`: libyaml-safer's scanner panics on that in a
/// flow collection and errors outside one.
///
/// A tag starts at a `!` that begins a token, so after a blank, a line
/// break, a byte-order mark or an indicator. Every `,` that ends the tag
/// characters after such a `!` is a candidate. One scan with each
/// candidate replaced by `` ` `` usually settles all of them (see
/// [`candidates_are_text`]); otherwise each candidate's prefix is scanned
/// in turn, within a budget linear in the source, and an exhausted budget
/// declines the document.
fn tag_ends_at_comma(source: &str) -> bool {
    let bytes = source.as_bytes();
    let tag_byte =
        |byte: u8| byte.is_ascii_alphanumeric() || b"-_;/?:@&=+$.%!~*'()".contains(&byte);
    // Runs and `>` searches resume where the previous one stopped, so every
    // byte is visited a bounded number of times.
    let mut run = 0..0;
    let mut next_gt = (0, None);
    let mut starts = Vec::new();
    let mut commas = Vec::new();
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
        let from = start + 1;
        let end = if bytes.get(from) == Some(&b'<') {
            // A verbatim tag `!<…>` ends at its first `>`.
            if next_gt.1.is_none_or(|gt| gt < from) {
                let searched = next_gt.0.max(from);
                let found = bytes
                    .get(searched..)
                    .and_then(|rest| rest.iter().position(|&byte| byte == b'>'))
                    .map(|offset| searched + offset);
                next_gt = (found.unwrap_or(bytes.len()), found);
            }
            next_gt.1.map_or(bytes.len(), |gt| gt + 1)
        } else {
            if !run.contains(&from) {
                let length = bytes
                    .get(from..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|&&byte| tag_byte(byte))
                    .count();
                run = from..from + length;
            }
            run.end
        };
        if bytes.get(end) == Some(&b',') {
            starts.push(start);
            commas.push(end);
        }
    }
    let bom = bom_length(source);
    if commas.is_empty() || candidates_are_text(bytes, &commas, bom) {
        return false;
    }
    let mut candidates = starts.into_iter().zip(commas).collect::<Vec<_>>();
    // Ordered by `,`, every `,` inside a later prefix has been cleared.
    candidates.sort_unstable_by_key(|&(start, comma)| (comma, start));
    let mut budget = 16 * source.len() + (1 << 20);
    for (start, comma) in candidates {
        let Some(rest) = budget.checked_sub(comma) else {
            return true;
        };
        budget = rest;
        if tag_starts_at(source.get(..comma).unwrap_or_default(), start - bom) {
            return true;
        }
    }
    false
}

/// The reader consumes a leading byte-order mark before the scanner counts.
fn bom_length(source: &str) -> usize {
    if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    }
}

/// Whether one scan with every candidate `,` replaced by `` ` `` keeps each
/// replacement as text of a quoted, block or block-context plain scalar or
/// a comment, where `,` is text too. The scan then follows the source's
/// tokens throughout, so no tag ends at a candidate. A scanner error or a
/// replacement inside a plain scalar of a flow collection, which `,` would
/// have ended, settles nothing.
fn candidates_are_text(bytes: &[u8], commas: &[usize], bom: usize) -> bool {
    let mut modified = bytes.to_vec();
    for &comma in commas {
        if let Some(byte) = modified.get_mut(comma) {
            *byte = b'`';
        }
    }
    let mut sorted = commas
        .iter()
        .map(|&comma| (comma - bom) as u64)
        .collect::<Vec<_>>();
    sorted.sort_unstable();
    let mut scanner = Scanner::new();
    scanner.set_input(modified.as_slice());
    let mut depth = 0_usize;
    for token in scanner {
        let Ok(token) = token else {
            return false;
        };
        match token.data {
            TokenData::FlowSequenceStart | TokenData::FlowMappingStart => depth += 1,
            TokenData::FlowSequenceEnd | TokenData::FlowMappingEnd => {
                depth = depth.saturating_sub(1);
            }
            TokenData::Scalar {
                style: ScalarStyle::Plain,
                ..
            } if depth > 0 => {
                let first = sorted.partition_point(|&comma| comma < token.start_mark.index);
                if sorted
                    .get(first)
                    .is_some_and(|&comma| comma < token.end_mark.index)
                {
                    return false;
                }
            }
            TokenData::StreamStart { .. }
            | TokenData::StreamEnd
            | TokenData::VersionDirective { .. }
            | TokenData::TagDirective { .. }
            | TokenData::DocumentStart
            | TokenData::DocumentEnd
            | TokenData::BlockSequenceStart
            | TokenData::BlockMappingStart
            | TokenData::BlockEnd
            | TokenData::BlockEntry
            | TokenData::FlowEntry
            | TokenData::Key
            | TokenData::Value
            | TokenData::Alias { .. }
            | TokenData::Anchor { .. }
            | TokenData::Tag { .. }
            | TokenData::Scalar { .. } => {}
        }
    }
    true
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

fn decode_document(value: GoValue) -> HelmYamlDocument {
    let mut unknown = Vec::new();
    match to_json(value, &mut String::new(), &mut unknown) {
        Ok(value) => HelmYamlDocument::Decoded { value, unknown },
        Err(err) => HelmYamlDocument::Failed(err),
    }
}

/// Finite float keys stay unknown. Null and uint64 keys are rejected.
fn json_key(key: &GoValue) -> Result<Option<String>, HelmYamlError> {
    Ok(match key {
        GoValue::String(key) => Some(key.clone()),
        GoValue::Bool(key) => Some(key.to_string()),
        GoValue::Int(key) => Some(key.to_string()),
        GoValue::Float(bits) if f64::from_bits(*bits) == f64::INFINITY => Some(".inf".to_owned()),
        GoValue::Float(bits) if f64::from_bits(*bits) == f64::NEG_INFINITY => {
            Some("-.inf".to_owned())
        }
        GoValue::Null | GoValue::Uint(_) => return Err(HelmYamlError::Rejected),
        GoValue::Float(_) | GoValue::RawString(_) | GoValue::Sequence(_) | GoValue::Mapping(_) => {
            None
        }
    })
}

/// `convertToJSONableObject` and `json.Marshal`: keys are stringified, and
/// a non-finite float refuses the document.
fn to_json(
    decoded: GoValue,
    pointer: &mut String,
    unknown: &mut Vec<String>,
) -> Result<Value, HelmYamlError> {
    Ok(match decoded {
        GoValue::Null => Value::Null,
        GoValue::Bool(value) => Value::Bool(value),
        GoValue::Int(value) => Value::from(value),
        GoValue::Uint(value) => Value::from(value),
        GoValue::Float(value) => {
            Value::Number(Number::from_f64(f64::from_bits(value)).ok_or(HelmYamlError::Rejected)?)
        }
        GoValue::String(value) => Value::String(value),
        GoValue::RawString(_) => {
            unknown.push(pointer.clone());
            Value::Null
        }
        GoValue::Sequence(items) => {
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
        GoValue::Mapping(map) => {
            let mut spellings = BTreeSet::new();
            let mut keys_known = true;
            for Entry {
                key,
                value: _,
                authored: _,
            } in &map
            {
                keys_known &= json_key(key)?.is_some_and(|key| spellings.insert(key));
            }
            let mut object = Map::new();
            let mut nested_unknown = Vec::new();
            for Entry {
                key,
                value,
                authored: _,
            } in map
            {
                let length = pointer.len();
                let key = json_key(&key)?;
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
