//! YAML the way Helm reads it, or a refusal.
//!
//! Helm decodes YAML with `sigs.k8s.io/yaml` (YAML 1.1 resolution, then
//! JSON), `serde_yaml` with YAML 1.2. The two agree on every document this
//! reader accepts; it refuses the plain scalars and keys where they differ,
//! each confirmed with Helm v4.2.3: `yes`/`off`/`y` are booleans, `012`
//! and `0o12` are 10, `1_000` is 1000, `0x1f` and `0b101` are numbers,
//! integers beyond 2^53 are rounded to float64, integer and boolean keys
//! are stringified, `<<` merges, and `.inf`/`.nan` and null keys abort.

use serde::Deserialize as _;
use serde_json::Value;
use yaml_rust::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust::scanner::{Marker, TScalarStyle};

use super::ValuesError;

const YAML_1_1_BOOLEANS: [&str; 16] = [
    "y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "on", "On", "ON", "off", "Off",
    "OFF",
];

/// The largest integer a float64 holds exactly.
const MAX_EXACT_INTEGER: u64 = 1 << 53;

/// Every YAML document of `bytes`, each decoded to JSON.
pub(super) fn documents(bytes: &[u8], file: &str) -> Result<Vec<Value>, ValuesError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let source = std::str::from_utf8(bytes)
        .map_err(|_| ValuesError::Unmodelled(format!("{file} is not UTF-8")))?;
    let mut scan = Scan {
        containers: Vec::new(),
        divergence: None,
    };
    Parser::new(source.chars())
        .load(&mut scan, true)
        .map_err(|error| ValuesError::Unmodelled(format!("{file}: {error}")))?;
    if let Some(divergence) = scan.divergence {
        return Err(ValuesError::Unmodelled(format!(
            "{file}: {divergence}, which Helm's YAML 1.1 decoding reads differently"
        )));
    }
    let mut documents = Vec::new();
    for document in serde_yaml::Deserializer::from_str(source) {
        let document = Value::deserialize(document)
            .map_err(|error| ValuesError::NotValidated(format!("cannot decode {file}: {error}")))?;
        documents.push(document);
    }
    Ok(documents)
}

enum Container {
    Mapping { expecting_key: bool },
    Sequence,
}

struct Scan {
    containers: Vec<Container>,
    divergence: Option<String>,
}

impl Scan {
    fn at_key(&self) -> bool {
        matches!(
            self.containers.last(),
            Some(Container::Mapping {
                expecting_key: true
            })
        )
    }

    fn finish_node(&mut self) {
        if let Some(Container::Mapping { expecting_key }) = self.containers.last_mut() {
            *expecting_key = !*expecting_key;
        }
    }

    fn diverge(&mut self, marker: Marker, what: &str) {
        if self.divergence.is_none() {
            self.divergence = Some(format!("line {}: {what}", marker.line()));
        }
    }
}

impl MarkedEventReceiver for Scan {
    fn on_event(&mut self, event: Event, marker: Marker) {
        match event {
            Event::Scalar(spelling, style, _, tag) => {
                if tag.is_some() {
                    self.diverge(marker, &format!("explicitly tagged scalar {spelling:?}"));
                } else if style == TScalarStyle::Plain
                    && let Some(what) = plain_divergence(&spelling, self.at_key())
                {
                    self.diverge(marker, &what);
                }
                self.finish_node();
            }
            Event::Alias(_) => self.finish_node(),
            Event::MappingStart(_) | Event::SequenceStart(_) if self.at_key() => {
                self.diverge(marker, "a collection as a mapping key");
                self.containers.push(Container::Sequence);
            }
            Event::MappingStart(_) => self.containers.push(Container::Mapping {
                expecting_key: true,
            }),
            Event::SequenceStart(_) => self.containers.push(Container::Sequence),
            Event::MappingEnd | Event::SequenceEnd => {
                self.containers.pop();
                self.finish_node();
            }
            Event::Nothing
            | Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart
            | Event::DocumentEnd => {}
        }
    }
}

/// Why a plain scalar reads differently under YAML 1.1, if it does.
fn plain_divergence(spelling: &str, key: bool) -> Option<String> {
    if key && spelling == "<<" {
        return Some("the merge key <<".to_string());
    }
    if YAML_1_1_BOOLEANS.contains(&spelling) {
        return Some(format!("the YAML 1.1 boolean {spelling}"));
    }
    let unsigned = spelling.strip_prefix(['-', '+']).unwrap_or(spelling);
    let lowered = unsigned.to_ascii_lowercase();
    if matches!(lowered.as_str(), ".inf" | ".nan") {
        return Some(format!("the special float {spelling}"));
    }
    let digits = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'_')
    };
    let base_prefixed = ["0x", "0X", "0o", "0O", "0b", "0B"]
        .iter()
        .any(|prefix| unsigned.strip_prefix(prefix).is_some_and(digits));
    let decimal_like = unsigned.starts_with(|character: char| character.is_ascii_digit())
        && unsigned
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_' || byte == b'.');
    let octal_like = unsigned.len() > 1
        && unsigned.starts_with('0')
        && unsigned.bytes().all(|byte| byte.is_ascii_digit());
    if base_prefixed || octal_like || (decimal_like && unsigned.contains('_')) {
        return Some(format!("the YAML 1.1 number {spelling}"));
    }
    if !unsigned.is_empty()
        && unsigned.bytes().all(|byte| byte.is_ascii_digit())
        && unsigned
            .parse::<u64>()
            .map_or(true, |value| value > MAX_EXACT_INTEGER)
    {
        return Some(format!("the integer {spelling}, beyond float64 precision"));
    }
    if key && resolves_to_non_string(spelling) {
        return Some(format!("the non-string mapping key {spelling}"));
    }
    None
}

/// Whether a plain scalar is a YAML 1.2 null, boolean, or number.
fn resolves_to_non_string(spelling: &str) -> bool {
    matches!(
        spelling,
        "" | "~"
            | "null"
            | "Null"
            | "NULL"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
    ) || (spelling.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '-' | '+' | '.')
    }) && spelling.parse::<f64>().is_ok())
}
