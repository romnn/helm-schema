use crate::{HelmAst, HelmParser, ParseError};

/// Parser implementation backed by the tree-sitter fused Helm+YAML grammar.
///
/// Uses the `go_template` grammar for top-level structure and the
/// `helm_template` grammar for re-parsing YAML fragments within text nodes.
pub struct TreeSitterParser;

impl HelmParser for TreeSitterParser {
    fn parse(&self, src: &str) -> Result<HelmAst, ParseError> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .map_err(|_| ParseError::TreeSitterParseFailed)?;

        let tree = parser
            .parse(src, None)
            .ok_or(ParseError::TreeSitterParseFailed)?;

        let root = tree.root_node();
        let mut blocks = Vec::new();
        let mut c = root.walk();
        for ch in root.children(&mut c) {
            blocks.push(ch);
        }

        let items = fuse_blocks(&blocks, src);
        Ok(HelmAst::Document { items })
    }
}

// ---------------------------------------------------------------------------
// Internal: fuse tree-sitter nodes into HelmAst
// ---------------------------------------------------------------------------

fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "range_action" | "with_action" | "define_action" | "block_action"
    )
}

fn children_with_field<'a>(node: tree_sitter::Node<'a>, field: &str) -> Vec<tree_sitter::Node<'a>> {
    let mut out = Vec::new();
    let child_count = node.child_count();
    for i in 0..child_count {
        let Some(ch) = node.child(i) else {
            continue;
        };
        if node.field_name_for_child(i as u32) != Some(field) {
            continue;
        }
        out.push(ch);
    }
    out
}

fn normalize_helm_template_text(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("{{") {
        s = rest;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    s = s.trim_start();
    if let Some(rest) = s.strip_suffix("}}") {
        s = rest;
    }
    s = s.strip_suffix('-').unwrap_or(s);
    s.trim().to_string()
}

fn deindent_yaml_fragment(fragment: &str) -> String {
    let mut min_indent: Option<usize> = None;
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            continue;
        }
        let indent = content
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        min_indent = Some(match min_indent {
            None => indent,
            Some(prev) => prev.min(indent),
        });
    }

    let Some(min_indent) = min_indent else {
        return fragment.to_string();
    };

    let mut out = String::with_capacity(fragment.len());
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            out.push_str(line);
            continue;
        }

        let mut removed = 0usize;
        let mut idx = 0usize;
        for ch in line.chars() {
            if removed >= min_indent {
                break;
            }
            if ch == ' ' || ch == '\t' {
                removed += 1;
                idx += ch.len_utf8();
                continue;
            }
            break;
        }
        out.push_str(&line[idx..]);
    }
    out
}

/// Re-parse a YAML fragment using the fused helm_template grammar and convert to HelmAst nodes.
fn parse_yaml_items(src: &str) -> Vec<HelmAst> {
    let src = deindent_yaml_fragment(src);
    if src.trim().is_empty() {
        return vec![];
    }

    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("set language");

    let Some(tree) = parser.parse(&src, None) else {
        return vec![];
    };

    let root = tree.root_node();
    let mut out = Vec::new();

    let mut cursor = root.walk();
    for doc in root.children(&mut cursor) {
        if !doc.is_named() || doc.kind() != "document" {
            continue;
        }

        let mut dc = doc.walk();
        for ch in doc.children(&mut dc) {
            if !ch.is_named() {
                continue;
            }
            if matches!(
                ch.kind(),
                "reserved_directive" | "tag_directive" | "yaml_directive"
            ) {
                continue;
            }
            out.push(yaml_node_to_ast(ch, &src));
        }
    }

    out
}

/// Convert a single tree-sitter YAML/Helm node into HelmAst.
fn yaml_node_to_ast(node: tree_sitter::Node<'_>, src: &str) -> HelmAst {
    match node.kind() {
        "block_node" | "flow_node" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: String::new(),
                }
            }
        }
        "document" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(
                    ch.kind(),
                    "reserved_directive" | "tag_directive" | "yaml_directive"
                ) {
                    continue;
                }
                kids.push(yaml_node_to_ast(ch, src));
            }
            HelmAst::Document { items: kids }
        }
        "block_mapping" | "flow_mapping" => {
            let pair_kind = if node.kind() == "block_mapping" {
                "block_mapping_pair"
            } else {
                "flow_pair"
            };
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == pair_kind {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            HelmAst::Mapping { items: kids }
        }
        "block_mapping_pair" | "flow_pair" => {
            let key = node
                .child_by_field_name("key")
                .map(|n| Box::new(yaml_node_to_ast(n, src)))
                .unwrap_or_else(|| {
                    Box::new(HelmAst::Scalar {
                        text: String::new(),
                    })
                });
            let value = node
                .child_by_field_name("value")
                .map(|n| Box::new(yaml_node_to_ast(n, src)));

            HelmAst::Pair { key, value }
        }
        "block_sequence" | "flow_sequence" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(ch.kind(), "block_sequence_item" | "flow_node" | "flow_pair") {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            HelmAst::Sequence { items: kids }
        }
        "block_sequence_item" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: String::new(),
                }
            }
        }
        "plain_scalar" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: node
                        .utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                }
            }
        }
        "string_scalar"
        | "double_quote_scalar"
        | "single_quote_scalar"
        | "block_scalar"
        | "integer_scalar"
        | "float_scalar"
        | "boolean_scalar" => HelmAst::Scalar {
            text: node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string(),
        },
        "null_scalar" => HelmAst::Scalar {
            text: String::new(),
        },
        "helm_template" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            HelmAst::HelmExpr {
                text: normalize_helm_template_text(text),
            }
        }
        _ => {
            // Generic fallback: recurse into named children.
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            if kids.is_empty() {
                HelmAst::Scalar {
                    text: node
                        .utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                }
            } else if kids.len() == 1 {
                kids.into_iter().next().unwrap()
            } else {
                HelmAst::Document { items: kids }
            }
        }
    }
}

/// Fuse a sequence of tree-sitter top-level nodes (text + control flow) into HelmAst nodes.
fn fuse_blocks(blocks: &[tree_sitter::Node<'_>], src: &str) -> Vec<HelmAst> {
    let mut out: Vec<HelmAst> = Vec::new();
    let mut pending = String::new();

    let flush_pending = |pending: &mut String, out: &mut Vec<HelmAst>| {
        if pending.trim().is_empty() {
            pending.clear();
            return;
        }
        let fragment = std::mem::take(pending);
        out.extend(parse_yaml_items(&fragment));
    };

    let mut i = 0usize;
    while i < blocks.len() {
        let b = blocks[i];

        // Detect comment actions: `{{` + `comment` node + `}}`
        if (b.kind() == "{{" || b.kind() == "{{-")
            && blocks
                .get(i + 1)
                .is_some_and(|n| n.is_named() && n.kind() == "comment")
        {
            flush_pending(&mut pending, &mut out);

            let comment_node = blocks[i + 1];
            let comment_text = comment_node.utf8_text(src.as_bytes()).unwrap_or("");
            out.push(HelmAst::HelmComment {
                text: comment_text.to_string(),
            });

            i += 2;

            // Skip whitespace tokens before closing delimiter.
            while i < blocks.len()
                && !blocks[i].is_named()
                && blocks[i].kind().chars().all(|c| c.is_whitespace())
            {
                i += 1;
            }

            // Skip closing delimiter.
            if i < blocks.len() && (blocks[i].kind() == "}}" || blocks[i].kind() == "-}}") {
                i += 1;
            }
            continue;
        }

        if is_control_flow(b.kind()) {
            flush_pending(&mut pending, &mut out);
            out.push(fuse_control_flow(b, src));
        } else {
            let r = b.byte_range();
            pending.push_str(&src[r]);
        }

        i += 1;
    }

    flush_pending(&mut pending, &mut out);
    out
}

/// Convert a tree-sitter control-flow node into HelmAst.
fn fuse_control_flow(node: tree_sitter::Node<'_>, src: &str) -> HelmAst {
    match node.kind() {
        "if_action" => {
            let cond_text = node
                .child_by_field_name("condition")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let then_blocks = children_with_field(node, "consequence");
            let else_blocks = children_with_field(node, "alternative");

            let then_items = fuse_blocks(&then_blocks, src);
            let base_else_items = fuse_blocks(&else_blocks, src);

            // Handle `else if` chains: tree-sitter inlines them as repeated
            // condition/option fields. We lower them into nested If nodes.
            let mut else_if_pairs: Vec<(String, Vec<tree_sitter::Node<'_>>)> = Vec::new();
            let mut seen_main_condition = false;
            for i in 0..node.child_count() {
                let Some(ch) = node.child(i) else {
                    continue;
                };
                match node.field_name_for_child(i as u32) {
                    Some("condition") => {
                        if !seen_main_condition {
                            seen_main_condition = true;
                        } else {
                            let cnd = ch
                                .utf8_text(src.as_bytes())
                                .unwrap_or("")
                                .trim()
                                .to_string();
                            else_if_pairs.push((cnd, Vec::new()));
                        }
                    }
                    Some("option") => {
                        if let Some((_, blocks)) = else_if_pairs.last_mut() {
                            blocks.push(ch);
                        }
                    }
                    _ => {}
                }
            }

            let else_items = if else_if_pairs.is_empty() {
                base_else_items
            } else {
                let mut tail = base_else_items;
                for (cnd, blocks) in else_if_pairs.into_iter().rev() {
                    let opt_items = fuse_blocks(&blocks, src);
                    tail = vec![HelmAst::If {
                        cond: cnd,
                        then_branch: opt_items,
                        else_branch: tail,
                    }];
                }
                tail
            };

            HelmAst::If {
                cond: cond_text,
                then_branch: then_items,
                else_branch: else_items,
            }
        }
        "range_action" => {
            let header = node
                .child_by_field_name("range")
                .or_else(|| {
                    // For `range $key, $value := expr`, the "range" field is
                    // nested inside a `range_variable_definition` child.
                    let mut c = node.walk();
                    node.children(&mut c)
                        .find(|ch| ch.kind() == "range_variable_definition")
                        .and_then(|rvd| rvd.child_by_field_name("range"))
                })
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let body = fuse_blocks(&children_with_field(node, "body"), src);
            let else_branch = fuse_blocks(&children_with_field(node, "alternative"), src);

            HelmAst::Range {
                header,
                body,
                else_branch,
            }
        }
        "with_action" => {
            let header = node
                .child_by_field_name("condition")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let body = fuse_blocks(&children_with_field(node, "consequence"), src);
            let else_branch = fuse_blocks(&children_with_field(node, "alternative"), src);

            HelmAst::With {
                header,
                body,
                else_branch,
            }
        }
        "define_action" => {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string();

            let body = fuse_blocks(&children_with_field(node, "body"), src);

            HelmAst::Define { name, body }
        }
        "block_action" => {
            let name_part = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();
            let arg_part = node
                .child_by_field_name("argument")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();

            let name = if arg_part.is_empty() {
                name_part.trim_matches('"').to_string()
            } else {
                format!("{} {}", name_part.trim_matches('"'), arg_part)
            };

            let body = fuse_blocks(&children_with_field(node, "body"), src);

            HelmAst::Block { name, body }
        }
        _ => HelmAst::Scalar {
            text: node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string(),
        },
    }
}
