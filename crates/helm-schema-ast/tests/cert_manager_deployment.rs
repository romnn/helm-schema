use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (Mapping
    (Pair
      (Scalar "apiVersion")
      (Scalar "apps/v1"))
    (Pair
      (Scalar "kind")
      (Scalar "Deployment"))
    (Pair
      (Scalar "metadata")
      (Mapping
        (Pair
          (Scalar "name")
          (HelmExpr "template \"cert-manager.fullname\" ."))
        (Pair
          (Scalar "namespace")
          (HelmExpr "include \"cert-manager.namespace\" ."))
        (Pair
          (Scalar "labels")
          (Mapping
            (Pair
              (Scalar "app")
              (HelmExpr "template \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/name")
              (HelmExpr "template \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/instance")
              (HelmExpr ".Release.Name"))
            (Pair
              (Scalar "app.kubernetes.io/component")
              (Scalar "controller")))))))
  (With ".Values.deploymentAnnotations"
    (body
      (Mapping
        (Pair
          (Scalar "annotations")
          (HelmExpr "toYaml . | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "replicas")
          (HelmExpr ".Values.replicaCount")))))
  (HelmComment "/* The if statement below is equivalent to {{- if $value }} but will also return true for 0. */")
  (If "not (has (quote .Values.global.revisionHistoryLimit) (list \"\" (quote \"\")))"
    (then
      (Mapping
        (Pair
          (Scalar "revisionHistoryLimit")
          (HelmExpr ".Values.global.revisionHistoryLimit")))))
  (Mapping
    (Pair
      (Scalar "selector")
      (Mapping
        (Pair
          (Scalar "matchLabels")
          (Mapping
            (Pair
              (Scalar "app.kubernetes.io/name")
              (HelmExpr "template \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/instance")
              (HelmExpr ".Release.Name"))
            (Pair
              (Scalar "app.kubernetes.io/component")
              (Scalar "controller")))))))
  (With ".Values.strategy"
    (body
      (Mapping
        (Pair
          (Scalar "strategy")
          (HelmExpr "toYaml . | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "template")
      (Mapping
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "labels")
              (Mapping
                (Pair
                  (Scalar "app")
                  (HelmExpr "template \"cert-manager.name\" ."))
                (Pair
                  (Scalar "app.kubernetes.io/name")
                  (HelmExpr "template \"cert-manager.name\" ."))
                (Pair
                  (Scalar "app.kubernetes.io/instance")
                  (HelmExpr ".Release.Name"))
                (Pair
                  (Scalar "app.kubernetes.io/component")
                  (Scalar "controller")))))))))
  (With ".Values.podLabels"
    (body
      (HelmExpr "toYaml . | nindent 8")))
  (With ".Values.podAnnotations"
    (body
      (Mapping
        (Pair
          (Scalar "annotations")
          (HelmExpr "toYaml . | nindent 8")))))
  (If "and .Values.prometheus.enabled (not (or .Values.prometheus.servicemonitor.enabled .Values.prometheus.podmonitor.enabled))"
    (then
      (If "not .Values.podAnnotations"
        (then
          (Mapping
            (Pair
              (Scalar "annotations")))))
      (Mapping
        (Pair
          (Scalar "prometheus.io/path")
          (Scalar "/metrics"))
        (Pair
          (Scalar "prometheus.io/scrape")
          (Scalar "true"))
        (Pair
          (Scalar "prometheus.io/port")
          (Scalar "9402")))))
  (Mapping
    (Pair
      (Scalar "spec")))
  (If "not .Values.serviceAccount.create"
    (then
      (With ".Values.global.imagePullSecrets"
        (body
          (Mapping
            (Pair
              (Scalar "imagePullSecrets")
              (HelmExpr "toYaml . | nindent 8")))))))
  (Mapping
    (Pair
      (Scalar "serviceAccountName")
      (HelmExpr "template \"cert-manager.serviceAccountName\" .")))
  (If "hasKey .Values \"automountServiceAccountToken\""
    (then
      (Mapping
        (Pair
          (Scalar "automountServiceAccountToken")
          (HelmExpr ".Values.automountServiceAccountToken")))))
  (Mapping
    (Pair
      (Scalar "enableServiceLinks")
      (HelmExpr ".Values.enableServiceLinks")))
  (With ".Values.global.priorityClassName"
    (body
      (Mapping
        (Pair
          (Scalar "priorityClassName")
          (HelmExpr ". | quote")))))
  (If "(hasKey .Values.global \"hostUsers\")"
    (then
      (Mapping
        (Pair
          (Scalar "hostUsers")
          (HelmExpr ".Values.global.hostUsers")))))
  (With ".Values.securityContext"
    (body
      (Mapping
        (Pair
          (Scalar "securityContext")
          (HelmExpr "toYaml . | nindent 8")))))
  (If "or .Values.volumes .Values.config"
    (then
      (Mapping
        (Pair
          (Scalar "volumes")))
      (If ".Values.config"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "config"))
              (Pair
                (Scalar "configMap")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"cert-manager.fullname\" ."))))))))
      (With ".Values.volumes"
        (body
          (HelmExpr "toYaml . | nindent 8")))))
  (Mapping
    (Pair
      (Scalar "containers")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "{{.Chart.Name}}-controller"))
          (Pair
            (Scalar "image")
            (Scalar "{{ template \"image\" (tuple .Values.image $.Chart.AppVersion) }}"))
          (Pair
            (Scalar "imagePullPolicy")
            (HelmExpr ".Values.image.pullPolicy"))
          (Pair
            (Scalar "args"))))))
  (HelmComment "/* The if statement below is equivalent to {{- if $value }} but will also return true for 0. */")
  (If "not (has (quote .Values.global.logLevel) (list \"\" (quote \"\")))"
    (then
      (Sequence
        (Scalar "--v={{.Values.global.logLevel}}"))))
  (If ".Values.config"
    (then
      (Sequence
        (Scalar "--config=/var/cert-manager/config/config.yaml"))))
  (HelmExpr "$config := default .Values.config \"\"")
  (If ".Values.clusterResourceNamespace"
    (then
      (Sequence
        (Scalar "--cluster-resource-namespace={{.Values.clusterResourceNamespace}}")))
    (else
      (Sequence
        (Scalar "--cluster-resource-namespace=$(POD_NAMESPACE)"))))
  (With ".Values.global.leaderElection"
    (body
      (Sequence
        (Scalar "--leader-election-namespace={{.namespace}}"))
      (If ".leaseDuration"
        (then
          (Sequence
            (Scalar "--leader-election-lease-duration={{.leaseDuration}}"))))
      (If ".renewDeadline"
        (then
          (Sequence
            (Scalar "--leader-election-renew-deadline={{.renewDeadline}}"))))
      (If ".retryPeriod"
        (then
          (Sequence
            (Scalar "--leader-election-retry-period={{.retryPeriod}}"))))))
  (With ".Values.acmesolver.image"
    (body
      (Sequence
        (Scalar "--acme-http01-solver-image="))
      (If ".registry"
        (then
          (Scalar "{{.registry}}/")))
      (HelmExpr ".repository")
      (If "(.digest)"
        (else
          (Scalar ":{{default $.Chart.AppVersion .tag }}")))))
  (With ".Values.extraArgs"
    (body
      (HelmExpr "toYaml . | nindent 10")))
  (With ".Values.ingressShim"
    (body
      (If ".defaultIssuerName"
        (then
          (Sequence
            (Scalar "--default-issuer-name={{.defaultIssuerName}}"))))
      (If ".defaultIssuerKind"
        (then
          (Sequence
            (Scalar "--default-issuer-kind={{.defaultIssuerKind}}"))))
      (If ".defaultIssuerGroup"
        (then
          (Sequence
            (Scalar "--default-issuer-group={{.defaultIssuerGroup}}"))))))
  (If ".Values.featureGates"
    (then
      (Sequence
        (Scalar "--feature-gates={{.Values.featureGates}}"))))
  (If ".Values.maxConcurrentChallenges"
    (then
      (Sequence
        (Scalar "--max-concurrent-challenges={{.Values.maxConcurrentChallenges}}"))))
  (If ".Values.enableCertificateOwnerRef"
    (then
      (Sequence
        (Scalar "--enable-certificate-owner-ref=true"))))
  (If ".Values.dns01RecursiveNameserversOnly"
    (then
      (Sequence
        (Scalar "--dns01-recursive-nameservers-only=true"))))
  (With ".Values.dns01RecursiveNameservers"
    (body
      (Sequence
        (Scalar "--dns01-recursive-nameservers={{.}}"))))
  (If ".Values.disableAutoApproval"
    (then
      (Sequence
        (Scalar "--controllers=-certificaterequests-approver"))))
  (Mapping
    (Pair
      (Scalar "ports")
      (Sequence
        (Mapping
          (Pair
            (Scalar "containerPort")
            (Scalar "9402"))
          (Pair
            (Scalar "name")
            (Scalar "http-metrics"))
          (Pair
            (Scalar "protocol")
            (Scalar "TCP")))
        (Mapping
          (Pair
            (Scalar "containerPort")
            (Scalar "9403"))
          (Pair
            (Scalar "name")
            (Scalar "http-healthz"))
          (Pair
            (Scalar "protocol")
            (Scalar "TCP"))))))
  (With ".Values.containerSecurityContext"
    (body
      (Mapping
        (Pair
          (Scalar "securityContext")
          (HelmExpr "toYaml . | nindent 12")))))
  (If "or .Values.config .Values.volumeMounts"
    (then
      (Mapping
        (Pair
          (Scalar "volumeMounts")))
      (If ".Values.config"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "config"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/var/cert-manager/config"))))))
      (With ".Values.volumeMounts"
        (body
          (HelmExpr "toYaml . | nindent 12")))))
  (Mapping
    (Pair
      (Scalar "env")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "POD_NAMESPACE"))
          (Pair
            (Scalar "valueFrom")
            (Mapping
              (Pair
                (Scalar "fieldRef")
                (Mapping
                  (Pair
                    (Scalar "fieldPath")
                    (Scalar "metadata.namespace"))))))))))
  (With ".Values.extraEnv"
    (body
      (HelmExpr "toYaml . | nindent 10")))
  (With ".Values.http_proxy"
    (body
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "HTTP_PROXY"))
          (Pair
            (Scalar "value")
            (HelmExpr "."))))))
  (With ".Values.https_proxy"
    (body
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "HTTPS_PROXY"))
          (Pair
            (Scalar "value")
            (HelmExpr "."))))))
  (With ".Values.no_proxy"
    (body
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "NO_PROXY"))
          (Pair
            (Scalar "value")
            (HelmExpr "."))))))
  (With ".Values.resources"
    (body
      (Mapping
        (Pair
          (Scalar "resources")
          (HelmExpr "toYaml . | nindent 12")))))
  (With ".Values.livenessProbe"
    (body
      (If ".enabled"
        (then
          (Mapping
            (Pair
              (Scalar "livenessProbe")
              (Mapping
                (Pair
                  (Scalar "httpGet")
                  (Mapping
                    (Pair
                      (Scalar "port")
                      (Scalar "http-healthz"))
                    (Pair
                      (Scalar "path")
                      (Scalar "/livez"))
                    (Pair
                      (Scalar "scheme")
                      (Scalar "HTTP"))))
                (Pair
                  (Scalar "initialDelaySeconds")
                  (HelmExpr ".initialDelaySeconds"))
                (Pair
                  (Scalar "periodSeconds")
                  (HelmExpr ".periodSeconds"))
                (Pair
                  (Scalar "timeoutSeconds")
                  (HelmExpr ".timeoutSeconds"))
                (Pair
                  (Scalar "successThreshold")
                  (HelmExpr ".successThreshold"))
                (Pair
                  (Scalar "failureThreshold")
                  (HelmExpr ".failureThreshold")))))))))
  (HelmExpr "$nodeSelector := .Values.global.nodeSelector | default dict")
  (With "$nodeSelector"
    (body
      (Mapping
        (Pair
          (Scalar "nodeSelector")))
      (Range "$key, $value := ."
        (body
          (Mapping
            (Pair
              (HelmExpr "$key")
              (HelmExpr "$value | quote")))))))
  (With ".Values.affinity"
    (body
      (Mapping
        (Pair
          (Scalar "affinity")
          (HelmExpr "toYaml . | nindent 8")))))
  (With ".Values.tolerations"
    (body
      (Mapping
        (Pair
          (Scalar "tolerations")
          (HelmExpr "toYaml . | nindent 8")))))
  (With ".Values.topologySpreadConstraints"
    (body
      (Mapping
        (Pair
          (Scalar "topologySpreadConstraints")
          (HelmExpr "toYaml . | nindent 8")))))
  (With ".Values.podDnsPolicy"
    (body
      (Mapping
        (Pair
          (Scalar "dnsPolicy")
          (HelmExpr ".")))))
  (With ".Values.podDnsConfig"
    (body
      (Mapping
        (Pair
          (Scalar "dnsConfig")
          (HelmExpr "toYaml . | nindent 8")))))
  (With ".Values.hostAliases"
    (body
      (Mapping
        (Pair
          (Scalar "hostAliases")
          (HelmExpr "toYaml . | nindent 8"))))))
"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
