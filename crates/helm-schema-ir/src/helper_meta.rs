//! Lexical transformations retained by executed scalar content.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// One lexical divergence between a raw string input and the transformed
/// value a capture observes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LexicalEscape {
    /// The transform is the identity on raw strings NOT containing the
    /// token (`replace TOKEN …`, `(split TOKEN …)._0`): captures exempt
    /// any raw string holding it.
    Contains(String),
    /// `trimPrefix TOKEN` — at most one leading occurrence is stripped
    /// before the capture's check runs, so the accepted raw language is
    /// the capture language optionally prefixed by the token.
    TrimPrefix(String),
    /// `trimSuffix TOKEN` — the suffix mirror of [`Self::TrimPrefix`].
    TrimSuffix(String),
    /// `regexReplaceAll "TOKEN.*$" … ""` — everything from the first TOKEN
    /// to the end of the string is erased before the capture's check runs,
    /// so the accepted raw language is the capture language optionally
    /// followed by a TOKEN-opened tail (cilium's `@digest` strip).
    CutAtToken(String),
}

/// Projects a lexical capture pattern through its escapes.
///
/// Edge escapes compose: each wraps its own side of an anchored pattern
/// (`^P$` becomes `^(?:P)(?:-jmx)?$` for `trimSuffix "-jmx"`, `^(?:v)?(?:P)$`
/// for `trimPrefix "v"`, `^(?:P)(?:@.*)?$` for a `CutAtToken("@")` erasure),
/// so a set with at most one escape per position — leading affix, trailing
/// affix, cut tail — composes in ANY application order into a superset of
/// the ordered preimage (a capture-valid string that itself wears an affix
/// is transformed first, so admitting it is a bounded widening, never a
/// rejection). `Contains` tokens — and any escape mix beyond one per edge —
/// weaken to exemption alternatives: a raw string containing the token
/// diverged from the observed value before the check ran, so the capture
/// accepts it unconditionally (JSON Schema `pattern` is an unanchored
/// search, so a bare token alternative matches any string containing it).
pub(crate) fn pattern_with_lexical_escapes(
    pattern: &str,
    escapes: &BTreeSet<LexicalEscape>,
) -> String {
    if escapes.is_empty() {
        return pattern.to_string();
    }
    if let Some(composed) = composed_edge_escape_pattern(pattern, escapes) {
        return composed;
    }
    let mut alternatives: Vec<String> = escapes
        .iter()
        .map(|escape| match escape {
            LexicalEscape::Contains(token)
            | LexicalEscape::TrimPrefix(token)
            | LexicalEscape::TrimSuffix(token)
            | LexicalEscape::CutAtToken(token) => crate::escape_regex_literal(token),
        })
        .collect();
    alternatives.push(format!("(?:{pattern})"));
    alternatives.join("|")
}

/// The exact edge composition of `pattern_with_lexical_escapes`, or `None`
/// when the escape set holds a `Contains` token, more than one escape per
/// edge position, or the pattern is not anchored on both sides.
fn composed_edge_escape_pattern(
    pattern: &str,
    escapes: &BTreeSet<LexicalEscape>,
) -> Option<String> {
    let anchored = pattern
        .strip_prefix('^')
        .and_then(|p| p.strip_suffix('$'))?;
    let mut prefix = None;
    let mut suffix = None;
    let mut cut = None;
    for escape in escapes {
        let slot = match escape {
            LexicalEscape::Contains(_) => return None,
            LexicalEscape::TrimPrefix(token) => (&mut prefix, token),
            LexicalEscape::TrimSuffix(token) => (&mut suffix, token),
            LexicalEscape::CutAtToken(token) => (&mut cut, token),
        };
        if slot.0.replace(slot.1).is_some() {
            return None;
        }
    }
    let mut composed = String::from("^");
    if let Some(token) = prefix {
        let _ = write!(composed, "(?:{})?", crate::escape_regex_literal(token));
    }
    let _ = write!(composed, "(?:{anchored})");
    if let Some(token) = suffix {
        let _ = write!(composed, "(?:{})?", crate::escape_regex_literal(token));
    }
    if let Some(token) = cut {
        let _ = write!(composed, "(?:{}.*)?", crate::escape_regex_literal(token));
    }
    composed.push('$');
    Some(composed)
}
