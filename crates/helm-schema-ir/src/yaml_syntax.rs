pub(crate) struct ParsedYamlKey {
    key: String,
}

impl ParsedYamlKey {
    pub(crate) fn into_key(self) -> String {
        self.key
    }
}

pub(crate) fn parse_yaml_key(after: &str) -> Option<ParsedYamlKey> {
    fn finalize_yaml_key(key: String) -> Option<ParsedYamlKey> {
        if key.is_empty() {
            return None;
        }
        Some(ParsedYamlKey { key })
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

pub(crate) fn first_mapping_colon_offset(line: &str) -> Option<usize> {
    // Find the first YAML mapping separator on the physical line while skipping
    // quoted scalars and Helm actions. This distinguishes template output that
    // contributes to a mapping key from output that contributes to the value.
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
mod tests {
    use super::{ParsedYamlKey, first_mapping_colon_offset, parse_yaml_key};
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn parse_yaml_key_handles_plain_and_quoted_keys() {
        sim_assert_eq!(
            have: parse_yaml_key("metadata.name: value").map(ParsedYamlKey::into_key),
            want: Some("metadata.name".to_string())
        );
        sim_assert_eq!(
            have: parse_yaml_key(r#""app.kubernetes.io/name": value"#).map(ParsedYamlKey::into_key),
            want: Some("app.kubernetes.io/name".to_string())
        );
        sim_assert_eq!(
            have: parse_yaml_key("'it''s': value").map(ParsedYamlKey::into_key),
            want: Some("it's".to_string())
        );
    }

    #[test]
    fn first_mapping_colon_skips_templates_and_quoted_scalars() {
        let line = r#"{{ printf "not:a:key" }}: value"#;
        sim_assert_eq!(have: first_mapping_colon_offset(line), want: Some(24));

        let line = r#""not:a:key": value"#;
        sim_assert_eq!(have: first_mapping_colon_offset(line), want: Some(11));
    }
}
