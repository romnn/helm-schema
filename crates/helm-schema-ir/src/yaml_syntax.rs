use crate::fragment_classification::is_fragment_exprs;
use crate::template_expr_cache::ParsedTemplateSnippet;

pub(crate) struct ParsedYamlKey {
    key: String,
    scalar_value_present: bool,
    starts_block_scalar: bool,
}

impl ParsedYamlKey {
    pub(crate) fn into_key(self) -> String {
        self.key
    }

    pub(crate) fn scalar_value_present(&self) -> bool {
        self.scalar_value_present
    }

    pub(crate) fn starts_block_scalar(&self) -> bool {
        self.starts_block_scalar
    }
}

pub(crate) fn parse_yaml_key(after: &str) -> Option<ParsedYamlKey> {
    fn finalize_yaml_key(key: String, rest: &str) -> Option<ParsedYamlKey> {
        if key.is_empty() {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        let rest = rest.trim_start();
        let starts_block_scalar = rest.starts_with('|') || rest.starts_with('>');
        let is_template_fragment =
            rest.starts_with("{{") && is_fragment_exprs(ParsedTemplateSnippet::new(rest).exprs());
        let is_block = rest.is_empty() || starts_block_scalar || is_template_fragment;
        Some(ParsedYamlKey {
            key,
            scalar_value_present: !is_block,
            starts_block_scalar,
        })
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
                    let rest = &quoted[(idx + ch.len_utf8())..];
                    return finalize_yaml_key(key, rest);
                }
                _ => key.push(ch),
            }
        }
        return None;
    }

    let mut chars = after.chars();
    let mut key = String::new();
    while let Some(ch) = chars.next() {
        if ch == ':' {
            return finalize_yaml_key(key.trim().to_string(), chars.as_str());
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

pub(crate) fn source_line_starts_block_scalar(after: &str) -> bool {
    let after = after.trim_start();
    let after = after.strip_prefix("- ").unwrap_or(after);
    parse_yaml_key(after).is_some_and(|key| key.starts_block_scalar)
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

    #[test]
    fn parse_yaml_key_handles_plain_and_quoted_keys() {
        assert_eq!(
            parse_yaml_key("metadata.name: value").map(ParsedYamlKey::into_key),
            Some("metadata.name".to_string())
        );
        assert_eq!(
            parse_yaml_key(r#""app.kubernetes.io/name": value"#).map(ParsedYamlKey::into_key),
            Some("app.kubernetes.io/name".to_string())
        );
        assert_eq!(
            parse_yaml_key("'it''s': value").map(ParsedYamlKey::into_key),
            Some("it's".to_string())
        );
    }

    #[test]
    fn first_mapping_colon_skips_templates_and_quoted_scalars() {
        let line = r#"{{ printf "not:a:key" }}: value"#;
        assert_eq!(first_mapping_colon_offset(line), Some(24));

        let line = r#""not:a:key": value"#;
        assert_eq!(first_mapping_colon_offset(line), Some(11));
    }
}
