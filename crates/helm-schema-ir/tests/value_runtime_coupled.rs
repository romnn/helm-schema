//! Public contract controls for the coupled value-runtime replacement.

use helm_schema_ast::DefineIndex;
use helm_schema_core::{ContractSchemaSignals, Predicate};
use helm_schema_ir::{ContractIr, SymbolicIrContext};
use test_util::prelude::sim_assert_eq;

fn expected_failure(fails: bool) -> ContractSchemaSignals {
    let mut contract = ContractIr::default();
    if fails {
        contract.add_terminal_fail_condition(Predicate::True);
    }
    contract.finalize().into_schema_signals()
}

fn assert_exact_text(setup: &str, expression: &str, expected: &str, helpers: Option<&str>) {
    let mut defines = DefineIndex::new();
    if let Some(helpers) = helpers {
        defines.add_file_source("templates/_helpers.tpl", helpers);
    }
    let context = SymbolicIrContext::new(&defines);
    for (comparison, fails) in [("eq", true), ("ne", false)] {
        let source = format!(
            "{setup}{{{{ if {comparison} ({expression}) {expected} }}}}{{{{ fail \"witness\" }}}}{{{{ end }}}}"
        );
        sim_assert_eq!(
            have: context.generate_contract_ir(&source).finalize().into_schema_signals(),
            want: expected_failure(fails),
            "{source}"
        );
    }
}

#[test]
fn serialization_freezes_before_a_later_alias_write() {
    assert_exact_text(
        r#"{{ $d := dict "x" "old" }}{{ $before := toJson $d }}{{ $alias := $d }}{{ $_ := set $alias "x" "new" }}"#,
        "$before",
        r#"`{"x":"old"}`"#,
        None,
    );
}

#[test]
fn short_circuit_does_not_execute_the_unreached_writer() {
    assert_exact_text(
        r#"{{ $d := dict "x" "old" }}{{ $_ := and false (set $d "x" "new") }}"#,
        r#"get $d "x""#,
        r#""old""#,
        None,
    );
}

#[test]
fn include_writes_the_callers_heap() {
    assert_exact_text(
        r#"{{ $d := dict "x" "old" }}{{ $_ := include "write" $d }}"#,
        r#"get $d "x""#,
        r#""new""#,
        Some(r#"{{ define "write" }}{{ $_ := set . "x" "new" }}{{ end }}"#),
    );
}

#[test]
fn include_rebinds_dollar_without_overwriting_the_caller_binding() {
    assert_exact_text(
        r#"{{ $d := dict "x" "old" }}{{ $caller := "kept" }}{{ $_ := include "write" $d }}"#,
        "$caller",
        r#""kept""#,
        Some(r#"{{ define "write" }}{{ $ := dict "x" "local" }}{{ get $ "x" }}{{ end }}"#),
    );
}

#[test]
fn required_accepts_false_zero_and_empty_maps() {
    for expression in [
        r#"required "message" false"#,
        r#"required "message" 0"#,
        r#"required "message" (dict)"#,
    ] {
        let source = format!("{{{{ $_ := {expression} }}}}");
        sim_assert_eq!(
            have: SymbolicIrContext::new(&DefineIndex::new()).generate_contract_ir(&source).finalize().into_schema_signals(),
            want: expected_failure(false)
        );
    }
    assert_exact_text("", r#"required "message" false"#, "false", None);
}

#[test]
fn nil_string_conversion_has_call_specific_semantics() {
    assert_exact_text("", "quote nil", r#""""#, None);
    assert_exact_text("", "squote nil", r#""""#, None);
    assert_exact_text("", "toString nil", r#""<nil>""#, None);
}

#[test]
fn get_distinguishes_present_nil_from_absence() {
    assert_exact_text(
        r#"{{ $d := dict "nil" nil }}"#,
        r#"get $d "missing""#,
        r#""""#,
        None,
    );
    assert_exact_text(
        r#"{{ $d := dict "nil" nil }}"#,
        r#"get $d "nil""#,
        "nil",
        None,
    );
}

#[test]
fn an_unknown_alternative_cannot_publish_a_universal_abort() {
    let contract = SymbolicIrContext::new(&DefineIndex::new())
        .generate_contract_ir(
            r#"{{ if eq (randAlphaNum 1) "a" }}{{ fail "no" }}{{ else }}ok{{ end }}"#,
        )
        .finalize();
    sim_assert_eq!(have: contract.into_schema_signals(), want: expected_failure(false));
}

#[test]
fn conservative_transfers_do_not_reject_helm_successes() {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    for source in [
        r#"{{ $_ := get (fromJson "[]") "Error" }}"#,
        r#"{{ $_ := required "nil map" (fromJson "null") }}"#,
        r#"{{ $_ := required "nil slice" (fromJsonArray "null") }}"#,
        r#"{{ if quote nil }}{{ fail "unexpected" }}{{ end }}"#,
        r#"{{ if eq (toString nil) "<no value>" }}{{ fail "wrong" }}{{ end }}"#,
        r#"{{ if eq (quote "a" "b") "\"b\"" }}{{ fail "wrong" }}{{ end }}"#,
        r#"{{ if eq (print 1 2) "12" }}{{ fail "wrong" }}{{ end }}"#,
        r#"{{ if eq (toJson "<") "\"<\"" }}{{ fail "wrong" }}{{ end }}"#,
        r#"{{ if eq (toJson 1.0) "1.0" }}{{ fail "wrong" }}{{ end }}"#,
        r#"{{ if eq (toString 1000000000.0) "1000000000" }}{{ fail "wrong" }}{{ end }}"#,
    ] {
        sim_assert_eq!(
            have: context.generate_contract_ir(source).finalize().into_schema_signals(),
            want: expected_failure(false),
            "{source}"
        );
    }
    sim_assert_eq!(
        have: context.generate_contract_ir(r#"{{ $_ := repeat 1.0 "x" }}"#).finalize().into_schema_signals(),
        want: expected_failure(false)
    );
}

#[test]
fn template_without_pipeline_uses_nil_dot() {
    let mut defines = DefineIndex::new();
    defines.add_file_source("templates/_helpers.tpl", r#"{{ define "h" }}ok{{ end }}"#);
    sim_assert_eq!(
        have: SymbolicIrContext::new(&defines).generate_contract_ir(r#"{{ template "h" }}"#).finalize().into_schema_signals(),
        want: expected_failure(false)
    );
}

#[test]
fn partial_policy_does_not_claim_unknown_capabilities_absent() {
    let context = SymbolicIrContext::with_policy(
        &DefineIndex::new(),
        helm_schema_ir::SymbolicPolicy {
            kubernetes_version: Some("1.29.0".to_owned()),
            ..helm_schema_ir::SymbolicPolicy::default()
        },
    );
    sim_assert_eq!(
        have: context.generate_contract_ir(r#"{{ if .Capabilities.APIVersions }}ok{{ else }}{{ fail "missing" }}{{ end }}"#).finalize().into_schema_signals(),
        want: expected_failure(false)
    );
}

#[test]
fn output_budget_neighbors_widen_the_whole_result() {
    for terms in [127, 128, 129] {
        let helper = format!(
            "{{{{ define \"bounded\" }}}}{}{{{{ end }}}}",
            r#"{{ "x" }}"#.repeat(terms)
        );
        let mut defines = DefineIndex::new();
        defines.add_file_source("templates/_helpers.tpl", &helper);
        let source = format!(
            "{{{{ if eq (include \"bounded\" .) \"{}\" }}}}{{{{ fail \"matched\" }}}}{{{{ end }}}}",
            "x".repeat(terms)
        );
        sim_assert_eq!(
            have: SymbolicIrContext::new(&defines).generate_contract_ir(&source).finalize().into_schema_signals(),
            want: expected_failure(terms <= 128),
            "output terms: {terms}"
        );
    }
}

#[test]
fn render_file_writes_reach_the_later_public_contract() {
    use helm_schema_ir::SymbolicTemplate;

    let context = SymbolicIrContext::new(&DefineIndex::new());
    for (written, fails) in [("false", false), ("true", true)] {
        let writer = format!("{{{{ $_ := set .Values \"enabled\" {written} }}}}");
        let mut combined = ContractIr::default();
        for contract in context.generate_contract_irs_for_sources(&[
            SymbolicTemplate {
                source: r#"{{ if .Values.enabled }}{{ $_ := required "bad" }}{{ end }}"#,
                source_path: "chart/templates/a-check.yaml",
            },
            SymbolicTemplate {
                source: &writer,
                source_path: "chart/templates/z-write.yaml",
            },
        ]) {
            combined.append(contract);
        }
        sim_assert_eq!(
            have: combined.finalize().into_schema_signals(),
            want: expected_failure(fails)
        );
    }
}

#[test]
fn yaml_decoding_resolves_plain_booleans_and_mapping_keys() {
    assert_exact_text("", r#"get (fromYaml "a: yes") "a""#, "true", None);
    assert_exact_text("", r#"first (fromYamlArray "- off")"#, "false", None);
    assert_exact_text("", r#"get (fromYaml "on: x") "true""#, r#""x""#, None);
    assert_exact_text("", r#"get (fromYaml "1: x") "1""#, r#""x""#, None);
    assert_exact_text("", r#"get (fromYaml "a: 'yes'") "a""#, r#""yes""#, None);
}

#[test]
fn yaml_decoding_expands_merge_keys_before_lookup() {
    assert_exact_text(
        r#"{{ $m := fromYaml "base: &base {p: kept}\nchild: {<<: *base, q: extra}" }}"#,
        "get $m.child \"p\"",
        r#""kept""#,
        None,
    );
}

#[test]
fn yaml_empty_and_null_maps_can_be_populated() {
    for document in ["", "null"] {
        assert_exact_text(
            &format!(
                r#"{{{{ $m := fromYaml {document:?} }}}}{{{{ $_ := set $m "x" "written" }}}}"#
            ),
            "get $m \"x\"",
            r#""written""#,
            None,
        );
    }
    assert_exact_text("", r#"eq (fromYamlArray "null") nil"#, "false", None);
}

#[test]
fn sprig_string_sequences_omit_nil_members() {
    assert_exact_text("", r#"first (toStrings (list nil "b"))"#, r#""b""#, None);
    assert_exact_text(
        "",
        r#"join "," (sortAlpha (list "b" nil "a"))"#,
        r#""a,b""#,
        None,
    );
    assert_exact_text(
        "",
        r#"join "," (list nil "a" nil "b" nil)"#,
        r#""a,b""#,
        None,
    );
}

#[test]
fn default_ignores_later_values_after_their_effects_execute() {
    assert_exact_text("", r#""c" | default "a" "b""#, r#""b""#, None);
    assert_exact_text("", r#""c" | default "a" """#, r#""a""#, None);
    assert_exact_text(
        r#"{{ $d := dict }}{{ $v := default "fallback" "first" (set $d "x" "written") }}"#,
        "get $d \"x\"",
        r#""written""#,
        None,
    );
}

#[test]
fn unspellable_yaml_cannot_create_an_exact_failure() {
    for source in [
        r#"{{ if hasKey (fromYaml "child: {~: x}\nkept: true") "kept" }}{{ fail "YAML error erased" }}{{ end }}"#,
        r#"{{ if hasKey (fromYaml "a: .inf\nkept: true") "kept" }}{{ fail "JSON conversion error erased" }}{{ end }}"#,
        r#"{{ if hasKey (fromYaml "'<<': {p: x}") "p" }}{{ fail "quoted key expanded" }}{{ end }}"#,
    ] {
        sim_assert_eq!(
            have: SymbolicIrContext::new(&DefineIndex::new()).generate_contract_ir(source).finalize().into_schema_signals(),
            want: expected_failure(false),
            "{source}"
        );
    }
}
