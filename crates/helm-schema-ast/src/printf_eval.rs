use std::collections::BTreeSet;

use crate::{Literal, TemplateExpr};

/// Returns the first argument when it is a literal `printf` format string.
#[must_use]
pub fn literal_printf_format(args: &[TemplateExpr]) -> Option<&str> {
    match args.first()?.deparen() {
        TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) => {
            Some(format.as_str())
        }
        _ => None,
    }
}

/// Evaluates a supported literal `printf` format over finite argument string sets.
#[must_use]
pub fn render_printf_string_sets(
    format: &str,
    arg_strings: &[BTreeSet<String>],
) -> Option<BTreeSet<String>> {
    let parts = parse_supported_printf_format(format)?;
    let substitutions = parts
        .iter()
        .filter(|part| matches!(part, PrintfPart::Substitution))
        .count();
    if substitutions != arg_strings.len() {
        return None;
    }

    let mut rendered: BTreeSet<String> = [String::new()].into_iter().collect();
    let mut arg_index = 0usize;
    for part in parts {
        match part {
            PrintfPart::Literal(literal) => {
                rendered = rendered
                    .into_iter()
                    .map(|mut current| {
                        current.push_str(literal);
                        current
                    })
                    .collect();
            }
            PrintfPart::Substitution => {
                let strings = arg_strings.get(arg_index)?;
                if strings.is_empty() {
                    return None;
                }
                let mut next = BTreeSet::new();
                for current in &rendered {
                    for value in strings {
                        let mut rendered_value = current.clone();
                        rendered_value.push_str(value);
                        next.insert(rendered_value);
                    }
                }
                rendered = next;
                arg_index += 1;
            }
        }
    }
    Some(rendered)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintfPart<'a> {
    Literal(&'a str),
    Substitution,
}

#[expect(
    clippy::indexing_slicing,
    reason = "this hot format scanner checks the byte cursor before every direct access"
)]
fn parse_supported_printf_format(format: &str) -> Option<Vec<PrintfPart<'_>>> {
    let mut parts = Vec::new();
    let mut literal_start = 0usize;
    let bytes = format.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }

        if literal_start < index {
            parts.push(PrintfPart::Literal(format.get(literal_start..index)?));
        }

        match *bytes.get(index + 1)? {
            b'%' => {
                parts.push(PrintfPart::Literal("%"));
                index += 2;
                literal_start = index;
            }
            b's' => {
                parts.push(PrintfPart::Substitution);
                index += 2;
                literal_start = index;
            }
            _ => return None,
        }
    }

    if literal_start < format.len() {
        parts.push(PrintfPart::Literal(format.get(literal_start..)?));
    }

    Some(parts)
}
