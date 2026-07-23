use helm_schema_json_schema_walk::visit_subschemas_mut;
use serde_json::Value;

/// Normalize regex dialects in every schema-position `pattern` keyword and
/// `patternProperties` key. Provider schemas carry Go/RE2 spellings —
/// notably a leading global `(?i)` — that Draft-07's ECMA-262 dialect
/// rejects; conforming validators refuse the whole schema over one such
/// pattern. Runs once at provider-fragment ingestion, the boundary where
/// foreign dialect text enters the system, so every downstream consumer
/// sees portable spellings. Rewrites are language-exact (a case fold,
/// never a widening), so an untranslatable pattern stays as-is for the
/// fixture hygiene gate to report rather than silently changing what the
/// schema accepts.
pub fn normalize_schema_pattern_dialects(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        if let Some(Value::String(pattern)) = object.get_mut("pattern")
            && let Some(normalized) = ecma_case_folded_pattern(pattern)
        {
            *pattern = normalized;
        }
        if let Some(Value::Object(pattern_properties)) = object.get_mut("patternProperties") {
            let renames: Vec<(String, String)> = pattern_properties
                .keys()
                .filter_map(|key| {
                    let normalized = ecma_case_folded_pattern(key)?;
                    // A collision with an existing key would clobber a
                    // sibling constraint; keep the original spelling.
                    (!pattern_properties.contains_key(&normalized))
                        .then(|| (key.clone(), normalized))
                })
                .collect();
            for (key, normalized) in renames {
                if let Some(subschema) = pattern_properties.remove(&key) {
                    pattern_properties.insert(normalized, subschema);
                }
            }
        }
    }
    visit_subschemas_mut(schema, &mut normalize_schema_pattern_dialects);
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
