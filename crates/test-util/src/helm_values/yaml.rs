//! YAML the way Helm reads it, or a refusal.
//!
//! Helm decodes YAML with `sigs.k8s.io/yaml` (YAML 1.1 resolution, then
//! JSON), `serde_yaml` with YAML 1.2. The two agree on every document this
//! reader accepts; it refuses tagged scalars, collection keys, and the plain
//! scalars and keys where they differ
//! ([`helm_schema_syntax::plain_scalar_divergence`]).

use helm_schema_syntax::{DialectDivergence, plain_scalar_divergence};
use serde::Deserialize as _;
use serde_json::Value;
use yaml_rust::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust::scanner::{Marker, TScalarStyle};

use super::ValuesError;

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
    Some(match plain_scalar_divergence(spelling, key)? {
        DialectDivergence::MergeKey => "the merge key <<".to_string(),
        DialectDivergence::Boolean => format!("the YAML 1.1 boolean {spelling}"),
        DialectDivergence::SpecialFloat => format!("the special float {spelling}"),
        DialectDivergence::Number => format!("the YAML 1.1 number {spelling}"),
        DialectDivergence::ImpreciseInteger => {
            format!("the integer {spelling}, beyond float64 precision")
        }
        DialectDivergence::NonStringKey => format!("the non-string mapping key {spelling}"),
    })
}
