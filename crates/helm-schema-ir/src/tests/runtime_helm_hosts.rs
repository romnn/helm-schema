use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::execute_document;
use crate::analysis_db::IrAnalysisDb;

#[test]
fn files_get_plain_text_and_missing_file_preserve_values_identity() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_files_get_source("f", "plain text");
    let db = IrAnalysisDb::new(&defines);
    for source in [
        r#"{{ tpl (.Files.Get "f") . }}{{ .Values.after }}"#,
        r#"{{ tpl (.Files.Get "missing") . }}{{ .Values.after }}"#,
    ] {
        let execution = execute_document(source, Some("templates/NOTES.txt"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        let output = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing final values observation")?;
        sim_assert_eq!(have: output.value().identity(), want: Some(&ValuesPath::parse("after")));
    }
    Ok(())
}

#[test]
fn files_get_program_executes_the_normal_tpl_transfer() {
    let mut defines = DefineIndex::new();
    defines.add_files_get_source("f", r#"{{ $_ := set .Values "after" "changed" }}executed"#);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(
        r#"{{ tpl (.Files.Get "f") . }}|{{ .Values.after }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("executed|changed".to_owned())]);
}

#[test]
fn missing_files_get_returns_empty_text() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"[{{ .Files.Get "missing" }}]"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("[]".to_owned())]);
}

#[test]
fn capabilities_has_is_read_only_even_with_an_unknown_result() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ $_ := .Capabilities.APIVersions.Has "apps/v1" }}{{ .Values.after }}"#,
        "{{ $_ := .Capabilities.KubeVersion.Version }}{{ .Values.after }}",
        "{{ $_ := .Capabilities.KubeVersion.Major }}{{ .Values.after }}",
        "{{ $_ := .Capabilities.KubeVersion.Minor }}{{ .Values.after }}",
        "{{ $_ := .Capabilities.KubeVersion.GitVersion }}{{ .Values.after }}",
        "{{ $_ := .Capabilities.KubeVersion.String }}{{ .Values.after }}",
    ] {
        let execution = execute_document(source, Some("templates/NOTES.txt"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        let output = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing final values observation")?;
        sim_assert_eq!(have: output.value().identity(), want: Some(&ValuesPath::parse("after")));
    }
    Ok(())
}

#[test]
fn files_get_uses_the_executing_chart_instance_catalog() {
    let source = r#"{{ tpl (.Files.Get "f") . }}"#;
    let mut defines = DefineIndex::new();
    defines.add_chart_file("root", "f", b"parent");
    defines.add_chart_file("root/charts/child", "f", b"child");
    defines.add_chart_template_source("root", "root/templates/main.yaml", source);
    defines.add_chart_template_source(
        "root/charts/child",
        "root/charts/child/templates/main.yaml",
        source,
    );
    let db = IrAnalysisDb::new(&defines);
    let documents = super::execute_documents(
        &[
            crate::symbolic::SymbolicTemplate {
                source_path: "root/templates/main.yaml",
                source,
            },
            crate::symbolic::SymbolicTemplate {
                source_path: "root/charts/child/templates/main.yaml",
                source,
            },
        ],
        &db,
    );
    sim_assert_eq!(have: documents.iter().map(|document| document.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>()).collect::<Vec<_>>(), want: vec![vec![Some("parent".to_owned())], vec![Some("child".to_owned())]]);
}

#[test]
fn readonly_file_methods_keep_known_payloads_and_receiver_types() {
    let contents = indoc! {"
        hello
        world
    "};
    let mut defines = DefineIndex::new();
    defines.add_files_get_source("config/a.txt", contents);
    defines.add_chart_file("", "binary", &[0, 255]);
    defines.add_files_get_source("plain/a", "hello");
    defines.add_chart_file("", "secret/a", &[0, 0, 0]);
    let db = IrAnalysisDb::new(&defines);
    for (source, expected) in [
        (
            r#"{{ (.Files.Glob "config/*").Get "config/a.txt" }}"#,
            contents,
        ),
        (
            r#"{{ range .Files.Lines "config/a.txt" }}[{{ . }}]{{ end }}"#,
            "[hello][world]",
        ),
        (r#"{{ typeOf (.Files.Lines "config/a.txt") }}"#, "[]string"),
        (
            r#"{{ toString (.Files.GetBytes "config/a.txt") }}"#,
            contents,
        ),
        (r#"{{ toJson (.Files.GetBytes "binary") }}"#, r#""AP8=""#),
        (
            r#"{{ typeOf (.Files.GetBytes "binary") }}:{{ len (.Files.GetBytes "binary") }}:{{ index (.Files.GetBytes "binary") 1 }}"#,
            "[]uint8:2:255",
        ),
        (
            r#"{{ typeOf (.Files.GetBytes "missing") }}:{{ len (.Files.GetBytes "missing") }}"#,
            "[]uint8:0",
        ),
        (
            r#"{{ $missing := index .Files "missing" }}{{ typeOf $missing }}:{{ toJson $missing }}:{{ eq $missing nil }}:{{ len $missing }}:{{ required "r" $missing | len }}"#,
            "[]uint8:null:true:0:0",
        ),
        ("{{ typeOf .Files.missing }}", "[]uint8"),
        (r#"{{ (.Files.Glob "plain/*").AsConfig }}"#, "a: hello"),
        (r#"{{ (.Files.Glob "secret/*").AsSecrets }}"#, "a: AAAA"),
    ] {
        let execution = execute_document(source, Some("templates/test.yaml"), &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some(expected.to_owned())]);
    }
}
