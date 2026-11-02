//! Supported Go format syntax for the canonical interpreter.

/// A parsed contribution from the supported Go format language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintfPart<'a> {
    /// Literal output, including an escaped percent sign.
    Literal(&'a str),
    /// One data operand rendered using this verb.
    Substitution(PrintfVerb),
}

/// Go formatting verbs whose output the interpreter can retain structurally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintfVerb {
    /// A string operand rendered without escaping.
    String,
    /// An integer operand rendered in base ten.
    Decimal,
}

/// Parses `%s`, `%d`, and `%%`, declining flags, widths, indexes, and other verbs.
#[must_use]
#[expect(
    clippy::indexing_slicing,
    reason = "this hot format scanner checks the byte cursor before every direct access"
)]
pub fn parse_supported_printf_format(format: &str) -> Option<Vec<PrintfPart<'_>>> {
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
            b's' | b'd' => {
                let verb = if bytes[index + 1] == b's' {
                    PrintfVerb::String
                } else {
                    PrintfVerb::Decimal
                };
                parts.push(PrintfPart::Substitution(verb));
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
