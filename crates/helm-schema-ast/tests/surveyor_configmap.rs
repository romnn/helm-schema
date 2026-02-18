use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR_FUSED: &str = r#"(Document
  (If ".Values.config.jetstream.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ConfigMap"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (Scalar "{{ include \"surveyor.fullname\" . }}-accounts"))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"surveyor.labels\" . | nindent 4"))))
        (Pair
          (Scalar "data")))
      (Range ".Values.config.jetstream.accounts"
        (body
          (HelmExpr "$d := dict")
          (If ".tls"
            (then
              (If ".tls.ca"
                (then
                  (HelmExpr "$_ := set $d \"tls_ca\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.ca)")))
              (If ".tls.cert"
                (then
                  (HelmExpr "$_ := set $d \"tls_cert\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.cert)")))
              (If ".tls.key"
                (then
                  (HelmExpr "$_ := set $d \"tls_key\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.key)")))))
          (Mapping
            (Pair
              (Scalar "{{.name}}.json")
              (Scalar "")))
          (HelmExpr "merge $d (omit . \"tls\") | toJson"))))))"#;

const EXPECTED_SEXPR_TREE: &str = r#"(Document
  (If ".Values.config.jetstream.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ConfigMap"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (Scalar "{{include \"surveyor.fullname\" . }}-accounts"))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"surveyor.labels\" . | nindent 4"))))
        (Pair
          (Scalar "data")))
      (Range ".Values.config.jetstream.accounts"
        (body
          (HelmExpr "$d := dict")
          (If ".tls"
            (then
              (If ".tls.ca"
                (then
                  (HelmExpr "$_ := set $d \"tls_ca\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.ca)")))
              (If ".tls.cert"
                (then
                  (HelmExpr "$_ := set $d \"tls_cert\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.cert)")))
              (If ".tls.key"
                (then
                  (HelmExpr "$_ := set $d \"tls_key\" (printf \"/etc/nats-certs/accounts/%s/%s\" .name .tls.key)")))))
          (Mapping
            (Pair
              (Scalar "{{.name}}.json")
              (Scalar "|")))
          (HelmExpr "merge $d (omit . \"tls\") | toJson"))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/configmap.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR_FUSED.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/configmap.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR_TREE.trim_end());
}
