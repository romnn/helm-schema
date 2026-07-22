//! Parses and formats the tree-sitter S-expressions used in parser tests.

use lexpr::Value;
use std::fmt;
use std::str::FromStr;

/// Describes why a tree-sitter S-expression fixture could not be parsed.
#[derive(Debug)]
pub enum ParseError {
    /// The input is not syntactically valid as an S-expression.
    Parse(lexpr::parse::Error),
    /// The root value is neither a list nor the empty expression.
    InvalidExpression(Value),
    /// A node's kind is not represented by a symbol.
    InvalidNodeKind(Value),
    /// A node declares a text attribute without a following value.
    MissingTextAttribute(String),
    /// A node's text keyword is not followed by any value.
    TextAttributeWithoutString(String),
    /// A node's text attribute contains a non-string value.
    NonStringTextAttribute(String),
    /// The expression contains no node kind.
    Empty,
}

impl From<lexpr::parse::Error> for ParseError {
    fn from(value: lexpr::parse::Error) -> Self {
        Self::Parse(value)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Parse(e) => write!(f, "{e}"),
            ParseError::InvalidExpression(v) => {
                write!(f, "expected an s-expression list or NULL, but found: {v:?}")
            }
            ParseError::InvalidNodeKind(v) => {
                write!(f, "expected node kind to be a symbol, found {v:?}")
            }
            ParseError::MissingTextAttribute(kind) => {
                write!(f, "node `{kind}` is missing a :text attribute")
            }
            ParseError::TextAttributeWithoutString(kind) => {
                write!(
                    f,
                    ":text keyword for node `{kind}` is not followed by a string value"
                )
            }
            ParseError::NonStringTextAttribute(kind) => {
                write!(f, ":text value for node `{kind}` must be a string")
            }
            ParseError::Empty => write!(f, "s-expression cannot be empty"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

/// Represents the relevant structure of a tree-sitter S-expression.
#[derive(Clone, PartialEq, Eq)]
pub enum SExpr {
    /// An empty expression.
    Empty,
    /// A node without structural children.
    Leaf {
        /// The tree-sitter node kind.
        kind: String,
        /// Optional source text attached to the node.
        text: Option<String>,
    },
    /// A node containing nested expressions.
    Node {
        /// The tree-sitter node kind.
        kind: String,
        /// Nested expressions in source order.
        children: Vec<SExpr>,
    },
}

impl FromStr for SExpr {
    type Err = ParseError;

    /// Parse an s-expression from a string.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the input is not a valid s-expression.
    fn from_str(text: &str) -> Result<Self, ParseError> {
        let options = lexpr::parse::Options::default()
            .with_keyword_syntax(lexpr::parse::KeywordSyntax::ColonPrefix);
        let value = lexpr::from_str_custom(text, options)?;
        convert_lexpr_to_sexpr(&value)
    }
}

impl SExpr {
    /// Formats the expression with nested nodes on indented lines.
    #[must_use]
    pub fn to_string_pretty(&self) -> String {
        let mut out = String::new();
        let _ = self.write_with_indent(0, &mut out);
        out
    }

    fn write_with_indent(&self, indent: usize, w: &mut impl fmt::Write) -> fmt::Result {
        let indent_str = " ".repeat(indent);
        match self {
            SExpr::Empty => write!(w, "{indent_str}()"),
            SExpr::Leaf { kind, text } => {
                if let Some(text) = text {
                    write!(
                        w,
                        "{indent_str}({kind} :text {})",
                        escape_lexpr_string(text)
                    )
                } else {
                    write!(w, "{indent_str}({kind})")
                }
            }
            SExpr::Node { kind, children } => {
                write!(w, "{indent_str}({kind}")?;
                for child in children {
                    writeln!(w)?;
                    child.write_with_indent(indent + 2, w)?;
                }
                write!(w, "\n{indent_str})")
            }
        }
    }
}

impl fmt::Debug for SExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_pretty())
    }
}

impl fmt::Display for SExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_pretty())
    }
}

fn escape_lexpr_string(s: &str) -> String {
    Value::String(s.into()).to_string()
}

fn convert_lexpr_to_sexpr(value: &Value) -> Result<SExpr, ParseError> {
    let list: &lexpr::Cons = match value {
        Value::Null => return Ok(SExpr::Empty),
        Value::Cons(list) => list,
        other => return Err(ParseError::InvalidExpression(other.clone())),
    };

    let mut items = list.iter().peekable();

    let kind_val = items.next().ok_or(ParseError::Empty)?.car();
    let kind = kind_val
        .as_symbol()
        .ok_or_else(|| ParseError::InvalidNodeKind(kind_val.clone()))?
        .to_string();

    let has_text = items
        .peek()
        .map(|item| item.car())
        .and_then(|item| item.as_keyword())
        .is_some_and(|kw| kw == "text");

    let text = if has_text {
        let _text_item = items
            .next()
            .ok_or_else(|| ParseError::MissingTextAttribute(kind.clone()))?;

        let text_val = items
            .next()
            .ok_or_else(|| ParseError::TextAttributeWithoutString(kind.clone()))?
            .car();

        let text = text_val
            .as_str()
            .ok_or_else(|| ParseError::NonStringTextAttribute(kind.clone()))?
            .to_string();

        Some(text)
    } else {
        None
    };

    let children: Vec<SExpr> = items
        .map(lexpr::Cons::car)
        .map(convert_lexpr_to_sexpr)
        .collect::<Result<_, _>>()?;

    if children.is_empty() {
        Ok(SExpr::Leaf { kind, text })
    } else if text.is_none() {
        Ok(SExpr::Node { kind, children })
    } else {
        let mut wrapped = Vec::with_capacity(children.len() + 1);
        wrapped.push(SExpr::Leaf {
            kind: "text".to_string(),
            text,
        });
        wrapped.extend(children);
        Ok(SExpr::Node {
            kind,
            children: wrapped,
        })
    }
}
