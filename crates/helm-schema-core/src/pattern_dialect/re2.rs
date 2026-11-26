const MAX_REPEAT: u32 = 1000;
const MAX_GROUP_DEPTH: u8 = 64;
const MAX_SOURCE_BYTES: usize = 4096;
const MAX_EXPANDED_BYTES: usize = 1_000_000;
const LINE_START: &str = r"(?:^|\n)";

/// Parses the shared subset of Go regexp and ECMA-262 and returns its portable spelling.
///
/// The spelling preserves search membership in Go, ECMA-262 with or without Unicode mode,
/// and Rust's matcher. Returns `None` for syntax outside the subset.
#[must_use]
pub fn re2_to_ecma_pattern(source: &str) -> Option<String> {
    if source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut parser = Parser {
        source,
        output: String::with_capacity(source.len()),
    };
    let multiline = parser.eat("(?m)");
    let anchored = if parser.eat(LINE_START) || multiline && parser.eat("^") {
        parser.output.push_str(LINE_START);
        true
    } else if parser.eat("^") {
        parser.output.push('^');
        true
    } else {
        false
    };
    let (expression, alternative) = parser.expression(0)?;
    // Counted expansion stays below Go's regexp compiler limit.
    if source.len() * usize::try_from(expression.copies).ok()? > MAX_EXPANDED_BYTES {
        return None;
    }
    let end_anchor = parser.eat("$");
    if !parser.source.is_empty()
        || end_anchor && multiline
        || alternative && (anchored || end_anchor)
    {
        return None;
    }
    if end_anchor {
        parser.output.push('$');
    }
    Some(parser.output)
}

struct Parser<'a> {
    source: &'a str,
    output: String,
}

#[derive(Clone, Copy)]
struct Expression {
    copies: u32,
    nonempty_wide_run: bool,
}

impl Expression {
    const EMPTY: Self = Self {
        copies: 1,
        nonempty_wide_run: false,
    };
}

#[derive(Clone, Copy)]
enum Atom {
    Group(Expression),
    CharacterClass { admits_astral: bool },
}

#[derive(Clone, Copy)]
struct Repeat {
    min: u32,
    max: Option<u32>,
    counted: bool,
}

impl Repeat {
    const ONCE: Self = Self {
        min: 1,
        max: Some(1),
        counted: false,
    };
}

impl Parser<'_> {
    fn eat(&mut self, token: &str) -> bool {
        if let Some(tail) = self.source.strip_prefix(token) {
            self.source = tail;
            true
        } else {
            false
        }
    }

    fn next_char(&mut self) -> Option<char> {
        let character = self.source.chars().next()?;
        self.source = self.source.get(character.len_utf8()..)?;
        Some(character)
    }

    fn expression(&mut self, depth: u8) -> Option<(Expression, bool)> {
        let mut expression = self.sequence(depth)?;
        let mut alternative = false;
        while self.eat("|") {
            alternative = true;
            self.output.push('|');
            let branch = self.sequence(depth)?;
            expression.copies = expression.copies.max(branch.copies);
            expression.nonempty_wide_run |= branch.nonempty_wide_run;
        }
        Some((expression, alternative))
    }

    fn sequence(&mut self, depth: u8) -> Option<Expression> {
        let mut expression = Expression::EMPTY;
        while !matches!(
            self.source.as_bytes().first(),
            None | Some(b')' | b'|' | b'$')
        ) {
            let atom = self.atom(depth)?;
            let repeat = self.repeat();
            let piece = repeated(atom, repeat)?;
            if expression.nonempty_wide_run && piece.nonempty_wide_run {
                return None;
            }
            expression.nonempty_wide_run |= piece.nonempty_wide_run;
            expression.copies = expression.copies.max(piece.copies);
        }
        Some(expression)
    }

    fn atom(&mut self, depth: u8) -> Option<Atom> {
        let admits_astral = match self.next_char()? {
            '(' if depth < MAX_GROUP_DEPTH => {
                self.output.push('(');
                if self.eat("?:") {
                    self.output.push_str("?:");
                }
                let (group, _) = self.expression(depth + 1)?;
                self.eat(")").then_some(())?;
                self.output.push(')');
                return Some(Atom::Group(group));
            }
            '[' => self.class()?,
            '\\' => match self.escape(false)? {
                Member::Scalar(scalar) => {
                    self.output.push_str(&scalar.spelling);
                    false
                }
                Member::Perl(perl) => {
                    self.output.push_str(if perl.negated { "[^" } else { "[" });
                    self.output.push_str(perl.members);
                    self.output.push(']');
                    perl.negated
                }
            },
            '(' | '.' | '^' | '*' | '+' | '?' => return None,
            '{' if counted_repeat(self.source).is_some() => return None,
            character => {
                self.output
                    .push_str(&scalar(character, false, false)?.spelling);
                false
            }
        };
        Some(Atom::CharacterClass { admits_astral })
    }

    fn repeat(&mut self) -> Repeat {
        let original = self.source;
        let repeat = if self.eat("*") {
            Repeat {
                min: 0,
                max: None,
                counted: false,
            }
        } else if self.eat("+") {
            Repeat {
                min: 1,
                max: None,
                counted: false,
            }
        } else if self.eat("?") {
            Repeat {
                min: 0,
                max: Some(1),
                counted: false,
            }
        } else if let Some((repeat, tail)) = self.source.strip_prefix('{').and_then(counted_repeat)
        {
            self.source = tail;
            repeat
        } else {
            return Repeat::ONCE;
        };
        self.eat("?");
        if let Some(token) = original.get(..original.len() - self.source.len()) {
            self.output.push_str(token);
        }
        repeat
    }

    fn class(&mut self) -> Option<bool> {
        let original = self.source;
        let mut negated = self.eat("^");
        // A second raw hyphen can turn the first into a range endpoint in Go.
        let leading_hyphen = !self.source.starts_with("--") && self.eat("-");
        let mut members = Vec::new();
        let mut trailing_hyphen = false;
        while !self.eat("]") {
            if !members.is_empty() && self.eat("-]") {
                trailing_hyphen = true;
                break;
            }
            let member = self.class_member()?;
            let end = if !self.source.starts_with("-]") && self.eat("-") {
                let Member::Scalar(end) = self.class_member()? else {
                    return None;
                };
                let Member::Scalar(start) = &member else {
                    return None;
                };
                (start.character <= end.character).then_some(())?;
                Some(end)
            } else {
                None
            };
            members.push((member, end));
        }
        if members.is_empty() && !leading_hyphen {
            return None;
        }
        if !leading_hyphen && !trailing_hyphen {
            match members.as_mut_slice() {
                [(Member::Perl(first), None), (Member::Perl(second), None)]
                    if first.members == second.members && first.negated != second.negated =>
                {
                    // The character class and its complement partition the alphabet in each engine.
                    self.output.push('[');
                    self.output
                        .push_str(original.get(..original.len() - self.source.len())?);
                    return Some(!negated);
                }
                [(Member::Perl(perl), None)] if perl.negated => {
                    negated = !negated;
                    perl.negated = false;
                }
                _ => {}
            }
        }
        self.output.push_str(if negated { "[^" } else { "[" });
        if leading_hyphen {
            self.output.push('-');
        }
        for (member, end) in members {
            match member {
                Member::Perl(perl) if !perl.negated => self.output.push_str(perl.members),
                Member::Perl(_) => return None,
                Member::Scalar(scalar) => {
                    if negated && !scalar.character.is_ascii() {
                        return None;
                    }
                    self.output.push_str(&scalar.spelling);
                }
            }
            if let Some(end) = end {
                if negated && !end.character.is_ascii() {
                    return None;
                }
                self.output.push('-');
                self.output.push_str(&end.spelling);
            }
        }
        if trailing_hyphen {
            self.output.push('-');
        }
        self.output.push(']');
        Some(negated)
    }

    fn class_member(&mut self) -> Option<Member> {
        match self.next_char()? {
            '\\' => self.escape(true),
            '[' | ']' | '-' => None,
            character => Some(Member::Scalar(scalar(character, true, false)?)),
        }
    }

    fn escape(&mut self, in_class: bool) -> Option<Member> {
        let escape = self.next_char()?;
        let members = match escape {
            'd' | 'D' => Some("0-9"),
            's' | 'S' => Some(r"\t\n\f\r "),
            'w' | 'W' => Some("0-9A-Z_a-z"),
            _ => None,
        };
        if let Some(members) = members {
            return Some(Member::Perl(Perl {
                members,
                negated: escape.is_ascii_uppercase(),
            }));
        }
        let scalar = match escape {
            'x' => {
                let braced = self.eat("{");
                let digits = if braced {
                    let (digits, tail) = self.source.split_once('}')?;
                    self.source = tail;
                    digits
                } else {
                    let digits = self.source.get(..2)?;
                    self.source = self.source.get(2..)?;
                    digits
                };
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                let character = char::from_u32(u32::from_str_radix(digits, 16).ok()?)?;
                if character <= '\u{FF}' {
                    Scalar {
                        character,
                        spelling: if braced {
                            format!("\\x{:02X}", u32::from(character))
                        } else {
                            format!("\\x{digits}")
                        },
                    }
                } else {
                    scalar(character, in_class, false)?
                }
            }
            'a' => Scalar {
                character: '\x07',
                spelling: r"\x07".to_owned(),
            },
            'f' | 't' | 'n' | 'r' | 'v' => Scalar {
                character: match escape {
                    'f' => '\x0C',
                    't' => '\t',
                    'n' => '\n',
                    'r' => '\r',
                    'v' => '\x0B',
                    _ => return None,
                },
                spelling: format!("\\{escape}"),
            },
            character if character.is_ascii() && !character.is_ascii_alphanumeric() => {
                scalar(character, in_class, true)?
            }
            _ => return None,
        };
        Some(Member::Scalar(scalar))
    }
}

enum Member {
    Scalar(Scalar),
    Perl(Perl),
}

struct Scalar {
    character: char,
    spelling: String,
}

struct Perl {
    members: &'static str,
    negated: bool,
}

fn scalar(character: char, in_class: bool, escaped: bool) -> Option<Scalar> {
    if u32::from(character) >= 0xD800 {
        return None;
    }
    let spelling = if in_class && matches!(character, '&' | '~') {
        format!("\\x{:02X}", u32::from(character))
    } else if matches!(character, ']' | '{' | '}')
        || escaped && (r"^$\.*+?()[]{}|/".contains(character) || in_class && character == '-')
    {
        format!("\\{character}")
    } else {
        character.to_string()
    };
    Some(Scalar {
        character,
        spelling,
    })
}

// Go treats a brace without a complete canonical count as a literal brace.
fn counted_repeat(source: &str) -> Option<(Repeat, &str)> {
    let (min, tail) = count(source)?;
    let (max, tail) = if let Some(tail) = tail.strip_prefix(',') {
        if tail.starts_with('}') {
            (None, tail)
        } else {
            let (max, tail) = count(tail)?;
            (Some(max), tail)
        }
    } else {
        (Some(min), tail)
    };
    Some((
        Repeat {
            min,
            max,
            counted: true,
        },
        tail.strip_prefix('}')?,
    ))
}

fn count(source: &str) -> Option<(u32, &str)> {
    let width = source.bytes().take_while(u8::is_ascii_digit).count();
    if width == 0 || width > 1 && source.starts_with('0') {
        return None;
    }
    let digits = source.get(..width)?;
    Some((
        digits.parse().unwrap_or(MAX_REPEAT + 1),
        source.get(width..)?,
    ))
}

fn repeated(atom: Atom, repeat: Repeat) -> Option<Expression> {
    let Repeat { min, max, counted } = repeat;
    if max.is_some_and(|max| max < min) {
        return None;
    }
    let mut expression = match atom {
        Atom::CharacterClass {
            admits_astral: false,
        } => Expression::EMPTY,
        Atom::CharacterClass {
            admits_astral: true,
        } => {
            // These classes accept every astral scalar and both surrogate halves.
            // An unbounded run can consume either representation.
            (max.is_none() && min <= 1).then_some(())?;
            Expression {
                copies: 1,
                nonempty_wide_run: min == 1,
            }
        }
        Atom::Group(expression) => {
            // Surrogate halves split across runs can move into the one run that
            // requires a character, or into either nullable run.
            if expression.nonempty_wide_run && max.is_none_or(|max| max > 1) {
                return None;
            }
            expression
        }
    };
    if max == Some(0) {
        return Some(Expression::EMPTY);
    }
    if counted {
        expression.copies = expression.copies.saturating_mul(max.unwrap_or(min).max(1));
        (expression.copies <= MAX_REPEAT).then_some(())?;
    }
    Some(expression)
}
