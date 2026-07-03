//! A compact, deterministic text rendering of the CST for golden tests.
//! One node per line, children indented; every node shows its byte span so
//! fixtures pin exact attribution geometry.

use std::fmt::Write as _;

use crate::cst::{
    BlockScalar, ControlKind, Node, OpaqueKind, ScalarPart, ScalarParts, Span, TemplatedDocument,
};

impl TemplatedDocument<'_> {
    /// Render the parsed document as a deterministic dump.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for (index, span) in self.document_spans.iter().enumerate() {
            let _ = writeln!(out, "document {index} {}", fmt_span(*span));
        }
        for node in &self.roots {
            dump_node(node, self.source, 0, &mut out);
        }
        out
    }
}

fn dump_node(node: &Node, source: &str, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    match node {
        Node::Mapping(entry) => {
            let shape = if entry.block.is_some() {
                "block"
            } else if entry.opens_scope {
                "open"
            } else {
                "closed"
            };
            let _ = write!(
                out,
                "{pad}entry {} {} indent={} key={}",
                fmt_span(entry.span),
                shape,
                entry.indent,
                fmt_parts_text(&entry.key, source),
            );
            if let Some(value) = &entry.value {
                let _ = write!(out, " value={}", fmt_parts(value, source));
            }
            if let Some(block) = &entry.block {
                let _ = write!(out, " {}", fmt_block(block));
            }
            out.push('\n');
            for child in &entry.children {
                dump_node(child, source, depth + 1, out);
            }
        }
        Node::Sequence(item) => {
            let _ = write!(
                out,
                "{pad}item {} indent={}",
                fmt_span(item.span),
                item.indent
            );
            if let Some(value) = &item.value {
                let _ = write!(out, " value={}", fmt_parts(value, source));
            }
            if let Some(block) = &item.block {
                let _ = write!(out, " {}", fmt_block(block));
            }
            out.push('\n');
            for child in &item.children {
                dump_node(child, source, depth + 1, out);
            }
        }
        Node::Control(region) => {
            let kind = match region.kind {
                ControlKind::If => "if",
                ControlKind::With => "with",
                ControlKind::Range => "range",
                ControlKind::Define => "define",
                ControlKind::Block => "block",
            };
            let nested = if region.well_nested {
                ""
            } else {
                " ill-nested"
            };
            let _ = writeln!(out, "{pad}control {kind} {}{nested}", fmt_span(region.span));
            for branch in &region.branches {
                let _ = writeln!(
                    out,
                    "{pad}  branch {} {}",
                    fmt_span(branch.header),
                    fmt_text(branch.header, source),
                );
                for child in &branch.body {
                    dump_node(child, source, depth + 2, out);
                }
            }
        }
        Node::Output(output) => {
            let _ = writeln!(
                out,
                "{pad}output {} {}",
                fmt_span(output.span),
                fmt_text(output.span, source),
            );
        }
        Node::Comment(comment) => {
            let _ = writeln!(
                out,
                "{pad}comment {} {}",
                fmt_span(comment.span),
                fmt_parts(&comment.content, source),
            );
        }
        Node::Scalar(scalar) => {
            let _ = writeln!(
                out,
                "{pad}scalar {} indent={} {}",
                fmt_span(scalar.span),
                scalar.indent,
                fmt_parts(&scalar.content, source),
            );
        }
        Node::Opaque(opaque) => {
            let kind = match opaque.kind {
                OpaqueKind::TemplateComment => "template-comment",
                OpaqueKind::Assignment => "assign",
                OpaqueKind::ControlAtom => "control-atom",
                OpaqueKind::InlineRegion => "inline-region",
                OpaqueKind::ActionLineText => "action-line-text",
                OpaqueKind::ParseError => "parse-error",
            };
            let _ = writeln!(
                out,
                "{pad}opaque {kind} {} {}",
                fmt_span(opaque.span),
                fmt_text(opaque.span, source),
            );
        }
    }
}

fn fmt_span(span: Span) -> String {
    format!("[{}..{})", span.start, span.end)
}

fn fmt_text(span: Span, source: &str) -> String {
    let text = source.get(span.start..span.end).unwrap_or("");
    format!("{text:?}")
}

/// Key text: literal source of the whole run (holes visible as raw actions).
fn fmt_parts_text(parts: &ScalarParts, source: &str) -> String {
    fmt_text(parts.span, source)
}

/// Value rendering: text runs verbatim, holes marked with ⟦…⟧ so partial
/// scalars show their exact split.
fn fmt_parts(parts: &ScalarParts, source: &str) -> String {
    let mut rendered = String::new();
    for part in &parts.parts {
        match part {
            ScalarPart::Text(span) => {
                rendered.push_str(source.get(span.start..span.end).unwrap_or(""));
            }
            ScalarPart::Hole(span) => {
                rendered.push('⟦');
                rendered.push_str(source.get(span.start..span.end).unwrap_or(""));
                rendered.push('⟧');
            }
        }
    }
    format!("{rendered:?}")
}

fn fmt_block(block: &BlockScalar) -> String {
    format!(
        "block-header={} body={} suppressed-holes={}",
        fmt_span(block.header),
        fmt_span(block.body),
        block.holes.len()
    )
}
