//! Scalar loop positions from go-yaml v2.4.3's `scannerc.go`.

use super::{At, Scanner, is_blank, is_blankz, is_break, is_breakz};
use crate::types::{Error, Mark, ScalarStyle, TokenKind};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum PlainAt {
    #[default]
    Check,
    Text,
    White,
    Indent,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Plain {
    start: Mark,
    end: Mark,
    indent: i32,
    at: PlainAt,
    text: String,
    whitespaces: String,
    leading_break: String,
    trailing_breaks: String,
    leading_blanks: bool,
}

impl Plain {
    pub(super) fn clear_payload(&mut self) {
        self.start = Mark::default();
        self.end = Mark::default();
        self.text.clear();
        self.whitespaces.clear();
        summarize_break(&mut self.leading_break);
        summarize_break(&mut self.trailing_breaks);
    }
    pub(super) fn new(start: Mark, indent: i32) -> Self {
        Self {
            start,
            end: start,
            indent,
            at: PlainAt::Check,
            text: String::new(),
            whitespaces: String::new(),
            leading_break: String::new(),
            trailing_breaks: String::new(),
            leading_blanks: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum QuotedAt {
    #[default]
    Check,
    Text,
    White,
    Hex {
        left: u8,
        value: u32,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Quoted {
    start: Mark,
    single: bool,
    at: QuotedAt,
    text: String,
    whitespaces: String,
    leading_break: String,
    trailing_breaks: String,
    leading_blanks: bool,
}

impl Quoted {
    pub(super) fn clear_payload(&mut self) {
        self.start = Mark::default();
        self.text.clear();
        self.whitespaces.clear();
        summarize_break(&mut self.leading_break);
        summarize_break(&mut self.trailing_breaks);
    }
    pub(super) fn new(start: Mark, single: bool) -> Self {
        Self {
            start,
            single,
            at: QuotedAt::Check,
            text: String::new(),
            whitespaces: String::new(),
            leading_break: String::new(),
            trailing_breaks: String::new(),
            leading_blanks: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum BlockAt {
    #[default]
    HeaderFirst,
    HeaderAfterChomp,
    HeaderAfterIndent,
    HeaderWhite,
    HeaderComment,
    HeaderEnd,
    BreakSpaces,
    BreakNext,
    ContentStart,
    Content,
    ContentBreak,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Block {
    start: Mark,
    end: Mark,
    literal: bool,
    at: BlockAt,
    chomping: i8,
    increment: i32,
    indent: usize,
    max_indent: usize,
    text: String,
    leading_break: String,
    trailing_breaks: String,
    leading_blank: bool,
}

impl Block {
    pub(super) fn clear_payload(&mut self) {
        self.start = Mark::default();
        self.end = Mark::default();
        self.text.clear();
        summarize_break(&mut self.leading_break);
        summarize_break(&mut self.trailing_breaks);
    }
    pub(super) fn new(start: Mark, literal: bool) -> Self {
        Self {
            start,
            end: start,
            literal,
            at: BlockAt::HeaderFirst,
            chomping: 0,
            increment: 0,
            indent: 0,
            max_indent: 0,
            text: String::new(),
            leading_break: String::new(),
            trailing_breaks: String::new(),
            leading_blank: false,
        }
    }
}

impl Scanner {
    /// Ports `yaml_parser_scan_plain_scalar`, `scannerc.go:2570`.
    pub(super) fn plain(&mut self, mut scalar: Plain) -> Result<bool, Error> {
        let needed = match scalar.at {
            PlainAt::Check => 4,
            PlainAt::Text => 2,
            PlainAt::White | PlainAt::Indent => 1,
        };
        if !self.available(needed) {
            self.at = At::Plain(scalar);
            return Ok(false);
        }
        let character = self.peek(0);
        match scalar.at {
            PlainAt::Check => {
                if self.document_indicator() || character == '#' {
                    self.end_plain(scalar);
                    return Ok(true);
                }
                scalar.at = PlainAt::Text;
            }
            PlainAt::Text => {
                if !is_blankz(character) {
                    if character == ':' && is_blankz(self.peek(1))
                        || self.flow_level > 0
                            && matches!(character, ',' | '?' | '[' | ']' | '{' | '}')
                    {
                        self.end_plain(scalar);
                        return Ok(true);
                    }
                    if scalar.leading_blanks {
                        fold(
                            &mut scalar.text,
                            &mut scalar.leading_break,
                            &mut scalar.trailing_breaks,
                        );
                        scalar.leading_blanks = false;
                    } else {
                        scalar.text.push_str(&scalar.whitespaces);
                        scalar.whitespaces.clear();
                    }
                    scalar.text.push(self.skip());
                    scalar.end = self.mark;
                } else if is_blank(character) || is_break(character) {
                    scalar.at = PlainAt::White;
                } else {
                    self.end_plain(scalar);
                    return Ok(true);
                }
            }
            PlainAt::White => {
                if is_blank(character) {
                    if scalar.leading_blanks
                        && i32::try_from(self.mark.column).unwrap_or(i32::MAX) < scalar.indent
                        && character == '\t'
                    {
                        return Err(self.error("found a tab character that violates indentation"));
                    }
                    if scalar.leading_blanks {
                        self.skip();
                    } else {
                        scalar.whitespaces.push(self.skip());
                    }
                } else if is_break(character) {
                    if !self.available(2) {
                        self.at = At::Plain(scalar);
                        return Ok(false);
                    }
                    if scalar.leading_blanks {
                        scalar.trailing_breaks.push(self.line());
                    } else {
                        scalar.whitespaces.clear();
                        scalar.leading_break.push(self.line());
                        scalar.leading_blanks = true;
                    }
                } else {
                    scalar.at = PlainAt::Indent;
                }
            }
            PlainAt::Indent => {
                if self.flow_level == 0
                    && i32::try_from(self.mark.column).unwrap_or(i32::MAX) < scalar.indent
                {
                    self.end_plain(scalar);
                    return Ok(true);
                }
                scalar.at = PlainAt::Check;
            }
        }
        self.at = At::Plain(scalar);
        Ok(true)
    }

    fn end_plain(&mut self, scalar: Plain) {
        if scalar.leading_blanks {
            self.simple_key_allowed = true;
        }
        self.token(
            TokenKind::Scalar {
                value: scalar.text,
                style: ScalarStyle::Plain,
            },
            scalar.start,
            scalar.end,
        );
    }

    /// Ports `yaml_parser_scan_flow_scalar`, `scannerc.go:2314`.
    #[expect(
        clippy::too_many_lines,
        reason = "One match keeps every suspended position of Go's quoted-scalar loop auditable against scannerc.go."
    )]
    pub(super) fn quoted(&mut self, mut scalar: Quoted) -> Result<bool, Error> {
        let needed = match scalar.at {
            QuotedAt::Check => 4,
            QuotedAt::Text => 2,
            QuotedAt::White => 1,
            QuotedAt::Hex { left, .. } => usize::from(left),
        };
        if !self.available(needed) {
            self.at = At::Quoted(scalar);
            return Ok(false);
        }
        let character = self.peek(0);
        match scalar.at {
            QuotedAt::Check => {
                if self.document_indicator() {
                    return Err(self.error("found unexpected document indicator"));
                }
                if character == '\0' {
                    return Err(self.error("found unexpected end of stream"));
                }
                scalar.leading_blanks = false;
                scalar.at = QuotedAt::Text;
            }
            QuotedAt::Text => {
                if scalar.single && character == '\'' && self.peek(1) == '\'' {
                    self.skip();
                    self.skip();
                    scalar.text.push('\'');
                } else if scalar.single && character == '\'' || !scalar.single && character == '"' {
                    self.skip();
                    let style = if scalar.single {
                        ScalarStyle::SingleQuoted
                    } else {
                        ScalarStyle::DoubleQuoted
                    };
                    self.token(
                        TokenKind::Scalar {
                            value: scalar.text,
                            style,
                        },
                        scalar.start,
                        self.mark,
                    );
                    return Ok(true);
                } else if !scalar.single && character == '\\' {
                    if is_break(self.peek(1)) {
                        if !self.available(3) {
                            self.at = At::Quoted(scalar);
                            return Ok(false);
                        }
                        self.skip();
                        self.line();
                        scalar.leading_blanks = true;
                        scalar.at = QuotedAt::White;
                    } else {
                        let escape = self.peek(1);
                        let value = match escape {
                            '0' => Some('\0'),
                            'a' => Some('\u{7}'),
                            'b' => Some('\u{8}'),
                            't' | '\t' => Some('\t'),
                            'n' => Some('\n'),
                            'v' => Some('\u{b}'),
                            'f' => Some('\u{c}'),
                            'r' => Some('\r'),
                            'e' => Some('\u{1b}'),
                            ' ' => Some(' '),
                            '"' => Some('"'),
                            '\'' => Some('\''),
                            '\\' => Some('\\'),
                            'N' => Some('\u{85}'),
                            '_' => Some('\u{a0}'),
                            'L' => Some('\u{2028}'),
                            'P' => Some('\u{2029}'),
                            'x' | 'u' | 'U' => None,
                            _ => return Err(self.error("found unknown escape character")),
                        };
                        self.skip();
                        self.skip();
                        if let Some(value) = value {
                            scalar.text.push(value);
                        } else {
                            scalar.at = QuotedAt::Hex {
                                left: match escape {
                                    'x' => 2,
                                    'u' => 4,
                                    _ => 8,
                                },
                                value: 0,
                            };
                        }
                    }
                } else if is_blankz(character) {
                    scalar.at = QuotedAt::White;
                } else {
                    scalar.text.push(self.skip());
                }
            }
            QuotedAt::Hex { left, value } => {
                let Some(digit) = character.to_digit(16) else {
                    return Err(self.error("did not find expected hexdecimal number"));
                };
                self.skip();
                let value = (value << 4) + digit;
                if left == 1 {
                    let Some(character) = char::from_u32(value) else {
                        return Err(self.error("found invalid Unicode character escape code"));
                    };
                    scalar.text.push(character);
                    scalar.at = QuotedAt::Text;
                } else {
                    scalar.at = QuotedAt::Hex {
                        left: left - 1,
                        value,
                    };
                }
            }
            QuotedAt::White => {
                if is_blank(character) {
                    if scalar.leading_blanks {
                        self.skip();
                    } else {
                        scalar.whitespaces.push(self.skip());
                    }
                } else if is_break(character) {
                    if !self.available(2) {
                        self.at = At::Quoted(scalar);
                        return Ok(false);
                    }
                    if scalar.leading_blanks {
                        scalar.trailing_breaks.push(self.line());
                    } else {
                        scalar.whitespaces.clear();
                        scalar.leading_break.push(self.line());
                        scalar.leading_blanks = true;
                    }
                } else {
                    if scalar.leading_blanks {
                        fold(
                            &mut scalar.text,
                            &mut scalar.leading_break,
                            &mut scalar.trailing_breaks,
                        );
                    } else {
                        scalar.text.push_str(&scalar.whitespaces);
                        scalar.whitespaces.clear();
                    }
                    scalar.at = QuotedAt::Check;
                }
            }
        }
        self.at = At::Quoted(scalar);
        Ok(true)
    }

    /// Ports `yaml_parser_scan_block_scalar` (`scannerc.go:2072`) and
    /// `yaml_parser_scan_block_scalar_breaks` (`scannerc.go:2260`).
    #[expect(
        clippy::too_many_lines,
        reason = "One match keeps the block-scalar header, indentation and chomping registers in Go's transition order."
    )]
    pub(super) fn block(&mut self, mut scalar: Block) -> Result<bool, Error> {
        let needed = if scalar.at == BlockAt::ContentBreak {
            2
        } else {
            1
        };
        if !self.available(needed) {
            self.at = At::Block(scalar);
            return Ok(false);
        }
        let character = self.peek(0);
        match scalar.at {
            BlockAt::HeaderFirst => {
                if matches!(character, '+' | '-') {
                    scalar.chomping = if character == '+' { 1 } else { -1 };
                    self.skip();
                    scalar.at = BlockAt::HeaderAfterChomp;
                } else if character.is_ascii_digit() {
                    scalar.increment = self.block_increment(character)?;
                    self.skip();
                    scalar.at = BlockAt::HeaderAfterIndent;
                } else {
                    scalar.at = BlockAt::HeaderWhite;
                }
            }
            BlockAt::HeaderAfterChomp => {
                if character.is_ascii_digit() {
                    scalar.increment = self.block_increment(character)?;
                    self.skip();
                }
                scalar.at = BlockAt::HeaderWhite;
            }
            BlockAt::HeaderAfterIndent => {
                if matches!(character, '+' | '-') {
                    scalar.chomping = if character == '+' { 1 } else { -1 };
                    self.skip();
                }
                scalar.at = BlockAt::HeaderWhite;
            }
            BlockAt::HeaderWhite => {
                if is_blank(character) {
                    self.skip();
                } else if character == '#' {
                    scalar.at = BlockAt::HeaderComment;
                } else {
                    scalar.at = BlockAt::HeaderEnd;
                }
            }
            BlockAt::HeaderComment => {
                if is_breakz(character) {
                    scalar.at = BlockAt::HeaderEnd;
                } else {
                    self.skip();
                }
            }
            BlockAt::HeaderEnd => {
                if !is_breakz(character) {
                    return Err(self.error("did not find expected comment or line break"));
                }
                if is_break(character) {
                    if !self.available(2) {
                        self.at = At::Block(scalar);
                        return Ok(false);
                    }
                    self.line();
                }
                scalar.end = self.mark;
                if scalar.increment > 0 {
                    scalar.indent = usize::try_from(if self.indent >= 0 {
                        self.indent + scalar.increment
                    } else {
                        scalar.increment
                    })
                    .unwrap_or(0);
                }
                scalar.at = BlockAt::BreakSpaces;
            }
            BlockAt::BreakSpaces => {
                if (scalar.indent == 0 || self.mark.column < scalar.indent) && character == ' ' {
                    self.skip();
                } else {
                    scalar.max_indent = scalar.max_indent.max(self.mark.column);
                    if (scalar.indent == 0 || self.mark.column < scalar.indent) && character == '\t'
                    {
                        return Err(self.error(
                            "found a tab character where an indentation space is expected",
                        ));
                    }
                    scalar.at = BlockAt::BreakNext;
                }
            }
            BlockAt::BreakNext => {
                if is_break(character) {
                    if !self.available(2) {
                        self.at = At::Block(scalar);
                        return Ok(false);
                    }
                    scalar.trailing_breaks.push(self.line());
                    scalar.end = self.mark;
                    scalar.at = BlockAt::BreakSpaces;
                } else {
                    if scalar.indent == 0 {
                        scalar.indent = scalar
                            .max_indent
                            .max(usize::try_from(self.indent + 1).unwrap_or(0))
                            .max(1);
                    }
                    scalar.at = BlockAt::ContentStart;
                }
            }
            BlockAt::ContentStart => {
                if self.mark.column != scalar.indent || character == '\0' {
                    if scalar.chomping != -1 {
                        scalar.text.push_str(&scalar.leading_break);
                    }
                    if scalar.chomping == 1 {
                        scalar.text.push_str(&scalar.trailing_breaks);
                    }
                    let style = if scalar.literal {
                        ScalarStyle::Literal
                    } else {
                        ScalarStyle::Folded
                    };
                    self.token(
                        TokenKind::Scalar {
                            value: scalar.text,
                            style,
                        },
                        scalar.start,
                        scalar.end,
                    );
                    return Ok(true);
                }
                let trailing_blank = is_blank(character);
                if !scalar.literal
                    && !scalar.leading_blank
                    && !trailing_blank
                    && scalar.leading_break.starts_with('\n')
                {
                    if scalar.trailing_breaks.is_empty() {
                        scalar.text.push(' ');
                    }
                } else {
                    scalar.text.push_str(&scalar.leading_break);
                }
                scalar.leading_break.clear();
                scalar.text.push_str(&scalar.trailing_breaks);
                scalar.trailing_breaks.clear();
                scalar.leading_blank = is_blank(character);
                scalar.at = BlockAt::Content;
            }
            BlockAt::Content => {
                if is_breakz(character) {
                    scalar.at = BlockAt::ContentBreak;
                } else {
                    scalar.text.push(self.skip());
                }
            }
            BlockAt::ContentBreak => {
                if is_break(character) {
                    scalar.leading_break.push(self.line());
                }
                scalar.end = self.mark;
                scalar.max_indent = 0;
                scalar.at = BlockAt::BreakSpaces;
            }
        }
        self.at = At::Block(scalar);
        Ok(true)
    }

    /// Ports the indentation indicator in `yaml_parser_scan_block_scalar`, `scannerc.go:2072`.
    fn block_increment(&self, character: char) -> Result<i32, Error> {
        if character == '0' {
            return Err(self.error("found an indentation indicator equal to 0"));
        }
        Ok(i32::try_from(character.to_digit(10).unwrap_or(0)).unwrap_or(0))
    }
}

/// Ports the whitespace folds in `yaml_parser_scan_flow_scalar` (`scannerc.go:2314`)
/// and `yaml_parser_scan_plain_scalar` (`scannerc.go:2570`).
fn fold(text: &mut String, leading: &mut String, trailing: &mut String) {
    if leading.starts_with('\n') {
        if trailing.is_empty() {
            text.push(' ');
        } else {
            text.push_str(trailing);
        }
    } else {
        text.push_str(leading);
        text.push_str(trailing);
    }
    leading.clear();
    trailing.clear();
}

fn summarize_break(value: &mut String) {
    let first = value.chars().next();
    value.clear();
    if let Some(first) = first {
        value.push(first);
    }
}
