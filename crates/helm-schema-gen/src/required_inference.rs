//! Heuristic required-inference for generated values schemas.
//!
//! Lives in its own module so the entire feature can be removed
//! cleanly. The output is a schema mutation that adds `required: [...]`
//! arrays at the parent objects of paths the chart references
//! unconditionally and never accesses via a `default` fallback.
//!
//! Why this is heuristic:
//!   - "unconditionally referenced" relies on header-use detection
//!     (Scalar use at empty `YamlPath` with empty `guards`) which can
//!     misfire on empty-body `if not`/`if or` blocks.
//!   - "never accessed via default" relies on the broader
//!     [`helm_schema_ir::required_inference::extract_default_fallback_paths`]
//!     regex which is text-based and can miss exotic syntax.
//!
//! The schemadiff tool already strips `required` arrays from both
//! sides before diffing — the only place this feature's output is
//! user-visible is the CLI's `--infer-required` flag. If the heuristic
//! ever proves more trouble than it's worth, deleting this file plus
//! the matching modules in `helm-schema-ir` and `helm-schema-cli` is
//! the entire rip surface.

use std::collections::BTreeSet;

use helm_schema_ir::{Guard, ValueKind, ValueUse};
use serde_json::{Map, Value};

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally and
/// never accesses via a `default` fallback.
///
/// `synthetic_value_paths` should contain any paths injected by the
/// caller that look syntactically identical to header references but
/// aren't real template references (e.g. CLI-seeded top-level
/// `values.yaml` keys).
///
/// `default_fallback_paths` should contain every path that has any
/// `default <expr> .Values.X` fallback in the template — typically
/// derived from [`helm_schema_ir::required_inference::extract_default_fallback_paths`]
/// applied to chart templates with appropriate prefix scoping.
pub fn apply_required_inference(
    schema: &mut Value,
    uses: &[ValueUse],
    synthetic_value_paths: &BTreeSet<String>,
    default_fallback_paths: &BTreeSet<String>,
) {
    let paths = collect_required_paths(uses, default_fallback_paths, synthetic_value_paths);
    for path in paths {
        add_path_to_required(schema, &path);
    }
}

/// Identify paths checked unconditionally at the top of a template — `if
/// .Values.X` / `eq .Values.X "..."` with no enclosing guards — and never
/// accessed via a `default` expression.
///
/// The signal comes from header uses emitted by `collect_if_with_guards`:
/// such a use is a Scalar at empty `YamlPath` with empty `guards` (the
/// matching guard is pushed *after* the header emit), uniquely
/// identifying a top-level guard header. To distinguish positive
/// (`if`/`eq`) from negative (`not`/`or`) headers — which look identical
/// at the emit site — paths that appear inside any `Guard::Not` or
/// `Guard::Or` anywhere in the IR are excluded. `with`-headers carry a
/// `Guard::With` and are skipped: `with nil` is a valid runtime input.
/// `range`-headers emit with a non-empty YamlPath and are skipped for
/// the same reason.
///
/// Known precision loss: an empty-body `{{ if not .Values.X }}{{ end }}`
/// generates no body uses carrying a `Not` guard, so the exclusion pass
/// can't see it and X is still (incorrectly) marked required. In
/// practice `if not` blocks always contain content; the failure mode is
/// rare. A proper fix would require tagging header emits with their
/// guard kind in the IR.
fn collect_required_paths(
    uses: &[ValueUse],
    default_fallback_paths: &BTreeSet<String>,
    synthetic_value_paths: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut conditionally_excluded: BTreeSet<&str> = BTreeSet::new();
    for u in uses {
        for g in &u.guards {
            match g {
                Guard::Not { path } => {
                    conditionally_excluded.insert(path.as_str());
                }
                Guard::Or { paths } => {
                    for p in paths {
                        conditionally_excluded.insert(p.as_str());
                    }
                }
                _ => {}
            }
        }
    }

    let mut required: BTreeSet<String> = BTreeSet::new();
    for u in uses {
        if u.kind != ValueKind::Scalar
            || !u.path.0.is_empty()
            || !u.guards.is_empty()
            || u.source_expr.trim().is_empty()
        {
            continue;
        }
        if default_fallback_paths.contains(&u.source_expr)
            || conditionally_excluded.contains(u.source_expr.as_str())
            || synthetic_value_paths.contains(&u.source_expr)
        {
            continue;
        }
        required.insert(u.source_expr.clone());
    }
    required
}

/// Locate `path`'s parent object schema and add the leaf segment to its
/// `required` list (sorted, de-duplicated). Silently no-ops if the
/// schema doesn't have a property tree at that path — the schema's
/// inferred shape may not include every path that drives required-
/// inference (e.g. when the path is referenced only via a guard).
fn add_path_to_required(schema: &mut Value, vp: &str) {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    let Some((leaf, parents)) = parts.split_last() else {
        return;
    };
    let Some(parent) = navigate_to_object_property(schema, parents) else {
        return;
    };
    add_to_required_list(parent, leaf);
}

/// Walk `segments` through `.properties.<seg>` accessors. Returns
/// `None` if any intermediate level is missing or isn't an object.
fn navigate_to_object_property<'a>(
    schema: &'a mut Value,
    segments: &[&str],
) -> Option<&'a mut Value> {
    let mut node = schema;
    for seg in segments {
        node = node
            .as_object_mut()?
            .get_mut("properties")?
            .as_object_mut()?
            .get_mut(*seg)?;
    }
    Some(node)
}

/// Add `key` to `node`'s `required` array (creating it if missing).
/// Keeps the array sorted and de-duplicated.
fn add_to_required_list(node: &mut Value, key: &str) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    let req = obj
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(arr) = req.as_array_mut() else {
        // Pre-existing non-array `required` — leave it alone rather
        // than overwrite a hand-authored shape we don't understand.
        return;
    };
    if !arr.iter().any(|v| v.as_str() == Some(key)) {
        arr.push(Value::String(key.to_string()));
    }
    arr.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
    arr.dedup();
    let _: &Map<String, Value> = obj; // keep ‘unused’ lint happy if Map gets refactored
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use indoc::indoc;
    use serde_json::Value;

    use super::apply_required_inference;
    use crate::{generate_values_schema_full, generate_values_schema_with_values_yaml};
    use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
    use helm_schema_ir::required_inference::extract_default_fallback_paths;
    use helm_schema_ir::{IrGenerator, SymbolicIrGenerator, ValueUse, extract_default_type_hints};
    use helm_schema_k8s::KubernetesJsonSchemaProvider;

    fn provider() -> KubernetesJsonSchemaProvider {
        KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true)
    }

    fn parse_ir(src: &str) -> Vec<ValueUse> {
        let ast = TreeSitterParser.parse(src).expect("parse");
        let idx = DefineIndex::new();
        SymbolicIrGenerator.generate(src, &ast, &idx)
    }

    fn collect_hints(src: &str) -> BTreeMap<String, Vec<Value>> {
        let mut hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (path, schema) in extract_default_type_hints(src) {
            hints.entry(path).or_default().push(schema);
        }
        hints
    }

    fn collect_fallbacks(src: &str) -> BTreeSet<String> {
        extract_default_fallback_paths(src).into_iter().collect()
    }

    fn generate_with_required(src: &str, values_yaml: Option<&str>) -> Value {
        let uses = parse_ir(src);
        let hints = collect_hints(src);
        let mut schema = generate_values_schema_full(&uses, &provider(), values_yaml, &hints);
        apply_required_inference(
            &mut schema,
            &uses,
            &BTreeSet::new(),
            &collect_fallbacks(src),
        );
        schema
    }

    /// Step 3: with `--infer-required`, an unconditional `if .Values.X` makes X
    /// `required` on its parent object.
    #[test]
    fn step3_infer_required_if_block_marks_required() {
        let src = indoc! {r"
            {{- if .Values.serviceAccount.create }}
            apiVersion: v1
            kind: ServiceAccount
            metadata:
              name: foo
            {{- end }}
        "};
        let schema = generate_with_required(src, None);

        let sa = schema
            .pointer("/properties/serviceAccount")
            .expect("serviceAccount present");
        let required = sa
            .get("required")
            .and_then(Value::as_array)
            .expect("serviceAccount must declare a required list");
        let names: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
        assert_eq!(names, vec!["create"]);
        assert!(
            schema.get("required").is_none(),
            "root schema should not declare serviceAccount required"
        );
    }

    /// Step 3: paths reachable via `default <literal> .Values.X` are NOT marked
    /// required, since the chart explicitly handles X being unset.
    #[test]
    fn step3_default_literal_excludes_path_from_required() {
        let src = indoc! {r#"
            {{- if .Values.feature }}
            foo: {{ default "x" .Values.feature }}
            {{- end }}
        "#};
        let schema = generate_with_required(src, None);

        assert!(
            schema.get("required").is_none(),
            "feature has a literal default fallback, should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Step 3 regression: non-literal default fallbacks
    /// (`default .Chart.Name .Values.X`) ALSO suppress required-inference.
    #[test]
    fn step3_default_non_literal_excludes_path_from_required() {
        let src = indoc! {r"
            {{- if .Values.nameOverride }}
            name: {{ default .Chart.Name .Values.nameOverride }}
            {{- end }}
        "};
        let fallbacks = collect_fallbacks(src);
        assert!(
            fallbacks.contains("nameOverride"),
            "fallback extractor must catch non-literal default, got {fallbacks:?}"
        );
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "nameOverride has a non-literal default fallback, should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Step 3 regression: a quoted-string-with-spaces fallback
    /// (`default "two words" .Values.X`) is recognised by the fallback
    /// extractor.
    #[test]
    fn step3_default_quoted_string_with_spaces_excludes_path_from_required() {
        let src = indoc! {r#"
            {{- if .Values.nameOverride }}
            name: {{ default "two words" .Values.nameOverride }}
            {{- end }}
        "#};
        let fallbacks = collect_fallbacks(src);
        assert!(
            fallbacks.contains("nameOverride"),
            "fallback extractor must catch quoted-string-with-spaces literal, got {fallbacks:?}"
        );
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "nameOverride has a `default \"two words\"` fallback, should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Step 3 regression: parenthesized default fallbacks
    /// (`default (printf "%s-foo" .Release.Name) .Values.X`) — common in
    /// fullname-style helpers — also suppress required-inference.
    #[test]
    fn step3_default_parenthesized_excludes_path_from_required() {
        let src = indoc! {r#"
            {{- if .Values.fullnameOverride }}
            name: {{ default (printf "%s-%s" .Release.Name "x") .Values.fullnameOverride }}
            {{- end }}
        "#};
        let fallbacks = collect_fallbacks(src);
        assert!(
            fallbacks.contains("fullnameOverride"),
            "fallback extractor must catch parenthesized default, got {fallbacks:?}"
        );
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "fullnameOverride has a parenthesized default fallback, should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Step 3 bug-fix: `if not .Values.X` must NOT mark X as required —
    /// the condition fires when X is empty/null, so X being unset is
    /// contractual.
    #[test]
    fn step3_not_guard_does_not_mark_required() {
        let src = indoc! {r"
            {{- if not .Values.legacyMode }}
            name: {{ .Values.name }}
            {{- end }}
        "};
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "legacyMode is checked with `not`; should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Step 3 bug-fix: `if or .Values.A .Values.B` must NOT mark A or B
    /// as required — only one of them needs to be truthy.
    #[test]
    fn step3_or_guard_does_not_mark_required() {
        let src = indoc! {r"
            {{- if or .Values.primary .Values.fallback }}
            name: {{ .Values.name }}
            {{- end }}
        "};
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "primary and fallback are an `or` pair; neither should be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Sanity: applying required-inference to a schema produced WITHOUT
    /// any required calls yields the same shape (modulo added `required`
    /// arrays). Verifies the core gen path stays clean of required logic.
    #[test]
    fn core_schema_generation_yields_no_required() {
        let src = indoc! {r"
            {{- if .Values.serviceAccount.create }}
            apiVersion: v1
            kind: ServiceAccount
            {{- end }}
        "};
        let schema = generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), None);
        // The core path must never emit `required` — that's the
        // separation of concerns this module exists to enforce.
        let any_required_anywhere = serde_json::to_string(&schema)
            .unwrap()
            .contains("\"required\"");
        assert!(
            !any_required_anywhere,
            "core schema generation must not emit `required` arrays, got: {}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }
}
