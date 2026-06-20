use helm_schema_ast::{HelmParser, TreeSitterParser};
use test_util::prelude::sim_assert_eq;

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml";

#[allow(clippy::too_many_lines)]
const EXPECTED_SEXPR: &str = r#"(Document
  (HelmComment "/*\nCopyright VMware, Inc.\nSPDX-License-Identifier: APACHE-2.0\n*/")
  (HelmExpr "$host := include \"postgresql.v1.primary.fullname\" .")
  (HelmExpr "$port := include \"postgresql.v1.service.port\" .")
  (HelmExpr "$customUser := include \"postgresql.v1.username\" .")
  (HelmExpr "$postgresPassword := include \"common.secrets.lookup\" (dict \"secret\" (include \"postgresql.v1.secretName\" .) \"key\" (coalesce .Values.global.postgresql.auth.secretKeys.adminPasswordKey .Values.auth.secretKeys.adminPasswordKey) \"defaultValue\" (ternary (coalesce .Values.global.postgresql.auth.password .Values.auth.password .Values.global.postgresql.auth.postgresPassword .Values.auth.postgresPassword) (coalesce .Values.global.postgresql.auth.postgresPassword .Values.auth.postgresPassword) (or (empty $customUser) (eq $customUser \"postgres\"))) \"context\" $) | trimAll \"\\\"\" | b64dec")
  (If "and (not $postgresPassword) .Values.auth.enablePostgresUser"
    (then
      (HelmExpr "$postgresPassword = randAlphaNum 10")))
  (HelmExpr "$replicationPassword := \"\"")
  (If "eq .Values.architecture \"replication\""
    (then
      (HelmExpr "$replicationPassword = include \"common.secrets.passwords.manage\" (dict \"secret\" (include \"postgresql.v1.secretName\" .) \"key\" (coalesce .Values.global.postgresql.auth.secretKeys.replicationPasswordKey .Values.auth.secretKeys.replicationPasswordKey) \"providedValues\" (list \"auth.replicationPassword\") \"context\" $) | trimAll \"\\\"\" | b64dec")))
  (HelmExpr "$ldapPassword := \"\"")
  (If "and .Values.ldap.enabled (or .Values.ldap.bind_password .Values.ldap.bindpw)"
    (then
      (HelmExpr "$ldapPassword = coalesce .Values.ldap.bind_password .Values.ldap.bindpw")))
  (HelmExpr "$password := \"\"")
  (If "and (not (empty $customUser)) (ne $customUser \"postgres\")"
    (then
      (HelmExpr "$password = include \"common.secrets.passwords.manage\" (dict \"secret\" (include \"postgresql.v1.secretName\" .) \"key\" (coalesce .Values.global.postgresql.auth.secretKeys.userPasswordKey .Values.auth.secretKeys.userPasswordKey) \"providedValues\" (list \"global.postgresql.auth.password\" \"auth.password\") \"context\" $) | trimAll \"\\\"\" | b64dec")))
  (HelmExpr "$database := include \"postgresql.v1.database\" .")
  (If "(include \"postgresql.v1.createSecret\" .)"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "v1"))
        (Pair
          (Scalar "kind")
          (Scalar "Secret"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "include \"common.names.fullname\" ."))
            (Pair
              (Scalar "namespace")
              (HelmExpr ".Release.Namespace | quote"))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")))))
      (If ".Values.commonAnnotations"
        (then
          (Mapping
            (Pair
              (Scalar "annotations")
              (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "type")
          (Scalar "Opaque"))
        (Pair
          (Scalar "data")))
      (If "$postgresPassword"
        (then
          (Mapping
            (Pair
              (Scalar "postgres-password")
              (HelmExpr "$postgresPassword | b64enc | quote")))))
      (If "$password"
        (then
          (Mapping
            (Pair
              (Scalar "password")
              (HelmExpr "$password | b64enc | quote")))))
      (If "$replicationPassword"
        (then
          (Mapping
            (Pair
              (Scalar "replication-password")
              (HelmExpr "$replicationPassword | b64enc | quote")))))
      (If "and .Values.ldap.enabled (or .Values.ldap.bind_password .Values.ldap.bindpw)"
        (then
          (Mapping
            (Pair
              (Scalar "ldap-password")
              (HelmExpr "$ldapPassword  | b64enc | quote")))))))
  (If ".Values.serviceBindings.enabled"
    (then
      (If "$postgresPassword"
        (then
          (Mapping
            (Pair
              (Scalar "apiVersion")
              (Scalar "v1"))
            (Pair
              (Scalar "kind")
              (Scalar "Secret"))
            (Pair
              (Scalar "metadata")
              (Mapping
                (Pair
                  (Scalar "name")
                  (Scalar "{{include \"common.names.fullname\" . }}-svcbind-postgres"))
                (Pair
                  (Scalar "namespace")
                  (HelmExpr ".Release.Namespace | quote"))
                (Pair
                  (Scalar "labels")
                  (HelmExpr "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")))))
          (If ".Values.commonAnnotations"
            (then
              (Mapping
                (Pair
                  (Scalar "annotations")
                  (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
          (Mapping
            (Pair
              (Scalar "type")
              (Scalar "servicebinding.io/postgresql"))
            (Pair
              (Scalar "data")
              (Mapping
                (Pair
                  (Scalar "provider")
                  (HelmExpr "print \"bitnami\" | b64enc | quote"))
                (Pair
                  (Scalar "type")
                  (HelmExpr "print \"postgresql\" | b64enc | quote"))
                (Pair
                  (Scalar "host")
                  (HelmExpr "$host | b64enc | quote"))
                (Pair
                  (Scalar "port")
                  (HelmExpr "$port | b64enc | quote"))
                (Pair
                  (Scalar "username")
                  (HelmExpr "print \"postgres\" | b64enc | quote"))
                (Pair
                  (Scalar "database")
                  (HelmExpr "print \"postgres\" | b64enc | quote"))
                (Pair
                  (Scalar "password")
                  (HelmExpr "$postgresPassword | b64enc | quote"))
                (Pair
                  (Scalar "uri")
                  (HelmExpr "printf \"postgresql://postgres:%s@%s:%s/postgres\" $postgresPassword $host $port | b64enc | quote")))))))
      (If "$password"
        (then
          (Mapping
            (Pair
              (Scalar "apiVersion")
              (Scalar "v1"))
            (Pair
              (Scalar "kind")
              (Scalar "Secret"))
            (Pair
              (Scalar "metadata")
              (Mapping
                (Pair
                  (Scalar "name")
                  (Scalar "{{include \"common.names.fullname\" . }}-svcbind-custom-user"))
                (Pair
                  (Scalar "namespace")
                  (HelmExpr ".Release.Namespace | quote"))
                (Pair
                  (Scalar "labels")
                  (HelmExpr "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")))))
          (If ".Values.commonAnnotations"
            (then
              (Mapping
                (Pair
                  (Scalar "annotations")
                  (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
          (Mapping
            (Pair
              (Scalar "type")
              (Scalar "servicebinding.io/postgresql"))
            (Pair
              (Scalar "data")
              (Mapping
                (Pair
                  (Scalar "provider")
                  (HelmExpr "print \"bitnami\" | b64enc | quote"))
                (Pair
                  (Scalar "type")
                  (HelmExpr "print \"postgresql\" | b64enc | quote"))
                (Pair
                  (Scalar "host")
                  (HelmExpr "$host | b64enc | quote"))
                (Pair
                  (Scalar "port")
                  (HelmExpr "$port | b64enc | quote"))
                (Pair
                  (Scalar "username")
                  (HelmExpr "$customUser | b64enc | quote"))
                (Pair
                  (Scalar "password")
                  (HelmExpr "$password | b64enc | quote")))))
          (If "$database"
            (then
              (Mapping
                (Pair
                  (Scalar "database")
                  (HelmExpr "$database | b64enc | quote")))))
          (Mapping
            (Pair
              (Scalar "uri")
              (HelmExpr "printf \"postgresql://%s:%s@%s:%s/%s\" $customUser $password $host $port $database | b64enc | quote"))))))))

"#;

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = TreeSitterParser.parse(&src).expect("parse");
    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }
    sim_assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
