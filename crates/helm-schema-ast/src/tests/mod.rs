use std::fmt::Write;

use crate::{
    DefineIndex, HelmAst, HelmParser, TemplateExpr, TemplateHeader, TreeSitterParser,
    contains_template_action,
};
use test_util::prelude::sim_assert_eq;

// ===========================================================================
// Simple template
// ===========================================================================

const SIMPLE_EXPECTED_SEXPR: &str = "\
(Document
  (If \".Values.enabled\"
    (then
      (Mapping
        (Pair
          (Scalar \"foo\")
          (Scalar \"bar\"))))))";

#[test]
fn tree_sitter_ast_simple() {
    let src = "{{- if .Values.enabled }}\nfoo: bar\n{{- end }}\n";
    let ast = TreeSitterParser.parse(src).expect("parse");
    sim_assert_eq!(have: ast_to_sexpr(&ast), want: SIMPLE_EXPECTED_SEXPR);
}

#[test]
fn tree_sitter_ast_control_flow_headers_are_typed() {
    let src =
        "{{- if .Values.enabled }}{{- range $i, $v := include \"items\" . }}x{{- end }}{{- end }}";
    let ast = TreeSitterParser.parse(src).expect("parse");
    let HelmAst::Document { items } = ast else {
        panic!("expected document root");
    };
    let [
        HelmAst::If {
            condition,
            then_branch,
            ..
        },
    ] = items.as_slice()
    else {
        panic!("expected one top-level if node");
    };
    sim_assert_eq!(have: condition.raw(), want: ".Values.enabled");
    sim_assert_eq!(
        have: condition.expr(),
        want: &TemplateExpr::Field(vec!["Values".to_string(), "enabled".to_string()])
    );

    let [HelmAst::Range { header, .. }] = then_branch.as_slice() else {
        panic!("expected nested range node");
    };
    sim_assert_eq!(have: header.raw(), want: "$i, $v := include \"items\" .");
    let mut saw_include = false;
    header.expr().walk(|expr| {
        if let TemplateExpr::Call { function, args } = expr
            && function == "include"
            && matches!(args.first(), Some(TemplateExpr::Literal(lit)) if lit.as_string() == Some("items"))
        {
            saw_include = true;
        }
    });
    assert!(saw_include, "expected typed range header include call");
}

#[test]
fn template_header_parse_control_normalizes_control_keyword_prefix() {
    let expected = TemplateExpr::Field(vec![
        "Values".to_string(),
        "signoz".to_string(),
        "serviceAccount".to_string(),
        "create".to_string(),
    ]);

    for raw in [
        "if .Values.signoz.serviceAccount.create",
        "{{- if .Values.signoz.serviceAccount.create -}}",
    ] {
        let header = TemplateHeader::parse_control(raw);
        sim_assert_eq!(have: header.expr(), want: &expected, "raw={raw}");
    }
}

#[test]
fn template_action_detection_finds_inline_output_action() {
    let src = "metadata:\n  name: {{ .Values.name }}\n";

    assert!(contains_template_action(src).expect("parse template source"));
}

#[test]
fn tree_sitter_ast_helm_exprs_are_typed() {
    let src = "{{ .Values.name | quote }}";
    let ast = TreeSitterParser.parse(src).expect("parse");
    let HelmAst::Document { items } = ast else {
        panic!("expected document root");
    };
    let [HelmAst::HelmExpr { action }] = items.as_slice() else {
        panic!("expected one top-level helm expr");
    };
    sim_assert_eq!(have: action.raw(), want: ".Values.name | quote");
    let [TemplateExpr::Pipeline(stages)] = action.exprs() else {
        panic!("expected one parsed pipeline expression");
    };
    assert!(matches!(
        stages.as_slice(),
        [
            TemplateExpr::Field(path),
            TemplateExpr::Call { function, args }
        ] if path == &vec!["Values".to_string(), "name".to_string()]
            && function == "quote"
            && args.is_empty()
    ));
}

#[test]
fn template_action_detection_accepts_literal_yaml_comments() {
    let src = "# comment\nmetadata:\n  name: demo\n";

    assert!(!contains_template_action(src).expect("parse template source"));
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

// ===========================================================================
// DefineIndex
// ===========================================================================

#[test]
fn define_index_from_helpers() {
    let helpers = test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl");

    let mut idx_ts = DefineIndex::new();
    idx_ts
        .add_source(&TreeSitterParser, &helpers)
        .expect("ts define index");

    let expected_defines = ["redis.image", "redis.sentinel.image", "redis.metrics.image"];
    for name in expected_defines {
        assert!(
            idx_ts.get(name).is_some(),
            "ts define index should find '{name}'"
        );
    }
}
