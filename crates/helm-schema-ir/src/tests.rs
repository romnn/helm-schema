use crate::{Guard, IrGenerator, SymbolicIrGenerator, ValueKind, YamlPath};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};

/// Simple template IR generation test.
#[test]
fn simple_template_ir() {
    let src = r"{{- if .Values.enabled }}
foo: {{ .Values.name }}
{{- end }}
";
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = SymbolicIrGenerator.generate(src, &ast, &idx);

    assert!(ir.iter().any(|u| u.source_expr == "enabled"
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
    assert!(ir.iter().any(|u| u.source_expr == "name"
        && u.path == YamlPath(vec!["foo".to_string()])
        && u.kind == ValueKind::Scalar
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
}

#[test]
fn document_output_projection_preserves_resource_claim() {
    let src = r"
apiVersion: v1
kind: Service
metadata:
  name: {{ .Values.serviceName }}
";
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = SymbolicIrGenerator.generate(src, &ast, &idx);

    let name_use = ir
        .iter()
        .find(|use_| use_.source_expr == "serviceName")
        .expect("serviceName use");

    assert_eq!(
        name_use.path,
        YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    let resource = name_use.resource.as_ref().expect("resource claim");
    assert_eq!(resource.api_version, "v1");
    assert_eq!(resource.kind, "Service");
}
