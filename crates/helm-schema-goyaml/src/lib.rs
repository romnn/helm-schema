//! A resumable Rust port of Helm's pinned go-yaml v2.4.3 reader.
//!
//! The scanner and parser follow the Go sources named on each ported function.
//! Concrete stages are independent transducers; completing or rejecting a YAML document
//! never stops Helm's manifest splitter.
//!
//! This crate contains modified ports of go-yaml and libyaml; see `NOTICE` and `LICENSE-APACHE`.

mod decode;
mod machine;
mod parser;
mod reader;
mod resolve;
mod scanner;
pub mod stages;
mod types;

pub use decode::{Document, Entry, Target, Value, decode_events, read_document, to_json};
pub use machine::{Reader, ReaderControl, Syntax};
pub use types::{Error, ErrorKind, Event, EventKind, Mark, ScalarStyle, Token, TokenKind};

/// Reads and decodes the first YAML document (`yaml.go:137-160`, `decode.go:170-178`).
///
/// # Errors
///
/// Returns the pinned reader's encoding, syntax, resolution or generic decode failure.
pub fn unmarshal(input: &[u8]) -> Result<Value, Error> {
    Ok(read_document(input)?.value)
}

/// Reads YAML into one of sigs' concrete JSON targets (`sigs.k8s.io/yaml/yaml.go:58-98`).
///
/// # Errors
///
/// Returns a YAML reader failure or an incompatible JSON conversion/target.
pub fn unmarshal_json(input: &[u8], target: Target) -> Result<serde_json::Value, Error> {
    to_json(&unmarshal(input)?, target)
}

#[cfg(test)]
mod tests;
