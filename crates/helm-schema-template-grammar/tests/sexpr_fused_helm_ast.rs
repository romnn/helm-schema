use indoc::indoc;
use test_util::sexpr::SExpr;

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

fn parse_yaml_items(src: &str) -> Vec<SExpr> {
    let src = deindent_yaml_fragment(src);
    if src.trim().is_empty() {
        return vec![];
    }

    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("set language");

    let Some(tree) = parser.parse(&src, None) else {
        return vec![SExpr::Leaf {
            kind: "yaml_parse_error".to_string(),
            text: Some(src),
        }];
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
            out.push(yaml_node_to_sexpr(ch, &src));
        }
    }

    out
}

fn yaml_node_to_sexpr(node: tree_sitter::Node<'_>, src: &str) -> SExpr {
    match node.kind() {
        "block_node" | "flow_node" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_sexpr(ch, src)
            } else {
                SExpr::Empty
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
                kids.push(yaml_node_to_sexpr(ch, src));
            }
            SExpr::Node {
                kind: "doc".to_string(),
                children: kids,
            }
        }
        "block_mapping" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == "block_mapping_pair" {
                    kids.push(yaml_node_to_sexpr(ch, src));
                }
            }
            if kids.is_empty() {
                SExpr::Leaf {
                    kind: "map".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "map".to_string(),
                    children: kids,
                }
            }
        }
        "flow_mapping" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == "flow_pair" {
                    kids.push(yaml_node_to_sexpr(ch, src));
                }
            }
            if kids.is_empty() {
                SExpr::Leaf {
                    kind: "map".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "map".to_string(),
                    children: kids,
                }
            }
        }
        "block_mapping_pair" | "flow_pair" => {
            let key = node
                .child_by_field_name("key")
                .map(|n| yaml_node_to_sexpr(n, src))
                .unwrap_or(SExpr::Empty);
            let value = node
                .child_by_field_name("value")
                .map(|n| yaml_node_to_sexpr(n, src))
                .unwrap_or(SExpr::Empty);

            SExpr::Node {
                kind: "entry".to_string(),
                children: vec![key, value],
            }
        }
        "block_sequence" | "flow_sequence" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(ch.kind(), "block_sequence_item" | "flow_node" | "flow_pair") {
                    kids.push(yaml_node_to_sexpr(ch, src));
                }
            }
            if kids.is_empty() {
                SExpr::Leaf {
                    kind: "seq".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "seq".to_string(),
                    children: kids,
                }
            }
        }
        "block_sequence_item" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_sexpr(ch, src)
            } else {
                SExpr::Empty
            }
        }
        "plain_scalar" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_sexpr(ch, src)
            } else {
                SExpr::Leaf {
                    kind: "str".to_string(),
                    text: Some(
                        node.utf8_text(src.as_bytes())
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    ),
                }
            }
        }
        "string_scalar" | "double_quote_scalar" | "single_quote_scalar" | "block_scalar" => {
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
            SExpr::Leaf {
                kind: "str".to_string(),
                text: Some(text),
            }
        }
        "integer_scalar" => {
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
            SExpr::Leaf {
                kind: "int".to_string(),
                text: Some(text),
            }
        }
        "float_scalar" => {
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
            SExpr::Leaf {
                kind: "real".to_string(),
                text: Some(text),
            }
        }
        "boolean_scalar" => {
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
            SExpr::Leaf {
                kind: "bool".to_string(),
                text: Some(text),
            }
        }
        "null_scalar" => SExpr::Leaf {
            kind: "null".to_string(),
            text: None,
        },
        "helm_template" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            let text = normalize_helm_template_text(text);
            SExpr::Leaf {
                kind: "helm_expr".to_string(),
                text: Some(text),
            }
        }
        other => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    kids.push(yaml_node_to_sexpr(ch, src));
                }
            }
            if kids.is_empty() {
                let text = node.utf8_text(src.as_bytes()).unwrap_or("").trim();
                SExpr::Leaf {
                    kind: other.to_string(),
                    text: if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    },
                }
            } else {
                SExpr::Node {
                    kind: other.to_string(),
                    children: kids,
                }
            }
        }
    }
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

fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "range_action" | "with_action" | "define_action" | "block_action"
    )
}

fn fuse_blocks<'a>(blocks: Vec<tree_sitter::Node<'a>>, src: &str) -> Vec<SExpr> {
    let mut out: Vec<SExpr> = Vec::new();
    let mut pending = String::new();

    let mut flush_pending = |pending: &mut String, out: &mut Vec<SExpr>| {
        if pending.trim().is_empty() {
            pending.clear();
            return;
        }
        let fragment = std::mem::take(pending);
        out.extend(parse_yaml_items(&fragment));
    };

    for b in blocks {
        if is_control_flow(b.kind()) {
            flush_pending(&mut pending, &mut out);
            out.push(fuse_control_flow(b, src));
        } else {
            let r = b.byte_range();
            pending.push_str(&src[r]);
        }
    }

    flush_pending(&mut pending, &mut out);
    out
}

fn fuse_control_flow(node: tree_sitter::Node<'_>, src: &str) -> SExpr {
    match node.kind() {
        "if_action" => {
            let cond_node = node.child_by_field_name("condition");
            let cond_text = cond_node
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let then_blocks = children_with_field(node, "consequence");
            let else_blocks = children_with_field(node, "alternative");

            let then_items = fuse_blocks(then_blocks, src);
            let base_else_items = fuse_blocks(else_blocks, src);

            // Tree-sitter's helm dialect inlines `else if` clauses into the `if_action` node
            // using repeated `condition:` and `option:` fields.
            //
            // We lower those into nested `(if ...)` nodes in the else branch to match the
            // pure-rust fused AST behavior.
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
                let mut tail_else_items = base_else_items;
                for (cnd, blocks) in else_if_pairs.into_iter().rev() {
                    let opt_items = fuse_blocks(blocks, src);
                    let nested = SExpr::Node {
                        kind: "if".to_string(),
                        children: vec![
                            SExpr::Leaf {
                                kind: "cond".to_string(),
                                text: Some(cnd),
                            },
                            if opt_items.is_empty() {
                                SExpr::Leaf {
                                    kind: "then".to_string(),
                                    text: None,
                                }
                            } else {
                                SExpr::Node {
                                    kind: "then".to_string(),
                                    children: opt_items,
                                }
                            },
                            if tail_else_items.is_empty() {
                                SExpr::Leaf {
                                    kind: "else".to_string(),
                                    text: None,
                                }
                            } else {
                                SExpr::Node {
                                    kind: "else".to_string(),
                                    children: tail_else_items,
                                }
                            },
                        ],
                    };

                    tail_else_items = vec![nested];
                }
                tail_else_items
            };

            SExpr::Node {
                kind: "if".to_string(),
                children: vec![
                    SExpr::Leaf {
                        kind: "cond".to_string(),
                        text: Some(cond_text),
                    },
                    if then_items.is_empty() {
                        SExpr::Leaf {
                            kind: "then".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "then".to_string(),
                            children: then_items,
                        }
                    },
                    if else_items.is_empty() {
                        SExpr::Leaf {
                            kind: "else".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "else".to_string(),
                            children: else_items,
                        }
                    },
                ],
            }
        }
        "range_action" => {
            let header = node
                .child_by_field_name("range")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let body_items = fuse_blocks(children_with_field(node, "body"), src);
            let else_items = fuse_blocks(children_with_field(node, "alternative"), src);

            SExpr::Node {
                kind: "range".to_string(),
                children: vec![
                    SExpr::Leaf {
                        kind: "header".to_string(),
                        text: Some(header),
                    },
                    if body_items.is_empty() {
                        SExpr::Leaf {
                            kind: "body".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "body".to_string(),
                            children: body_items,
                        }
                    },
                    if else_items.is_empty() {
                        SExpr::Leaf {
                            kind: "else".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "else".to_string(),
                            children: else_items,
                        }
                    },
                ],
            }
        }
        "with_action" => {
            let header = node
                .child_by_field_name("condition")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();

            let body_items = fuse_blocks(children_with_field(node, "consequence"), src);
            let else_items = fuse_blocks(children_with_field(node, "alternative"), src);

            SExpr::Node {
                kind: "with".to_string(),
                children: vec![
                    SExpr::Leaf {
                        kind: "header".to_string(),
                        text: Some(header),
                    },
                    if body_items.is_empty() {
                        SExpr::Leaf {
                            kind: "body".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "body".to_string(),
                            children: body_items,
                        }
                    },
                    if else_items.is_empty() {
                        SExpr::Leaf {
                            kind: "else".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "else".to_string(),
                            children: else_items,
                        }
                    },
                ],
            }
        }
        "define_action" => {
            let header = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();
            let body_items = fuse_blocks(children_with_field(node, "body"), src);

            SExpr::Node {
                kind: "define".to_string(),
                children: vec![
                    SExpr::Leaf {
                        kind: "header".to_string(),
                        text: Some(header),
                    },
                    if body_items.is_empty() {
                        SExpr::Leaf {
                            kind: "body".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "body".to_string(),
                            children: body_items,
                        }
                    },
                ],
            }
        }
        "block_action" => {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();
            let arg = node
                .child_by_field_name("argument")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();

            let header = if arg.is_empty() {
                name.to_string()
            } else {
                format!("{name} {arg}")
            };

            let body_items = fuse_blocks(children_with_field(node, "body"), src);

            SExpr::Node {
                kind: "block".to_string(),
                children: vec![
                    SExpr::Leaf {
                        kind: "header".to_string(),
                        text: Some(header),
                    },
                    if body_items.is_empty() {
                        SExpr::Leaf {
                            kind: "body".to_string(),
                            text: None,
                        }
                    } else {
                        SExpr::Node {
                            kind: "body".to_string(),
                            children: body_items,
                        }
                    },
                ],
            }
        }
        other => {
            let text = node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
            SExpr::Leaf {
                kind: other.to_string(),
                text: Some(text),
            }
        }
    }
}

fn parse_fused_template(src: &str) -> SExpr {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("set language");

    let tree = parser.parse(src, None).expect("parse");
    let root = tree.root_node();

    let mut blocks = Vec::new();
    let mut c = root.walk();
    for ch in root.children(&mut c) {
        blocks.push(ch);
    }

    SExpr::Node {
        kind: "doc".to_string(),
        children: fuse_blocks(blocks, src),
    }
}

#[test]
fn if_else_end_with_yaml_branches() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        foo: bar
        {{- else }}
        {}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".Values.enabled")
            (then
              (map
                (entry
                  (str :text "foo")
                  (str :text "bar")
                )
              )
            )
            (else
              (map)
            )
          )
        )
    "#};

    let have = parse_fused_template(src);
    let want = SExpr::from_str(want).expect("parse expected");
    similar_asserts::assert_eq!(have, want);
}

#[test]
fn if_else_end_with_yaml_branches() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        foo: bar
        {{- else }}
        {}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".Values.enabled")
            (then
              (map
                (entry
                  (str :text "foo")
                  (str :text "bar")
                )
              )
            )
            (else
              (map)
            )
          )
        )
    "#};

    let have = parse_fused_template(src);
    let want = SExpr::from_str(want).expect("parse expected");
    similar_asserts::assert_eq!(have, want);
}

#[test]
fn else_if_chain_is_nested_if_in_else_branch() {
    let src = indoc! {r#"
        {{- if .A }}
        foo: 1
        {{- else if .B }}
        foo: 2
        {{- else }}
        foo: 3
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".A")
            (then
              (map
                (entry
                  (str :text "foo")
                  (int :text "1")
                )
              )
            )
            (else
              (if
                (cond :text ".B")
                (then
                  (map
                    (entry
                      (str :text "foo")
                      (int :text "2")
                    )
                  )
                )
                (else
                  (map
                    (entry
                      (str :text "foo")
                      (int :text "3")
                    )
                  )
                )
              )
            )
          )
        )
    "#};

    let have = parse_fused_template(src);
    let want = SExpr::from_str(want).expect("parse expected");
    similar_asserts::assert_eq!(have, want);
}

#[test]
fn range_action_body_contains_yaml_and_inline_exprs() {
    let src = indoc! {r#"
        {{- range .Values.items }}
        - name: {{ .name }}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (range
            (header :text ".Values.items")
            (body
              (seq
                (map
                  (entry
                    (str :text "name")
                    (helm_expr :text ".name")
                  )
                )
              )
            )
            (else)
          )
        )
    "#};

    let have = parse_fused_template(src);
    let want = SExpr::from_str(want).expect("parse expected");
    similar_asserts::assert_eq!(have, want);
}

#[test]
fn inline_helm_expr_is_part_of_yaml_structure() {
    let src = indoc! {r#"
        name: {{ .Release.Name }}
    "#};

    let want = indoc! {r#"
        (doc
          (map
            (entry
              (str :text "name")
              (helm_expr :text ".Release.Name")
            )
          )
        )
    "#};

    let have = parse_fused_template(src);
    let want = SExpr::from_str(want).expect("parse expected");
    similar_asserts::assert_eq!(have, want);
}
