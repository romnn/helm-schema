use helm_schema_json_schema_walk::visit_subschemas_mut;
use regex_syntax::ast::{self, Ast, ClassSetItem, LiteralKind};
use serde_json::Value;

/// Quotes literal text for every regular-expression dialect emitted by helm-schema.
#[must_use]
pub fn escape_regex_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Normalize regex dialects in every schema-position `pattern` keyword and
/// `patternProperties` key. Provider schemas carry Go/RE2 spellings —
/// notably a leading global `(?i)` and escaped punctuation such as `\ ` —
/// that Draft-07's ECMA-262 dialect rejects; conforming validators refuse
/// the whole schema over one such pattern. Runs once at provider-fragment
/// ingestion, the boundary where foreign dialect text enters the system, so
/// every downstream consumer sees portable spellings. Rewrites are
/// language-exact (a case fold or a respelled escape, never a widening), so
/// an untranslatable pattern stays as-is for the fixture hygiene gate to
/// report rather than silently changing what the schema accepts.
/// Constructs that both dialects spell alike but read differently, such as
/// Go's ASCII-only `\s`, pass through unchanged.
pub fn normalize_schema_pattern_dialects(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        if let Some(Value::String(pattern)) = object.get_mut("pattern")
            && let Some(normalized) = portable_provider_pattern(pattern)
        {
            *pattern = normalized;
        }
        if let Some(Value::Object(pattern_properties)) = object.get_mut("patternProperties") {
            let targets: Vec<(String, String)> = pattern_properties
                .keys()
                .map(|key| {
                    let target = portable_provider_pattern(key).unwrap_or_else(|| key.clone());
                    (key.clone(), target)
                })
                .collect();
            for (key, target) in &targets {
                // Keys that meet at one spelling keep their originals:
                // renaming would drop a sibling constraint, and merging them
                // under `allOf` would change what a `$ref` to either means.
                let shared = targets.iter().filter(|(_, other)| other == target).count() > 1;
                if key != target
                    && !shared
                    && let Some(subschema) = pattern_properties.remove(key)
                {
                    pattern_properties.insert(target.clone(), subschema);
                }
            }
        }
    }
    visit_subschemas_mut(
        schema,
        helm_schema_json_schema_walk::ReferenceSiblings::Skip,
        &mut normalize_schema_pattern_dialects,
    );
}

/// The ECMA-262 spelling of a provider pattern, or `None` when it is already
/// portable or no exact rewrite applies.
fn portable_provider_pattern(pattern: &str) -> Option<String> {
    let folded = ecma_case_folded_pattern(pattern);
    let base = folded.as_deref().unwrap_or(pattern);
    unicode_mode_escaped_pattern(base).or(folded)
}

/// Respells every escaped ASCII character that ECMA-262 rejects under the
/// `u` flag, such as `\ `, `\%`, `\<`, or `\-` outside a class.
///
/// RE2 reads an escaped ASCII character other than a letter or digit as that
/// literal character, and so does ECMA-262 without the `u` flag.
/// Under the `u` flag, which JSON Schema validators apply to `pattern`, only
/// the syntax characters `^$\.*+?()[]{}|`, `/`, and a class-member `-` may be
/// escaped.
/// Every other such escape therefore becomes its bare literal, which matches
/// the same strings in RE2 and both ECMA-262 modes.
/// Inside a class, `&` and `~` become `\x26` and `\x7E` instead, because a
/// doubled bare `&&` or `~~` is a class set operator in Rust-based validators.
///
/// Returns `None` when the pattern needs no rewrite or when [`regex_syntax`],
/// the `regex` crate's RE2-family parser, cannot locate the escapes exactly:
/// the pattern does not parse, or it uses syntax where Rust and RE2 disagree
/// (nested classes, class set operations, or the `x` flag, under which an
/// escaped space is significant).
/// Other RE2-only spellings, such as `\x{20}`, stay for the fixture hygiene
/// gate to report.
#[must_use]
pub fn unicode_mode_escaped_pattern(pattern: &str) -> Option<String> {
    let parsed = ast::parse::Parser::new().parse(pattern).ok()?;
    let mut respellings = ast::visit(&parsed, SuperfluousEscapes::default()).ok()?;
    if respellings.is_empty() {
        return None;
    }
    respellings.sort_unstable_by_key(|respelling| respelling.span.start.offset);
    let mut rewritten = String::with_capacity(pattern.len());
    let mut copied = 0;
    for respelling in respellings {
        rewritten.push_str(pattern.get(copied..respelling.span.start.offset)?);
        rewritten.push_str(&respelling.text);
        copied = respelling.span.end.offset;
    }
    rewritten.push_str(pattern.get(copied..)?);
    Some(rewritten)
}

/// The escapes that [`unicode_mode_escaped_pattern`] respells, collected
/// by a visit that fails to abstain from the whole pattern.
#[derive(Default)]
struct SuperfluousEscapes {
    respellings: Vec<Respelling>,
}

/// The ECMA-262 text that replaces one escape's source span.
struct Respelling {
    span: ast::Span,
    text: String,
}

impl SuperfluousEscapes {
    fn record(&mut self, literal: &ast::Literal, in_class: bool) {
        let escaped = matches!(literal.kind, LiteralKind::Superfluous | LiteralKind::Meta);
        // ECMA-262's `u` flag admits only these identity escapes, plus `-` in a class.
        let unicode_mode_escapable =
            r"^$\.*+?()[]{}|/".contains(literal.c) || (in_class && literal.c == '-');
        if escaped && !unicode_mode_escapable {
            let text = if in_class && matches!(literal.c, '&' | '~') {
                format!("\\x{:02X}", u32::from(literal.c))
            } else {
                literal.c.to_string()
            };
            self.respellings.push(Respelling {
                span: literal.span,
                text,
            });
        }
    }
}

impl ast::Visitor for SuperfluousEscapes {
    type Output = Vec<Respelling>;
    type Err = ();

    fn finish(self) -> Result<Self::Output, Self::Err> {
        Ok(self.respellings)
    }

    fn visit_pre(&mut self, ast: &Ast) -> Result<(), Self::Err> {
        match ast {
            Ast::Literal(literal) => self.record(literal, false),
            // Rust reads `\<` and `\>` as word-boundary assertions, where RE2
            // reads them as the literal angle brackets.
            Ast::Assertion(assertion) => {
                let text = match assertion.kind {
                    ast::AssertionKind::WordBoundaryStartAngle => "<",
                    ast::AssertionKind::WordBoundaryEndAngle => ">",
                    _ => return Ok(()),
                };
                self.respellings.push(Respelling {
                    span: assertion.span,
                    text: text.to_string(),
                });
            }
            Ast::Flags(set) if ignores_whitespace(&set.flags) => return Err(()),
            Ast::Group(group) => {
                if let ast::GroupKind::NonCapturing(flags) = &group.kind
                    && ignores_whitespace(flags)
                {
                    return Err(());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn visit_class_set_item_pre(&mut self, item: &ClassSetItem) -> Result<(), Self::Err> {
        match item {
            ClassSetItem::Literal(literal) => self.record(literal, true),
            ClassSetItem::Range(range) => {
                self.record(&range.start, true);
                self.record(&range.end, true);
            }
            // RE2 reads a `[` inside a class literally, where Rust opens a nested class.
            ClassSetItem::Bracketed(_) => return Err(()),
            _ => {}
        }
        Ok(())
    }

    // RE2 reads `&&`, `--`, and `~~` inside a class literally, where Rust combines sets.
    fn visit_class_set_binary_op_pre(
        &mut self,
        _op: &ast::ClassSetBinaryOp,
    ) -> Result<(), Self::Err> {
        Err(())
    }
}

fn ignores_whitespace(flags: &ast::Flags) -> bool {
    flags
        .items
        .iter()
        .any(|item| item.kind == ast::FlagsItemKind::Flag(ast::Flag::IgnoreWhitespace))
}

/// Rewrite a leading global case-insensitivity group (`(?i)…` or `^(?i)…`)
/// into an explicit per-letter case fold: `^(?i)(abort|warn)?$` becomes
/// `^([aA][bB][oO][rR][tT]|[wW][aA][rR][nN])?$`. The fold preserves the
/// accepted language exactly, including RE2's Unicode simple-fold partners
/// for `k` (U+212A KELVIN SIGN) and `s` (U+017F LONG S), and the rewritten
/// pattern stays valid in both the ECMA-262 and Go dialects. Returns `None`
/// — leave the pattern unchanged — when there is no leading `(?i)` or the
/// tail uses any construct whose fold is not provably exact (letter-typed
/// escapes, class ranges, groups beyond `(?:`, non-ASCII text).
fn ecma_case_folded_pattern(pattern: &str) -> Option<String> {
    let (anchor, rest) = match pattern.strip_prefix('^') {
        Some(rest) => ("^", rest),
        None => ("", pattern),
    };
    let tail = rest.strip_prefix("(?i)")?;

    let mut out = String::with_capacity(anchor.len() + tail.len() * 2);
    out.push_str(anchor);
    let mut chars = tail.chars().peekable();
    let mut in_class = false;
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                let escaped = chars.next()?;
                // Escapes that denote letters indirectly (`\x41`, `\u`,
                // `\p{L}`), reference groups, or change case semantics
                // (`\Q…\E`) cannot fold character-wise.
                if matches!(
                    escaped,
                    'x' | 'u' | 'p' | 'P' | 'k' | 'Q' | 'E' | 'A' | 'z' | 'Z' | '1'..='9'
                ) {
                    return None;
                }
                out.push('\\');
                out.push(escaped);
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                if chars.peek() == Some(&'^') {
                    chars.next();
                    out.push('^');
                }
                // POSIX classes (`[[:alpha:]]`) are RE2-only; a leading
                // literal `]` complicates class parsing — both abstain.
                if matches!(chars.peek(), Some(&'[' | &']')) {
                    return None;
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            // A range endpoint may be a letter, and folding a range
            // member-wise is wrong (`[a-z]` is not `[aA]-[zZ]`); a `-`
            // that is not the class's last member abstains.
            '-' if in_class => {
                if chars.peek() != Some(&']') {
                    return None;
                }
                out.push('-');
            }
            '(' if !in_class => {
                out.push('(');
                if chars.peek() == Some(&'?') {
                    chars.next();
                    // Only the non-capturing group folds transparently;
                    // lookarounds, names, and inline flags abstain.
                    if chars.next() != Some(':') {
                        return None;
                    }
                    out.push_str("?:");
                }
            }
            'a'..='z' | 'A'..='Z' => {
                let lower = character.to_ascii_lowercase();
                let upper = character.to_ascii_uppercase();
                if !in_class {
                    out.push('[');
                }
                out.push(lower);
                out.push(upper);
                match lower {
                    'k' => out.push('\u{212A}'),
                    's' => out.push('\u{017F}'),
                    _ => {}
                }
                if !in_class {
                    out.push(']');
                }
            }
            character if !character.is_ascii() => return None,
            character => out.push(character),
        }
    }
    if in_class {
        return None;
    }
    Some(out)
}

#[cfg(test)]
#[path = "tests/pattern_dialect.rs"]
mod tests;
