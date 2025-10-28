#![allow(warnings)]
use color_eyre::eyre::{self, OptionExt};
use helm_schema_mapper::ValueUse;
use helm_schema_mapper::sanitize::{pretty_yaml_error, validate_yaml_strict_all_docs};
use helm_schema_template::parse::parse_gotmpl_document;
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::*;
use vfs::VfsPath;

use helm_schema_mapper::analyze::{Occurrence, analyze_template_file};
use helm_schema_mapper::analyze::{Role, group_uses};
use helm_schema_mapper::analyze::{compute_define_closure, index_defines_in_dir};

#[cfg(false)]
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

    let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
    let mut ph = Vec::new();
    // No need to collect values for this test
    let sanitized =
        build_sanitized_with_placeholders(src, &parsed.tree, &mut ph, |_node| Vec::new());

    // Must be valid YAML (multiple docs are allowed; this one is a single doc)
    if let Err(e) = validate_yaml_strict_all_docs(&sanitized) {
        eyre::bail!(
            "Sanitized YAML should parse, but got:\n{}\n{}",
            sanitized,
            pretty_yaml_error(&sanitized, &e)
        );
    }

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

    // Later we should also see a *standalone* placeholder line for the top-level include (no trailing ': 0' on the same line).
    let has_top_level_placeholder = lines.iter().any(|l| {
        l.starts_with("\"__TSG_PLACEHOLDER_") && l.ends_with("\"") && !l.ends_with("\": 0")
    });
    assert!(
        has_top_level_placeholder,
        "expected a top-level scalar placeholder somewhere; got:\n{sanitized}"
    );
    Ok(())
}

#[cfg(false)]
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

    let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
    let mut ph = Vec::new();
    let sanitized =
        build_sanitized_with_placeholders(src, &parsed.tree, &mut ph, |_node| Vec::new());

    if let Err(e) = validate_yaml_strict_all_docs(&sanitized) {
        eyre::bail!(
            "Should parse, but got:\n{}\n{}",
            sanitized,
            pretty_yaml_error(&sanitized, &e)
        );
    }

    // Find 'parent:'; ensure the *two* next non-empty entries under it are both mapping placeholders.
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
        if l.starts_with('"') && l.contains("__TSG_PLACEHOLDER_") && l.ends_with("\": 0") {
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
