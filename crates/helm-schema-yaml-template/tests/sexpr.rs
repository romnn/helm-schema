use std::fmt;

use lexpr::Value;

use yaml_rust::scanner::ScanError;
use yaml_rust::FusedNode;
use yaml_rust::{Yaml, YamlLoader};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error(transparent)]
    Parse(#[from] lexpr::parse::Error),

    #[error("expected an s-expression list or NULL, but found: {0:?}")]
    InvalidExpression(Value),

    #[error("expected node kind to be a symbol, found {0:?}")]
    InvalidNodeKind(Value),

    #[error("node `{0}` is missing a :text attribute")]
    MissingTextAttribute(String),

    #[error(":text keyword for node `{0}` is not followed by a string value")]
    TextAttributeWithoutString(String),

    #[error(":text value for node `{0}` must be a string")]
    NonStringTextAttribute(String),

    #[error("s-expression cannot be empty")]
    Empty,
}

#[derive(Clone, PartialEq, Eq)]
pub enum SExpr {
    Empty,
    Leaf { kind: String, text: Option<String> },
    Node { kind: String, children: Vec<SExpr> },
}

impl SExpr {
    pub fn from_str(text: &str) -> Result<Self, ParseError> {
        let options = lexpr::parse::Options::default()
            .with_keyword_syntax(lexpr::parse::KeywordSyntax::ColonPrefix);
        let value = lexpr::from_str_custom(text, options)?;
        convert_lexpr_to_sexpr(&value)
    }

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
                    write!(w, "{indent_str}({kind} :text {})", escape_lexpr_string(text))
                } else {
                    write!(w, "{indent_str}({kind})")
                }
            }
            SExpr::Node { kind, children } => {
                write!(w, "{indent_str}({kind}")?;
                for child in children {
                    write!(w, "\n")?;
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
        .map(|item| item.car())
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

pub fn yaml_to_sexpr(doc: &Yaml) -> SExpr {
    match doc {
        Yaml::Null => SExpr::Leaf {
            kind: "null".to_string(),
            text: None,
        },
        Yaml::Boolean(b) => SExpr::Leaf {
            kind: "bool".to_string(),
            text: Some(b.to_string()),
        },
        Yaml::Integer(i) => SExpr::Leaf {
            kind: "int".to_string(),
            text: Some(i.to_string()),
        },
        Yaml::Real(s) => SExpr::Leaf {
            kind: "real".to_string(),
            text: Some(s.clone()),
        },
        Yaml::String(s) => SExpr::Leaf {
            kind: "str".to_string(),
            text: Some(s.clone()),
        },
        Yaml::Array(items) => SExpr::Node {
            kind: "seq".to_string(),
            children: items.iter().map(yaml_to_sexpr).collect(),
        },
        Yaml::Hash(h) => {
            let children = h
                .iter()
                .map(|(k, v)| SExpr::Node {
                    kind: "entry".to_string(),
                    children: vec![yaml_to_sexpr(k), yaml_to_sexpr(v)],
                })
                .collect();
            SExpr::Node {
                kind: "map".to_string(),
                children,
            }
        }
        Yaml::Alias(id) => SExpr::Leaf {
            kind: "alias".to_string(),
            text: Some(id.to_string()),
        },
        Yaml::BadValue => SExpr::Leaf {
            kind: "bad".to_string(),
            text: None,
        },
    }
}

pub fn yaml_stream_to_sexpr(docs: &[Yaml]) -> SExpr {
    SExpr::Node {
        kind: "stream".to_string(),
        children: docs
            .iter()
            .map(|doc| SExpr::Node {
                kind: "doc".to_string(),
                children: vec![yaml_to_sexpr(doc)],
            })
            .collect(),
    }
}

pub fn assert_yaml_matches_sexpr(src: &str, want: &str) {
    let docs = YamlLoader::load_from_str(src).expect("parse yaml");
    let have = yaml_stream_to_sexpr(&docs);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}

pub fn assert_yaml_doc_matches_sexpr(src: &str, want: &str) {
    let docs = YamlLoader::load_from_str(src).expect("parse yaml");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    let have = yaml_to_sexpr(&docs[0]);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}

pub fn parse_yaml_doc_sexpr(src: &str) -> Result<SExpr, ScanError> {
    let docs = YamlLoader::load_from_str(src)?;
    Ok(yaml_stream_to_sexpr(&docs))
}

fn fused_to_sexpr(node: &FusedNode) -> SExpr {
    match node {
        FusedNode::Stream { items } => SExpr::Node {
            kind: "stream".to_string(),
            children: items.iter().map(fused_to_sexpr).collect(),
        },
        FusedNode::Document { items } => SExpr::Node {
            kind: "doc".to_string(),
            children: items.iter().map(fused_to_sexpr).collect(),
        },
        FusedNode::Mapping { items } => {
            if items.is_empty() {
                SExpr::Leaf {
                    kind: "map".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "map".to_string(),
                    children: items.iter().map(fused_to_sexpr).collect(),
                }
            }
        }
        FusedNode::Pair { key, value } => {
            let mut children = Vec::with_capacity(2);
            children.push(fused_to_sexpr(key));
            if let Some(v) = value {
                children.push(fused_to_sexpr(v));
            } else {
                children.push(SExpr::Empty);
            }
            SExpr::Node {
                kind: "entry".to_string(),
                children,
            }
        }
        FusedNode::Sequence { items } => {
            if items.is_empty() {
                SExpr::Leaf {
                    kind: "seq".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "seq".to_string(),
                    children: items.iter().map(fused_to_sexpr).collect(),
                }
            }
        }
        FusedNode::Item { value } => {
            if let Some(v) = value {
                fused_to_sexpr(v)
            } else {
                SExpr::Empty
            }
        }
        FusedNode::Scalar { kind, text } => SExpr::Leaf {
            kind: kind.clone(),
            text: Some(text.clone()),
        },
        FusedNode::HelmExpr { text } => SExpr::Leaf {
            kind: "helm_expr".to_string(),
            text: Some(text.clone()),
        },
        FusedNode::HelmComment { text } => SExpr::Leaf {
            kind: "helm_comment".to_string(),
            text: Some(text.clone()),
        },
        FusedNode::If {
            cond,
            then_branch,
            else_branch,
        } => SExpr::Node {
            kind: "if".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "cond".to_string(),
                    text: Some(cond.clone()),
                },
                SExpr::Node {
                    kind: "then".to_string(),
                    children: then_branch.iter().map(fused_to_sexpr).collect(),
                },
                SExpr::Node {
                    kind: "else".to_string(),
                    children: else_branch.iter().map(fused_to_sexpr).collect(),
                },
            ],
        },
        FusedNode::Range {
            header,
            body,
            else_branch,
        } => SExpr::Node {
            kind: "range".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                SExpr::Node {
                    kind: "body".to_string(),
                    children: body.iter().map(fused_to_sexpr).collect(),
                },
                SExpr::Node {
                    kind: "else".to_string(),
                    children: else_branch.iter().map(fused_to_sexpr).collect(),
                },
            ],
        },
        FusedNode::With {
            header,
            body,
            else_branch,
        } => SExpr::Node {
            kind: "with".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                SExpr::Node {
                    kind: "body".to_string(),
                    children: body.iter().map(fused_to_sexpr).collect(),
                },
                SExpr::Node {
                    kind: "else".to_string(),
                    children: else_branch.iter().map(fused_to_sexpr).collect(),
                },
            ],
        },
        FusedNode::Define { header, body } => SExpr::Node {
            kind: "define".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                SExpr::Node {
                    kind: "body".to_string(),
                    children: body.iter().map(fused_to_sexpr).collect(),
                },
            ],
        },
        FusedNode::Block { header, body } => SExpr::Node {
            kind: "block".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                SExpr::Node {
                    kind: "body".to_string(),
                    children: body.iter().map(fused_to_sexpr).collect(),
                },
            ],
        },
        FusedNode::Unknown { kind, text, children } => {
            let mut out_children = Vec::new();
            if let Some(text) = text {
                out_children.push(SExpr::Leaf {
                    kind: "text".to_string(),
                    text: Some(text.clone()),
                });
            }
            out_children.extend(children.iter().map(fused_to_sexpr));
            SExpr::Node {
                kind: kind.clone(),
                children: out_children,
            }
        }
    }
}

pub fn assert_fused_matches_sexpr(src: &str, want: &str) {
    let have = yaml_rust::parse_fused_yaml_helm(src).expect("parse fused");
    let have = fused_to_sexpr(&have);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}
