#![allow(warnings)]
use color_eyre::eyre::{self, OptionExt};
use helm_schema_mapper::ValueUse;
use helm_schema_mapper::sanitize::{pretty_yaml_error, validate_yaml_strict_all_docs};
use helm_schema_template::parse::parse_gotmpl_document;
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::*;
use vfs::VfsPath;

use helm_schema_mapper::analyze::{
    DefineIndex, ExpansionGuard, ExprOrigin, InlineOut, Occurrence, Scope, analyze_template_file,
    collect_defines, inline_emit_tree,
};
use helm_schema_mapper::analyze::{Role, group_uses};
use helm_schema_mapper::analyze::{compute_define_closure, index_defines_in_dir};

fn sanitized_yaml(src: &str) -> eyre::Result<String> {
    let root = VfsPath::new(vfs::MemoryFS::new());

    let src_path = write(&root.join("template.yaml")?, src)?;

    let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;

    // Start scope at chart root (treat dot/dollar as .Values by convention for analysis)
    let scope = Scope {
        dot: ExprOrigin::Selector(vec!["Values".into()]),
        dollar: ExprOrigin::Selector(vec!["Values".into()]),
        bindings: Default::default(),
    };

    let define_index = collect_defines(&[src_path])?;
    let inline = helm_schema_mapper::analyze::InlineState {
        define_index: &define_index,
    };
    let mut guard = ExpansionGuard::new();

    // Inline + sanitize to a single YAML buffer with placeholders
    let mut out = InlineOut::default();
    inline_emit_tree(&parsed.tree, &src, &scope, &inline, &mut guard, &mut out)?;

    println!(
        "=== SANITIZED YAML ===\n\n{}\n========================",
        out.buf
    );

    // Must be valid YAML (multiple docs are allowed; this one is a single doc)
    if let Err(e) = validate_yaml_strict_all_docs(&out.buf) {
        eyre::bail!(
            "Sanitized YAML should parse, but got:\n{}\n{}",
            out.buf,
            pretty_yaml_error(&out.buf, &e)
        );
    }
    Ok(out.buf)
}

#[test]
fn mapping_vs_top_level_via_nindent() -> eyre::Result<()> {
    let src = indoc! {r#"
        {{- define "helper" -}}
        foo: {{ .Values.foo }}
        {{- end -}}
        {{- define "top" -}}
        bar: {{ .Values.bar }}
        {{- end -}}

        parent:
        {{ include "helper" . | nindent 2 }}
        {{ include "top" . | nindent 0 }}
    "#};

    let sanitized = sanitized_yaml(src)?;

    // After "parent:" the *next non-empty sanitized line* must be a placeholder mapping entry (ends with `": 0"`).
    let mut lines = sanitized.lines().map(|s| s.trim_end()).collect::<Vec<_>>();
    let mut i_parent = None;
    for (i, l) in lines.iter().enumerate() {
        if *l == "parent:" {
            i_parent = Some(i);
            break;
        }
    }
    let i_parent = i_parent.expect("missing 'parent:' line in sanitized");
    let next_non_empty = lines[(i_parent + 1)..]
        .iter()
        .find(|l| !l.trim().is_empty())
        .unwrap();
    assert!(
        next_non_empty.contains("__TSG_PLACEHOLDER_") && next_non_empty.ends_with("\": 0"),
        "expected a mapping placeholder right under 'parent:', got: {:?}",
        next_non_empty
    );

    // Later we should also see a *top-level mapping* placeholder line (no leading spaces).
    let has_top_level_mapping_placeholder = lines.iter().any(|l| {
        // top-level: no indent
        !l.starts_with(' ') && l.starts_with("\"__TSG_PLACEHOLDER_") && l.ends_with("\": 0")
    });
    assert!(
        has_top_level_mapping_placeholder,
        "expected a top-level mapping placeholder somewhere; got:\n{sanitized}"
    );

    Ok(())
}

#[test]
fn two_fragments_under_same_parent_keep_mapping_context() -> eyre::Result<()> {
    let src = indoc! {r#"
        {{- define "a" -}}
        foo: {{ .Values.foo }}
        {{- end -}}
        {{- define "b" -}}
        bar: {{ .Values.bar }}
        {{- end -}}

        parent:
        {{ include "a" . | nindent 2 }}
        {{ include "b" . | nindent 2 }}
    "#};

    let sanitized = sanitized_yaml(src)?;

    let lines = sanitized.lines().map(|s| s.trim_end()).collect::<Vec<_>>();
    let i_parent = lines
        .iter()
        .position(|l| *l == "parent:")
        .expect("missing 'parent:'");

    let under = lines[(i_parent + 1)..]
        .iter()
        .filter(|l| !l.trim().is_empty());

    let mut count = 0;
    for l in under {
        let ls = l.trim_start(); // <<< allow indentation
        if ls.starts_with('"') && ls.contains("__TSG_PLACEHOLDER_") && ls.ends_with("\": 0") {
            count += 1;
            if count == 2 {
                break;
            }
        } else {
            // stop once mapping section seems over
            break;
        }
    }
    assert_eq!(
        count, 2,
        "expected two mapping placeholders under 'parent:', got only {count}\n{sanitized}"
    );

    Ok(())
}
