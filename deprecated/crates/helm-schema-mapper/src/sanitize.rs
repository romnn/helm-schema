use crate::analyze::Role;
use std::ops::Range;

/// Record of a placeholder we inserted while sanitizing
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Placeholder {
    pub id: usize,
    pub role: Role,
    pub action_span: Range<usize>,
    pub values: Vec<String>, // the .Values paths that live under this action
    /// True when this placeholder comes from a fragment-producing expression
    /// like `include` (rendering YAML) or `toYaml ... | nindent`.
    pub is_fragment_output: bool,
}

pub(crate) fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_clause"
    )
}

// Template root is also a container we must descend into
pub(crate) fn is_container(kind: &str) -> bool {
    matches!(kind, "template" | "define_action")
}

// Assignment actions shouldn't render; we only record their uses.
pub(crate) fn is_assignment_kind(kind: &str) -> bool {
    matches!(
        kind,
        "short_variable_declaration"
            | "variable_declaration"
            | "assignment"
            | "variable_definition"
    )
}

pub fn validate_yaml_strict_all_docs(src: &str) -> Result<(), serde_yaml::Error> {
    use serde::de::Deserialize;
    let mut de = serde_yaml::Deserializer::from_str(src);
    // Deserialize the whole stream (YAML can contain multiple documents)
    while let Some(doc) = de.next() {
        serde_yaml::Value::deserialize(doc)?; // parse or error with location
    }
    Ok(())
}

pub fn pretty_yaml_error(src: &str, err: &serde_yaml::Error) -> String {
    if let Some(loc) = err.location() {
        let (line0, col0) = (loc.line().saturating_sub(1), loc.column().saturating_sub(1));
        let line_txt = src.lines().nth(line0).unwrap_or("");
        let caret = " ".repeat(col0) + "^";
        format!(
            "YAML error at {}:{}: {}\n{}\n{}",
            loc.line(),
            loc.column(),
            err,
            line_txt,
            caret
        )
    } else {
        err.to_string()
    }
}

// Where would a node go *syntactically* if we rendered something here?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Slot {
    MappingValue, // previous non-empty line ends with ':'
    SequenceItem, // current line (after indentation) starts with "- "
    Plain,        // anywhere else
}

pub(crate) fn ensure_current_line_indent(buf: &mut String, spaces: usize) {
    let start = buf.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let mut existing = 0usize;
    for ch in buf[start..].chars() {
        if ch == ' ' {
            existing += 1;
        } else {
            break;
        }
    }
    for _ in existing..spaces {
        buf.push(' ');
    }
}
