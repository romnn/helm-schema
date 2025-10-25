use color_eyre::eyre::{self, OptionExt};
use helm_schema_template::{parse::parse_gotmpl_document, values::extract_values_paths};
use indoc::indoc;
use test_util::prelude::*;

fn collect(expr: &str) -> eyre::Result<std::collections::BTreeSet<String>> {
    let full = format!("{{{{ {expr} }}}}");
    let parsed = parse_gotmpl_document(&full).ok_or_eyre("failed to parse")?;
    let values = extract_values_paths(&parsed.tree, &full)
        .into_iter()
        .map(|p| p.0)
        .collect();
    Ok(values)
}

#[test]
fn selector_chain_basic() -> eyre::Result<()> {
    Builder::default().build();
    let got = collect(".Values.foo.bar.baz")?;
    assert_that!(got, contains(eq("foo.bar.baz")));
    Ok(())
}

#[test]
fn index_with_strings() -> eyre::Result<()> {
    Builder::default().build();
    let got = collect(r#"index .Values "ingress" "pathType""#)?;
    assert_that!(got, contains(eq("ingress.pathType")));
    Ok(())
}

#[test]
fn nested_in_wrappers() -> eyre::Result<()> {
    Builder::default().build();
    let got = collect(r#"tpl .Values.a.b . | default (index .Values "x" "y")"#)?;
    assert_that!(got, contains(eq("a.b")));
    assert_that!(got, contains(eq("x.y")));
    Ok(())
}

#[test]
fn ignore_non_values() -> eyre::Result<()> {
    Builder::default().build();
    let got = collect(".Chart.Name")?;
    assert_that!(got, is_empty());
    Ok(())
}

#[test]
fn hyphen_braces_supported() -> eyre::Result<()> {
    Builder::default().build();
    let full = indoc! {r#"
        {{- if .Values.a }}
        {{- index .Values "b" "c" -}}
        {{- end -}}
    "#};
    let parsed = parse_gotmpl_document(full).ok_or_eyre("failed to parse")?;
    let all = extract_values_paths(&parsed.tree, full);
    let have: Vec<_> = all.into_iter().map(|p| p.0).collect();
    assert!(have.contains(&"a".to_string()));
    assert!(have.contains(&"b.c".to_string()));
    Ok(())
}

#[test]
fn index_with_backticks() -> eyre::Result<()> {
    Builder::default().build();
    // {{ index .Values `ingress` `hostname` }} => ingress.hostname
    let src = "{{ index .Values `ingress` `hostname` }}";
    let p = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
    let got = extract_values_paths(&p.tree, &p.source);
    let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
    assert_eq!(have, vec!["ingress.hostname"]);
    Ok(())
}

#[test]
fn selector_chain_no_head_no_leaf() -> eyre::Result<()> {
    Builder::default().build();
    // {{ .Values.ingress.enabled }} => only "ingress.enabled"
    let p = parse_gotmpl_document("{{ .Values.ingress.enabled }}").ok_or_eyre("failed to parse")?;
    let got = extract_values_paths(&p.tree, &p.source);
    let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
    assert_eq!(have, vec!["ingress.enabled"]);
    Ok(())
}

#[test]
fn index_with_selector_head() -> eyre::Result<()> {
    Builder::default().build();
    // {{ index .Values "a" "b" "c" }} => a.b.c
    let p = parse_gotmpl_document(r#"{{ index .Values "a" "b" "c" }}"#)
        .ok_or_eyre("failed to parse")?;
    let got = extract_values_paths(&p.tree, &p.source);
    let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
    assert_eq!(have, vec!["a.b.c"]);
    Ok(())
}
