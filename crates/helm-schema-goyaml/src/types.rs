//! Tokens and events of go-yaml v2.4.3 (`yamlh.go:180-457`).

use serde::{Deserialize, Serialize};

/// A zero-based character position (`yamlh.go:60`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Mark {
    /// Number of decoded characters consumed.
    pub index: usize,
    /// Line number before presentation-layer adjustment.
    pub line: usize,
    /// Column measured in decoded characters.
    pub column: usize,
}

/// The layer reporting a failure (`yamlh.go:38-55`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    /// Byte encoding or forbidden character.
    Reader,
    /// Invalid YAML token.
    Scanner,
    /// Invalid YAML production.
    Parser,
    /// Resolution, alias or generic decode failure.
    Decode,
    /// YAML-to-JSON conversion or JSON target failure.
    Json,
}

/// A pinned-reader diagnostic, with go-yaml's displayed line number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, thiserror::Error, Serialize, Deserialize)]
#[error("{message}")]
pub struct Error {
    /// The reader layer that rejected the input.
    pub kind: ErrorKind,
    /// Displayed line; zero means that go-yaml omits the line prefix.
    pub line: usize,
    /// Diagnostic without the common `yaml:` prefix.
    pub message: String,
}

impl Error {
    /// Identifies the reader layer that rejected the input.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the line as displayed by go-yaml's `decode.go:111`.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }
}

/// Scalar presentation (`yamlh.go:103-119`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScalarStyle {
    /// Unquoted scalar.
    Plain,
    /// Single-quoted scalar.
    SingleQuoted,
    /// Double-quoted scalar.
    DoubleQuoted,
    /// Literal block scalar.
    Literal,
    /// Folded block scalar.
    Folded,
}

/// A scanner token (`yamlh.go:183-281`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Token {
    /// Token payload.
    pub kind: TokenKind,
    /// First character.
    pub start: Mark,
    /// Position following the token.
    pub end: Mark,
}

/// A token's grammatical role (`yamlh.go:183-229`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum TokenKind {
    /// Beginning of the stream.
    StreamStart,
    /// End of the stream.
    StreamEnd,
    /// YAML version directive.
    VersionDirective(i32, i32),
    /// Tag handle and prefix directive.
    TagDirective(String, String),
    /// Explicit document start.
    DocumentStart,
    /// Explicit document end.
    DocumentEnd,
    /// Implicit block sequence start.
    BlockSequenceStart,
    /// Implicit block mapping start.
    BlockMappingStart,
    /// Implicit block collection end.
    BlockEnd,
    /// Opening square bracket.
    FlowSequenceStart,
    /// Closing square bracket.
    FlowSequenceEnd,
    /// Opening curly bracket.
    FlowMappingStart,
    /// Closing curly bracket.
    FlowMappingEnd,
    /// Block sequence entry.
    BlockEntry,
    /// Flow collection comma.
    FlowEntry,
    /// Explicit or inserted key marker.
    Key,
    /// Mapping value marker.
    Value,
    /// Alias name.
    Alias(String),
    /// Anchor name.
    Anchor(String),
    /// Tag handle and suffix.
    Tag(String, String),
    /// Decoded scalar bytes and presentation.
    Scalar {
        /// Scalar content.
        value: String,
        /// Scalar quoting and folding style.
        style: ScalarStyle,
    },
}

/// A parser event (`yamlh.go:283-386`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Event {
    /// Event payload.
    pub kind: EventKind,
    /// First character.
    pub start: Mark,
    /// Position following the event.
    pub end: Mark,
}

/// A parser event's grammatical role (`yamlh.go:283-321`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    /// Beginning of the stream.
    StreamStart,
    /// End of the stream.
    StreamEnd,
    /// Beginning of the first document.
    DocumentStart {
        /// Whether the document has no explicit marker.
        implicit: bool,
    },
    /// End of the first document.
    DocumentEnd {
        /// Whether the document has no explicit end marker.
        implicit: bool,
    },
    /// Alias reference.
    Alias(String),
    /// Scalar node.
    Scalar {
        /// Anchor name, or empty.
        anchor: String,
        /// Resolved tag, or empty.
        tag: String,
        /// Scalar content.
        value: String,
        /// Presentation style.
        style: ScalarStyle,
        /// Go's plain-implicit flag.
        implicit: bool,
    },
    /// Sequence node start.
    SequenceStart {
        /// Anchor name, or empty.
        anchor: String,
        /// Resolved tag, or empty.
        tag: String,
        /// Whether the tag was omitted.
        implicit: bool,
    },
    /// Sequence node end.
    SequenceEnd,
    /// Mapping node start.
    MappingStart {
        /// Anchor name, or empty.
        anchor: String,
        /// Resolved tag, or empty.
        tag: String,
        /// Whether the tag was omitted.
        implicit: bool,
    },
    /// Mapping node end.
    MappingEnd,
}
