use std::borrow::Cow;

pub trait Node<'tree, C, K>
where
    Self: Copy + Sized + std::fmt::Debug,
{
    fn is_named(&self) -> bool;
    fn byte_range(&self) -> std::ops::Range<usize>;
    fn start_byte(&self) -> usize {
        self.byte_range().start
    }
    fn end_byte(&self) -> usize {
        self.byte_range().end
    }
    fn raw_kind(&self) -> &'static str;
    fn kind_id(&self) -> u16;
    fn kind(&self) -> K;
    fn children(&self) -> Vec<Self>;
    fn child(&self, idx: usize) -> Option<Self>;
    fn child_by_field_name(&self, name: &str) -> Option<Self>;

    /// Format this node as nicely indented and colored S-expression.
    fn to_pretty_sexpr(&self, source: &str) -> String {
        let expr = SExpr::parse_tree(self, source);
        expr.to_string_pretty()
    }
}

impl<'tree> Node<'tree, tree_sitter::TreeCursor<'tree>, &'static str> for tree_sitter::Node<'tree> {
    fn is_named(&self) -> bool {
        tree_sitter::Node::is_named(self)
    }

    fn byte_range(&self) -> std::ops::Range<usize> {
        tree_sitter::Node::byte_range(self)
    }

    fn kind_id(&self) -> u16 {
        tree_sitter::Node::kind_id(self)
    }

    fn raw_kind(&self) -> &'static str {
        tree_sitter::Node::kind(self)
    }

    fn kind(&self) -> &'static str {
        tree_sitter::Node::kind(self)
    }

    fn children(&self) -> Vec<Self> {
        tree_sitter::Node::children(self, &mut self.walk()).collect()
    }

    fn child(&self, idx: usize) -> Option<tree_sitter::Node<'tree>> {
        tree_sitter::Node::child(self, idx)
    }

    fn child_by_field_name(&self, name: &str) -> Option<tree_sitter::Node<'tree>> {
        tree_sitter::Node::child_by_field_name(self, name)
    }
}

const DEFAULT_PREVIEW_MAX_CHARS: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to parse lexpr with keywords")]
    Parse(#[from] lexpr::parse::Error),

    #[error("expected an s-expression list or NULL, but found: {0:?}")]
    InvalidExpression(lexpr::Value),

    #[error("expected node kind to be a symbol, found {0:?}")]
    InvalidNodeKind(lexpr::Value),

    #[error("node `{0}` is missing a :text attribute")]
    MissingTextAttribute(String),

    #[error(":text keyword for node `{0}` is not followed by a string value")]
    TextAttributeWithoutString(String),

    #[error(":text value for node `{0}` must be a string")]
    NonStringTextAttribute(String),

    #[error("s-expression cannot be empty")]
    Empty,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreviewStyle {
    #[default]
    All,
    LeafNodesOnly,
    None,
}

#[derive(Debug)]
pub struct PrintOptions {
    pub color: bool,
    pub max_preview_chars: Option<usize>,
    pub preview: PreviewStyle,
}

impl Default for PrintOptions {
    fn default() -> Self {
        let color = std::io::IsTerminal::is_terminal(&std::io::stdout());
        Self {
            color,
            max_preview_chars: Some(DEFAULT_PREVIEW_MAX_CHARS),
            preview: PreviewStyle::default(),
        }
    }
}

pub fn is_leaf(kind: &str) -> bool {
    false
    // matches!(
    //     kind,
    //     "inline"
    //         | "html_block"
    //         | "indented_code_block"
    //         | "emphasis"
    //         | "strong_emphasis"
    //         | "code_fence_content"
    //     // image
    //         | "image_description"
    //         | "language"
    //         | "shortcut_link"
    //         | "inline_link"
    //         | "collapsed_reference_link"
    //         | "pipe_table_cell"
    //         | "uri_autolink"
    //         | "email_autolink"
    //         | "link_title"
    //         | "link_destination"
    //         | "link_label"
    //         | "link_text"
    // )
}

pub fn strip_non_leaf_text(expr: &mut SExpr) {
    if let SExpr::Node {
        kind,
        text,
        children,
    } = expr
    {
        if !is_leaf(kind) {
            *text = None;
        }
        for child in children {
            strip_non_leaf_text(child);
        }
    }
}

pub fn escape_lexpr_string(s: &str) -> String {
    lexpr::Value::String(s.into()).to_string()
}

/// Truncates a string to at most `max_len` chars at the end of the string, chopping off from the start.
pub fn truncate_start(s: &str, max_chars: usize) -> &str {
    let char_count = s.chars().count();

    // Find the byte index of the character that starts the suffix we want to keep.
    // `char_indices()` gives us pairs of (byte_index, char).
    // We skip the first `char_count - max_len` characters to find our starting point.
    let start_byte_index = s
        .char_indices()
        .nth(char_count.saturating_sub(max_chars))
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    // Now we can safely slice the string by byte index.
    &s[start_byte_index..]
}

/// Truncates a string to at most `max_chars` chars at the start of the string, chopping off from the end.
pub fn truncate_end(s: &str, max_chars: usize) -> &str {
    // Find the byte index of the last character that we want to keep.
    let end_byte_index = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    &s[..end_byte_index]
}

pub fn truncate_middle_with_dots<'a>(s: &'a str, max_chars: usize) -> Cow<'a, str> {
    let total_chars = s.chars().count();
    if max_chars >= total_chars {
        s.into()
    } else {
        let half = max_chars.saturating_sub(5) as f32 / 2.0;
        let left = half.ceil() as usize;
        let right = half.floor() as usize;
        let left = truncate_end(s, left);
        let right = truncate_start(s, right);
        format!("{left} ... {right}").into()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum SExpr {
    Empty,
    Leaf {
        kind: String,
        text: Option<String>,
    },
    Node {
        kind: String,
        text: Option<String>,
        children: Vec<SExpr>,
    },
}

impl SExpr {
    // pub fn from_str(text: &str) -> Result<Self, Error> {
    //     parse_sexp_with_keywords(text)
    // }

    pub fn parse_tree<'tree, N, C, K>(node: &N, source: &str) -> SExpr
    where
        N: Node<'tree, C, K>,
    {
        let named_children: Vec<_> = node
            .children()
            .into_iter()
            .filter(|child| child.is_named())
            .collect();
        if named_children.is_empty() {
            let kind = node.raw_kind().to_string();
            let text = source[node.byte_range()].to_string();
            SExpr::Leaf {
                kind,
                text: Some(text),
            }
        } else {
            let kind = node.raw_kind().to_string();
            let text = source[node.byte_range()].to_string();
            let children = named_children
                .into_iter()
                .map(|c| SExpr::parse_tree(&c, source))
                .collect();
            SExpr::Node {
                kind,
                text: Some(text),
                children,
            }
        }
    }

    fn write_with_indent(
        &self,
        indent: Option<usize>,
        w: &mut impl std::fmt::Write,
        options: &PrintOptions,
    ) -> std::fmt::Result {
        use owo_colors::OwoColorize;
        use std::fmt::{Debug, Display};

        let indent_str = " ".repeat(indent.unwrap_or(1));
        match self {
            SExpr::Empty => {
                write!(w, r#"{indent_str}()"#)
            }
            SExpr::Leaf { kind, text } => {
                let kind = if options.color {
                    &kind.blue() as &dyn Display
                } else {
                    kind as &dyn Display
                };

                if options.preview == PreviewStyle::None {
                    write!(w, r#"{indent_str}({kind})"#)
                } else if let Some(text) = text {
                    let text = escape_lexpr_string(text);
                    let text = if options.color {
                        &text.dimmed() as &dyn Display
                    } else {
                        &text as &dyn Display
                    };
                    write!(w, r#"{indent_str}({kind} :text {text})"#)
                } else {
                    write!(w, r#"{indent_str}({kind})"#)
                }
            }
            SExpr::Node {
                kind,
                text,
                children,
            } => {
                let include_text = options.preview == PreviewStyle::All
                    || (is_leaf(kind) && options.preview == PreviewStyle::LeafNodesOnly);

                let kind = if options.color {
                    &kind.blue() as &dyn Display
                } else {
                    kind as &dyn Display
                };

                if let Some(text) = text
                    && include_text
                {
                    let text = escape_lexpr_string(text);
                    let text_preview = if let Some(max_chars) = options.max_preview_chars {
                        truncate_middle_with_dots(&text, max_chars)
                    } else {
                        text.into()
                    };
                    let text_preview = if options.color {
                        &text_preview.dimmed() as &dyn Display
                    } else {
                        &text_preview as &dyn Display
                    };
                    write!(w, r#"{indent_str}({kind} :text {text_preview}"#)?;
                } else {
                    write!(w, r#"{indent_str}({kind}"#)?;
                }
                for child in children {
                    if indent.is_some() {
                        write!(w, "\n")?;
                    }
                    child.write_with_indent(indent.map(|indent| indent + 2), w, options)?;
                }
                if indent.is_some() {
                    write!(w, "\n")?;
                }
                write!(w, "{})", indent_str)
            }
        }
    }

    pub fn to_string_pretty(&self) -> String {
        let options = PrintOptions::default();
        let mut buf = String::new();
        let _ = self.write_with_indent(Some(0), &mut buf, &options);
        buf
    }

    pub fn to_string_pretty_with_options(&self, options: &PrintOptions) -> String {
        let mut buf = String::new();
        let _ = self.write_with_indent(Some(0), &mut buf, options);
        buf
    }
}

impl std::fmt::Debug for SExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_with_indent(
            Some(2),
            f,
            &PrintOptions {
                color: false,
                max_preview_chars: None,
                preview: PreviewStyle::LeafNodesOnly,
            },
        )
    }
}

impl std::fmt::Display for SExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_with_indent(None, f, &PrintOptions::default())
    }
}
