use helm_schema_ast::{FusedRustParser, HelmParser};

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml";

#[allow(clippy::too_many_lines)]
const EXPECTED_SEXPR: &str = r#"(Document
  (Mapping
    (Pair
      (Scalar "apiVersion")
      (HelmExpr "include \"common.capabilities.statefulset.apiVersion\" ."))
    (Pair
      (Scalar "kind")
      (Scalar "StatefulSet"))
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
              (Scalar "zookeeper"))
            (Pair
              (Scalar "role")
              (Scalar "zookeeper")))))))
  (If ".Values.commonLabels"
    (then
      (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonLabels \"context\" $ ) | nindent 4")))
  (If ".Values.commonAnnotations"
    (then
      (Mapping
        (Pair
          (Scalar "annotations")
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "replicas")
          (HelmExpr ".Values.replicaCount"))
        (Pair
          (Scalar "podManagementPolicy")
          (HelmExpr ".Values.podManagementPolicy"))
        (Pair
          (Scalar "selector")
          (Mapping
            (Pair
              (Scalar "matchLabels")
              (Mapping
                (Pair
                  (Scalar "app.kubernetes.io/component")
                  (Scalar "zookeeper"))))))
        (Pair
          (Scalar "serviceName")
          (HelmExpr "printf \"%s-%s\" (include \"common.names.fullname\" .) (default \"headless\" .Values.service.headless.servicenameOverride) | trunc 63 | trimSuffix \"-\"")))))
  (If ".Values.updateStrategy"
    (then
      (Mapping
        (Pair
          (Scalar "updateStrategy")
          (HelmExpr "toYaml .Values.updateStrategy | nindent 4")))))
  (Mapping
    (Pair
      (Scalar "template")
      (Mapping
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "annotations")))))))
  (If ".Values.podAnnotations"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.podAnnotations \"context\" $) | nindent 8")))
  (If "(include \"zookeeper.createConfigmap\" .)"
    (then
      (Mapping
        (Pair
          (Scalar "checksum/configuration")
          (HelmExpr "include (print $.Template.BasePath \"/configmap.yaml\") . | sha256sum")))))
  (If "or (include \"zookeeper.quorum.createSecret\" .) (include \"zookeeper.client.createSecret\" .) (include \"zookeeper.client.createTlsPasswordsSecret\" .) (include \"zookeeper.quorum.createTlsPasswordsSecret\" .)"
    (then
      (Mapping
        (Pair
          (Scalar "checksum/secrets")
          (HelmExpr "include (print $.Template.BasePath \"/secrets.yaml\") . | sha256sum")))))
  (If "or (include \"zookeeper.client.createTlsSecret\" .) (include \"zookeeper.quorum.createTlsSecret\" .)"
    (then
      (Mapping
        (Pair
          (Scalar "checksum/tls-secrets")
          (HelmExpr "include (print $.Template.BasePath \"/tls-secrets.yaml\") . | sha256sum")))))
  (Mapping
    (Pair
      (Scalar "labels")
      (Mapping
        (Pair
          (Scalar "app.kubernetes.io/component")
          (Scalar "zookeeper")))))
  (If ".Values.podLabels"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.podLabels \"context\" $) | nindent 8")))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "serviceAccountName")
          (HelmExpr "template \"zookeeper.serviceAccountName\" .")))))
  (If ".Values.hostAliases"
    (then
      (Mapping
        (Pair
          (Scalar "hostAliases")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.hostAliases \"context\" $) | nindent 8")))))
  (If ".Values.affinity"
    (then
      (Mapping
        (Pair
          (Scalar "affinity")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.affinity \"context\" $) | nindent 8"))))
    (else
      (Mapping
        (Pair
          (Scalar "affinity")
          (Mapping
            (Pair
              (Scalar "podAffinity")
              (HelmExpr "include \"common.affinities.pods\" (dict \"type\" .Values.podAffinityPreset \"component\" \"zookeeper\" \"context\" $) | nindent 10"))
            (Pair
              (Scalar "podAntiAffinity")
              (HelmExpr "include \"common.affinities.pods\" (dict \"type\" .Values.podAntiAffinityPreset \"component\" \"zookeeper\" \"context\" $) | nindent 10"))
            (Pair
              (Scalar "nodeAffinity")
              (HelmExpr "include \"common.affinities.nodes\" (dict \"type\" .Values.nodeAffinityPreset.type \"key\" .Values.nodeAffinityPreset.key \"values\" .Values.nodeAffinityPreset.values) | nindent 10")))))))
  (If ".Values.nodeSelector"
    (then
      (Mapping
        (Pair
          (Scalar "nodeSelector")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.nodeSelector \"context\" $) | nindent 8")))))
  (If ".Values.tolerations"
    (then
      (Mapping
        (Pair
          (Scalar "tolerations")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.tolerations \"context\" $) | nindent 8")))))
  (If ".Values.topologySpreadConstraints"
    (then
      (Mapping
        (Pair
          (Scalar "topologySpreadConstraints")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.topologySpreadConstraints \"context\" .) | nindent 8")))))
  (If ".Values.priorityClassName"
    (then
      (Mapping
        (Pair
          (Scalar "priorityClassName")
          (HelmExpr ".Values.priorityClassName")))))
  (If ".Values.schedulerName"
    (then
      (Mapping
        (Pair
          (Scalar "schedulerName")
          (HelmExpr ".Values.schedulerName")))))
  (If ".Values.podSecurityContext.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "securityContext")
          (HelmExpr "omit .Values.podSecurityContext \"enabled\" | toYaml | nindent 8")))))
  (Mapping
    (Pair
      (Scalar "initContainers")))
  (If "and .Values.volumePermissions.enabled .Values.persistence.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "volume-permissions"))
          (Pair
            (Scalar "image")
            (HelmExpr "template \"zookeeper.volumePermissions.image\" ."))
          (Pair
            (Scalar "imagePullPolicy")
            (HelmExpr "default \"\" .Values.volumePermissions.image.pullPolicy | quote"))
          (Pair
            (Scalar "command")
            (Sequence
              (Scalar "/bin/bash")))
          (Pair
            (Scalar "args")
            (Sequence
              (Scalar "-ec")
              (Scalar "mkdir -p /bitnami/zookeeper\nchown -R {{.Values.containerSecurityContext.runAsUser}}:{{.Values.podSecurityContext.fsGroup}} /bitnami/zookeeper\nfind /bitnami/zookeeper -mindepth 1 -maxdepth 1 -not -name \".snapshot\" -not -name \"lost+found\" | xargs -r chown -R {{.Values.containerSecurityContext.runAsUser}}:{{.Values.podSecurityContext.fsGroup}}\n")))))
      (If ".Values.dataLogDir"
        (then
          (Scalar "mkdir -p {{.Values.dataLogDir}} chown -R {{.Values.containerSecurityContext.runAsUser}}:{{.Values.podSecurityContext.fsGroup}} {{.Values.dataLogDir}} find {{.Values.dataLogDir}} -mindepth 1 -maxdepth 1 -not -name \".snapshot\" -not -name \"lost+found\" | xargs -r chown -R {{.Values.containerSecurityContext.runAsUser}}:{{.Values.podSecurityContext.fsGroup}}")))
      (If ".Values.volumePermissions.containerSecurityContext.enabled"
        (then
          (Mapping
            (Pair
              (Scalar "securityContext")
              (HelmExpr "omit .Values.volumePermissions.containerSecurityContext \"enabled\" | toYaml | nindent 12")))))
      (If ".Values.volumePermissions.resources"
        (then
          (Mapping
            (Pair
              (Scalar "resources")
              (HelmExpr "toYaml .Values.volumePermissions.resources | nindent 12")))))
      (Mapping
        (Pair
          (Scalar "volumeMounts")
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "data"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/bitnami/zookeeper"))))))
      (If ".Values.dataLogDir"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "data-log"))
              (Pair
                (Scalar "mountPath")
                (HelmExpr ".Values.dataLogDir"))))))))
  (If "or .Values.tls.client.enabled .Values.tls.quorum.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "init-certs"))
          (Pair
            (Scalar "image")
            (HelmExpr "include \"zookeeper.image\" ."))
          (Pair
            (Scalar "imagePullPolicy")
            (HelmExpr ".Values.image.pullPolicy | quote"))))
      (If ".Values.containerSecurityContext.enabled"
        (then
          (Mapping
            (Pair
              (Scalar "securityContext")
              (HelmExpr "omit .Values.containerSecurityContext \"enabled\" | toYaml | nindent 12")))))
      (Mapping
        (Pair
          (Scalar "command")
          (Sequence
            (Scalar "/scripts/init-certs.sh")))
        (Pair
          (Scalar "env")
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "MY_POD_NAME"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "fieldRef")
                    (Mapping
                      (Pair
                        (Scalar "apiVersion")
                        (Scalar "v1"))
                      (Pair
                        (Scalar "fieldPath")
                        (Scalar "metadata.name"))))))))))
      (If "or .Values.tls.client.passwordsSecretName (include \"zookeeper.client.createTlsPasswordsSecret\" .)"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_CLIENT_KEYSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordKeystoreKey\" .")))))))
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_CLIENT_TRUSTSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordTruststoreKey\" ."))))))))))
      (If "or .Values.tls.quorum.passwordsSecretName (include \"zookeeper.quorum.createTlsPasswordsSecret\" .)"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_QUORUM_KEYSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordKeystoreKey\" .")))))))
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_QUORUM_TRUSTSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordTruststoreKey\" ."))))))))))
      (If ".Values.tls.resources"
        (then
          (Mapping
            (Pair
              (Scalar "resources")
              (HelmExpr "toYaml .Values.tls.resources | nindent 12")))))
      (Mapping
        (Pair
          (Scalar "volumeMounts")
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "scripts"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/scripts/init-certs.sh"))
              (Pair
                (Scalar "subPath")
                (Scalar "init-certs.sh"))))))
      (If "or .Values.tls.client.enabled"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "client-certificates"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/certs/client")))
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "client-shared-certs"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/opt/bitnami/zookeeper/config/certs/client"))))))
      (If "or .Values.tls.quorum.enabled"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "quorum-certificates"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/certs/quorum")))
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "quorum-shared-certs"))
              (Pair
                (Scalar "mountPath")
                (Scalar "/opt/bitnami/zookeeper/config/certs/quorum"))))))))
  (If ".Values.initContainers"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.initContainers \"context\" $) | trim | nindent 8")))
  (Mapping
    (Pair
      (Scalar "containers")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "zookeeper"))
          (Pair
            (Scalar "image")
            (HelmExpr "template \"zookeeper.image\" ."))
          (Pair
            (Scalar "imagePullPolicy")
            (HelmExpr ".Values.image.pullPolicy | quote"))))))
  (If ".Values.containerSecurityContext.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "securityContext")
          (HelmExpr "omit .Values.containerSecurityContext \"enabled\" | toYaml | nindent 12")))))
  (If ".Values.diagnosticMode.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "command")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.diagnosticMode.command \"context\" $) | nindent 12"))))
    (else
      (If ".Values.command"
        (then
          (Mapping
            (Pair
              (Scalar "command")
              (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.command \"context\" $) | nindent 12")))))))
  (If ".Values.diagnosticMode.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "args")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.diagnosticMode.args \"context\" $) | nindent 12"))))
    (else
      (If ".Values.args"
        (then
          (Mapping
            (Pair
              (Scalar "args")
              (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.args \"context\" $) | nindent 12")))))))
  (If ".Values.resources"
    (then
      (Mapping
        (Pair
          (Scalar "resources")
          (HelmExpr "toYaml .Values.resources | nindent 12")))))
  (Mapping
    (Pair
      (Scalar "env")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "BITNAMI_DEBUG"))
          (Pair
            (Scalar "value")
            (HelmExpr "ternary \"true\" \"false\" (or .Values.image.debug .Values.diagnosticMode.enabled) | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_DATA_LOG_DIR"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.dataLogDir | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_PORT_NUMBER"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.containerPorts.client | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TICK_TIME"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tickTime | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_INIT_LIMIT"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.initLimit | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_SYNC_LIMIT"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.syncLimit | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_PRE_ALLOC_SIZE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.preAllocSize | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_SNAPCOUNT"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.snapCount | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_MAX_CLIENT_CNXNS"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.maxClientCnxns | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_4LW_COMMANDS_WHITELIST"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.fourlwCommandsWhitelist | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_LISTEN_ALLIPS_ENABLED"))
          (Pair
            (Scalar "value")
            (HelmExpr "ternary \"yes\" \"no\" .Values.listenOnAllIPs | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_AUTOPURGE_INTERVAL"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.autopurge.purgeInterval | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_AUTOPURGE_RETAIN_COUNT"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.autopurge.snapRetainCount | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_MAX_SESSION_TIMEOUT"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.maxSessionTimeout | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_SERVERS"))))))
  (HelmExpr "$replicaCount := int .Values.replicaCount")
  (HelmExpr "$minServerId := int .Values.minServerId")
  (HelmExpr "$followerPort := int .Values.containerPorts.follower")
  (HelmExpr "$electionPort := int .Values.containerPorts.election")
  (HelmExpr "$releaseNamespace := include \"zookeeper.namespace\" .")
  (HelmExpr "$zookeeperFullname := include \"common.names.fullname\" .")
  (HelmExpr "$zookeeperHeadlessServiceName := printf \"%s-%s\" $zookeeperFullname \"headless\" | trunc 63")
  (HelmExpr "$clusterDomain := .Values.clusterDomain")
  (Mapping
    (Pair
      (Scalar "value")))
  (Range "$i, $e := until $replicaCount"
    (body
      (Scalar "{{$zookeeperFullname}}-{{$e}}.{{$zookeeperHeadlessServiceName}}.{{$releaseNamespace}}.svc.{{$clusterDomain}}:{{$followerPort}}:{{$electionPort}}::{{ add $e $minServerId }}")))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "ZOO_ENABLE_AUTH"))
      (Pair
        (Scalar "value")
        (HelmExpr "ternary \"yes\" \"no\" .Values.auth.client.enabled | quote"))))
  (If ".Values.auth.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_CLIENT_USER"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.auth.client.clientUser | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_CLIENT_PASSWORD"))
          (Pair
            (Scalar "valueFrom")
            (Mapping
              (Pair
                (Scalar "secretKeyRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"zookeeper.client.secretName\" ."))
                  (Pair
                    (Scalar "key")
                    (Scalar "client-password")))))))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_SERVER_USERS"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.auth.client.serverUsers | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_SERVER_PASSWORDS"))
          (Pair
            (Scalar "valueFrom")
            (Mapping
              (Pair
                (Scalar "secretKeyRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"zookeeper.client.secretName\" ."))
                  (Pair
                    (Scalar "key")
                    (Scalar "server-password"))))))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "ZOO_ENABLE_QUORUM_AUTH"))
      (Pair
        (Scalar "value")
        (HelmExpr "ternary \"yes\" \"no\" .Values.auth.quorum.enabled | quote"))))
  (If ".Values.auth.quorum.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_QUORUM_LEARNER_USER"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.auth.quorum.learnerUser | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_QUORUM_LEARNER_PASSWORD"))
          (Pair
            (Scalar "valueFrom")
            (Mapping
              (Pair
                (Scalar "secretKeyRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"zookeeper.quorum.secretName\" ."))
                  (Pair
                    (Scalar "key")
                    (Scalar "quorum-learner-password")))))))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_QUORUM_SERVER_USERS"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.auth.quorum.serverUsers | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_QUORUM_SERVER_PASSWORDS"))
          (Pair
            (Scalar "valueFrom")
            (Mapping
              (Pair
                (Scalar "secretKeyRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"zookeeper.quorum.secretName\" ."))
                  (Pair
                    (Scalar "key")
                    (Scalar "quorum-server-password"))))))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "ZOO_HEAP_SIZE"))
      (Pair
        (Scalar "value")
        (HelmExpr ".Values.heapSize | quote")))
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "ZOO_LOG_LEVEL"))
      (Pair
        (Scalar "value")
        (HelmExpr ".Values.logLevel | quote")))
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "ALLOW_ANONYMOUS_LOGIN"))
      (Pair
        (Scalar "value")
        (HelmExpr "ternary \"no\" \"yes\" .Values.auth.client.enabled | quote"))))
  (If ".Values.jvmFlags"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "JVMFLAGS"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.jvmFlags | quote"))))))
  (If ".Values.metrics.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_ENABLE_PROMETHEUS_METRICS"))
          (Pair
            (Scalar "value")
            (Scalar "yes")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_PROMETHEUS_METRICS_PORT_NUMBER"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.metrics.containerPort | quote"))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_PORT_NUMBER"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.containerPorts.tls | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_CLIENT_ENABLE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.client.enabled | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_CLIENT_AUTH"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.client.auth | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_CLIENT_KEYSTORE_FILE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.client.keystorePath | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_CLIENT_TRUSTSTORE_FILE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.client.truststorePath | quote"))))
      (If "or .Values.tls.client.keystorePassword .Values.tls.client.passwordsSecretName .Values.tls.client.autoGenerated"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_CLIENT_KEYSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordKeystoreKey\" ."))))))))))
      (If "or .Values.tls.client.truststorePassword .Values.tls.client.passwordsSecretName .Values.tls.client.autoGenerated"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_CLIENT_TRUSTSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.client.tlsPasswordTruststoreKey\" ."))))))))))))
  (If ".Values.tls.quorum.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_QUORUM_ENABLE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.quorum.enabled | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_QUORUM_CLIENT_AUTH"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.quorum.auth | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_QUORUM_KEYSTORE_FILE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.quorum.keystorePath | quote")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ZOO_TLS_QUORUM_TRUSTSTORE_FILE"))
          (Pair
            (Scalar "value")
            (HelmExpr ".Values.tls.quorum.truststorePath | quote"))))
      (If "or .Values.tls.quorum.keystorePassword .Values.tls.quorum.passwordsSecretName .Values.tls.quorum.autoGenerated"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_QUORUM_KEYSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordKeystoreKey\" ."))))))))))
      (If "or .Values.tls.quorum.truststorePassword .Values.tls.quorum.passwordsSecretName .Values.tls.quorum.autoGenerated"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "ZOO_TLS_QUORUM_TRUSTSTORE_PASSWORD"))
              (Pair
                (Scalar "valueFrom")
                (Mapping
                  (Pair
                    (Scalar "secretKeyRef")
                    (Mapping
                      (Pair
                        (Scalar "name")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordsSecret\" ."))
                      (Pair
                        (Scalar "key")
                        (HelmExpr "include \"zookeeper.quorum.tlsPasswordTruststoreKey\" ."))))))))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "POD_NAME"))
      (Pair
        (Scalar "valueFrom")
        (Mapping
          (Pair
            (Scalar "fieldRef")
            (Mapping
              (Pair
                (Scalar "apiVersion")
                (Scalar "v1"))
              (Pair
                (Scalar "fieldPath")
                (Scalar "metadata.name"))))))))
  (If ".Values.extraEnvVars"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.extraEnvVars \"context\" $) | nindent 12")))
  (If "or .Values.extraEnvVarsCM .Values.extraEnvVarsSecret"
    (then
      (Mapping
        (Pair
          (Scalar "envFrom")))
      (If ".Values.extraEnvVarsCM"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "configMapRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.extraEnvVarsCM \"context\" $)"))))))))
      (If ".Values.extraEnvVarsSecret"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "secretRef")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.extraEnvVarsSecret \"context\" $)"))))))))))
  (Mapping
    (Pair
      (Scalar "ports")))
  (If "not .Values.service.disableBaseClientPort"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "client"))
          (Pair
            (Scalar "containerPort")
            (HelmExpr ".Values.containerPorts.client"))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "client-tls"))
          (Pair
            (Scalar "containerPort")
            (HelmExpr ".Values.containerPorts.tls"))))))
  (Sequence
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "follower"))
      (Pair
        (Scalar "containerPort")
        (HelmExpr ".Values.containerPorts.follower")))
    (Mapping
      (Pair
        (Scalar "name")
        (Scalar "election"))
      (Pair
        (Scalar "containerPort")
        (HelmExpr ".Values.containerPorts.election"))))
  (If ".Values.metrics.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "metrics"))
          (Pair
            (Scalar "containerPort")
            (HelmExpr ".Values.metrics.containerPort"))))))
  (If "not .Values.diagnosticMode.enabled"
    (then
      (If ".Values.customLivenessProbe"
        (then
          (Mapping
            (Pair
              (Scalar "livenessProbe")
              (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.customLivenessProbe \"context\" $) | nindent 12"))))
        (else
          (If ".Values.livenessProbe.enabled"
            (then
              (Mapping
                (Pair
                  (Scalar "livenessProbe")
                  (Mapping
                    (Pair
                      (Scalar "exec")))))
              (If "not .Values.service.disableBaseClientPort"
                (then
                  (Mapping
                    (Pair
                      (Scalar "command")
                      (Sequence
                        (Scalar "/bin/bash")
                        (Scalar "-c")
                        (Scalar "echo \"ruok\" | timeout {{.Values.livenessProbe.probeCommandTimeout}} nc -w {{.Values.livenessProbe.probeCommandTimeout}} localhost {{.Values.containerPorts.client}} | grep imok")))))
                (else
                  (If "not .Values.tls.client.enabled"
                    (then
                      (Mapping
                        (Pair
                          (Scalar "command")
                          (Sequence
                            (Scalar "/bin/bash")
                            (Scalar "-c")
                            (Scalar "echo \"ruok\" | timeout {{.Values.livenessProbe.probeCommandTimeout}} openssl s_client -quiet -crlf -connect localhost:{{.Values.containerPorts.tls}} | grep imok")))))
                    (else
                      (Mapping
                        (Pair
                          (Scalar "command")
                          (Sequence
                            (Scalar "/bin/bash")
                            (Scalar "-c")
                            (Scalar "echo \"ruok\" | timeout {{.Values.livenessProbe.probeCommandTimeout}} openssl s_client -quiet -crlf -connect localhost:{{.Values.containerPorts.tls}} -cert {{.Values.service.tls.client_cert_pem_path}} -key {{.Values.service.tls.client_key_pem_path}} | grep imok"))))))))))))
      (If ".Values.customReadinessProbe"
        (then
          (Mapping
            (Pair
              (Scalar "readinessProbe")
              (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.customReadinessProbe \"context\" $) | nindent 12"))))
        (else
          (If ".Values.readinessProbe.enabled"
            (then
              (Mapping
                (Pair
                  (Scalar "readinessProbe")
                  (Mapping
                    (Pair
                      (Scalar "exec")))))
              (If "not .Values.service.disableBaseClientPort"
                (then
                  (Mapping
                    (Pair
                      (Scalar "command")
                      (Sequence
                        (Scalar "/bin/bash")
                        (Scalar "-c")
                        (Scalar "echo \"ruok\" | timeout {{.Values.readinessProbe.probeCommandTimeout}} nc -w {{.Values.readinessProbe.probeCommandTimeout}} localhost {{.Values.containerPorts.client}} | grep imok")))))
                (else
                  (If "not .Values.tls.client.enabled"
                    (then
                      (Mapping
                        (Pair
                          (Scalar "command")
                          (Sequence
                            (Scalar "/bin/bash")
                            (Scalar "-c")
                            (Scalar "echo \"ruok\" | timeout {{.Values.readinessProbe.probeCommandTimeout}} openssl s_client -quiet -crlf -connect localhost:{{.Values.containerPorts.tls}} | grep imok")))))
                    (else
                      (Mapping
                        (Pair
                          (Scalar "command")
                          (Sequence
                            (Scalar "/bin/bash")
                            (Scalar "-c")
                            (Scalar "echo \"ruok\" | timeout {{.Values.readinessProbe.probeCommandTimeout}} openssl s_client -quiet -crlf -connect localhost:{{.Values.containerPorts.tls}} -cert {{.Values.service.tls.client_cert_pem_path}} -key {{.Values.service.tls.client_key_pem_path}} | grep imok"))))))))))))
      (If ".Values.customStartupProbe"
        (then
          (Mapping
            (Pair
              (Scalar "startupProbe")
              (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.customStartupProbe \"context\" $) | nindent 12"))))
        (else
          (If ".Values.startupProbe.enabled"
            (then
              (Mapping
                (Pair
                  (Scalar "startupProbe")
                  (Mapping
                    (Pair
                      (Scalar "tcpSocket")))))
              (If "not .Values.service.disableBaseClientPort"
                (then
                  (Mapping
                    (Pair
                      (Scalar "port")
                      (Scalar "client"))))
                (else
                  (Mapping
                    (Pair
                      (Scalar "port")
                      (Scalar "follower")))))))))))
  (If ".Values.lifecycleHooks"
    (then
      (Mapping
        (Pair
          (Scalar "lifecycle")
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.lifecycleHooks \"context\" $) | nindent 12")))))
  (Mapping
    (Pair
      (Scalar "volumeMounts")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "scripts"))
          (Pair
            (Scalar "mountPath")
            (Scalar "/scripts/setup.sh"))
          (Pair
            (Scalar "subPath")
            (Scalar "setup.sh")))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "data"))
          (Pair
            (Scalar "mountPath")
            (Scalar "/bitnami/zookeeper"))))))
  (If ".Values.dataLogDir"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "data-log"))
          (Pair
            (Scalar "mountPath")
            (HelmExpr ".Values.dataLogDir"))))))
  (If "or .Values.configuration .Values.existingConfigmap"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "config"))
          (Pair
            (Scalar "mountPath")
            (Scalar "/opt/bitnami/zookeeper/conf/zoo.cfg"))
          (Pair
            (Scalar "subPath")
            (Scalar "zoo.cfg"))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "client-shared-certs"))
          (Pair
            (Scalar "mountPath")
            (Scalar "/opt/bitnami/zookeeper/config/certs/client"))
          (Pair
            (Scalar "readOnly")
            (Scalar "true"))))))
  (If ".Values.tls.quorum.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "quorum-shared-certs"))
          (Pair
            (Scalar "mountPath")
            (Scalar "/opt/bitnami/zookeeper/config/certs/quorum"))
          (Pair
            (Scalar "readOnly")
            (Scalar "true"))))))
  (If ".Values.extraVolumeMounts"
    (then
      (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.extraVolumeMounts \"context\" $ ) | nindent 12")))
  (If ".Values.sidecars"
    (then
      (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.sidecars \"context\" $ ) | nindent 8")))
  (Mapping
    (Pair
      (Scalar "volumes")
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "scripts"))
          (Pair
            (Scalar "configMap")
            (Mapping
              (Pair
                (Scalar "name")
                (HelmExpr "printf \"%s-scripts\" (include \"common.names.fullname\" .)"))
              (Pair
                (Scalar "defaultMode")
                (Scalar "755"))))))))
  (If "or .Values.configuration .Values.existingConfigmap"
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
                (HelmExpr "include \"zookeeper.configmapName\" ."))))))))
  (If "and .Values.persistence.enabled .Values.persistence.existingClaim"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "data"))
          (Pair
            (Scalar "persistentVolumeClaim")
            (Mapping
              (Pair
                (Scalar "claimName")
                (HelmExpr "printf \"%s\" (tpl .Values.persistence.existingClaim .)")))))))
    (else
      (If "not .Values.persistence.enabled"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "data"))
              (Pair
                (Scalar "emptyDir")
                (Mapping))))))))
  (If "and .Values.persistence.enabled .Values.persistence.dataLogDir.existingClaim"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "data-log"))
          (Pair
            (Scalar "persistentVolumeClaim")
            (Mapping
              (Pair
                (Scalar "claimName")
                (HelmExpr "printf \"%s\" (tpl .Values.persistence.dataLogDir.existingClaim .)")))))))
    (else
      (If "and ( not .Values.persistence.enabled ) .Values.dataLogDir"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (Scalar "data-log"))
              (Pair
                (Scalar "emptyDir")
                (Mapping))))))))
  (If ".Values.tls.client.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "client-certificates"))
          (Pair
            (Scalar "secret")
            (Mapping
              (Pair
                (Scalar "secretName")
                (HelmExpr "include \"zookeeper.client.tlsSecretName\" ."))
              (Pair
                (Scalar "defaultMode")
                (Scalar "256")))))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "client-shared-certs"))
          (Pair
            (Scalar "emptyDir")
            (Mapping))))))
  (If ".Values.tls.quorum.enabled"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "quorum-certificates"))
          (Pair
            (Scalar "secret")
            (Mapping
              (Pair
                (Scalar "secretName")
                (HelmExpr "include \"zookeeper.quorum.tlsSecretName\" ."))
              (Pair
                (Scalar "defaultMode")
                (Scalar "256")))))
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "quorum-shared-certs"))
          (Pair
            (Scalar "emptyDir")
            (Mapping))))))
  (If ".Values.extraVolumes"
    (then
      (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.extraVolumes \"context\" $) | nindent 8")))
  (If "and .Values.persistence.enabled (not (and .Values.persistence.existingClaim .Values.persistence.dataLogDir.existingClaim) )"
    (then
      (Mapping
        (Pair
          (Scalar "volumeClaimTemplates")))
      (If "not .Values.persistence.existingClaim"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "metadata")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (Scalar "data"))))))
          (If ".Values.persistence.annotations"
            (then
              (Mapping
                (Pair
                  (Scalar "annotations")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.annotations \"context\" $) | nindent 10")))))
          (If ".Values.persistence.labels"
            (then
              (Mapping
                (Pair
                  (Scalar "labels")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.labels \"context\" $) | nindent 10")))))
          (Mapping
            (Pair
              (Scalar "spec")
              (Mapping
                (Pair
                  (Scalar "accessModes")))))
          (Range ".Values.persistence.accessModes"
            (body
              (Sequence
                (HelmExpr ". | quote"))))
          (Mapping
            (Pair
              (Scalar "resources")
              (Mapping
                (Pair
                  (Scalar "requests")
                  (Mapping
                    (Pair
                      (Scalar "storage")
                      (HelmExpr ".Values.persistence.size | quote")))))))
          (HelmExpr "include \"common.storage.class\" (dict \"persistence\" .Values.persistence \"global\" .Values.global) | nindent 8")
          (If ".Values.persistence.selector"
            (then
              (Mapping
                (Pair
                  (Scalar "selector")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.selector \"context\" $) | nindent 10")))))))
      (If "and (not .Values.persistence.dataLogDir.existingClaim) .Values.dataLogDir"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "metadata")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (Scalar "data-log"))))))
          (If ".Values.persistence.annotations"
            (then
              (Mapping
                (Pair
                  (Scalar "annotations")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.annotations \"context\" $) | nindent 10")))))
          (If ".Values.persistence.labels"
            (then
              (Mapping
                (Pair
                  (Scalar "labels")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.labels \"context\" $) | nindent 10")))))
          (Mapping
            (Pair
              (Scalar "spec")
              (Mapping
                (Pair
                  (Scalar "accessModes")))))
          (Range ".Values.persistence.accessModes"
            (body
              (Sequence
                (HelmExpr ". | quote"))))
          (Mapping
            (Pair
              (Scalar "resources")
              (Mapping
                (Pair
                  (Scalar "requests")
                  (Mapping
                    (Pair
                      (Scalar "storage")
                      (HelmExpr ".Values.persistence.dataLogDir.size | quote")))))))
          (HelmExpr "include \"common.storage.class\" (dict \"persistence\" .Values.persistence \"global\" .Values.global) | nindent 8")
          (If ".Values.persistence.dataLogDir.selector"
            (then
              (Mapping
                (Pair
                  (Scalar "selector")
                  (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.persistence.dataLogDir.selector \"context\" $) | nindent 10"))))))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR);
}
