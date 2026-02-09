use crate::{DefaultIrGenerator, Guard, IrGenerator, ValueKind, YamlPath};
use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};

/// Simple template IR generation test.
#[test]
fn simple_template_ir() {
    let src = r#"{{- if .Values.enabled }}
foo: {{ .Values.name }}
{{- end }}
"#;
    let ast = FusedRustParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = DefaultIrGenerator.generate(&ast, &idx);

    assert!(
        ir.iter()
            .any(|u| u.source_expr == "enabled" && u.guards.is_empty())
    );
    assert!(ir.iter().any(|u| u.source_expr == "name"
        && u.path == YamlPath(vec!["foo".to_string()])
        && u.kind == ValueKind::Scalar
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
}
