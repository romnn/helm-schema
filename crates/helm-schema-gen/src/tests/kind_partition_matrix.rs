use super::*;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ConditionalOverlayFlavor;
use indoc::formatdoc;
use test_util::prelude::sim_assert_eq;

#[test]
fn contract_builder_emits_typed_kind_branch_evidence() -> eyre::Result<()> {
    let source = indoc! {r#"
        apiVersion: apps/v1
        kind: {{ .Values.workload.kind }}
        metadata:
          name: test
        spec:
          {{- if eq .Values.workload.kind "Deployment" }}
          strategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- else if eq .Values.workload.kind "StatefulSet" }}
          updateStrategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- end }}
    "#};
    let signals = schema_signals_for(parse_ir(source));
    let evidence = signals
        .evidence_for(&helm_schema_core::ValuesPath::parse("workload.strategy"))
        .ok_or_eyre("workload strategy evidence missing")?;

    eyre::ensure!(!evidence.conditional_overlays.is_empty());
    for overlay in &evidence.conditional_overlays {
        sim_assert_eq!(have: overlay.flavor, want: ConditionalOverlayFlavor::KindBranch);
        eyre::ensure!(!overlay.evidence.provider_schema_uses.is_empty());
        for use_ in &overlay.evidence.provider_schema_uses {
            eyre::ensure!(use_.resource.kind_branches.is_empty());
            eyre::ensure!(use_.resource.kind_candidates.is_empty());
        }
    }
    Ok(())
}

#[test]
fn contract_builder_retains_literal_control_kind_branches() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- if eq .Values.local.kind "ConfigMap" }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        immutable: {{ .Values.local.setting }}
        {{- else if eq .Values.local.kind "Service" }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          type: {{ .Values.local.setting }}
        {{- end }}
    "#};
    let signals = schema_signals_for(parse_ir(source));
    let evidence = signals
        .evidence_for(&helm_schema_core::ValuesPath::parse("local.setting"))
        .ok_or_eyre("local setting evidence missing")?;

    eyre::ensure!(
        evidence
            .conditional_overlays
            .iter()
            .all(|overlay| overlay.flavor == ConditionalOverlayFlavor::KindBranch),
        "literal kind branches were not retained: {evidence:#?}"
    );
    Ok(())
}

fn strict_provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
            .with_cache_dir(super::bundle_cache_dir())
            .with_allow_download(false),
    )])
}

/// bitnami-redis master: a values-selected `kind:` crossed with a
/// helper-resolved apiVersion partitions the strategy slot per kind. The
/// Deployment arm places `spec.strategy` (no `rollingUpdate.partition`),
/// every other arm places `spec.updateStrategy` (no `maxSurge`), and the
/// provider projection must follow the selected partition instead of
/// blending the kinds.
#[test]
fn values_selected_kind_partitions_strategy_provider_projection() {
    let helpers = indoc! {r#"
        {{- define "common.capabilities.statefulset.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: {{ include "common.capabilities.statefulset.apiVersion" . }}
        kind: {{ .Values.master.kind }}
        metadata:
          name: test
        spec:
          {{- if not (eq .Values.master.kind "DaemonSet") }}
          replicas: {{ .Values.master.count }}
          {{- end }}
          {{- if (eq .Values.master.kind "StatefulSet") }}
          serviceName: test-headless
          {{- end }}
          {{- if .Values.master.updateStrategy }}
          {{- if (eq .Values.master.kind "Deployment") }}
          strategy: {{- toYaml .Values.master.updateStrategy | nindent 4 }}
          {{- else }}
          updateStrategy: {{- toYaml .Values.master.updateStrategy | nindent 4 }}
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        master:
          kind: StatefulSet
          count: 1
          updateStrategy:
            type: RollingUpdate
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    );

    for instance in [
        serde_json::json!({ "master": { "kind": "Deployment", "updateStrategy": { "rollingUpdate": { "maxSurge": "25%" } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "rollingUpdate": { "partition": 1 } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "type": "RollingUpdate" } } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the strategy field set matching the selected kind renders and validates: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "master": { "kind": "Deployment", "updateStrategy": { "rollingUpdate": { "partition": 1 } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "rollingUpdate": { "maxSurge": "25%" } } } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a strategy field from the OTHER kind's schema is rejected on this partition: \
             instance={instance}; schema={schema}"
        );
    }
}

/// airflow's scheduler: an INLINE LOCAL selects the workload kind
/// (`kind: {{ if $stateful }}StatefulSet{{ else }}Deployment{{ end }}`)
/// and the body's strategy slots are guarded by the same local. The kind
/// arms carry the selecting predicate, so rows entailed by one arm
/// concretize to that arm's kind and the provider projection follows the
/// partition: the Deployment arm owns `spec.strategy`, the `StatefulSet` arm
/// `spec.updateStrategy`, and each rejects the shape it cannot hold.
#[test]
fn inline_local_kind_partition_projects_per_arm_provider_schemas() {
    let src = indoc! {r#"
        {{- $stateful := and (contains "Local" .Values.executor) .Values.persistence.enabled }}
        apiVersion: apps/v1
        kind: {{ if $stateful }}StatefulSet{{ else }}Deployment{{ end }}
        metadata:
          name: test
        spec:
          replicas: {{ .Values.replicas }}
          {{- if and $stateful .Values.updateStrategy }}
          updateStrategy: {{- toYaml .Values.updateStrategy | nindent 4 }}
          {{- end }}
          {{- if and (not $stateful) .Values.strategy }}
          strategy: {{- toYaml .Values.strategy | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        executor: CeleryExecutor
        persistence:
          enabled: false
        replicas: 1
        updateStrategy: ~
        strategy: ~
    "};
    let signals = schema_signals_for(parse_ir(src));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    );

    // Cases compose over the declared defaults: `contains "Local"
    // .Values.executor` reads the executor on every render and aborts on a
    // nil operand.
    for overrides in [
        serde_json::json!({ "strategy": { "rollingUpdate": { "maxSurge": "25%" } } }),
        serde_json::json!({ "strategy": { "type": "RollingUpdate" } }),
        serde_json::json!({
            "executor": "LocalExecutor",
            "persistence": { "enabled": true },
            "updateStrategy": { "rollingUpdate": { "partition": 1 } },
        }),
        // The strategy value from the OTHER kind's arm is harmless while
        // its own arm is dead: the template never renders it.
        serde_json::json!({
            "executor": "LocalExecutor",
            "persistence": { "enabled": true },
            "strategy": 7,
        }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the arm-matching strategy shape renders and validates: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "strategy": 7 }),
        serde_json::json!({ "strategy": { "rollingUpdate": { "partition": 1 } } }),
        serde_json::json!({
            "executor": "LocalExecutor",
            "persistence": { "enabled": true },
            "updateStrategy": { "rollingUpdate": { "maxSurge": "25%" } },
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a strategy shape outside the LIVE arm's kind is rejected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// Both arms of an inline-local kind chain write the SAME manifest slot
/// (`spec.updateStrategy`) from different values paths, and both kinds
/// hold that slot with different member sets (`StatefulSet` `partition`,
/// `DaemonSet` `maxSurge`). Pointer-miss fallback cannot pick the arm here
/// — only the row conjunction entailing the arm's selecting predicate
/// resolves each row to ITS kind's schema.
#[test]
fn shared_slot_kind_arms_resolve_through_selecting_predicates() {
    let src = indoc! {r#"
        {{- $stateful := and (contains "Local" .Values.executor) .Values.persistence.enabled }}
        apiVersion: apps/v1
        kind: {{ if $stateful }}StatefulSet{{ else }}DaemonSet{{ end }}
        metadata:
          name: test
        spec:
          {{- if and $stateful .Values.updateStrategy }}
          updateStrategy: {{- toYaml .Values.updateStrategy | nindent 4 }}
          {{- end }}
          {{- if and (not $stateful) .Values.daemonUpdateStrategy }}
          updateStrategy: {{- toYaml .Values.daemonUpdateStrategy | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        executor: CeleryExecutor
        persistence:
          enabled: false
        updateStrategy: ~
        daemonUpdateStrategy: ~
    "};
    let signals = schema_signals_for(parse_ir(src));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    );

    // Cases compose over the declared defaults: `contains "Local"
    // .Values.executor` reads the executor on every render and aborts on a
    // nil operand.
    for overrides in [
        // The default executor keeps the DaemonSet arm live: its slot
        // accepts maxSurge, which the primary (first-literal) StatefulSet
        // schema would reject.
        serde_json::json!({ "daemonUpdateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
        serde_json::json!({
            "executor": "LocalExecutor",
            "persistence": { "enabled": true },
            "updateStrategy": { "rollingUpdate": { "partition": 1 } },
        }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "each arm's row resolves to its OWN kind's slot schema: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "daemonUpdateStrategy": { "rollingUpdate": { "partition": 1 } } }),
        serde_json::json!({
            "executor": "LocalExecutor",
            "persistence": { "enabled": true },
            "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } },
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a member from the OTHER kind's slot schema is rejected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// nfs-subdir-external-provisioner: `maxUnavailable` flows through
/// `default 1` into a `PodDisruptionBudget`'s int-or-string slot, so the
/// declared integer default documents intent without narrowing away the
/// provider-accepted percentage string.
#[test]
fn pdb_int_or_string_survives_declared_integer_default() {
    let helpers = indoc! {r#"
        {{- define "pdb.apiVersion" -}}
        {{- if semverCompare ">=1.21-0" .Capabilities.KubeVersion.GitVersion -}}
        {{- print "policy/v1" -}}
        {{- else -}}
        {{- print "policy/v1beta1" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.podDisruptionBudget.enabled }}
        apiVersion: {{ template "pdb.apiVersion" . }}
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          maxUnavailable: {{ .Values.podDisruptionBudget.maxUnavailable | default 1 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        podDisruptionBudget:
          enabled: false
          maxUnavailable: 1
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    );

    for instance in [
        serde_json::json!({ "podDisruptionBudget": { "enabled": true, "maxUnavailable": "50%" } }),
        serde_json::json!({ "podDisruptionBudget": { "enabled": true, "maxUnavailable": 1 } }),
        serde_json::json!({ "podDisruptionBudget": { "enabled": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the provider slot accepts int-or-string and the default covers absence: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "podDisruptionBudget": { "enabled": true, "maxUnavailable": { "a": 1 } }
            })
        ),
        "Go's plain formatting turns a safe mapping into a string accepted by IntOrString: {schema}"
    );
}

/// loki gateways: `hostUsers` renders ONLY where `kindIs "bool"` says so —
/// every other kind is silently omitted and the chart still renders — so
/// the declared default's string intent must not close the path against
/// maps: the self dispatch proves the complement never reaches the sink.
#[test]
fn self_kind_dispatch_keeps_complement_kinds_open() {
    let helpers = indoc! {r#"
        {{- define "test.kubeVersion" -}}
        {{- .Capabilities.KubeVersion.Version -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          {{- if and (semverCompare ">=1.33-0" (include "test.kubeVersion" .)) (kindIs "bool" .Values.hostUsers) }}
          hostUsers: {{ .Values.hostUsers }}
          {{- end }}
          containers:
            - name: main
    "#};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some("hostUsers: nil\n"))),
    );
    assert!(
        schema
            .get("properties")
            .and_then(|properties| properties.get("hostUsers"))
            .is_some(),
        "the dispatched path stays a referenced property: {schema}"
    );
    for instance in [
        serde_json::json!({ "hostUsers": { "a": 1 } }),
        serde_json::json!({ "hostUsers": true }),
        serde_json::json!({ "hostUsers": "nil" }),
        serde_json::json!({ "hostUsers": 7 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a kind outside the dispatch is omitted, not rejected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// vault's affinity helper: `typeOf` dispatch selects `tpl` for strings
/// and `toYaml` for everything else, so structured affinity values are
/// chart-handled and must validate against the provider slot instead of
/// being rejected as non-strings.
#[test]
fn type_of_dispatch_keeps_serialized_arm_structured() {
    let helpers = indoc! {r#"
        {{- define "test.affinity" -}}
          {{- if .Values.affinity }}
      affinity:
        {{ $tp := typeOf .Values.affinity }}
        {{- if eq $tp "string" }}
          {{- tpl .Values.affinity . | nindent 8 | trim }}
        {{- else }}
          {{- toYaml .Values.affinity | nindent 8 }}
        {{- end }}
          {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              {{ template "test.affinity" . }}
              containers:
                - name: main
    "#};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    assert!(
        signals
            .evidence_for(&helm_schema_core::ValuesPath::parse("affinity"))
            .is_some_and(
                |evidence| evidence.conditional_overlays.iter().any(|overlay| {
                    overlay.guards.iter().any(|guard| {
                        matches!(
                            guard,
                            helm_schema_core::ConditionalGuard::Not(inner)
                                if matches!(
                                    inner.as_ref(),
                                    helm_schema_core::ConditionalGuard::TypeIs {
                                        path,
                                        schema_type,
                                    } if path == &helm_schema_core::ValuesPath::parse("affinity")
                                        && schema_type == "string"
                                )
                        )
                    }) && !overlay.evidence.facts.used_as_serialized
                })
            ),
        "the structure-preserving complement must keep provider evidence ahead of the scalar declared default"
    );
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider()).with_values_documents(
            &prepared_values_documents(Some(indoc! {"
                affinity: |
                  nodeAffinity: {}
            "})),
        ),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "affinity": { "nodeAffinity": {
                "requiredDuringSchedulingIgnoredDuringExecution": { "nodeSelectorTerms": [] }
            } } }),
            true,
        ),
        (serde_json::json!({ "affinity": "nodeAffinity: {}" }), true),
        (
            serde_json::json!({ "affinity": { "nodeAffinity": 7 } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the toYaml arm keeps provider typing for structured values: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A `kind:` rendered by a called helper whose arms select the shared
/// `spec.updateStrategy` slot's kind, with the body's two slot writes
/// guarded by the same `.Values.flag`.
fn helper_kind_schema(kind_action: &str, helpers: &str, values_yaml: &str) -> Value {
    let src = formatdoc! {r"
        apiVersion: apps/v1
        kind: {kind_action}
        metadata:
          name: test
        spec:
          {{{{- if and .Values.flag .Values.updateStrategy }}}}
          updateStrategy: {{{{- toYaml .Values.updateStrategy | nindent 4 }}}}
          {{{{- end }}}}
          {{{{- if and (not .Values.flag) .Values.daemonUpdateStrategy }}}}
          updateStrategy: {{{{- toYaml .Values.daemonUpdateStrategy | nindent 4 }}}}
          {{{{- end }}}}
    "};
    strict_schema(&src, helpers, values_yaml)
}

fn strict_schema(src: &str, helpers: &str, values_yaml: &str) -> Value {
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    )
}

const HELPER_KIND: &str = r#"{{- define "kind" -}}{{ if .Values.flag }}StatefulSet{{ else }}DaemonSet{{ end }}{{- end -}}"#;

/// Astra's cell K with a shared slot: `include "kind" .` hands the helper
/// the caller's own context, so its `.Values.flag` arms bind exactly as an
/// inline chain would and each guarded row resolves to its arm's kind
/// (Helm 4.2.3, `rework/helm/f1-matrix.log` `f1-same`: `flag=true` renders
/// a `StatefulSet`, `flag=false` a `DaemonSet`).
#[test]
fn same_context_helper_kind_arms_partition_the_shared_slot() {
    let values_yaml = indoc! {"
        flag: false
        updateStrategy: ~
        daemonUpdateStrategy: ~
    "};
    let schema = helper_kind_schema(r#"{{ include "kind" . }}"#, HELPER_KIND, values_yaml);

    for overrides in [
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "partition": 1 } } }),
        serde_json::json!({ "daemonUpdateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "each arm's row resolves to its OWN kind's slot schema: \
             instance={instance}; schema={schema}"
        );
    }
    for overrides in [
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
        serde_json::json!({ "daemonUpdateStrategy": { "rollingUpdate": { "partition": 1 } } }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a member from the OTHER kind's slot schema is rejected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// `include "kind" (dict "Values" .Values.inner)` binds the helper's
/// `.Values.flag` to `inner.flag`, not the document's `flag`: the arms say
/// nothing about the document's rows, so the partition abstains (Helm
/// 4.2.3, `f1-dict`: `flag=true, inner.flag=false` renders a `DaemonSet`
/// with `maxSurge`).
#[test]
fn different_context_helper_kind_arms_abstain() {
    let values_yaml = indoc! {"
        flag: false
        inner:
          flag: false
        updateStrategy: ~
        daemonUpdateStrategy: ~
    "};
    let schema = helper_kind_schema(
        r#"{{ include "kind" (dict "Values" .Values.inner) }}"#,
        HELPER_KIND,
        values_yaml,
    );

    for overrides in [
        serde_json::json!({
            "flag": true,
            "inner": { "flag": false },
            "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } },
        }),
        serde_json::json!({
            "inner": { "flag": true },
            "daemonUpdateStrategy": { "rollingUpdate": { "partition": 1 } },
        }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a rendered manifest valid for the helper-selected kind is accepted: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A helper that forwards `.` from inside `with .Values.inner` hands the
/// kind helper the rebound dot, so its arms bind to `inner.Values.flag`
/// and the partition abstains (Helm 4.2.3, `f1-with`: `flag=true` with the
/// default `inner.Values.flag=false` renders a `DaemonSet` with
/// `maxSurge`).
#[test]
fn rebound_dot_helper_kind_arms_abstain() {
    let helpers = formatdoc! {r#"
        {HELPER_KIND}
        {{{{- define "outer" -}}}}{{{{ with .Values.inner }}}}{{{{ include "kind" . }}}}{{{{ end }}}}{{{{- end -}}}}
    "#};
    let values_yaml = indoc! {"
        flag: false
        inner:
          Values:
            flag: false
        updateStrategy: ~
        daemonUpdateStrategy: ~
    "};
    let schema = helper_kind_schema(r#"{{ include "outer" . }}"#, &helpers, values_yaml);

    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a rendered manifest valid for the helper-selected kind is accepted: \
         instance={instance}; schema={schema}"
    );
}

/// The regression cells: the helper's arms are unbound (another context
/// or `$`), so the kind stays an unresolved `StatefulSet`/`DaemonSet` union.
/// The row's `eq .Values.mode "DaemonSet"` guard compares a value to a
/// candidate kind but does not select the kind, so it must not partition
/// the row onto the `DaemonSet` schema.
fn unrelated_mode_guard_schema(kind_action: &str) -> (Value, &'static str) {
    let src = formatdoc! {r#"
        apiVersion: apps/v1
        kind: {kind_action}
        metadata:
          name: test
        spec:
        {{{{- if and .Values.flag (eq .Values.mode "DaemonSet") .Values.updateStrategy }}}}
          updateStrategy: {{{{- toYaml .Values.updateStrategy | nindent 4 }}}}
        {{{{- end }}}}
    "#};
    let values_yaml = indoc! {"
        flag: false
        mode: DaemonSet
        updateStrategy: ~
    "};
    (strict_schema(&src, HELPER_KIND, values_yaml), values_yaml)
}

/// Helm 4.2.3, `rework/helm/f5-f6-matrix.log` `f5-dict`: `flag=true`
/// renders a `StatefulSet` whose `rollingUpdate.partition: 1` is valid.
#[test]
fn unbound_dict_helper_kind_is_not_partitioned_by_an_unrelated_guard() {
    let (schema, values_yaml) =
        unrelated_mode_guard_schema(r#"{{ include "kind" (dict "Values" .Values) }}"#);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "partition": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the StatefulSet Helm renders accepts its partition: instance={instance}; schema={schema}"
    );
}

/// Helm 4.2.3, `f5-root`: the same cell through `include "kind" $`.
#[test]
fn unbound_root_helper_kind_is_not_partitioned_by_an_unrelated_guard() {
    let (schema, values_yaml) = unrelated_mode_guard_schema(r#"{{ include "kind" $ }}"#);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "partition": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the StatefulSet Helm renders accepts its partition: instance={instance}; schema={schema}"
    );
}

/// A document-level region that rebinds the dot around the `kind:` line:
/// the helper's arms bind to the region's dot, while the `spec` rows'
/// `.Values.flag` reads the root.
fn rebound_header_schema(open: &str, values_yaml: &str) -> Value {
    let src = formatdoc! {r#"
        apiVersion: apps/v1
        {open}
        kind: {{{{ include "kind" . }}}}
        {{{{- end }}}}
        metadata:
          name: test
        spec:
        {{{{- if and .Values.flag .Values.updateStrategy }}}}
          updateStrategy: {{{{- toYaml .Values.updateStrategy | nindent 4 }}}}
        {{{{- end }}}}
    "#};
    strict_schema(&src, HELPER_KIND, values_yaml)
}

/// Helm 4.2.3, `rework/helm/f5-f6-matrix.log` `f6-with`: `flag=true` with
/// the default `inner.Values.flag=false` renders a `DaemonSet` whose
/// `maxSurge` is valid.
#[test]
fn with_rebound_header_helper_kind_arms_abstain() {
    let values_yaml = indoc! {"
        flag: false
        inner:
          Values:
            flag: false
        updateStrategy: ~
    "};
    let schema = rebound_header_schema("{{- with .Values.inner }}", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}

/// Helm 4.2.3, `f6-range`: the same cell through `range $k, $v := .Values.items`.
#[test]
fn range_rebound_header_helper_kind_arms_abstain() {
    let values_yaml = indoc! {"
        flag: false
        items:
          one:
            Values:
              flag: false
        updateStrategy: ~
    "};
    let schema = rebound_header_schema("{{- range $k, $v := .Values.items }}", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "flag": true, "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}

/// `kind: {{ .Values.mode }}` inside a document-level region that rebinds
/// the dot reads the region's `Values.mode`, not the root `mode` the row's
/// guard compares, so the root comparisons prove neither the kind nor its
/// candidates.
fn direct_rebound_schema(open: &str, values_yaml: &str) -> Value {
    let src = formatdoc! {r#"
        apiVersion: apps/v1
        {open}
        kind: {{{{ .Values.mode }}}}
        {{{{- end }}}}
        metadata:
          name: test
        spec:
        {{{{- if and (eq .Values.mode "StatefulSet") .Values.updateStrategy }}}}
          updateStrategy: {{{{- toYaml .Values.updateStrategy | nindent 4 }}}}
        {{{{- end }}}}
    "#};
    strict_schema(&src, "", values_yaml)
}

/// Helm 4.2.3, `rework/helm/f8-f10-matrix.log` `f8-with`: root
/// `mode=StatefulSet` with `inner.Values.mode=DaemonSet` renders a
/// `DaemonSet` whose `maxSurge` is valid.
#[test]
fn with_rebound_direct_kind_selector_is_not_proven() {
    let values_yaml = indoc! {"
        mode: StatefulSet
        inner:
          Values:
            mode: DaemonSet
        updateStrategy: ~
    "};
    let schema = direct_rebound_schema("{{- with .Values.inner }}", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}

/// Helm 4.2.3, `f8-range`: the same cell through `range $k, $v := .Values.items`.
#[test]
fn range_rebound_direct_kind_selector_is_not_proven() {
    let values_yaml = indoc! {"
        mode: StatefulSet
        items:
          one:
            Values:
              mode: DaemonSet
        updateStrategy: ~
    "};
    let schema = direct_rebound_schema("{{- range $k, $v := .Values.items }}", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}

/// Helm 4.2.3, `rework/helm/f8-f10-matrix.log` `f9-nested`: the nested
/// `mode` chain is not the parent's only kind-writing arm; the parent's
/// `else` renders a `DaemonSet` whatever `mode` says, so `flag=false` with
/// `mode=StatefulSet` renders a `DaemonSet` whose `maxSurge` is valid.
#[test]
fn nested_chain_selector_does_not_escape_a_competing_parent_arm() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        {{- if .Values.flag }}
        {{- if eq .Values.mode "Deployment" }}
        kind: Deployment
        {{- else }}
        kind: StatefulSet
        {{- end }}
        {{- else }}
        kind: DaemonSet
        {{- end }}
        metadata:
          name: test
        spec:
        {{- if and (eq .Values.mode "StatefulSet") .Values.updateStrategy }}
          updateStrategy: {{- toYaml .Values.updateStrategy | nindent 4 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        flag: false
        mode: StatefulSet
        updateStrategy: ~
    "};
    let schema = strict_schema(src, "", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}

/// Astra's `dynamic-parent-arm-closed` cell: the nested `mode` chain
/// competes with a parent `else` writing `kind: {{ .Values.otherKind }}`.
/// That unresolved kind write proves the literal candidates non-exhaustive,
/// so neither the nested selector nor the candidates may type the row.
/// Helm 4.2.3, `rework/helm/f12-matrix.log` `f12-dynamic`: `flag=false`
/// with `mode=StatefulSet` renders `otherKind=DaemonSet` with a valid
/// `maxSurge`.
#[test]
fn dynamic_competing_kind_arm_voids_the_nested_selector() {
    let src = indoc! {r#"
        {{- if .Values.flag }}
        {{- if eq .Values.mode "Deployment" }}
        kind: Deployment
        {{- else }}
        kind: StatefulSet
        {{- end }}
        apiVersion: apps/v1
        {{- else }}
        kind: {{ .Values.otherKind }}
        apiVersion: apps/v1
        {{- end }}
        metadata:
          name: test
        spec:
        {{- if and (eq .Values.mode "StatefulSet") .Values.updateStrategy }}
          updateStrategy: {{- toYaml .Values.updateStrategy | nindent 4 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        flag: false
        mode: StatefulSet
        otherKind: DaemonSet
        updateStrategy: ~
    "};
    let schema = strict_schema(src, "", values_yaml);
    let instance = composed_instance(
        values_yaml,
        serde_json::json!({ "updateStrategy": { "rollingUpdate": { "maxSurge": 1 } } }),
    );
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the DaemonSet Helm renders accepts its maxSurge: instance={instance}; schema={schema}"
    );
}
