use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, Literal, TemplateExpr};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::{DefinitionInventory, IrAnalysisDb, ParsedProgram, ParsedProgramSyntax};
use crate::function_semantics::HeapEffect;
use crate::value::{
    State, Value, eval::local_name, namespace::Namespace, write_footprint::local_targets,
};

pub(super) fn may_write(
    source: &str,
    syntax: &ParsedProgramSyntax,
    same_source_names: &BTreeSet<String>,
) -> bool {
    if syntax.actions.len() > 128 || syntax.tree.root_node().has_error() {
        return true;
    }
    let mut definitions = BTreeSet::new();
    let mut floor = false;
    for action in syntax.expressions.starting_in(Span::new(0, source.len())) {
        for expression in action.expressions.iter() {
            expression.walk(|expression| match expression {
                TemplateExpr::VariableDefinition { name, value: _ } => {
                    let name = local_name(name);
                    if name != "_" {
                        floor |= !definitions.insert(name.to_string());
                    }
                }
                TemplateExpr::Assignment { .. } | TemplateExpr::Unknown(_) => floor = true,
                TemplateExpr::Call { function, args } => {
                    if matches!(function.as_str(), "include" | "template") {
                        floor |= !matches!(args.first().map(TemplateExpr::deparen), Some(TemplateExpr::Literal(Literal::String(name))) if same_source_names.contains(name));
                    } else {
                        floor |= crate::function_semantics::function_semantics(function).heap_effect != HeapEffect::ReadOnly;
                    }
                }
                TemplateExpr::Literal(_)
                | TemplateExpr::Field(_)
                | TemplateExpr::Selector { .. }
                | TemplateExpr::Variable(_)
                | TemplateExpr::Pipeline(_)
                | TemplateExpr::Parenthesized(_) => {}
            });
        }
    }
    let mut nodes = vec![syntax.tree.root_node()];
    while let Some(node) = nodes.pop() {
        if node.kind() == "range_action" {
            floor |= helm_schema_ast::range_uses_assignment(node);
            for name in [
                helm_schema_ast::range_destructured_key_variable(node, source),
                helm_schema_ast::range_destructured_value_variable(node, source),
            ]
            .into_iter()
            .flatten()
            {
                floor |= definitions.contains(&name);
            }
        }
        if node.kind() == "range_variable_definition" {
            let mut cursor = node.walk();
            for variable in node
                .children(&mut cursor)
                .take_while(|child| !matches!(child.kind(), ":=" | "="))
                .filter(|child| child.kind() == "variable")
            {
                let Ok(name) = variable.utf8_text(source.as_bytes()) else {
                    return true;
                };
                floor |= definitions.contains(local_name(name));
            }
        }
        let mut cursor = node.walk();
        nodes.extend(node.named_children(&mut cursor));
    }
    floor
}

struct Fixture {
    namespace: Rc<Namespace>,
    program: Rc<ParsedProgram>,
}

impl Fixture {
    fn new(source: &str, helpers: &str) -> Self {
        let mut defines = DefineIndex::new();
        if !helpers.is_empty() {
            defines.add_file_source("chart/templates/_helpers.tpl", helpers);
        }
        let db = IrAnalysisDb::new(&defines);
        let state = State::new(Value::nil());
        let source = db.parsed_source(source, None, "chart/templates/main.yaml");
        let sources = db
            .template_sources()
            .iter()
            .cloned()
            .chain(std::iter::once(Rc::clone(&source)))
            .collect();
        let namespace =
            Namespace::from_sources(sources, DefinitionInventory::Complete, &state.heap.arena)
                .namespace;
        Self {
            namespace,
            program: Rc::clone(&source.root),
        }
    }

    fn syntax(&self) -> eyre::Result<&ParsedProgramSyntax> {
        self.program
            .syntax()
            .ok_or_eyre("missing certificate syntax")
    }

    fn certificate(&self) -> bool {
        let source = self.program.source();
        let names = self
            .namespace
            .same_source_names(source)
            .into_iter()
            .map(str::to_owned)
            .collect();
        self.program
            .syntax()
            .is_none_or(|syntax| may_write(source, syntax, &names))
    }
}

#[test]
fn readonly_program_is_a_nonvacuous_false_witness() {
    let fixture = Fixture::new("{{ .Values.name | quote }}", "");
    sim_assert_eq!(have: fixture.certificate(), want: false);
}

#[test]
fn certificate_covers_every_frozen_local_targets_fallback() -> eyre::Result<()> {
    let large = "{{ .Values.read }}".repeat(129);
    for source in [
        large.as_str(),
        r#"{{ $value := "a" }}{{ $value = "b" }}"#,
        r#"{{ $value := "a" }}{{ $value := "b" }}"#,
        r#"{{ include "reader" . }}"#,
        r#"{{ template "reader" . }}"#,
        "{{ include .Values.helper . }}",
        r#"{{ tpl "readonly text" . }}"#,
        r#"{{ set . "key" true }}"#,
        r#"{{ unset . "key" }}"#,
        "{{ unknownFunction . }}",
        r#"{{ $value := "a" }}{{ range $value = .Values.items }}{{ . }}{{ end }}"#,
        "{{ $value := dict }}{{ range $value := .Values.items }}{{ . }}{{ end }}",
        "{{ $key := dict }}{{ range $key, $value := .Values.items }}{{ . }}{{ end }}",
        "{{ $value := dict }}{{ range $key, $value := .Values.items }}{{ . }}{{ end }}",
    ] {
        let fixture = Fixture::new(source, r#"{{ define "reader" }}readonly{{ end }}"#);
        let targets = local_targets(source, fixture.syntax()?, &fixture.namespace);
        sim_assert_eq!(have: targets, want: None);
        sim_assert_eq!(have: fixture.certificate(), want: true);
    }
    Ok(())
}

#[test]
fn parser_gaps_and_literal_tpl_are_may_write() {
    for source in ["{{ if }}", r#"{{ tpl "readonly text" . }}"#] {
        let fixture = Fixture::new(source, "");
        sim_assert_eq!(have: fixture.certificate(), want: true);
    }
}

#[test]
fn same_source_names_control_literal_include_classification() -> eyre::Result<()> {
    let source = r#"{{ include "reader" . }}"#;
    let fixture = Fixture::new(source, r#"{{ define "reader" }}readonly{{ end }}"#);
    let names = fixture.namespace.same_source_names(source);
    sim_assert_eq!(have: names.contains(&"reader"), want: false);
    sim_assert_eq!(have: fixture.certificate(), want: true);
    let same_source = BTreeSet::from(["reader".to_owned()]);
    sim_assert_eq!(have: may_write(source, fixture.syntax()?, &same_source), want: false);
    sim_assert_eq!(have: may_write(source, fixture.syntax()?, &BTreeSet::new()), want: true);
    Ok(())
}
