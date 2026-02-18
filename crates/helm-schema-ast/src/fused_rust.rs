use yaml_rust::fused::{FusedNode, parse_fused_yaml_helm};

use crate::{HelmAst, HelmParser, ParseError};

/// Parser implementation backed by the pure-Rust yaml-rust fused parser.
pub struct FusedRustParser;

impl HelmParser for FusedRustParser {
    fn parse(&self, src: &str) -> Result<HelmAst, ParseError> {
        let fused = parse_fused_yaml_helm(src)?;
        Ok(convert_fused_node(&fused))
    }
}

fn convert_fused_node(node: &FusedNode) -> HelmAst {
    match node {
        FusedNode::Stream { items } => {
            // Flatten: if a stream has exactly one document, return the document directly.
            let converted: Vec<HelmAst> = items.iter().map(convert_fused_node).collect();
            if converted.len() == 1 {
                converted.into_iter().next().unwrap()
            } else {
                HelmAst::Document { items: converted }
            }
        }
        FusedNode::Document { items } => HelmAst::Document {
            items: items.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Mapping { items } => HelmAst::Mapping {
            items: items.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Pair { key, value } => {
            // Normalize null YAML values (kind: "null") to None so that
            // `key:` with no value produces Pair { value: None }, matching
            // the tree-sitter parser's behavior.
            let v = value.as_ref().map(|v| convert_fused_node(v));
            let v = match &v {
                Some(HelmAst::Scalar { text }) if text == "null" => {
                    // Only suppress if the original FusedNode was kind "null"
                    // (true YAML null), not a string that happens to say "null".
                    match value.as_deref() {
                        Some(FusedNode::Scalar { kind, .. }) if kind == "null" => None,
                        _ => v,
                    }
                }
                _ => v,
            };
            HelmAst::Pair {
                key: Box::new(convert_fused_node(key)),
                value: v.map(Box::new),
            }
        }
        FusedNode::Sequence { items } => HelmAst::Sequence {
            items: items.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Item { value } => {
            // Unwrap Item wrapper; if empty, produce an empty scalar.
            match value {
                Some(v) => convert_fused_node(v),
                None => HelmAst::Scalar {
                    text: String::new(),
                },
            }
        }
        FusedNode::Scalar { text, .. } => HelmAst::Scalar { text: text.clone() },
        FusedNode::HelmExpr { text } => HelmAst::HelmExpr { text: text.clone() },
        FusedNode::HelmComment { text } => HelmAst::HelmComment { text: text.clone() },
        FusedNode::If {
            cond,
            then_branch,
            else_branch,
        } => HelmAst::If {
            cond: cond.clone(),
            then_branch: then_branch.iter().map(convert_fused_node).collect(),
            else_branch: else_branch.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Range {
            header,
            body,
            else_branch,
        } => HelmAst::Range {
            header: header.clone(),
            body: body.iter().map(convert_fused_node).collect(),
            else_branch: else_branch.iter().map(convert_fused_node).collect(),
        },
        FusedNode::With {
            header,
            body,
            else_branch,
        } => HelmAst::With {
            header: header.clone(),
            body: body.iter().map(convert_fused_node).collect(),
            else_branch: else_branch.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Define { header, body } => HelmAst::Define {
            name: header.trim().trim_matches('"').to_string(),
            body: body.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Block { header, body } => HelmAst::Block {
            name: header.trim().trim_matches('"').to_string(),
            body: body.iter().map(convert_fused_node).collect(),
        },
        FusedNode::Unknown { children, .. } => {
            let items: Vec<HelmAst> = children.iter().map(convert_fused_node).collect();
            if items.len() == 1 {
                items.into_iter().next().unwrap()
            } else {
                HelmAst::Document { items }
            }
        }
    }
}
