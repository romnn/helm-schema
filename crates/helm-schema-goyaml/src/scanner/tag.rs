//! Directive, anchor and tag loops from go-yaml v2.4.3's `scannerc.go`.

use super::{At, Scanner, is_alpha, is_blank, is_blankz, is_break, is_breakz};
use crate::types::{Error, Mark, TokenKind};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct Uri {
    bytes: Vec<u8>,
    has_tag: bool,
    escaped_left: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum TagAt {
    #[default]
    Start,
    Handle,
    Uri,
    CanonicalEnd,
    End,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Tag {
    start: Mark,
    at: TagAt,
    handle: String,
    uri: Uri,
    canonical: bool,
}

impl Tag {
    pub(super) fn clear_payload(&mut self) {
        self.start = Mark::default();
    }
    pub(super) fn new(start: Mark) -> Self {
        Self {
            start,
            at: TagAt::Start,
            handle: String::new(),
            uri: Uri::default(),
            canonical: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum DirectiveAt {
    #[default]
    Name,
    VersionSpace,
    Major,
    Minor,
    TagSpace,
    TagHandle,
    TagHandleEnd,
    TagUriSpace,
    TagUri,
    EndWhite,
    EndComment,
    End,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Directive {
    start: Mark,
    end: Mark,
    at: DirectiveAt,
    name: String,
    major: i32,
    minor: i32,
    digits: usize,
    handle: String,
    uri: Uri,
    kind: Option<TokenKind>,
}

impl Directive {
    pub(super) fn clear_payload(&mut self) {
        self.start = Mark::default();
        self.end = Mark::default();
    }
    pub(super) fn new(start: Mark) -> Self {
        Self {
            start,
            end: start,
            at: DirectiveAt::Name,
            name: String::new(),
            major: 0,
            minor: 0,
            digits: 0,
            handle: String::new(),
            uri: Uri::default(),
            kind: None,
        }
    }
}

impl Scanner {
    /// Ports `yaml_parser_scan_tag` (`scannerc.go:1838`) and
    /// `yaml_parser_scan_tag_handle` (`scannerc.go:1923`).
    pub(super) fn tag(&mut self, mut tag: Tag) -> Result<bool, Error> {
        let needed = match tag.at {
            TagAt::Start => 2,
            TagAt::Uri if tag.uri.escaped_left != 0 || self.peek(0) == '%' => 3,
            TagAt::Handle | TagAt::Uri | TagAt::CanonicalEnd | TagAt::End => 1,
        };
        if !self.available(needed) {
            self.at = At::Tag(tag);
            return Ok(false);
        }
        let character = self.peek(0);
        match tag.at {
            TagAt::Start => {
                self.skip();
                if self.peek(0) == '<' {
                    self.skip();
                    tag.canonical = true;
                    tag.at = TagAt::Uri;
                } else {
                    tag.handle.push('!');
                    tag.at = TagAt::Handle;
                }
            }
            TagAt::Handle => {
                if is_alpha(character) {
                    tag.handle.push(self.skip());
                } else if character == '!' {
                    tag.handle.push(self.skip());
                    tag.at = TagAt::Uri;
                } else {
                    tag.uri
                        .bytes
                        .extend(tag.handle.chars().skip(1).collect::<String>().bytes());
                    tag.uri.has_tag = !tag.handle.is_empty();
                    tag.handle = "!".to_owned();
                    tag.at = TagAt::Uri;
                }
            }
            TagAt::Uri => {
                if self.uri(&mut tag.uri)? {
                    self.at = At::Tag(tag);
                    return Ok(true);
                }
                if !tag.uri.has_tag {
                    return Err(self.error("did not find expected tag URI"));
                }
                tag.at = if tag.canonical {
                    TagAt::CanonicalEnd
                } else {
                    TagAt::End
                };
            }
            TagAt::CanonicalEnd => {
                if character != '>' {
                    return Err(self.error("did not find the expected '>'"));
                }
                self.skip();
                tag.at = TagAt::End;
            }
            TagAt::End => {
                if !is_blankz(character) {
                    return Err(self.error("did not find expected whitespace or line break"));
                }
                let mut suffix = String::from_utf8_lossy(&tag.uri.bytes).into_owned();
                if !tag.canonical && tag.handle == "!" && suffix.is_empty() {
                    tag.handle.clear();
                    suffix.push('!');
                }
                self.token(TokenKind::Tag(tag.handle, suffix), tag.start, self.mark);
                return Ok(true);
            }
        }
        self.at = At::Tag(tag);
        Ok(true)
    }

    /// Ports `yaml_parser_scan_tag_uri` (`scannerc.go:1968`) and
    /// `yaml_parser_scan_uri_escapes` (`scannerc.go:2026`).
    fn uri(&mut self, uri: &mut Uri) -> Result<bool, Error> {
        let character = self.peek(0);
        if uri.escaped_left != 0 || character == '%' {
            if character != '%'
                || !self.peek(1).is_ascii_hexdigit()
                || !self.peek(2).is_ascii_hexdigit()
            {
                return Err(self.error("did not find URI escaped octet"));
            }
            let octet = self.peek(1).to_digit(16).unwrap_or(0) * 16
                + self.peek(2).to_digit(16).unwrap_or(0);
            let octet = u8::try_from(octet).unwrap_or(0);
            if uri.escaped_left == 0 {
                uri.escaped_left = if octet & 0x80 == 0 {
                    1
                } else if octet & 0xe0 == 0xc0 {
                    2
                } else if octet & 0xf0 == 0xe0 {
                    3
                } else if octet & 0xf8 == 0xf0 {
                    4
                } else {
                    return Err(self.error("found an incorrect leading UTF-8 octet"));
                };
            } else if octet & 0xc0 != 0x80 {
                return Err(self.error("found an incorrect trailing UTF-8 octet"));
            }
            uri.bytes.push(octet);
            uri.escaped_left -= 1;
            for _ in 0..3 {
                self.skip();
            }
            uri.has_tag = true;
            return Ok(true);
        }
        if is_alpha(character)
            || matches!(
                character,
                ';' | '/'
                    | '?'
                    | ':'
                    | '@'
                    | '&'
                    | '='
                    | '+'
                    | '$'
                    | ','
                    | '.'
                    | '!'
                    | '~'
                    | '*'
                    | '\''
                    | '('
                    | ')'
                    | '['
                    | ']'
            )
        {
            let mut bytes = [0; 4];
            uri.bytes
                .extend_from_slice(self.skip().encode_utf8(&mut bytes).as_bytes());
            uri.has_tag = true;
            return Ok(true);
        }
        Ok(false)
    }

    /// Ports `yaml_parser_scan_directive` (`scannerc.go:1508`),
    /// `yaml_parser_scan_directive_name` (`scannerc.go:1609`),
    /// `yaml_parser_scan_version_directive_value` (`scannerc.go:1645`),
    /// `yaml_parser_scan_version_directive_number` (`scannerc.go:1686`) and
    /// `yaml_parser_scan_tag_directive_value` (`scannerc.go:1722`).
    #[expect(
        clippy::too_many_lines,
        reason = "The directive match directly records the suspended positions of Go's mutually dependent scanning loops."
    )]
    pub(super) fn directive(&mut self, mut directive: Directive) -> Result<bool, Error> {
        let needed = if directive.at == DirectiveAt::TagUri
            && (directive.uri.escaped_left != 0 || self.peek(0) == '%')
        {
            3
        } else {
            1
        };
        if !self.available(needed) {
            self.at = At::Directive(directive);
            return Ok(false);
        }
        let character = self.peek(0);
        match directive.at {
            DirectiveAt::Name => {
                if is_alpha(character) {
                    directive.name.push(self.skip());
                } else {
                    if directive.name.is_empty() {
                        return Err(self.error("could not find expected directive name"));
                    }
                    if !is_blankz(character) {
                        return Err(self.error("found unexpected non-alphabetical character"));
                    }
                    directive.at = match directive.name.as_str() {
                        "YAML" => DirectiveAt::VersionSpace,
                        "TAG" => DirectiveAt::TagSpace,
                        _ => return Err(self.error("found unknown directive name")),
                    };
                }
            }
            DirectiveAt::VersionSpace => {
                if is_blank(character) {
                    self.skip();
                } else {
                    directive.at = DirectiveAt::Major;
                }
            }
            DirectiveAt::Major | DirectiveAt::Minor => {
                if character.is_ascii_digit() {
                    directive.digits += 1;
                    if directive.digits > 2 {
                        return Err(self.error("found extremely long version number"));
                    }
                    let digit = i32::try_from(character.to_digit(10).unwrap_or(0)).unwrap_or(0);
                    if directive.at == DirectiveAt::Major {
                        directive.major = directive.major * 10 + digit;
                    } else {
                        directive.minor = directive.minor * 10 + digit;
                    }
                    self.skip();
                } else {
                    if directive.digits == 0 {
                        return Err(self.error("did not find expected version number"));
                    }
                    if directive.at == DirectiveAt::Major {
                        if character != '.' {
                            return Err(self.error("did not find expected digit or '.' character"));
                        }
                        self.skip();
                        directive.digits = 0;
                        directive.at = DirectiveAt::Minor;
                    } else {
                        directive.end = self.mark;
                        directive.kind = Some(TokenKind::VersionDirective(
                            directive.major,
                            directive.minor,
                        ));
                        directive.at = DirectiveAt::EndWhite;
                    }
                }
            }
            DirectiveAt::TagSpace => {
                if is_blank(character) {
                    self.skip();
                } else {
                    if character != '!' {
                        return Err(self.error("did not find expected '!'"));
                    }
                    directive.handle.push(self.skip());
                    directive.at = DirectiveAt::TagHandle;
                }
            }
            DirectiveAt::TagHandle => {
                if is_alpha(character) {
                    directive.handle.push(self.skip());
                } else {
                    if character == '!' {
                        directive.handle.push(self.skip());
                    } else if directive.handle != "!" {
                        return Err(self.error("did not find expected '!'"));
                    }
                    directive.at = DirectiveAt::TagHandleEnd;
                }
            }
            DirectiveAt::TagHandleEnd => {
                if !is_blank(character) {
                    return Err(self.error("did not find expected whitespace"));
                }
                directive.at = DirectiveAt::TagUriSpace;
            }
            DirectiveAt::TagUriSpace => {
                if is_blank(character) {
                    self.skip();
                } else {
                    directive.at = DirectiveAt::TagUri;
                }
            }
            DirectiveAt::TagUri => {
                if self.uri(&mut directive.uri)? {
                    self.at = At::Directive(directive);
                    return Ok(true);
                }
                if !directive.uri.has_tag {
                    return Err(self.error("did not find expected tag URI"));
                }
                if !is_blankz(character) {
                    return Err(self.error("did not find expected whitespace or line break"));
                }
                directive.end = self.mark;
                directive.kind = Some(TokenKind::TagDirective(
                    directive.handle.clone(),
                    String::from_utf8_lossy(&directive.uri.bytes).into_owned(),
                ));
                directive.at = DirectiveAt::EndWhite;
            }
            DirectiveAt::EndWhite => {
                if is_blank(character) {
                    self.skip();
                } else if character == '#' {
                    directive.at = DirectiveAt::EndComment;
                } else {
                    directive.at = DirectiveAt::End;
                }
            }
            DirectiveAt::EndComment => {
                if is_breakz(character) {
                    directive.at = DirectiveAt::End;
                } else {
                    self.skip();
                }
            }
            DirectiveAt::End => {
                if !is_breakz(character) {
                    return Err(self.error("did not find expected comment or line break"));
                }
                if is_break(character) {
                    if !self.available(2) {
                        self.at = At::Directive(directive);
                        return Ok(false);
                    }
                    self.line();
                }
                if let Some(kind) = directive.kind {
                    self.token(kind, directive.start, directive.end);
                }
                return Ok(true);
            }
        }
        self.at = At::Directive(directive);
        Ok(true)
    }
}
