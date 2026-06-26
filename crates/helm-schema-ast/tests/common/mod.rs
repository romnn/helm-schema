use std::fmt::Write;

use helm_schema_ast::{parse_go_template, parse_helm_template};
use test_util::prelude::sim_assert_eq;

pub mod cases;

#[derive(Clone, Copy)]
pub struct AstCorpusCase<'a> {
    pub template_path: &'a str,
    pub expected_fixture: &'a str,
}

pub fn assert_ast_fixture(case: AstCorpusCase<'_>) {
    let src = test_util::read_testdata(case.template_path);
    let tree = template_tree_for_fixture(&src, case.template_path);
    let have = tree_sitter_sexpr(tree.root_node(), &src);
    sim_assert_eq!(have: have, want: case.expected_fixture.trim_end());
}

pub fn template_tree_for_fixture(src: &str, label: &str) -> tree_sitter::Tree {
    let tree = parse_helm_template(src).expect("parse template");
    assert!(
        tree.root_node().child_count() > 0,
        "expected non-empty parse tree for {}",
        label
    );

    if tree.root_node().has_error() {
        let go_template_tree = parse_go_template(src).expect("parse go template");
        assert!(
            !go_template_tree.root_node().has_error(),
            "fused Helm/YAML parse recovered with errors, and go-template parse also has errors for {}\nhelm_sexpr={}\ngo_template_sexpr={}",
            label,
            tree.root_node().to_sexp(),
            go_template_tree.root_node().to_sexp(),
        );
        return go_template_tree;
    }

    tree
}

pub fn tree_sitter_sexpr(root: tree_sitter::Node<'_>, src: &str) -> String {
    let mut out = String::new();
    write_tree_sitter_node(root, src, None, 0, &mut out);
    out
}

fn write_tree_sitter_node(
    node: tree_sitter::Node<'_>,
    src: &str,
    field: Option<&str>,
    indent: usize,
    out: &mut String,
) {
    let pad = " ".repeat(indent);
    if let Some(field) = field {
        let _ = write!(out, "{pad}{field}: ");
    } else {
        out.push_str(&pad);
    }

    let children = named_children_for_sexpr(node, src);
    if children.is_empty() {
        let _ = write!(out, "({}", node.kind());
        if let Ok(text) = node.utf8_text(src.as_bytes()) {
            let text = text.trim();
            if !text.is_empty() {
                let quoted = serde_json::to_string(text).expect("string quoting cannot fail");
                let _ = write!(out, " :text {quoted}");
            }
        }
        out.push(')');
        return;
    }

    let _ = write!(out, "({}", node.kind());
    for (field, child) in children {
        out.push('\n');
        write_tree_sitter_node(child, src, field, indent + 2, out);
    }
    let _ = write!(out, "\n{pad})");
}

fn named_children_for_sexpr<'a>(
    node: tree_sitter::Node<'a>,
    src: &str,
) -> Vec<(Option<&'a str>, tree_sitter::Node<'a>)> {
    let mut children = Vec::new();
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return children;
    }

    loop {
        let child = cursor.node();
        if child.is_named() && should_include_sexpr_node(child, src) {
            children.push((cursor.field_name(), child));
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    children
}

fn should_include_sexpr_node(node: tree_sitter::Node<'_>, src: &str) -> bool {
    if node.kind() != "text" {
        return true;
    }
    node.utf8_text(src.as_bytes())
        .is_ok_and(|text| !text.trim().is_empty())
}
