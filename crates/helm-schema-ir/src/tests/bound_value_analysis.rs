use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

struct GetBinding {
    base: helm_schema_core::ValuesPath,
    key_var: String,
}

fn execute(source: &str) -> crate::value::DocumentExecution {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
}

fn extract_bound_values(
    text: &str,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> eyre::Result<Vec<String>> {
    let mut source = String::new();
    for (name, domain) in range_domains {
        write!(source, "{{{{- range ${name} := list")?;
        for member in domain {
            write!(source, " {member:?}")?;
        }
        source.push_str(" -}}");
    }
    for (name, binding) in get_bindings {
        write!(
            source,
            "{{{{- ${name} := get .Values.{} ${} -}}}}",
            binding.base.encode(),
            binding.key_var
        )?;
    }
    if text.trim_start().starts_with("{{") {
        source.push_str(text);
    } else {
        write!(source, "{{{{ {text} }}}}")?;
    }
    for _ in range_domains {
        source.push_str("{{- end -}}");
    }
    Ok(execute(&source)
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| site.value().inputs())
        .map(|path| path.encode())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

#[test]
fn parse_get_binding_detects_declaration_from_ast() -> eyre::Result<()> {
    let execution = execute(
        r#"{{- $key := "frontend" -}}{{- $value := get $.Values.config $key -}}{{- $value -}}"#,
    );
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing bound output")?;
    sim_assert_eq!(have: site.value().identity().map(helm_schema_core::ValuesPath::encode), want: Some("config.frontend".to_string()));
    Ok(())
}

#[test]
fn parse_get_binding_detects_assignment_from_ast() -> eyre::Result<()> {
    let execution = execute(
        r#"{{- $key := "frontend" -}}{{- $value := nil -}}{{- $value = get .Values.config $key -}}{{- $value -}}"#,
    );
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing assigned output")?;
    sim_assert_eq!(have: site.value().identity().map(helm_schema_core::ValuesPath::encode), want: Some("config.frontend".to_string()));
    Ok(())
}

#[test]
fn parse_literal_list_range_detects_variable_definition_from_ast() {
    sim_assert_eq!(
        have: parse_action_expressions(r#"{{ $scope := list "frontend" "backend" }}"#),
        want: vec![TemplateExpr::VariableDefinition {
            name: "$scope".to_string(),
            value: Box::new(TemplateExpr::Call {
                function: "list".to_string(),
                args: vec![
                    TemplateExpr::Literal(helm_schema_ast::Literal::String("frontend".to_string())),
                    TemplateExpr::Literal(helm_schema_ast::Literal::String("backend".to_string())),
                ],
            }),
        }]
    );
}

#[test]
fn extract_bound_values_resolves_selector_reads_from_ast() -> eyre::Result<()> {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "scope".to_string(),
        vec!["frontend".to_string(), "backend".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: helm_schema_core::ValuesPath::parse("config"),
            key_var: "scope".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"{{- printf "%s" $config.port -}}"#,
            &range_domains,
            &get_bindings
        )?,
        want: vec![
            "config.backend.port".to_string(),
            "config.frontend.port".to_string()
        ]
    );
    Ok(())
}

#[test]
fn extract_bound_values_respects_or_short_circuit_eq_predicate() -> eyre::Result<()> {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "protocol".to_string(),
        vec!["nats".to_string(), "websocket".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: helm_schema_core::ValuesPath::parse("config"),
            key_var: "protocol".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"or (eq $protocol "nats") $config.enabled"#,
            &range_domains,
            &get_bindings
        )?,
        want: vec!["config.websocket.enabled".to_string()]
    );
    Ok(())
}

#[test]
fn extract_bound_values_respects_and_short_circuit_eq_predicate() -> eyre::Result<()> {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "protocol".to_string(),
        vec!["nats".to_string(), "websocket".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: helm_schema_core::ValuesPath::parse("config"),
            key_var: "protocol".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"and (eq $protocol "nats") $config.enabled"#,
            &range_domains,
            &get_bindings
        )?,
        want: vec!["config.nats.enabled".to_string()]
    );
    Ok(())
}
