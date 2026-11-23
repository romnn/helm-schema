//! Resumable port of go-yaml v2.4.3's `scannerc.go`.
//!
//! The original scanner is Copyright 2006 Kirill Simonov and Copyright 2011-2016
//! Canonical Ltd, distributed under the MIT and Apache 2.0 licenses.

use std::collections::{BTreeMap, VecDeque};

use crate::types::{Error, ErrorKind, Mark, Token, TokenKind};

mod scalar;
mod tag;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct SimpleKey {
    possible: bool,
    required: bool,
    number: usize,
    mark: Mark,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
enum At {
    #[default]
    Between,
    Dispatch,
    Comment,
    TriviaBreak,
    Plain(scalar::Plain),
    Quoted(scalar::Quoted),
    Block(scalar::Block),
    Anchor {
        start: Mark,
        alias: bool,
        value: String,
    },
    Tag(tag::Tag),
    Directive(tag::Directive),
}

/// The scanner's complete suspended control state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Scanner {
    at: At,
    look: VecDeque<char>,
    mark: Mark,
    flow_level: usize,
    indent: i32,
    indents: Vec<i32>,
    simple_key_allowed: bool,
    simple_keys: Vec<SimpleKey>,
    simple_keys_by_tok: BTreeMap<usize, usize>,
    held: VecDeque<Token>,
    parsed: usize,
    started: bool,
    ended: bool,
    eof: bool,
    fetching: bool,
    buffer_head: Option<char>,
    buffer_end: usize,
    buffer_open: bool,
    cache_wait: Option<usize>,
}

/// Scanner control with scalar, anchor and token payloads kept outside equality.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScannerControl(Scanner);

impl Default for Scanner {
    fn default() -> Self {
        Self {
            at: At::Between,
            look: VecDeque::new(),
            mark: Mark::default(),
            flow_level: 0,
            indent: -1,
            indents: Vec::new(),
            simple_key_allowed: true,
            simple_keys: vec![SimpleKey::default()],
            simple_keys_by_tok: BTreeMap::new(),
            held: VecDeque::new(),
            parsed: 0,
            started: false,
            ended: false,
            eof: false,
            fetching: false,
            buffer_head: None,
            buffer_end: 0,
            buffer_open: true,
            cache_wait: None,
        }
    }
}

impl Scanner {
    /// Projects control state, retaining every register consulted by a transition.
    pub fn control(&self) -> ScannerControl {
        let mut scanner = self.clone();
        match &mut scanner.at {
            At::Plain(scalar) => scalar.clear_payload(),
            At::Quoted(scalar) => scalar.clear_payload(),
            At::Block(scalar) => scalar.clear_payload(),
            At::Anchor { start, value, .. } => {
                *start = Mark::default();
                if !value.is_empty() {
                    *value = "a".to_owned();
                }
            }
            At::Tag(tag) => tag.clear_payload(),
            At::Directive(directive) => directive.clear_payload(),
            At::Between | At::Dispatch | At::Comment | At::TriviaBreak => {}
        }
        for token in &mut scanner.held {
            token.start = Mark::default();
            token.end = Mark::default();
            clear_token_payload(&mut token.kind);
        }
        for key in &mut scanner.simple_keys {
            if key.possible {
                key.mark.line = usize::from(key.mark.line == self.mark.line);
                key.mark.index = 1025 - self.mark.index.saturating_sub(key.mark.index).min(1025);
                key.number = key
                    .number
                    .checked_sub(self.parsed)
                    .map_or(0, |number| 1025 + number);
            } else {
                let number = key
                    .number
                    .checked_sub(self.parsed)
                    .map_or(0, |number| 1025 + number);
                *key = SimpleKey {
                    number,
                    ..SimpleKey::default()
                };
            }
        }
        scanner.simple_keys_by_tok = scanner
            .simple_keys_by_tok
            .into_iter()
            .filter_map(|(number, level)| {
                number
                    .checked_sub(self.parsed)
                    .map(|number| (1025 + number, level))
            })
            .collect();
        scanner.mark.line = 1;
        scanner.mark.index = 1025;
        scanner.buffer_end = if self.buffer_open {
            0
        } else {
            1025 + self.buffer_end.saturating_sub(self.mark.index)
        };
        if scanner.buffer_head.is_some() && scanner.buffer_head != Some('\u{feff}') {
            scanner.buffer_head = Some('a');
        }
        scanner.parsed = 1025;
        ScannerControl(scanner)
    }

    /// Supplies one decoded character without scanning past a parser's demand.
    pub fn push_input(&mut self, character: char) {
        if !self.eof && !self.ended {
            if self.buffer_head.is_none() {
                self.buffer_head = Some(character);
            }
            self.look.push_back(character);
            self.buffer_end = self.buffer_end.max(self.mark.index + self.look.len());
        }
    }

    /// Records a completed raw window before resuming Go's decoded-cache refill.
    pub fn close_buffer(&mut self, decoded_end: usize) {
        self.buffer_end = decoded_end;
        self.buffer_open = false;
    }

    /// Allows another raw window to supply the characters requested by `cache`.
    pub fn open_buffer(&mut self) {
        self.buffer_open = true;
    }

    /// Marks the end of the decoded stream.
    pub fn finish_input(&mut self) {
        self.eof = true;
    }

    /// Returns the next token, or suspends until another character arrives.
    ///
    /// Ports `yaml_parser_scan` (`scannerc.go:571`) and
    /// `yaml_parser_fetch_more_tokens` (`scannerc.go:626`).
    ///
    /// # Errors
    ///
    /// Returns the pinned scanner's syntax or depth error.
    pub fn next_token(&mut self) -> Result<Option<Token>, Error> {
        loop {
            if !self.fetching && !self.held.is_empty() {
                let blocked =
                    if let Some(level) = self.simple_keys_by_tok.get(&self.parsed).copied() {
                        self.valid_key(level)?
                    } else {
                        false
                    };
                if !blocked {
                    self.parsed += 1;
                    return Ok(self.held.pop_front());
                }
            }
            if self.ended {
                return Ok(None);
            }
            self.fetching = true;
            if !self.advance()? {
                return Ok(None);
            }
        }
    }

    /// Ports `cache` (`scannerc.go:485`) and decoded-buffer refill (`readerc.go:91`).
    fn available(&mut self, count: usize) -> bool {
        let remaining = self.buffer_end.saturating_sub(self.mark.index);
        if let Some(wait) = self.cache_wait {
            if self.buffer_open || remaining >= wait {
                self.cache_wait = None;
            } else if self.eof {
                self.buffer_end = self.buffer_end.max(self.mark.index.saturating_add(wait));
                self.cache_wait = None;
            }
        } else if !self.buffer_open && remaining < count {
            self.buffer_head = self.look.front().copied();
            if self.eof {
                self.buffer_end = self.buffer_end.max(self.mark.index.saturating_add(count));
            } else {
                self.cache_wait = Some(count);
            }
        }
        self.eof || self.look.len() >= count
    }
    fn peek(&self, offset: usize) -> char {
        self.look.get(offset).copied().unwrap_or('\0')
    }

    /// Ports `skip`, `scannerc.go:491`.
    fn skip(&mut self) -> char {
        let character = self.look.pop_front().unwrap_or('\0');
        self.mark.index += 1;
        self.mark.column += 1;
        character
    }

    /// Ports `read_line` (`scannerc.go:538`) and `skip_line` (`scannerc.go:498`).
    fn line(&mut self) -> char {
        let character = self.look.pop_front().unwrap_or('\0');
        self.mark.index += 1;
        if character == '\r' && self.peek(0) == '\n' {
            self.look.pop_front();
            self.mark.index += 1;
        }
        self.mark.column = 0;
        self.mark.line += 1;
        if matches!(character, '\r' | '\n' | '\u{85}') {
            '\n'
        } else {
            character
        }
    }

    /// Ports `yaml_parser_set_scanner_error`, `scannerc.go:600`.
    fn error(&self, message: &str) -> Error {
        Error {
            kind: ErrorKind::Scanner,
            line: if self.mark.line == 0 {
                0
            } else {
                self.mark.line + 1
            },
            message: message.to_owned(),
        }
    }

    fn token(&mut self, kind: TokenKind, start: Mark, end: Mark) {
        if !matches!(
            kind,
            TokenKind::BlockEnd | TokenKind::BlockMappingStart | TokenKind::BlockSequenceStart
        ) {
            self.fetching = false;
        }
        self.held.push_back(Token { kind, start, end });
    }

    /// Ports `yaml_simple_key_is_valid`, `scannerc.go:822`.
    fn valid_key(&mut self, level: usize) -> Result<bool, Error> {
        let Some(key) = self.simple_keys.get(level) else {
            return Ok(false);
        };
        if !key.possible {
            return Ok(false);
        }
        if key.mark.line < self.mark.line || key.mark.index.saturating_add(1024) < self.mark.index {
            if key.required {
                return Err(self.error("could not find expected ':'"));
            }
            if let Some(key) = self.simple_keys.get_mut(level) {
                key.possible = false;
            }
            return Ok(false);
        }
        Ok(true)
    }

    /// Ports `yaml_parser_save_simple_key`, `scannerc.go:850`.
    fn save_key(&mut self) -> Result<(), Error> {
        if self.simple_key_allowed {
            self.remove_key()?;
            let required =
                self.flow_level == 0 && i32::try_from(self.mark.column).ok() == Some(self.indent);
            let number = self.parsed + self.held.len();
            if let Some(key) = self.simple_keys.last_mut() {
                *key = SimpleKey {
                    possible: true,
                    required,
                    number,
                    mark: self.mark,
                };
            }
            self.simple_keys_by_tok
                .insert(number, self.simple_keys.len().saturating_sub(1));
        }
        Ok(())
    }

    /// Ports `yaml_parser_remove_simple_key`, `scannerc.go:878`.
    fn remove_key(&mut self) -> Result<(), Error> {
        if self
            .simple_keys
            .last()
            .is_some_and(|key| key.possible && key.required)
        {
            return Err(self.error("could not find expected ':'"));
        }
        if let Some(key) = self.simple_keys.last_mut()
            && key.possible
        {
            key.possible = false;
            self.simple_keys_by_tok.remove(&key.number);
        }
        Ok(())
    }

    /// Ports `yaml_parser_roll_indent`, `scannerc.go:934`.
    fn roll_indent(
        &mut self,
        column: usize,
        number: Option<usize>,
        kind: TokenKind,
        mark: Mark,
    ) -> Result<(), Error> {
        if self.flow_level != 0 {
            return Ok(());
        }
        let column =
            i32::try_from(column).map_err(|_| self.error("exceeded max depth of 10000"))?;
        if self.indent < column {
            self.indents.push(self.indent);
            self.indent = column;
            if self.indents.len() > 10000 {
                return Err(self.error("exceeded max depth of 10000"));
            }
            let token = Token {
                kind,
                start: mark,
                end: mark,
            };
            if let Some(number) = number.filter(|number| *number >= self.parsed) {
                self.held
                    .insert((number - self.parsed).min(self.held.len()), token);
            } else {
                self.held.push_back(token);
            }
        }
        Ok(())
    }

    /// Ports `yaml_parser_unroll_indent`, `scannerc.go:968`.
    fn unroll_indent(&mut self, column: i32) {
        if self.flow_level != 0 {
            return;
        }
        while self.indent > column {
            self.token(TokenKind::BlockEnd, self.mark, self.mark);
            self.indent = self.indents.pop().unwrap_or(-1);
        }
    }

    /// Ports `yaml_parser_fetch_next_token`, `scannerc.go:652`.
    fn advance(&mut self) -> Result<bool, Error> {
        if !self.started {
            self.started = true;
            self.token(TokenKind::StreamStart, self.mark, self.mark);
            return Ok(true);
        }
        match std::mem::take(&mut self.at) {
            At::Between => self.between(),
            At::Dispatch => self.dispatch(),
            At::TriviaBreak => self.trivia_break(),
            At::Comment => {
                if !self.available(1) {
                    self.at = At::Comment;
                    return Ok(false);
                }
                if is_breakz(self.peek(0)) {
                    return self.trivia_break();
                }
                self.skip();
                self.at = At::Comment;
                Ok(true)
            }
            At::Plain(state) => self.plain(state),
            At::Quoted(state) => self.quoted(state),
            At::Block(state) => self.block(state),
            At::Anchor {
                start,
                alias,
                mut value,
            } => {
                if !self.available(1) {
                    self.at = At::Anchor {
                        start,
                        alias,
                        value,
                    };
                    return Ok(false);
                }
                let character = self.peek(0);
                if is_alpha(character) {
                    value.push(self.skip());
                    self.at = At::Anchor {
                        start,
                        alias,
                        value,
                    };
                } else {
                    if value.is_empty()
                        || !(is_blankz(character)
                            || matches!(character, '?' | ':' | ',' | ']' | '}' | '%' | '@' | '`'))
                    {
                        return Err(
                            self.error("did not find expected alphabetic or numeric character")
                        );
                    }
                    let kind = if alias {
                        TokenKind::Alias(value)
                    } else {
                        TokenKind::Anchor(value)
                    };
                    self.token(kind, start, self.mark);
                }
                Ok(true)
            }
            At::Tag(state) => self.tag(state),
            At::Directive(state) => self.directive(state),
        }
    }

    /// Ports `yaml_parser_scan_to_next_token`, `scannerc.go:1443`.
    fn between(&mut self) -> Result<bool, Error> {
        if !self.available(1) {
            return Ok(false);
        }
        let character = self.peek(0);
        if self.mark.column == 0 && self.buffer_head == Some('\u{feff}') {
            self.skip();
            return Ok(true);
        }
        if character == ' '
            || character == '\t' && (self.flow_level > 0 || !self.simple_key_allowed)
        {
            self.skip();
            return Ok(true);
        }
        if character == '#' {
            self.at = At::Comment;
            return Ok(true);
        }
        self.trivia_break()
    }

    /// Ports the trailing break branch of `yaml_parser_scan_to_next_token`, `scannerc.go:1443`.
    fn trivia_break(&mut self) -> Result<bool, Error> {
        let character = self.peek(0);
        if is_break(character) {
            if !self.available(2) {
                self.at = At::TriviaBreak;
                return Ok(false);
            }
            self.line();
            if self.flow_level == 0 {
                self.simple_key_allowed = true;
            }
            return Ok(true);
        }
        let column = i32::try_from(self.mark.column)
            .map_err(|_| self.error("exceeded max depth of 10000"))?;
        self.unroll_indent(column);
        self.dispatch()
    }

    /// Ports the indicator dispatch of `yaml_parser_fetch_next_token`, `scannerc.go:652`.
    #[expect(
        clippy::too_many_lines,
        reason = "The ordered dispatch follows scannerc.go exactly; splitting its overlapping indicator cases obscures their precedence."
    )]
    fn dispatch(&mut self) -> Result<bool, Error> {
        if !self.available(4) {
            self.at = At::Dispatch;
            return Ok(false);
        }
        let character = self.peek(0);
        let start = self.mark;
        if character == '\0' {
            if self.mark.column != 0 {
                self.mark.column = 0;
                self.mark.line += 1;
            }
            self.unroll_indent(-1);
            self.remove_key()?;
            self.simple_key_allowed = false;
            self.token(TokenKind::StreamEnd, self.mark, self.mark);
            self.ended = true;
            return Ok(true);
        }
        if self.mark.column == 0 && character == '%' {
            self.unroll_indent(-1);
            self.remove_key()?;
            self.simple_key_allowed = false;
            self.skip();
            self.at = At::Directive(tag::Directive::new(start));
            return Ok(true);
        }
        if self.document_indicator() {
            self.unroll_indent(-1);
            self.remove_key()?;
            self.simple_key_allowed = false;
            for _ in 0..3 {
                self.skip();
            }
            self.token(
                if character == '-' {
                    TokenKind::DocumentStart
                } else {
                    TokenKind::DocumentEnd
                },
                start,
                self.mark,
            );
            return Ok(true);
        }
        match character {
            '[' | '{' => {
                self.save_key()?;
                self.flow_level += 1;
                self.simple_keys.push(SimpleKey {
                    mark: self.mark,
                    number: self.parsed + self.held.len(),
                    ..SimpleKey::default()
                });
                if self.flow_level > 10000 {
                    return Err(self.error("exceeded max depth of 10000"));
                }
                self.simple_key_allowed = true;
                self.skip();
                self.token(
                    if character == '[' {
                        TokenKind::FlowSequenceStart
                    } else {
                        TokenKind::FlowMappingStart
                    },
                    start,
                    self.mark,
                );
            }
            ']' | '}' => {
                self.remove_key()?;
                if self.flow_level > 0 {
                    self.flow_level -= 1;
                    if let Some(key) = self.simple_keys.pop() {
                        self.simple_keys_by_tok.remove(&key.number);
                    }
                }
                self.simple_key_allowed = false;
                self.skip();
                self.token(
                    if character == ']' {
                        TokenKind::FlowSequenceEnd
                    } else {
                        TokenKind::FlowMappingEnd
                    },
                    start,
                    self.mark,
                );
            }
            ',' => {
                self.remove_key()?;
                self.simple_key_allowed = true;
                self.skip();
                self.token(TokenKind::FlowEntry, start, self.mark);
            }
            '-' if is_blankz(self.peek(1)) => {
                if self.flow_level == 0 {
                    if !self.simple_key_allowed {
                        return Err(
                            self.error("block sequence entries are not allowed in this context")
                        );
                    }
                    self.roll_indent(start.column, None, TokenKind::BlockSequenceStart, start)?;
                }
                self.remove_key()?;
                self.simple_key_allowed = true;
                self.skip();
                self.token(TokenKind::BlockEntry, start, self.mark);
            }
            '?' if self.flow_level > 0 || is_blankz(self.peek(1)) => {
                if self.flow_level == 0 {
                    if !self.simple_key_allowed {
                        return Err(self.error("mapping keys are not allowed in this context"));
                    }
                    self.roll_indent(start.column, None, TokenKind::BlockMappingStart, start)?;
                }
                self.remove_key()?;
                self.simple_key_allowed = self.flow_level == 0;
                self.skip();
                self.token(TokenKind::Key, start, self.mark);
            }
            ':' if self.flow_level > 0 || is_blankz(self.peek(1)) => self.value()?,
            '*' | '&' => {
                self.save_key()?;
                self.simple_key_allowed = false;
                self.skip();
                self.at = At::Anchor {
                    start,
                    alias: character == '*',
                    value: String::new(),
                };
            }
            '!' => {
                self.save_key()?;
                self.simple_key_allowed = false;
                self.at = At::Tag(tag::Tag::new(start));
            }
            '|' | '>' if self.flow_level == 0 => {
                self.remove_key()?;
                self.simple_key_allowed = true;
                self.skip();
                self.at = At::Block(scalar::Block::new(start, character == '|'));
            }
            '\'' | '"' => {
                self.save_key()?;
                self.simple_key_allowed = false;
                self.skip();
                self.at = At::Quoted(scalar::Quoted::new(start, character == '\''));
            }
            _ => {
                let allowed = !is_blankz(character)
                    && !matches!(
                        character,
                        '-' | '?'
                            | ':'
                            | ','
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '#'
                            | '&'
                            | '*'
                            | '!'
                            | '|'
                            | '>'
                            | '\''
                            | '"'
                            | '%'
                            | '@'
                            | '`'
                    )
                    || character == '-' && !is_blank(self.peek(1))
                    || self.flow_level == 0
                        && matches!(character, '?' | ':')
                        && !is_blankz(self.peek(1));
                if !allowed {
                    return Err(self.error("found character that cannot start any token"));
                }
                self.save_key()?;
                self.simple_key_allowed = false;
                self.at = At::Plain(scalar::Plain::new(start, self.indent + 1));
            }
        }
        Ok(true)
    }

    /// Ports `yaml_parser_fetch_value`, `scannerc.go:1278`.
    fn value(&mut self) -> Result<(), Error> {
        let level = self.simple_keys.len().saturating_sub(1);
        if self.valid_key(level)? {
            if let Some(key) = self.simple_keys.get(level).cloned() {
                let position = key
                    .number
                    .checked_sub(self.parsed)
                    .unwrap_or(self.held.len())
                    .min(self.held.len());
                self.held.insert(
                    position,
                    Token {
                        kind: TokenKind::Key,
                        start: key.mark,
                        end: key.mark,
                    },
                );
                self.roll_indent(
                    key.mark.column,
                    Some(key.number),
                    TokenKind::BlockMappingStart,
                    key.mark,
                )?;
                if let Some(key) = self.simple_keys.get_mut(level) {
                    key.possible = false;
                }
                self.simple_keys_by_tok.remove(&key.number);
            }
            self.simple_key_allowed = false;
        } else {
            if self.flow_level == 0 {
                if !self.simple_key_allowed {
                    return Err(self.error("mapping values are not allowed in this context"));
                }
                self.roll_indent(
                    self.mark.column,
                    None,
                    TokenKind::BlockMappingStart,
                    self.mark,
                )?;
            }
            self.simple_key_allowed = self.flow_level == 0;
        }
        let start = self.mark;
        self.skip();
        self.token(TokenKind::Value, start, self.mark);
        Ok(())
    }

    fn document_indicator(&self) -> bool {
        self.mark.column == 0
            && ((self.peek(0) == '-' && self.peek(1) == '-' && self.peek(2) == '-')
                || (self.peek(0) == '.' && self.peek(1) == '.' && self.peek(2) == '.'))
            && is_blankz(self.peek(3))
    }
}

/// Ports `is_alpha`, `yamlprivateh.go:26`.
fn is_alpha(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}
/// Ports `is_blank`, `yamlprivateh.go:96`.
fn is_blank(character: char) -> bool {
    matches!(character, ' ' | '\t')
}
/// Returns whether the character is a YAML line break (`is_break` in `yamlprivateh.go:102`).
#[must_use]
pub fn is_break(character: char) -> bool {
    matches!(character, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}
/// Ports `is_breakz`, `yamlprivateh.go:115`.
fn is_breakz(character: char) -> bool {
    character == '\0' || is_break(character)
}
/// Ports `is_blankz`, `yamlprivateh.go:142`.
fn is_blankz(character: char) -> bool {
    is_blank(character) || is_breakz(character)
}

fn clear_token_payload(kind: &mut TokenKind) {
    match kind {
        TokenKind::Alias(value) | TokenKind::Scalar { value, .. } => value.clear(),
        TokenKind::Anchor(value) if !value.is_empty() => *value = "a".to_owned(),
        TokenKind::StreamStart
        | TokenKind::TagDirective(..)
        | TokenKind::Tag(..)
        | TokenKind::Anchor(_)
        | TokenKind::StreamEnd
        | TokenKind::VersionDirective(_, _)
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
        | TokenKind::Value => {}
    }
}
