//! Token-by-token port of go-yaml v2.4.3's `parserc.go`.
//!
//! The original parser is Copyright 2006 Kirill Simonov and Copyright 2011-2016
//! Canonical Ltd, distributed under the MIT and Apache 2.0 licenses.

use std::collections::VecDeque;

use crate::types::{Error, ErrorKind, Event, EventKind, Mark, ScalarStyle, Token, TokenKind};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum State {
    #[default]
    StreamStart,
    ImplicitDocumentStart,
    DocumentStart,
    DocumentContent,
    DocumentEnd,
    BlockNode,
    BlockNodeOrIndentlessSequence,
    FlowNode,
    BlockSequenceFirstEntry,
    BlockSequenceEntry,
    BlockSequenceAfterEntry(Mark),
    IndentlessSequenceEntry,
    IndentlessSequenceAfterEntry(Mark),
    BlockMappingFirstKey,
    BlockMappingKey,
    BlockMappingAfterKey(Mark),
    BlockMappingValue,
    BlockMappingAfterValue(Mark),
    FlowSequenceFirstEntry,
    FlowSequenceEntry,
    FlowSequenceContent,
    FlowSequenceEntryMappingKey,
    FlowSequenceEntryMappingValue,
    FlowSequenceEntryMappingAfterValue(Mark),
    FlowSequenceEntryMappingEnd,
    FlowMappingFirstKey,
    FlowMappingKey,
    FlowMappingContent,
    FlowMappingAfterKey,
    FlowMappingValue,
    FlowMappingAfterValue,
    FlowMappingEmptyValue,
    End,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TagDirective {
    handle: String,
    prefix: String,
}

/// The grammar stack and lookahead of the pinned Go parser.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Parser {
    state: State,
    states: Vec<State>,
    marks: Vec<Mark>,
    tokens: VecDeque<Token>,
    tag_directives: Vec<TagDirective>,
    version_directive: bool,
    directive_start: Option<Mark>,
    done: bool,
}

/// A parser merge key with write-only scalar and alias payloads removed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ParserControl(Parser);

impl Parser {
    /// Supplies one token and emits all events it makes decidable.
    ///
    /// Port of `parserc.go:62` (`yaml_parser_parse`).
    ///
    /// # Errors
    ///
    /// Returns the pinned parser's production and directive failures.
    pub fn push(&mut self, token: Token) -> Result<Vec<Event>, Error> {
        if self.done {
            return Ok(Vec::new());
        }
        self.tokens.push_back(token);
        self.drain_events()
    }

    /// Emits any remaining events after the scanner's stream-end token.
    ///
    /// Port of `parserc.go:62` (`yaml_parser_parse`).
    ///
    /// # Errors
    ///
    /// Returns a grammar failure if a buffered token cannot continue the production.
    pub fn finish(&mut self) -> Result<Vec<Event>, Error> {
        self.drain_events()
    }

    /// Returns the suspended grammar state without write-only node content.
    ///
    /// Port of the state fields in `yamlh.go:583-593`.
    pub fn control(&self) -> ParserControl {
        let mut parser = self.clone();
        parser.state = control_state(parser.state);
        for state in &mut parser.states {
            *state = control_state(*state);
        }
        parser.marks.fill(Mark::default());
        parser.directive_start = parser.directive_start.map(|_| Mark::default());
        for token in &mut parser.tokens {
            token.start = Mark::default();
            token.end = Mark::default();
            match &mut token.kind {
                TokenKind::Scalar { value, .. } | TokenKind::Alias(value) => value.clear(),
                TokenKind::Anchor(value) if !value.is_empty() => *value = "_".to_owned(),
                TokenKind::StreamStart
                | TokenKind::StreamEnd
                | TokenKind::VersionDirective(..)
                | TokenKind::TagDirective(..)
                | TokenKind::DocumentStart
                | TokenKind::DocumentEnd
                | TokenKind::BlockSequenceStart
                | TokenKind::BlockMappingStart
                | TokenKind::BlockEnd
                | TokenKind::FlowSequenceStart
                | TokenKind::FlowSequenceEnd
                | TokenKind::FlowMappingStart
                | TokenKind::FlowMappingEnd
                | TokenKind::BlockEntry
                | TokenKind::FlowEntry
                | TokenKind::Key
                | TokenKind::Value
                | TokenKind::Anchor(_)
                | TokenKind::Tag(..) => {}
            }
        }
        ParserControl(parser)
    }

    /// Port of `parserc.go:93` (`yaml_parser_state_machine`).
    fn drain_events(&mut self) -> Result<Vec<Event>, Error> {
        let mut events = Vec::new();
        while !self.done && !self.tokens.is_empty() {
            match self.next_event()? {
                Progress::Event(event) => {
                    if matches!(
                        event.kind,
                        EventKind::DocumentEnd { .. } | EventKind::StreamEnd
                    ) {
                        self.done = true;
                    }
                    events.push(event);
                }
                Progress::Continue => {}
                Progress::Wait => break,
            }
        }
        Ok(events)
    }

    /// Port of `parserc.go:93-171` (`yaml_parser_state_machine`).
    fn next_event(&mut self) -> Result<Progress, Error> {
        let Some(token) = self.tokens.front().cloned() else {
            return Ok(Progress::Wait);
        };
        match self.state {
            State::StreamStart => self.stream_start(&token),
            State::ImplicitDocumentStart => self.document_start(token, true),
            State::DocumentStart => self.document_start(token, false),
            State::DocumentContent => self.document_content(&token),
            State::DocumentEnd => Ok(self.document_end(&token)),
            State::BlockNode => self.node(true, false),
            State::BlockNodeOrIndentlessSequence => self.node(true, true),
            State::FlowNode => self.node(false, false),
            State::BlockSequenceFirstEntry => {
                self.open_collection(&token, State::BlockSequenceEntry);
                Ok(Progress::Continue)
            }
            State::BlockSequenceEntry => self.block_sequence_entry(&token),
            State::BlockSequenceAfterEntry(mark) => {
                Ok(self.block_sequence_after_entry(&token, mark))
            }
            State::IndentlessSequenceEntry => self.indentless_sequence_entry(&token),
            State::IndentlessSequenceAfterEntry(mark) => {
                Ok(self.indentless_sequence_after_entry(&token, mark))
            }
            State::BlockMappingFirstKey => {
                self.open_collection(&token, State::BlockMappingKey);
                Ok(Progress::Continue)
            }
            State::BlockMappingKey => self.block_mapping_key(&token),
            State::BlockMappingAfterKey(mark) => Ok(self.block_mapping_after_key(&token, mark)),
            State::BlockMappingValue => Ok(self.block_mapping_value(&token)),
            State::BlockMappingAfterValue(mark) => Ok(self.block_mapping_after_value(&token, mark)),
            State::FlowSequenceFirstEntry => {
                self.open_collection(&token, State::FlowSequenceContent);
                Ok(Progress::Continue)
            }
            State::FlowSequenceEntry => self.flow_sequence_entry(&token),
            State::FlowSequenceContent => self.flow_sequence_content(&token),
            State::FlowSequenceEntryMappingKey => Ok(self.flow_sequence_mapping_key(&token)),
            State::FlowSequenceEntryMappingValue => Ok(self.flow_sequence_mapping_value(&token)),
            State::FlowSequenceEntryMappingAfterValue(mark) => {
                Ok(self.flow_sequence_mapping_after_value(&token, mark))
            }
            State::FlowSequenceEntryMappingEnd => {
                self.state = State::FlowSequenceEntry;
                Ok(Progress::Event(event(
                    EventKind::MappingEnd,
                    token.start,
                    token.start,
                )))
            }
            State::FlowMappingFirstKey => {
                self.open_collection(&token, State::FlowMappingContent);
                Ok(Progress::Continue)
            }
            State::FlowMappingKey => self.flow_mapping_key(&token),
            State::FlowMappingContent => self.flow_mapping_content(&token),
            State::FlowMappingAfterKey => Ok(self.flow_mapping_after_key(&token)),
            State::FlowMappingValue => Ok(self.flow_mapping_value(&token)),
            State::FlowMappingAfterValue => Ok(self.flow_mapping_after_value(&token)),
            State::FlowMappingEmptyValue => {
                self.state = State::FlowMappingKey;
                Ok(Progress::Event(empty_scalar(token.start)))
            }
            State::End => Ok(Progress::Wait),
        }
    }

    /// Port of `parserc.go:174` (`yaml_parser_parse_stream_start`).
    fn stream_start(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind != TokenKind::StreamStart {
            return Err(parser_error(
                "did not find expected <stream-start>",
                token.start,
                None,
            ));
        }
        self.tokens.pop_front();
        self.state = State::ImplicitDocumentStart;
        Ok(Progress::Event(event(
            EventKind::StreamStart,
            token.start,
            token.end,
        )))
    }

    /// Port of `parserc.go:198` (`yaml_parser_parse_document_start`).
    fn document_start(&mut self, token: Token, implicit: bool) -> Result<Progress, Error> {
        if !implicit && token.kind == TokenKind::DocumentEnd {
            self.tokens.pop_front();
            return Ok(Progress::Continue);
        }
        if implicit
            && self.directive_start.is_none()
            && !matches!(
                token.kind,
                TokenKind::VersionDirective(..)
                    | TokenKind::TagDirective(..)
                    | TokenKind::DocumentStart
                    | TokenKind::StreamEnd
            )
        {
            self.default_directives();
            self.states.push(State::DocumentEnd);
            self.state = State::BlockNode;
            return Ok(Progress::Event(event(
                EventKind::DocumentStart { implicit: false },
                token.start,
                token.end,
            )));
        }
        if token.kind == TokenKind::StreamEnd && self.directive_start.is_none() {
            self.tokens.pop_front();
            self.state = State::End;
            return Ok(Progress::Event(event(
                EventKind::StreamEnd,
                token.start,
                token.end,
            )));
        }
        match token.kind {
            TokenKind::VersionDirective(major, minor) => {
                self.directive_start.get_or_insert(token.start);
                if self.version_directive {
                    return Err(parser_error(
                        "found duplicate %YAML directive",
                        token.start,
                        None,
                    ));
                }
                if major != 1 || minor != 1 {
                    return Err(parser_error(
                        "found incompatible YAML document",
                        token.start,
                        None,
                    ));
                }
                self.version_directive = true;
                self.tokens.pop_front();
                return Ok(Progress::Continue);
            }
            TokenKind::TagDirective(handle, prefix) => {
                self.directive_start.get_or_insert(token.start);
                self.append_tag_directive(TagDirective { handle, prefix }, token.start)?;
                self.tokens.pop_front();
                return Ok(Progress::Continue);
            }
            TokenKind::StreamStart
            | TokenKind::StreamEnd
            | TokenKind::DocumentStart
            | TokenKind::DocumentEnd
            | TokenKind::BlockSequenceStart
            | TokenKind::BlockMappingStart
            | TokenKind::BlockEnd
            | TokenKind::FlowSequenceStart
            | TokenKind::FlowSequenceEnd
            | TokenKind::FlowMappingStart
            | TokenKind::FlowMappingEnd
            | TokenKind::BlockEntry
            | TokenKind::FlowEntry
            | TokenKind::Key
            | TokenKind::Value
            | TokenKind::Alias(_)
            | TokenKind::Anchor(_)
            | TokenKind::Tag(..)
            | TokenKind::Scalar { .. } => {}
        }
        self.default_directives();
        if token.kind != TokenKind::DocumentStart {
            return Err(parser_error(
                "did not find expected <document start>",
                token.start,
                None,
            ));
        }
        let start = self.directive_start.take().unwrap_or(token.start);
        self.states.push(State::DocumentEnd);
        self.state = State::DocumentContent;
        self.tokens.pop_front();
        Ok(Progress::Event(event(
            EventKind::DocumentStart { implicit: false },
            start,
            token.end,
        )))
    }

    /// Port of `parserc.go:282` (`yaml_parser_parse_document_content`).
    fn document_content(&mut self, token: &Token) -> Result<Progress, Error> {
        if matches!(
            token.kind,
            TokenKind::VersionDirective(..)
                | TokenKind::TagDirective(..)
                | TokenKind::DocumentStart
                | TokenKind::DocumentEnd
                | TokenKind::StreamEnd
        ) {
            self.return_state(token.start)?;
            return Ok(Progress::Event(empty_scalar(token.start)));
        }
        self.state = State::BlockNode;
        Ok(Progress::Continue)
    }

    /// Port of `parserc.go:305` (`yaml_parser_parse_document_end`).
    fn document_end(&mut self, token: &Token) -> Progress {
        let implicit = token.kind != TokenKind::DocumentEnd;
        let end = if implicit { token.start } else { token.end };
        if !implicit {
            self.tokens.pop_front();
        }
        self.tag_directives.clear();
        self.state = State::DocumentStart;
        Progress::Event(event(EventKind::DocumentEnd { implicit }, token.start, end))
    }

    /// Port of `parserc.go:359-576` (`yaml_parser_parse_node`).
    #[expect(
        clippy::too_many_lines,
        reason = "preserves Go's ordered node alternatives and their shared anchor, tag and token-consumption registers"
    )]
    fn node(&mut self, block: bool, indentless: bool) -> Result<Progress, Error> {
        let Some(first) = self.tokens.front() else {
            return Ok(Progress::Wait);
        };
        if let TokenKind::Alias(anchor) = &first.kind {
            let output = event(EventKind::Alias(anchor.clone()), first.start, first.end);
            let mark = first.start;
            self.tokens.pop_front();
            self.return_state(mark)?;
            return Ok(Progress::Event(output));
        }
        let start = first.start;
        let mut end = start;
        let mut anchor = String::new();
        let mut tag_parts = None;
        let mut consumed = 0;
        match &first.kind {
            TokenKind::Anchor(value) => {
                anchor.clone_from(value);
                end = first.end;
                consumed += 1;
                let Some(next) = self.tokens.get(consumed) else {
                    return Ok(Progress::Wait);
                };
                if let TokenKind::Tag(handle, suffix) = &next.kind {
                    tag_parts = Some(TagParts {
                        handle: handle.clone(),
                        suffix: suffix.clone(),
                        mark: next.start,
                    });
                    end = next.end;
                    consumed += 1;
                }
            }
            TokenKind::Tag(handle, suffix) => {
                tag_parts = Some(TagParts {
                    handle: handle.clone(),
                    suffix: suffix.clone(),
                    mark: first.start,
                });
                end = first.end;
                consumed += 1;
                let Some(next) = self.tokens.get(consumed) else {
                    return Ok(Progress::Wait);
                };
                if let TokenKind::Anchor(value) = &next.kind {
                    anchor.clone_from(value);
                    end = next.end;
                    consumed += 1;
                }
            }
            TokenKind::StreamStart
            | TokenKind::StreamEnd
            | TokenKind::VersionDirective(..)
            | TokenKind::TagDirective(..)
            | TokenKind::DocumentStart
            | TokenKind::DocumentEnd
            | TokenKind::BlockSequenceStart
            | TokenKind::BlockMappingStart
            | TokenKind::BlockEnd
            | TokenKind::FlowSequenceStart
            | TokenKind::FlowSequenceEnd
            | TokenKind::FlowMappingStart
            | TokenKind::FlowMappingEnd
            | TokenKind::BlockEntry
            | TokenKind::FlowEntry
            | TokenKind::Key
            | TokenKind::Value
            | TokenKind::Alias(_)
            | TokenKind::Scalar { .. } => {}
        }
        let Some(token) = self.tokens.get(consumed).cloned() else {
            return Ok(Progress::Wait);
        };
        let tag = self.resolve_tag(tag_parts, start)?;
        let implicit = tag.is_empty();
        let output = if indentless && token.kind == TokenKind::BlockEntry {
            self.state = State::IndentlessSequenceEntry;
            event(
                EventKind::SequenceStart {
                    anchor,
                    tag,
                    implicit,
                },
                start,
                token.end,
            )
        } else {
            match token.kind {
                TokenKind::Scalar { value, style } => {
                    let plain_implicit =
                        (tag.is_empty() && style == ScalarStyle::Plain) || tag == "!";
                    consumed += 1;
                    self.return_state(start)?;
                    event(
                        EventKind::Scalar {
                            anchor,
                            tag,
                            value,
                            style,
                            implicit: plain_implicit,
                        },
                        start,
                        token.end,
                    )
                }
                TokenKind::FlowSequenceStart => {
                    self.state = State::FlowSequenceFirstEntry;
                    event(
                        EventKind::SequenceStart {
                            anchor,
                            tag,
                            implicit,
                        },
                        start,
                        token.end,
                    )
                }
                TokenKind::FlowMappingStart => {
                    self.state = State::FlowMappingFirstKey;
                    event(
                        EventKind::MappingStart {
                            anchor,
                            tag,
                            implicit,
                        },
                        start,
                        token.end,
                    )
                }
                TokenKind::BlockSequenceStart if block => {
                    self.state = State::BlockSequenceFirstEntry;
                    event(
                        EventKind::SequenceStart {
                            anchor,
                            tag,
                            implicit,
                        },
                        start,
                        token.end,
                    )
                }
                TokenKind::BlockMappingStart if block => {
                    self.state = State::BlockMappingFirstKey;
                    event(
                        EventKind::MappingStart {
                            anchor,
                            tag,
                            implicit,
                        },
                        start,
                        token.end,
                    )
                }
                TokenKind::StreamStart
                | TokenKind::StreamEnd
                | TokenKind::VersionDirective(..)
                | TokenKind::TagDirective(..)
                | TokenKind::DocumentStart
                | TokenKind::DocumentEnd
                | TokenKind::BlockSequenceStart
                | TokenKind::BlockMappingStart
                | TokenKind::BlockEnd
                | TokenKind::FlowSequenceEnd
                | TokenKind::FlowMappingEnd
                | TokenKind::BlockEntry
                | TokenKind::FlowEntry
                | TokenKind::Key
                | TokenKind::Value
                | TokenKind::Alias(_)
                | TokenKind::Anchor(_)
                | TokenKind::Tag(..) => {
                    if anchor.is_empty() && tag.is_empty() {
                        return Err(parser_error(
                            "did not find expected node content",
                            token.start,
                            Some(start),
                        ));
                    }
                    self.return_state(start)?;
                    event(
                        EventKind::Scalar {
                            anchor,
                            tag,
                            value: String::new(),
                            style: ScalarStyle::Plain,
                            implicit,
                        },
                        start,
                        end,
                    )
                }
            }
        };
        self.tokens.drain(..consumed);
        Ok(Progress::Event(output))
    }

    /// Port of `parserc.go:359` (`yaml_parser_parse_node`, tag resolution).
    fn resolve_tag(&self, parts: Option<TagParts>, start: Mark) -> Result<String, Error> {
        let Some(TagParts {
            handle,
            suffix,
            mark,
        }) = parts
        else {
            return Ok(String::new());
        };
        if handle.is_empty() {
            return Ok(suffix);
        }
        let directive = self
            .tag_directives
            .iter()
            .find(|directive| directive.handle == handle)
            .ok_or_else(|| parser_error("found undefined tag handle", mark, Some(start)))?;
        Ok(format!("{}{suffix}", directive.prefix))
    }

    /// Port of `parserc.go:579` (`yaml_parser_parse_block_sequence_entry`).
    fn block_sequence_entry(&mut self, token: &Token) -> Result<Progress, Error> {
        match token.kind {
            TokenKind::BlockEntry => {
                self.tokens.pop_front();
                self.state = State::BlockSequenceAfterEntry(token.end);
                Ok(Progress::Continue)
            }
            TokenKind::BlockEnd => self.close_collection(token, EventKind::SequenceEnd),
            TokenKind::StreamStart
            | TokenKind::StreamEnd
            | TokenKind::VersionDirective(..)
            | TokenKind::TagDirective(..)
            | TokenKind::DocumentStart
            | TokenKind::DocumentEnd
            | TokenKind::BlockSequenceStart
            | TokenKind::BlockMappingStart
            | TokenKind::FlowSequenceStart
            | TokenKind::FlowSequenceEnd
            | TokenKind::FlowMappingStart
            | TokenKind::FlowMappingEnd
            | TokenKind::FlowEntry
            | TokenKind::Key
            | TokenKind::Value
            | TokenKind::Alias(_)
            | TokenKind::Anchor(_)
            | TokenKind::Tag(..)
            | TokenKind::Scalar { .. } => Err(parser_error(
                "did not find expected '-' indicator",
                token.start,
                self.marks.last().copied(),
            )),
        }
    }

    /// Port of `parserc.go:579` (`yaml_parser_parse_block_sequence_entry`).
    fn block_sequence_after_entry(&mut self, token: &Token, mark: Mark) -> Progress {
        if matches!(token.kind, TokenKind::BlockEntry | TokenKind::BlockEnd) {
            self.state = State::BlockSequenceEntry;
            return Progress::Event(empty_scalar(mark));
        }
        self.states.push(State::BlockSequenceEntry);
        self.state = State::BlockNode;
        Progress::Continue
    }

    /// Port of `parserc.go:631` (`yaml_parser_parse_indentless_sequence_entry`).
    fn indentless_sequence_entry(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::BlockEntry {
            self.tokens.pop_front();
            self.state = State::IndentlessSequenceAfterEntry(token.end);
            return Ok(Progress::Continue);
        }
        self.return_state(token.start)?;
        Ok(Progress::Event(event(
            EventKind::SequenceEnd,
            token.start,
            token.start,
        )))
    }

    /// Port of `parserc.go:631` (`yaml_parser_parse_indentless_sequence_entry`).
    fn indentless_sequence_after_entry(&mut self, token: &Token, mark: Mark) -> Progress {
        if matches!(
            token.kind,
            TokenKind::BlockEntry | TokenKind::Key | TokenKind::Value | TokenKind::BlockEnd
        ) {
            self.state = State::IndentlessSequenceEntry;
            return Progress::Event(empty_scalar(mark));
        }
        self.states.push(State::IndentlessSequenceEntry);
        self.state = State::BlockNode;
        Progress::Continue
    }

    /// Port of `parserc.go:675` (`yaml_parser_parse_block_mapping_key`).
    fn block_mapping_key(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::Key {
            self.tokens.pop_front();
            self.state = State::BlockMappingAfterKey(token.end);
            return Ok(Progress::Continue);
        }
        if token.kind == TokenKind::BlockEnd {
            return self.close_collection(token, EventKind::MappingEnd);
        }
        Err(parser_error(
            "did not find expected key",
            token.start,
            self.marks.last().copied(),
        ))
    }

    /// Port of `parserc.go:675` (`yaml_parser_parse_block_mapping_key`).
    fn block_mapping_after_key(&mut self, token: &Token, mark: Mark) -> Progress {
        if matches!(
            token.kind,
            TokenKind::Key | TokenKind::Value | TokenKind::BlockEnd
        ) {
            self.state = State::BlockMappingValue;
            return Progress::Event(empty_scalar(mark));
        }
        self.states.push(State::BlockMappingValue);
        self.state = State::BlockNodeOrIndentlessSequence;
        Progress::Continue
    }

    /// Port of `parserc.go:733` (`yaml_parser_parse_block_mapping_value`).
    fn block_mapping_value(&mut self, token: &Token) -> Progress {
        if token.kind == TokenKind::Value {
            self.tokens.pop_front();
            self.state = State::BlockMappingAfterValue(token.end);
            return Progress::Continue;
        }
        self.state = State::BlockMappingKey;
        Progress::Event(empty_scalar(token.start))
    }

    /// Port of `parserc.go:733` (`yaml_parser_parse_block_mapping_value`).
    fn block_mapping_after_value(&mut self, token: &Token, mark: Mark) -> Progress {
        if matches!(
            token.kind,
            TokenKind::Key | TokenKind::Value | TokenKind::BlockEnd
        ) {
            self.state = State::BlockMappingKey;
            return Progress::Event(empty_scalar(mark));
        }
        self.states.push(State::BlockMappingKey);
        self.state = State::BlockNodeOrIndentlessSequence;
        Progress::Continue
    }

    /// Port of `parserc.go:770` (`yaml_parser_parse_flow_sequence_entry`).
    fn flow_sequence_entry(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::FlowSequenceEnd {
            return self.close_collection(token, EventKind::SequenceEnd);
        }
        if token.kind != TokenKind::FlowEntry {
            return Err(parser_error(
                "did not find expected ',' or ']'",
                token.start,
                self.marks.last().copied(),
            ));
        }
        self.tokens.pop_front();
        self.state = State::FlowSequenceContent;
        Ok(Progress::Continue)
    }

    /// Port of `parserc.go:770` (`yaml_parser_parse_flow_sequence_entry`).
    fn flow_sequence_content(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::Key {
            self.state = State::FlowSequenceEntryMappingKey;
            self.tokens.pop_front();
            return Ok(Progress::Event(event(
                EventKind::MappingStart {
                    anchor: String::new(),
                    tag: String::new(),
                    implicit: true,
                },
                token.start,
                token.end,
            )));
        }
        if token.kind == TokenKind::FlowSequenceEnd {
            return self.close_collection(token, EventKind::SequenceEnd);
        }
        self.states.push(State::FlowSequenceEntry);
        self.state = State::FlowNode;
        Ok(Progress::Continue)
    }

    /// Port of `parserc.go:833` (`yaml_parser_parse_flow_sequence_entry_mapping_key`).
    fn flow_sequence_mapping_key(&mut self, token: &Token) -> Progress {
        if !matches!(
            token.kind,
            TokenKind::Value | TokenKind::FlowEntry | TokenKind::FlowSequenceEnd
        ) {
            self.states.push(State::FlowSequenceEntryMappingValue);
            self.state = State::FlowNode;
            return Progress::Continue;
        }
        self.tokens.pop_front();
        self.state = State::FlowSequenceEntryMappingValue;
        Progress::Event(empty_scalar(token.end))
    }

    /// Port of `parserc.go:854` (`yaml_parser_parse_flow_sequence_entry_mapping_value`).
    fn flow_sequence_mapping_value(&mut self, token: &Token) -> Progress {
        if token.kind == TokenKind::Value {
            self.tokens.pop_front();
            self.state = State::FlowSequenceEntryMappingAfterValue(token.start);
            return Progress::Continue;
        }
        self.state = State::FlowSequenceEntryMappingEnd;
        Progress::Event(empty_scalar(token.start))
    }

    /// Port of `parserc.go:854` (`yaml_parser_parse_flow_sequence_entry_mapping_value`).
    fn flow_sequence_mapping_after_value(&mut self, token: &Token, mark: Mark) -> Progress {
        if matches!(
            token.kind,
            TokenKind::FlowEntry | TokenKind::FlowSequenceEnd
        ) {
            self.state = State::FlowSequenceEntryMappingEnd;
            return Progress::Event(empty_scalar(mark));
        }
        self.states.push(State::FlowSequenceEntryMappingEnd);
        self.state = State::FlowNode;
        Progress::Continue
    }

    /// Port of `parserc.go:904` (`yaml_parser_parse_flow_mapping_key`).
    fn flow_mapping_key(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::FlowMappingEnd {
            return self.close_collection(token, EventKind::MappingEnd);
        }
        if token.kind != TokenKind::FlowEntry {
            return Err(parser_error(
                "did not find expected ',' or '}'",
                token.start,
                self.marks.last().copied(),
            ));
        }
        self.tokens.pop_front();
        self.state = State::FlowMappingContent;
        Ok(Progress::Continue)
    }

    /// Port of `parserc.go:904` (`yaml_parser_parse_flow_mapping_key`).
    fn flow_mapping_content(&mut self, token: &Token) -> Result<Progress, Error> {
        if token.kind == TokenKind::Key {
            self.tokens.pop_front();
            self.state = State::FlowMappingAfterKey;
            return Ok(Progress::Continue);
        }
        if token.kind == TokenKind::FlowMappingEnd {
            return self.close_collection(token, EventKind::MappingEnd);
        }
        self.states.push(State::FlowMappingEmptyValue);
        self.state = State::FlowNode;
        Ok(Progress::Continue)
    }

    /// Port of `parserc.go:904` (`yaml_parser_parse_flow_mapping_key`).
    fn flow_mapping_after_key(&mut self, token: &Token) -> Progress {
        if matches!(
            token.kind,
            TokenKind::Value | TokenKind::FlowEntry | TokenKind::FlowMappingEnd
        ) {
            self.state = State::FlowMappingValue;
            return Progress::Event(empty_scalar(token.start));
        }
        self.states.push(State::FlowMappingValue);
        self.state = State::FlowNode;
        Progress::Continue
    }

    /// Port of `parserc.go:970` (`yaml_parser_parse_flow_mapping_value`).
    fn flow_mapping_value(&mut self, token: &Token) -> Progress {
        if token.kind == TokenKind::Value {
            self.tokens.pop_front();
            self.state = State::FlowMappingAfterValue;
            return Progress::Continue;
        }
        self.state = State::FlowMappingKey;
        Progress::Event(empty_scalar(token.start))
    }

    /// Port of `parserc.go:970` (`yaml_parser_parse_flow_mapping_value`).
    fn flow_mapping_after_value(&mut self, token: &Token) -> Progress {
        if matches!(token.kind, TokenKind::FlowEntry | TokenKind::FlowMappingEnd) {
            self.state = State::FlowMappingKey;
            return Progress::Event(empty_scalar(token.start));
        }
        self.states.push(State::FlowMappingKey);
        self.state = State::FlowNode;
        Progress::Continue
    }

    /// Port of `parserc.go:579,675,770,904` (collection opening).
    fn open_collection(&mut self, token: &Token, state: State) {
        self.marks.push(token.start);
        self.tokens.pop_front();
        self.state = state;
    }

    /// Port of `parserc.go:579,675,770,904` (collection closing).
    fn close_collection(&mut self, token: &Token, kind: EventKind) -> Result<Progress, Error> {
        self.return_state(token.start)?;
        self.marks.pop();
        self.tokens.pop_front();
        Ok(Progress::Event(event(kind, token.start, token.end)))
    }

    /// Port of `parserc.go:359` (return through the parser state stack).
    fn return_state(&mut self, mark: Mark) -> Result<(), Error> {
        self.state = self
            .states
            .pop()
            .ok_or_else(|| parser_error("invalid parser state stack", mark, None))?;
        Ok(())
    }

    /// Port of `parserc.go:1013` (`yaml_parser_process_directives`).
    fn default_directives(&mut self) {
        for (handle, prefix) in [("!", "!"), ("!!", "tag:yaml.org,2002:")] {
            if !self
                .tag_directives
                .iter()
                .any(|directive| directive.handle == handle)
            {
                self.tag_directives.push(TagDirective {
                    handle: handle.to_owned(),
                    prefix: prefix.to_owned(),
                });
            }
        }
    }

    /// Port of `parserc.go:1075` (`yaml_parser_append_tag_directive`).
    fn append_tag_directive(&mut self, directive: TagDirective, mark: Mark) -> Result<(), Error> {
        if self
            .tag_directives
            .iter()
            .any(|entry| entry.handle == directive.handle)
        {
            return Err(parser_error("found duplicate %TAG directive", mark, None));
        }
        self.tag_directives.push(directive);
        Ok(())
    }
}

fn control_state(state: State) -> State {
    match state {
        State::BlockSequenceAfterEntry(_) => State::BlockSequenceAfterEntry(Mark::default()),
        State::IndentlessSequenceAfterEntry(_) => {
            State::IndentlessSequenceAfterEntry(Mark::default())
        }
        State::BlockMappingAfterKey(_) => State::BlockMappingAfterKey(Mark::default()),
        State::BlockMappingAfterValue(_) => State::BlockMappingAfterValue(Mark::default()),
        State::FlowSequenceEntryMappingAfterValue(_) => {
            State::FlowSequenceEntryMappingAfterValue(Mark::default())
        }
        State::StreamStart
        | State::ImplicitDocumentStart
        | State::DocumentStart
        | State::DocumentContent
        | State::DocumentEnd
        | State::BlockNode
        | State::BlockNodeOrIndentlessSequence
        | State::FlowNode
        | State::BlockSequenceFirstEntry
        | State::BlockSequenceEntry
        | State::IndentlessSequenceEntry
        | State::BlockMappingFirstKey
        | State::BlockMappingKey
        | State::BlockMappingValue
        | State::FlowSequenceFirstEntry
        | State::FlowSequenceEntry
        | State::FlowSequenceContent
        | State::FlowSequenceEntryMappingKey
        | State::FlowSequenceEntryMappingValue
        | State::FlowSequenceEntryMappingEnd
        | State::FlowMappingFirstKey
        | State::FlowMappingKey
        | State::FlowMappingContent
        | State::FlowMappingAfterKey
        | State::FlowMappingValue
        | State::FlowMappingAfterValue
        | State::FlowMappingEmptyValue
        | State::End => state,
    }
}

enum Progress {
    Event(Event),
    Continue,
    Wait,
}

struct TagParts {
    handle: String,
    suffix: String,
    mark: Mark,
}

fn event(kind: EventKind, start: Mark, end: Mark) -> Event {
    Event { kind, start, end }
}

/// Port of `parserc.go:995` (`yaml_parser_process_empty_scalar`).
fn empty_scalar(mark: Mark) -> Event {
    event(
        EventKind::Scalar {
            anchor: String::new(),
            tag: String::new(),
            value: String::new(),
            style: ScalarStyle::Plain,
            implicit: true,
        },
        mark,
        mark,
    )
}

/// Port of `parserc.go:76,83` and `decode.go:111` (error presentation).
fn parser_error(message: &str, problem: Mark, context: Option<Mark>) -> Error {
    Error {
        kind: ErrorKind::Parser,
        line: if problem.line == 0 {
            context.map_or(0, |mark| mark.line)
        } else {
            problem.line
        },
        message: message.to_owned(),
    }
}
