use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml";

#[allow(clippy::too_many_lines)]
const EXPECTED_SEXPR_FUSED: &str = r#"(Document
  (Mapping
    (Pair
      (Scalar "apiVersion")
      (Scalar "v1"))
    (Pair
      (Scalar "kind")
      (Scalar "Service"))
    (Pair
      (Scalar "metadata")
      (Mapping
        (Pair
          (Scalar "name")
          (HelmExpr "template \"common.names.fullname\" ."))
        (Pair
          (Scalar "namespace")
          (HelmExpr "template \"zookeeper.namespace\" ."))
        (Pair
          (Scalar "labels")
          (Mapping
            (Pair
              (Scalar "app.kubernetes.io/component")
              (Scalar "zookeeper")))))))
  (If ".Values.commonLabels"
    (then
      (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonLabels \"context\" $ ) | nindent 4")))
  (If "or .Values.commonAnnotations .Values.service.annotations"
    (then
      (Mapping
        (Pair
          (Scalar "annotations")))
      (If ".Values.service.annotations"
        (then
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.service.annotations \"context\" $ ) | nindent 4")))
      (If ".Values.commonAnnotations"
        (then
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "type")
          (HelmExpr ".Values.service.type")))))
  (If "and .Values.service.clusterIP (eq .Values.service.type \"ClusterIP\")"
    (then
      (Mapping
        (Pair
          (Scalar "clusterIP")
          (HelmExpr ".Values.service.clusterIP")))))
  (If "or (eq .Values.service.type \"LoadBalancer\") (eq .Values.service.type \"NodePort\")"
    (then
      (Mapping
        (Pair
          (Scalar "externalTrafficPolicy")
          (HelmExpr ".Values.service.externalTrafficPolicy | quote")))))
  (If "and (eq .Values.service.type \"LoadBalancer\") (not (empty .Values.service.loadBalancerSourceRanges))"
    (then
      (Mapping
        (Pair
          (Scalar "loadBalancerSourceRanges")
          (HelmExpr ".Values.service.loadBalancerSourceRanges")))))
  (If "and (eq .Values.service.type \"LoadBalancer\") (not (empty .Values.service.loadBalancerIP))"
    (then
      (Mapping
        (Pair
          (Scalar "loadBalancerIP")
          (HelmExpr ".Values.service.loadBalancerIP")))))
  (If ".Values.service.sessionAffinity"
    (then
      (Mapping
        (Pair
          (Scalar "sessionAffinity")
          (HelmExpr ".Values.service.sessionAffinity")))))
  (If ".Values.service.sessionAffinityConfig"
    (then
      (Mapping
        (Pair
          (Scalar "sessionAffinityConfig")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.service.sessionAffinityConfig \"context\" $) | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "ports")))
  (If "not .Values.service.disableBaseClientPort"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "tcp-client"))
          (Pair
            (Scalar "port")
            (HelmExpr ".Values.service.ports.client"))
          (Pair
            (Scalar "targetPort")
            (Scalar "client"))))
      (If "and (or (eq .Values.service.type \"NodePort\") (eq .Values.service.type \"LoadBalancer\")) (not (empty .Values.service.nodePorts.client))"
        (then
          (Mapping
            (Pair
              (Scalar "nodePort")
              (HelmExpr ".Values.service.nodePorts.client"))))
        (else
          (If "eq .Values.service.type \"ClusterIP\""
            (then
              (Mapping
                (Pair
                  (Scalar "nodePort")))))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "tcp-client-tls"))
          (Pair
            (Scalar "port")
            (HelmExpr ".Values.service.ports.tls"))
          (Pair
            (Scalar "targetPort")
            (Scalar "client-tls"))))
      (If "and (or (eq .Values.service.type \"NodePort\") (eq .Values.service.type \"LoadBalancer\")) (not (empty .Values.service.nodePorts.tls))"
        (then
          (Mapping
            (Pair
              (Scalar "nodePort")
              (HelmExpr ".Values.service.nodePorts.tls"))))
        (else
          (If "eq .Values.service.type \"ClusterIP\""
            (then
              (Mapping
                (Pair
                  (Scalar "nodePort")))))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "tcp-follower"))
      (Pair
        (Scalar "port")
        (HelmExpr ".Values.service.ports.follower"))
      (Pair
        (Scalar "targetPort")
        (Scalar "follower")))
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "tcp-election"))
      (Pair
        (Scalar "port")
        (HelmExpr ".Values.service.ports.election"))
      (Pair
        (Scalar "targetPort")
        (Scalar "election"))))
  (If ".Values.service.extraPorts"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.service.extraPorts \"context\" $) | nindent 4")))
  (Mapping
    (Pair
      (Scalar "selector")
      (Mapping
        (Pair
          (Scalar "app.kubernetes.io/component")
          (Scalar "zookeeper"))))))
"#;

#[allow(clippy::too_many_lines)]
const EXPECTED_SEXPR_TREE_SITTER: &str = r#"(Document
  (Mapping
    (Pair
      (Scalar "apiVersion")
      (Scalar "v1"))
    (Pair
      (Scalar "kind")
      (Scalar "Service"))
    (Pair
      (Scalar "metadata")
      (Mapping
        (Pair
          (Scalar "name")
          (HelmExpr "template \"common.names.fullname\" ."))
        (Pair
          (Scalar "namespace")
          (HelmExpr "template \"zookeeper.namespace\" ."))
        (Pair
          (Scalar "labels")
          (HelmExpr "include \"common.labels.standard\" . | nindent 4")))))
  (If ".Values.commonLabels"
    (then
      (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonLabels \"context\" $ ) | nindent 4")))
  (If "or .Values.commonAnnotations .Values.service.annotations"
    (then
      (Mapping
        (Pair
          (Scalar "annotations")))
      (If ".Values.service.annotations"
        (then
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.service.annotations \"context\" $ ) | nindent 4")))
      (If ".Values.commonAnnotations"
        (then
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "type")
          (HelmExpr ".Values.service.type")))))
  (If "and .Values.service.clusterIP (eq .Values.service.type \"ClusterIP\")"
    (then
      (Mapping
        (Pair
          (Scalar "clusterIP")
          (HelmExpr ".Values.service.clusterIP")))))
  (If "or (eq .Values.service.type \"LoadBalancer\") (eq .Values.service.type \"NodePort\")"
    (then
      (Mapping
        (Pair
          (Scalar "externalTrafficPolicy")
          (HelmExpr ".Values.service.externalTrafficPolicy | quote")))))
  (If "and (eq .Values.service.type \"LoadBalancer\") (not (empty .Values.service.loadBalancerSourceRanges))"
    (then
      (Mapping
        (Pair
          (Scalar "loadBalancerSourceRanges")
          (HelmExpr ".Values.service.loadBalancerSourceRanges")))))
  (If "and (eq .Values.service.type \"LoadBalancer\") (not (empty .Values.service.loadBalancerIP))"
    (then
      (Mapping
        (Pair
          (Scalar "loadBalancerIP")
          (HelmExpr ".Values.service.loadBalancerIP")))))
  (If ".Values.service.sessionAffinity"
    (then
      (Mapping
        (Pair
          (Scalar "sessionAffinity")
          (HelmExpr ".Values.service.sessionAffinity")))))
  (If ".Values.service.sessionAffinityConfig"
    (then
      (Mapping
        (Pair
          (Scalar "sessionAffinityConfig")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.service.sessionAffinityConfig \"context\" $) | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "ports")))
  (If "not .Values.service.disableBaseClientPort"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "tcp-client"))
          (Pair
            (Scalar "port")
            (HelmExpr ".Values.service.ports.client"))
          (Pair
            (Scalar "targetPort")
            (Scalar "client"))))
      (If "and (or (eq .Values.service.type \"NodePort\") (eq .Values.service.type \"LoadBalancer\")) (not (empty .Values.service.nodePorts.client))"
        (then
          (Mapping
            (Pair
              (Scalar "nodePort")
              (HelmExpr ".Values.service.nodePorts.client"))))
        (else
          (If "eq .Values.service.type \"ClusterIP\""
            (then
              (Mapping
                (Pair
                  (Scalar "nodePort")
                  (Scalar "")))))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "tcp-client-tls"))
          (Pair
            (Scalar "port")
            (HelmExpr ".Values.service.ports.tls"))
          (Pair
            (Scalar "targetPort")
            (Scalar "client-tls"))))
      (If "and (or (eq .Values.service.type \"NodePort\") (eq .Values.service.type \"LoadBalancer\")) (not (empty .Values.service.nodePorts.tls))"
        (then
          (Mapping
            (Pair
              (Scalar "nodePort")
              (HelmExpr ".Values.service.nodePorts.tls"))))
        (else
          (If "eq .Values.service.type \"ClusterIP\""
            (then
              (Mapping
                (Pair
                  (Scalar "nodePort")
                  (Scalar "")))))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "tcp-follower"))
      (Pair
        (Scalar "port")
        (HelmExpr ".Values.service.ports.follower"))
      (Pair
        (Scalar "targetPort")
        (Scalar "follower")))
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "tcp-election"))
      (Pair
        (Scalar "port")
        (HelmExpr ".Values.service.ports.election"))
      (Pair
        (Scalar "targetPort")
        (Scalar "election"))))
  (If ".Values.service.extraPorts"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.service.extraPorts \"context\" $) | nindent 4")))
  (Mapping
    (Pair
      (Scalar "selector")
      (HelmExpr "include \"common.labels.matchLabels\" . | nindent 4"))))
"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR_FUSED.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(
        have: ast.to_sexpr(),
        want: EXPECTED_SEXPR_TREE_SITTER.trim_end()
    );
}
