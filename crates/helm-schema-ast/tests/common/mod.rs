use std::fmt::Write;

use helm_schema_ast::{HelmAst, HelmParser, TreeSitterParser};
use test_util::prelude::sim_assert_eq;

pub mod cases;

#[derive(Clone, Copy)]
pub struct AstCorpusCase<'a> {
    pub template_path: &'a str,
    pub expected_fixture: &'a str,
}

pub fn assert_ast_fixture(case: AstCorpusCase<'_>) {
    let src = test_util::read_testdata(case.template_path);
    let ast = TreeSitterParser.parse(&src).expect("parse");
    sim_assert_eq!(have: ast_to_sexpr(&ast), want: case.expected_fixture.trim_end());
}

fn ast_to_sexpr(ast: &HelmAst) -> String {
    let mut out = String::new();
    write_ast_sexpr(ast, &mut out, 0);
    out
}

fn write_ast_sexpr(ast: &HelmAst, out: &mut String, indent: usize) {
    let pad = "  ".repeat(indent);
    match ast {
        HelmAst::Document { items } => write_list_sexpr("Document", None, items, out, indent),
        HelmAst::Mapping { items } => write_list_sexpr("Mapping", None, items, out, indent),
        HelmAst::Sequence { items } => write_list_sexpr("Sequence", None, items, out, indent),
        HelmAst::Pair { key, value } => {
            let _ = write!(out, "{pad}(Pair\n");
            write_ast_sexpr(key, out, indent + 1);
            if let Some(value) = value {
                out.push('\n');
                write_ast_sexpr(value, out, indent + 1);
            }
            out.push(')');
        }
        HelmAst::Scalar { text } => {
            let _ = write!(out, "{pad}(Scalar {text:?})");
        }
        HelmAst::HelmExpr { action } => {
            let _ = write!(out, "{pad}(HelmExpr {:?})", action.raw());
        }
        HelmAst::HelmComment { text } => {
            let _ = write!(out, "{pad}(HelmComment {text:?})");
        }
        HelmAst::If {
            condition,
            then_branch,
            else_branch,
        } => write_branch_sexpr(
            "If",
            condition.raw(),
            "then",
            then_branch,
            else_branch,
            out,
            indent,
        ),
        HelmAst::Range {
            header,
            body,
            else_branch,
        } => write_branch_sexpr(
            "Range",
            header.raw(),
            "body",
            body,
            else_branch,
            out,
            indent,
        ),
        HelmAst::With {
            header,
            body,
            else_branch,
        } => write_branch_sexpr("With", header.raw(), "body", body, else_branch, out, indent),
        HelmAst::Define { name, body } => {
            write_list_sexpr("Define", Some(name), body, out, indent);
        }
        HelmAst::Block { name, body } => {
            write_list_sexpr("Block", Some(name), body, out, indent);
        }
    }
}

fn write_list_sexpr(
    kind: &str,
    name: Option<&str>,
    items: &[HelmAst],
    out: &mut String,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    if let Some(name) = name {
        let _ = write!(out, "{pad}({kind} {name:?}");
    } else {
        let _ = write!(out, "{pad}({kind}");
    }
    for item in items {
        out.push('\n');
        write_ast_sexpr(item, out, indent + 1);
    }
    out.push(')');
}

fn write_branch_sexpr(
    kind: &str,
    header: &str,
    body_label: &str,
    body: &[HelmAst],
    else_branch: &[HelmAst],
    out: &mut String,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    let _ = write!(out, "{pad}({kind} {header:?}");
    if !body.is_empty() {
        let _ = write!(out, "\n{pad}  ({body_label}");
        for item in body {
            out.push('\n');
            write_ast_sexpr(item, out, indent + 2);
        }
        out.push(')');
    }
    if !else_branch.is_empty() {
        let _ = write!(out, "\n{pad}  (else");
        for item in else_branch {
            out.push('\n');
            write_ast_sexpr(item, out, indent + 2);
        }
        out.push(')');
    }
    out.push(')');
}
