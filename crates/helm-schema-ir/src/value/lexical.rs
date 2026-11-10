//! String-language preimages of executed text transforms.
//!
//! A pattern over a transform's output becomes a pattern over its source: the source strings whose
//! transformed text satisfies it. Every preimage is a sound subset of the true one; `exact` marks a
//! complete preimage, whose complement within strings is the preimage of the complement.

use std::collections::BTreeSet;

use helm_schema_core::{GuardValue, ValuePattern};
use regex_syntax::hir::{Class, Hir, HirKind, Look};

use super::relationship::TextTransform;

/// A source-side pattern and whether it is the complete preimage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Lexical {
    pub(super) pattern: ValuePattern,
    pub(super) exact: bool,
}

impl Lexical {
    fn exact(pattern: ValuePattern) -> Self {
        Self {
            pattern,
            exact: true,
        }
    }

    fn subset(pattern: ValuePattern) -> Self {
        Self {
            pattern,
            exact: false,
        }
    }
}

/// Every Go string.
pub(super) fn any_string() -> ValuePattern {
    ValuePattern::String {
        pattern: String::new(),
    }
}

/// Strings matching `pattern` somewhere.
fn searched(pattern: String) -> ValuePattern {
    ValuePattern::String { pattern }
}

/// Lifts `output`, a pattern over the transform's text, to the transform's string source.
///
/// The source is a Go string: every transform here has a string parameter, so other kinds abort
/// before the transform runs and never reach its output.
pub(super) fn text_preimage(transform: &TextTransform, output: &ValuePattern) -> Option<Lexical> {
    let output = on_text(output)?;
    let unchanged = |region: ValuePattern| {
        ValuePattern::AllOf(vec![
            any_string(),
            ValuePattern::Not(Box::new(region)),
            output.clone(),
        ])
    };
    if let Some(compact) = optional_affix_preimage(transform, &output) {
        return Some(Lexical::exact(compact));
    }
    match transform {
        TextTransform::TrimPrefix(prefix) => {
            let stripped = searched(format!("^{}", regex::escape(prefix)));
            Some(Lexical::exact(ValuePattern::AnyOf(vec![
                ValuePattern::AllOf(vec![
                    stripped.clone(),
                    shifted(&output, &Shift::Affix { prefix, suffix: "" })?,
                ]),
                unchanged(stripped),
            ])))
        }
        TextTransform::TrimSuffix(suffix) => {
            let stripped = searched(format!("{}$", regex::escape(suffix)));
            Some(Lexical::exact(ValuePattern::AnyOf(vec![
                ValuePattern::AllOf(vec![
                    stripped.clone(),
                    shifted(&output, &Shift::Affix { prefix: "", suffix })?,
                ]),
                unchanged(stripped),
            ])))
        }
        TextTransform::Replace { from, to } if from == to => {
            Some(Lexical::exact(ValuePattern::AllOf(vec![
                any_string(),
                output,
            ])))
        }
        // `strings.ReplaceAll` is the identity exactly on strings without an occurrence.
        TextTransform::Replace { from, to: _ } if !from.is_empty() => {
            Some(Lexical::subset(unchanged(searched(regex::escape(from)))))
        }
        TextTransform::Cut(token) => Some(cut_preimage(token, r"[\s\S]*$", None, &output)),
        TextTransform::RegexReplace {
            pattern,
            replacement,
        } => {
            if replacement.is_empty()
                && let Some(token) = erased_line_tail(pattern)
            {
                // Go erases from the first token after the LAST newline (`"a#b\nc#d"` keeps
                // `"a#b\nc"`), while the cut shape takes the first token overall. The two agree on
                // single-line sources only, so multi-line ones with an erased tail stay undecided.
                let single_line = searched(r"^[^\n]*$".to_owned());
                return Some(Lexical {
                    exact: false,
                    ..cut_preimage(&token, r"[^\n]*$", Some(single_line), &output)
                });
            }
            // An unparsable pattern keeps no preimage: Go and the schema dialect could disagree.
            regex_syntax::Parser::new().parse(pattern).ok()?;
            Some(Lexical::subset(unchanged(searched(pattern.clone()))))
        }
        TextTransform::Replace { from: _, to: _ }
        | TextTransform::Truncate(_)
        | TextTransform::Upper
        | TextTransform::Lower => None,
    }
}

/// The preimage of a finite anchored language under trimming an affix none of its words carries:
/// each word either was the source or had the affix removed, so the affix is optional.
fn optional_affix_preimage(
    transform: &TextTransform,
    output: &ValuePattern,
) -> Option<ValuePattern> {
    let ValuePattern::String { pattern } = output else {
        return None;
    };
    let words = finite_words(output)?;
    let Searched {
        start: true,
        body,
        end: true,
    } = searched_parts(pattern)?
    else {
        return None;
    };
    let pattern = match transform {
        TextTransform::TrimSuffix(suffix)
            if !suffix.is_empty() && words.iter().all(|word| !word.ends_with(suffix.as_str())) =>
        {
            format!(
                "^(?:{body})(?:{})?$",
                helm_schema_core::escape_regex_literal(suffix)
            )
        }
        TextTransform::TrimPrefix(prefix)
            if !prefix.is_empty()
                && words.iter().all(|word| !word.starts_with(prefix.as_str())) =>
        {
            format!(
                "^(?:{})?(?:{body})$",
                helm_schema_core::escape_regex_literal(prefix)
            )
        }
        _ => return None,
    };
    Some(searched(pattern))
}

/// Lifts a pattern through Go's `%v` printing of the operand: the identity on strings.
pub(super) fn printed_preimage(output: &ValuePattern) -> Option<Lexical> {
    Some(Lexical::subset(ValuePattern::AllOf(vec![
        any_string(),
        on_text(output)?,
    ])))
}

/// The non-nil, non-string operands whose Go `%v` spelling is one of `words`, when every word's
/// producers are enumerable (the caller owns nil's spelling): `true`/`false` are booleans, and a canonical integer
/// below 10^6 in magnitude prints the same from a float64 values file and an int64 `--set`.
/// Other numeric-looking words (fractions, exponents, wide integers whose spelling depends on the
/// values transport) and map or slice spellings leave the set inexact.
pub(super) fn printed_scalars(words: &BTreeSet<String>) -> Option<Vec<GuardValue>> {
    let mut scalars = Vec::new();
    for word in words {
        match word.as_str() {
            "true" => scalars.push(GuardValue::Bool(true)),
            "false" => scalars.push(GuardValue::Bool(false)),
            _ if word.starts_with('[') || word.starts_with("map[") => return None,
            _ => {
                let digits = word.strip_prefix('-').unwrap_or(word);
                let canonical = !digits.is_empty()
                    && digits.len() <= 6
                    && digits.bytes().all(|byte| byte.is_ascii_digit())
                    && !(digits.len() > 1 && digits.starts_with('0'))
                    && word != "-0";
                if canonical {
                    scalars.push(GuardValue::Int(word.parse().ok()?));
                } else if !word.is_empty()
                    && word
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte))
                    || matches!(word.as_str(), "NaN" | "+Inf" | "-Inf")
                {
                    return None;
                }
            }
        }
    }
    Some(scalars)
}

/// The finite set of strings a pattern admits, when it is finite and small.
pub(super) fn finite_words(pattern: &ValuePattern) -> Option<BTreeSet<String>> {
    const LIMIT: usize = 64;
    match pattern {
        ValuePattern::String { pattern } => {
            let hir = regex_syntax::Parser::new().parse(pattern).ok()?;
            let HirKind::Concat(parts) = hir.kind() else {
                return None;
            };
            let [start, middle @ .., end] = parts.as_slice() else {
                return None;
            };
            if !matches!(start.kind(), HirKind::Look(Look::Start))
                || !matches!(end.kind(), HirKind::Look(Look::End))
            {
                return None;
            }
            let mut words = BTreeSet::from([String::new()]);
            for part in middle {
                let tails = hir_words(part, LIMIT)?;
                let mut next = BTreeSet::new();
                for word in &words {
                    for tail in &tails {
                        next.insert(format!("{word}{tail}"));
                    }
                }
                (next.len() <= LIMIT).then_some(())?;
                words = next;
            }
            Some(words)
        }
        ValuePattern::AnyOf(patterns) => {
            let mut words = BTreeSet::new();
            for pattern in patterns {
                words.extend(finite_words(pattern)?);
            }
            (words.len() <= LIMIT).then_some(words)
        }
        ValuePattern::AllOf(patterns) => {
            let finite = patterns.iter().find_map(finite_words)?;
            let mut words = BTreeSet::new();
            for word in finite {
                let text = serde_json::Value::String(word.clone());
                if pattern.matches(&text)? {
                    words.insert(word);
                }
            }
            Some(words)
        }
        _ => None,
    }
}

/// The words of an anchor-free regex fragment, when finite and small.
fn hir_words(hir: &Hir, limit: usize) -> Option<BTreeSet<String>> {
    Some(match hir.kind() {
        HirKind::Empty => BTreeSet::from([String::new()]),
        HirKind::Literal(literal) => {
            BTreeSet::from([std::str::from_utf8(&literal.0).ok()?.to_owned()])
        }
        HirKind::Capture(capture) => hir_words(&capture.sub, limit)?,
        HirKind::Alternation(parts) => {
            let mut words = BTreeSet::new();
            for part in parts {
                words.extend(hir_words(part, limit)?);
            }
            (words.len() <= limit).then_some(words)?
        }
        HirKind::Concat(parts) => {
            let mut words = BTreeSet::from([String::new()]);
            for part in parts {
                let tails = hir_words(part, limit)?;
                let mut next = BTreeSet::new();
                for word in &words {
                    for tail in &tails {
                        next.insert(format!("{word}{tail}"));
                    }
                }
                (next.len() <= limit).then_some(())?;
                words = next;
            }
            words
        }
        HirKind::Repetition(repetition) if repetition.max.is_some_and(|max| max <= 1) => {
            let mut words = hir_words(&repetition.sub, limit)?;
            if repetition.min == 0 {
                words.insert(String::new());
            }
            words
        }
        HirKind::Class(Class::Unicode(class)) => {
            let mut words = BTreeSet::new();
            for range in class.iter() {
                for character in range.start()..=range.end() {
                    words.insert(character.to_string());
                    (words.len() <= limit).then_some(())?;
                }
            }
            words
        }
        HirKind::Class(Class::Bytes(_)) | HirKind::Look(_) | HirKind::Repetition(_) => {
            return None;
        }
    })
}

/// The canonical Go `strconv.Quote` spelling's content, for words without escapes.
pub(super) fn unquoted(word: &str) -> Option<&str> {
    let inner = word.strip_prefix('"')?.strip_suffix('"')?;
    // `strconv.Quote` keeps exactly printable ASCII other than `"` and `\` unescaped; it escapes
    // controls and many non-ASCII characters (U+00A0 among them).
    inner
        .bytes()
        .all(|byte| (b' '..=b'~').contains(&byte) && byte != b'"' && byte != b'\\')
        .then_some(inner)
}

/// A pattern admitting exactly `words`.
pub(super) fn word_language(words: &BTreeSet<String>) -> ValuePattern {
    ValuePattern::AnyOf(
        words
            .iter()
            .map(|word| ValuePattern::String {
                pattern: format!("^{}$", helm_schema_core::escape_regex_literal(word)),
            })
            .collect(),
    )
}

/// The token of a regex that matches exactly `TOKEN` followed by non-newline text to the end.
fn erased_line_tail(pattern: &str) -> Option<String> {
    let hir = regex_syntax::Parser::new().parse(pattern).ok()?;
    let HirKind::Concat(parts) = hir.kind() else {
        return None;
    };
    let [literal, tail, end] = parts.as_slice() else {
        return None;
    };
    let HirKind::Literal(literal) = literal.kind() else {
        return None;
    };
    let token = std::str::from_utf8(&literal.0).ok()?.to_owned();
    let HirKind::Repetition(repetition) = tail.kind() else {
        return None;
    };
    let erases = repetition.min == 0
        && repetition.max.is_none()
        && repetition.greedy
        && *repetition.sub == Hir::dot(regex_syntax::hir::Dot::AnyCharExceptLF);
    (erases && matches!(end.kind(), HirKind::Look(Look::End)) && !token.is_empty()).then_some(token)
}

/// Erasing from the first `token` occurrence through a `tail` that runs to the end.
///
/// A source without the cut is unchanged. With it, a one-character token leaves exactly the text
/// before the token's first occurrence, so every output pattern whose language avoids that
/// character shifts onto that prefix. `cut_domain`, when present, further restricts the sources
/// the cut arm covers.
fn cut_preimage(
    token: &str,
    tail: &str,
    cut_domain: Option<ValuePattern>,
    output: &ValuePattern,
) -> Lexical {
    let cut = searched(format!("{}{tail}", regex::escape(token)));
    let unchanged = ValuePattern::AllOf(vec![
        any_string(),
        ValuePattern::Not(Box::new(cut.clone())),
        output.clone(),
    ]);
    let mut characters = token.chars();
    let (Some(separator), None) = (characters.next(), characters.next()) else {
        return Lexical::subset(unchanged);
    };
    let Some(prefixed) = shifted(output, &Shift::Cut { separator, tail }) else {
        return Lexical::subset(unchanged);
    };
    let cut_arm = match cut_domain {
        Some(domain) => vec![domain, cut, prefixed],
        None => vec![cut, prefixed],
    };
    Lexical::exact(ValuePattern::AnyOf(vec![
        unchanged,
        ValuePattern::AllOf(cut_arm),
    ]))
}

/// How the transformed text `y` sits inside its source `x`.
enum Shift<'a> {
    /// `x = prefix + y + suffix`.
    Affix { prefix: &'a str, suffix: &'a str },
    /// `x = y + separator + rest`, where `y` holds no separator and `rest` matches `tail`.
    Cut { separator: char, tail: &'a str },
}

/// Rewrites patterns over `y` into patterns over `x` within the shift's region.
fn shifted(output: &ValuePattern, shift: &Shift<'_>) -> Option<ValuePattern> {
    Some(match output {
        ValuePattern::Any => ValuePattern::Any,
        ValuePattern::String { pattern } => {
            let Searched { start, body, end } = searched_parts(pattern)?;
            let open = |anchored: bool, free: &str| if anchored { "" } else { free }.to_owned();
            match shift {
                Shift::Affix { prefix, suffix } => searched(format!(
                    "^{}{}(?:{body}){}{}$",
                    regex::escape(prefix),
                    open(start, r"[\s\S]*"),
                    open(end, r"[\s\S]*"),
                    regex::escape(suffix),
                )),
                Shift::Cut { separator, tail } => {
                    if language_may_contain(body, *separator) {
                        return None;
                    }
                    let free = format!("[^{}]*", class_escape(*separator));
                    let close = if end {
                        format!("{}{tail}", regex::escape(&separator.to_string()))
                    } else {
                        String::new()
                    };
                    searched(format!("^{}(?:{body}){close}", open(start, &free)))
                }
            }
        }
        ValuePattern::Not(pattern) => ValuePattern::Not(Box::new(shifted(pattern, shift)?)),
        ValuePattern::AllOf(patterns) => ValuePattern::AllOf(
            patterns
                .iter()
                .map(|pattern| shifted(pattern, shift))
                .collect::<Option<_>>()?,
        ),
        ValuePattern::AnyOf(patterns) => ValuePattern::AnyOf(
            patterns
                .iter()
                .map(|pattern| shifted(pattern, shift))
                .collect::<Option<_>>()?,
        ),
        ValuePattern::Scalar
        | ValuePattern::TemplateFree
        | ValuePattern::TemplateActionFree
        | ValuePattern::MarkerFree
        | ValuePattern::YamlFlowStable
        | ValuePattern::StringMap
        | ValuePattern::Array {
            prefix: _,
            items: _,
        }
        | ValuePattern::Object { keys: _, values: _ }
        | ValuePattern::Contains(_) => return None,
    })
}

/// Restricts a pattern to string subjects: non-string structure never matches a string.
fn on_text(pattern: &ValuePattern) -> Option<ValuePattern> {
    Some(match pattern {
        ValuePattern::Any => ValuePattern::Any,
        ValuePattern::String { pattern } => searched(pattern.clone()),
        ValuePattern::Scalar
        | ValuePattern::StringMap
        | ValuePattern::Array {
            prefix: _,
            items: _,
        }
        | ValuePattern::Object { keys: _, values: _ }
        | ValuePattern::Contains(_) => ValuePattern::AnyOf(Vec::new()),
        ValuePattern::Not(pattern) => ValuePattern::Not(Box::new(on_text(pattern)?)),
        ValuePattern::AllOf(patterns) => {
            ValuePattern::AllOf(patterns.iter().map(on_text).collect::<Option<_>>()?)
        }
        ValuePattern::AnyOf(patterns) => {
            ValuePattern::AnyOf(patterns.iter().map(on_text).collect::<Option<_>>()?)
        }
        ValuePattern::TemplateFree
        | ValuePattern::TemplateActionFree
        | ValuePattern::MarkerFree
        | ValuePattern::YamlFlowStable => return None,
    })
}

/// A searched regex as an anchor-free body between optional start and end anchors.
struct Searched<'a> {
    start: bool,
    body: &'a str,
    end: bool,
}

fn searched_parts(pattern: &str) -> Option<Searched<'_>> {
    let hir = regex_syntax::Parser::new().parse(pattern).ok()?;
    let parts = match hir.kind() {
        HirKind::Concat(parts) => parts.as_slice(),
        _ if !contains_look(&hir) => {
            return Some(Searched {
                start: false,
                body: pattern,
                end: false,
            });
        }
        _ => return None,
    };
    let start = parts
        .first()
        .is_some_and(|part| matches!(part.kind(), HirKind::Look(Look::Start)));
    let end = parts.len() > usize::from(start)
        && parts
            .last()
            .is_some_and(|part| matches!(part.kind(), HirKind::Look(Look::End)));
    let middle = parts
        .get(usize::from(start)..parts.len() - usize::from(end))
        .unwrap_or_default();
    if middle.iter().any(contains_look) {
        return None;
    }
    let body = if start {
        pattern.strip_prefix('^')?
    } else {
        pattern
    };
    let body = if end { body.strip_suffix('$')? } else { body };
    Some(Searched { start, body, end })
}

fn contains_look(hir: &Hir) -> bool {
    match hir.kind() {
        HirKind::Look(_) => true,
        HirKind::Empty | HirKind::Literal(_) | HirKind::Class(_) => false,
        HirKind::Repetition(repetition) => contains_look(&repetition.sub),
        HirKind::Capture(capture) => contains_look(&capture.sub),
        HirKind::Concat(parts) | HirKind::Alternation(parts) => parts.iter().any(contains_look),
    }
}

/// Whether any string the regex matches could contain `character`; unparsable means maybe.
fn language_may_contain(pattern: &str, character: char) -> bool {
    fn contains(hir: &Hir, character: char) -> bool {
        match hir.kind() {
            HirKind::Empty | HirKind::Look(_) => false,
            HirKind::Literal(literal) => {
                std::str::from_utf8(&literal.0).map_or(true, |text| text.contains(character))
            }
            HirKind::Class(Class::Unicode(class)) => class
                .iter()
                .any(|range| range.start() <= character && character <= range.end()),
            HirKind::Class(Class::Bytes(class)) => u8::try_from(character).map_or(true, |byte| {
                class
                    .iter()
                    .any(|range| range.start() <= byte && byte <= range.end())
            }),
            HirKind::Repetition(repetition) => contains(&repetition.sub, character),
            HirKind::Capture(capture) => contains(&capture.sub, character),
            HirKind::Concat(parts) | HirKind::Alternation(parts) => {
                parts.iter().any(|part| contains(part, character))
            }
        }
    }
    regex_syntax::Parser::new()
        .parse(pattern)
        .map_or(true, |hir| contains(&hir, character))
}

/// A character escaped for use inside a negated class.
fn class_escape(character: char) -> String {
    if character.is_ascii_alphanumeric() {
        character.to_string()
    } else if character.is_ascii() {
        format!("\\{character}")
    } else {
        format!("\\u{{{:04X}}}", u32::from(character))
    }
}

#[cfg(test)]
#[path = "../tests/lexical_preimages.rs"]
mod tests;
