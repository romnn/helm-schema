//! Heuristic required-inference for generated values schemas.
//!
//! Lives in its own module so the entire feature can be removed
//! cleanly. The output is a schema mutation that adds `required: [...]`
//! arrays at the parent objects of paths the chart references
//! unconditionally and never accesses via a `default` fallback.
//!
//! Why this is heuristic:
//!   - Helm truthiness in a positive header is not by itself proof that a
//!     value is user-required.
//!
//! The schemadiff tool already strips `required` arrays from both
//! sides before diffing — the only place this feature's output is
//! user-visible is the CLI's `--infer-required` flag. If the heuristic
//! ever proves more trouble than it's worth, deleting this file plus
//! the matching CLI module is the entire rip surface.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ir::ContractPathSchemaEvidence;
use serde_json::Value;

#[derive(Debug, Default, Clone, Copy)]
struct RequiredInferencePolicy;

struct RequiredInferenceInputs<'a> {
    schema_evidence_by_value_path: &'a BTreeMap<String, ContractPathSchemaEvidence>,
    explicit_default_value_paths: &'a BTreeSet<String>,
}

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally and
/// never accesses via a `default` fallback.
///
/// `explicit_default_value_paths` should contain any values paths explicitly
/// present in the composed chart defaults. Those paths are already satisfied
/// by the chart and must not be inferred as user-required, even if they also
/// appear in positive guard headers.
///
pub fn apply_required_inference(
    schema: &mut Value,
    schema_evidence_by_value_path: &BTreeMap<String, ContractPathSchemaEvidence>,
    explicit_default_value_paths: &BTreeSet<String>,
) {
    let paths = RequiredInferencePolicy.required_paths(RequiredInferenceInputs {
        schema_evidence_by_value_path,
        explicit_default_value_paths,
    });
    for path in paths {
        add_path_to_required(schema, &path);
    }
}

/// Identify paths checked in positive header positions, lacking explicit chart
/// defaults, and also consumed by at least one non-self-guarded render use.
///
/// This remains heuristic because Helm truthiness does not by itself imply
/// user-requiredness. The extra render-use eligibility check filters out
/// common feature-toggle and helper-override patterns like:
/// `if .Values.fullnameOverride }}{{ .Values.fullnameOverride }}{{ else }}...`.
impl RequiredInferencePolicy {
    fn required_paths(self, input: RequiredInferenceInputs<'_>) -> BTreeSet<String> {
        let mut required: BTreeSet<String> = BTreeSet::new();
        for (path, evidence) in input.schema_evidence_by_value_path {
            if !evidence.is_required_inference_candidate()
                || input.explicit_default_value_paths.contains(path)
            {
                continue;
            }
            required.insert(path.clone());
        }
        required
    }
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use test_util::prelude::sim_assert_eq;

    use indoc::indoc;
    use serde_json::Value;

    use super::apply_required_inference;
    use crate::{ValuesSchemaInput, generate_values_schema};
    use helm_schema_ast::DefineIndex;
    use helm_schema_ir::{
        ContractIr, ContractUse, Guard, GuardValue, SymbolicIrContext, ValueKind, YamlPath,
    };
    use helm_schema_k8s::KubernetesJsonSchemaProvider;

    fn provider() -> KubernetesJsonSchemaProvider {
        KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true)
    }

    fn parse_contract(src: &str) -> ContractIr {
        let idx = DefineIndex::new();
        SymbolicIrContext::new(&idx).generate_contract_ir(src, &idx)
    }

    fn contract_for(uses: Vec<ContractUse>) -> ContractIr {
        ContractIr::from_contract_uses(uses)
    }

    fn generate_with_required(src: &str, values_yaml: Option<&str>) -> Value {
        let contract = parse_contract(src);
        let schema_signals = contract.into_schema_signals();
        let mut schema = generate_values_schema(
            ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(values_yaml),
        );
        apply_required_inference(
            &mut schema,
            schema_signals.schema_evidence_by_value_path(),
            &BTreeSet::new(),
        );
        schema
    }

    #[test]
    fn contract_default_guard_excludes_path_without_external_fallback_scan() {
        let contract = contract_for(vec![
            ContractUse {
                source_expr: "feature".to_string(),
                path: YamlPath(Vec::new()),
                kind: ValueKind::Scalar,
                guards: Vec::new(),
                resource: None,
                provenance: Vec::new(),
            },
            ContractUse {
                source_expr: "feature".to_string(),
                path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                kind: ValueKind::Scalar,
                guards: vec![Guard::Default {
                    path: "feature".to_string(),
                }],
                resource: None,
                provenance: Vec::new(),
            },
        ]);
        let schema_signals = contract.into_schema_signals();
        let mut schema =
            generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));

        apply_required_inference(
            &mut schema,
            schema_signals.schema_evidence_by_value_path(),
            &BTreeSet::new(),
        );

        assert!(
            schema.get("required").is_none(),
            "contract default guards should suppress required inference without a text fallback scan, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    #[test]
    fn plain_pathless_scalar_use_does_not_mark_required_without_header_guard() {
        let contract = contract_for(vec![ContractUse {
            source_expr: "feature".to_string(),
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
            provenance: Vec::new(),
        }]);
        let schema_signals = contract.into_schema_signals();
        let mut schema =
            generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));

        apply_required_inference(
            &mut schema,
            schema_signals.schema_evidence_by_value_path(),
            &BTreeSet::new(),
        );

        assert!(
            schema.get("required").is_none(),
            "plain pathless scalar uses are not enough to infer required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    #[test]
    fn explicit_nested_values_defaults_suppress_required_inference() {
        let contract = contract_for(vec![ContractUse {
            source_expr: "controller.kind".to_string(),
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            guards: vec![Guard::Eq {
                path: "controller.kind".to_string(),
                value: GuardValue::string("Deployment"),
            }],
            resource: None,
            provenance: Vec::new(),
        }]);
        let schema_signals = contract.into_schema_signals();
        let mut schema =
            generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));
        let explicit_default_value_paths =
            BTreeSet::from(["controller.kind".to_string(), "controller".to_string()]);

        apply_required_inference(
            &mut schema,
            schema_signals.schema_evidence_by_value_path(),
            &explicit_default_value_paths,
        );

        assert!(
            schema.get("required").is_none(),
            "explicit nested chart defaults should suppress required inference, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    /// Guard-only feature toggles are not strong enough evidence for
    /// user-requiredness: omission is a legitimate "branch disabled" choice.
    #[test]
    fn step3_guard_only_if_block_does_not_mark_required() {
        let src = indoc! {r"
            {{- if .Values.serviceAccount.create }}
            apiVersion: v1
            kind: ServiceAccount
            metadata:
              name: foo
            {{- end }}
        "};
        let schema = generate_with_required(src, None);

        assert!(
            schema.get("required").is_none(),
            "guard-only feature toggles should not become required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
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
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "fullnameOverride has a parenthesized default fallback, should not be required, schema={}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    #[test]
    fn default_after_intervening_required_call_does_not_suppress_required() {
        let src = indoc! {r#"
            {{- if .Values.name }}
            enabled: true
            {{- end }}
            name: {{ .Values.name | required "name is required" | default "fallback" }}
        "#};
        let schema = generate_with_required(src, None);
        sim_assert_eq!(
            have: schema.get("required"),
            want: Some(&serde_json::json!(["name"])),
            "default after required should not suppress required inference, schema={}",
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

    #[test]
    fn self_guarded_helper_override_does_not_mark_required() {
        let src = indoc! {r"
            metadata:
              name: {{- if .Values.fullnameOverride -}}
                {{ .Values.fullnameOverride }}
              {{- else -}}
                generated
              {{- end -}}
        "};
        let schema = generate_with_required(src, None);
        assert!(
            schema.get("required").is_none(),
            "self-guarded helper override branches should not become required, schema={}",
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
        let schema_signals = parse_contract(src).into_schema_signals();
        let schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));
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
