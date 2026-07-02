pub fn parse_yaml_key(after: &str) -> Option<String> {
    fn finalize_yaml_key(key: String) -> Option<String> {
        if key.is_empty() {
            return None;
        }
        Some(key)
    }

    let after = after.trim_end();
    if after.starts_with("{{") {
        return None;
    }

    if let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let quoted = &after[quote.len_utf8()..];
        let mut chars = quoted.char_indices().peekable();
        let mut key = String::new();
        while let Some((idx, ch)) = chars.next() {
            match (quote, ch) {
                ('"', '\\') => {
                    let (_, escaped) = chars.next()?;
                    key.push(escaped);
                }
                ('\'', '\'') if chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                    let _ = chars.next();
                    key.push('\'');
                }
                (_, ch) if ch == quote => {
                    return quoted[(idx + ch.len_utf8())..]
                        .trim_start()
                        .starts_with(':')
                        .then(|| finalize_yaml_key(key))
                        .flatten();
                }
                _ => key.push(ch),
            }
        }
        return None;
    }

    let chars = after.chars();
    let mut key = String::new();
    for ch in chars {
        if ch == ':' {
            return finalize_yaml_key(key.trim().to_string());
        }
        if ch.is_whitespace() {
            return None;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            key.push(ch);
            continue;
        }
        return None;
    }
    None
}

/// Strip one matching pair of surrounding single or double quotes from a
/// YAML scalar, returning the inner text. Unquoted (or mismatched-quote)
/// input is returned unchanged.
pub fn unquote_yaml_scalar(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

/// Whether the colon at `colon` separates a mapping key from its value.
/// YAML only treats `:` as a mapping separator when it ends the line or is
/// followed by whitespace; `key:value` is a plain scalar.
pub(crate) fn mapping_colon_is_structural(text: &str, colon: usize) -> bool {
    text[colon + 1..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace)
}

pub fn first_mapping_colon_offset(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' if bytes.get(idx + 1) == Some(&b'{') => {
                idx += 2;
                while idx + 1 < bytes.len() {
                    if bytes[idx] == b'}' && bytes[idx + 1] == b'}' {
                        idx += 2;
                        break;
                    }
                    idx += 1;
                }
            }
            b'"' => {
                idx += 1;
                while idx < bytes.len() {
                    match bytes[idx] {
                        b'\\' => idx += 2,
                        b'"' => {
                            idx += 1;
                            break;
                        }
                        _ => idx += 1,
                    }
                }
            }
            b'\'' => {
                idx += 1;
                while idx < bytes.len() {
                    if bytes[idx] == b'\'' {
                        if bytes.get(idx + 1) == Some(&b'\'') {
                            idx += 2;
                            continue;
                        }
                        idx += 1;
                        break;
                    }
                    idx += 1;
                }
            }
            b':' => return Some(idx),
            _ => idx += 1,
        }
    }
    None
}

#[cfg(test)]
#[path = "tests/yaml_syntax.rs"]
mod tests;
