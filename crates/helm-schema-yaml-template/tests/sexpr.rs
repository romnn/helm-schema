use std::str::FromStr;
use test_util::sexpr::SExpr;

use yaml_rust::scanner::ScanError;
use yaml_rust::FusedNode;
use yaml_rust::{Yaml, YamlLoader};

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

#[must_use]
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

/// # Panics
/// Panics if parsing or comparison fails.
pub fn assert_yaml_matches_sexpr(src: &str, want: &str) {
    let docs = YamlLoader::load_from_str(src).expect("parse yaml");
    let have = yaml_stream_to_sexpr(&docs);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}

/// # Panics
/// Panics if parsing or comparison fails.
pub fn assert_yaml_doc_matches_sexpr(src: &str, want: &str) {
    let docs = YamlLoader::load_from_str(src).expect("parse yaml");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    let have = yaml_to_sexpr(&docs[0]);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}

/// # Errors
/// Returns a [`ScanError`] if the YAML input is invalid.
pub fn parse_yaml_doc_sexpr(src: &str) -> Result<SExpr, ScanError> {
    let docs = YamlLoader::load_from_str(src)?;
    Ok(yaml_stream_to_sexpr(&docs))
}

#[allow(clippy::too_many_lines)]
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
                if then_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "then".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "then".to_string(),
                        children: then_branch.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
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
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
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
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
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
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
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
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::Unknown {
            kind,
            text,
            children,
        } => {
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

/// # Panics
/// Panics if parsing or comparison fails.
pub fn assert_fused_matches_sexpr(src: &str, want: &str) {
    let have = yaml_rust::parse_fused_yaml_helm(src).expect("parse fused");
    let have = fused_to_sexpr(&have);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}
