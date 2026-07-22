use super::*;

/// Bitnami `validateValues` aggregators: a local bound to
/// DERIVED TEXT (`$message := join "\n" $messages`) is falsy when NO
/// validator fired, not when its input identities are falsy — the truthy
/// stand-in over the flowing paths must count as approximate so the fail
/// negation abstains instead of rejecting every document whose inputs are
/// truthy (bitnami minio drive-count validator, surfaced by NOTES.txt
/// analysis).
#[test]
fn derived_text_aggregate_condition_does_not_negate_input_truthiness() {
    let src = indoc! {r#"
        Thank you for installing.

        {{- include "app.validateValues" . }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.validateValues.totalDrives" -}}
        {{- $replicaCount := int .Values.statefulset.replicaCount }}
        {{- $drivesPerNode := int .Values.statefulset.drivesPerNode }}
        {{- $zones := int .Values.statefulset.zones }}
        {{- $totalDrives := mul $replicaCount $zones $drivesPerNode }}
        {{- if and (eq .Values.mode "distributed") (lt $totalDrives 4) -}}
        minio: total drives
        {{- end -}}
        {{- end -}}

        {{- define "app.validateValues" -}}
        {{- $messages := list -}}
        {{- $messages := append $messages (include "app.validateValues.totalDrives" .) -}}
        {{- $messages := without $messages "" -}}
        {{- $message := join "
" $messages -}}
        {{- if $message -}}
        {{-   printf "
VALUES VALIDATION:
%s" $message | fail -}}
        {{- end -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        mode: standalone
        statefulset:
          replicaCount: 1
          drivesPerNode: 1
          zones: 1
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "mode": "standalone",
                "statefulset": { "replicaCount": 1, "drivesPerNode": 1, "zones": 1 }
            })
        ),
        "standalone mode never reaches the drive-count validator; truthy \
         inputs must not satisfy the aggregate's negation: {schema}"
    );
}

/// A local error string built by guarded self-appends is truthy whenever
/// one of those guards ran. The final fail therefore retains the original
/// type-test dependency instead of becoming an unbound local condition.
#[test]
fn monotone_error_accumulator_preserves_guarded_fail_condition() {
    let src = indoc! {r#"
        Thank you for installing.

        {{- $breaking := "" }}
        {{- if typeIs "map[string]interface {}" .Values.location }}
        {{- $breaking = print $breaking "legacy map form is unsupported" }}
        {{- end }}
        {{- if $breaking }}
        {{- fail $breaking }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("location: ~\n"));

    for instance in [
        serde_json::json!({ "location": [{ "name": "default" }] }),
        serde_json::json!({ "location": "ignored" }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "non-map forms do not populate the error accumulator: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "location": { "name": "default" } })
        ),
        "the map type test populates the accumulator and reaches fail: {schema}"
    );
}

/// An error accumulator appended INSIDE a range whose arm is narrowed by a
/// range-key equality keeps those member conditions through the range join:
/// the final `if $breaking` fail rejects exactly the legacy member shapes
/// (velero's NOTES.txt fs-restore label/image gates).
#[test]
fn range_appended_error_accumulator_reaches_the_final_fail() {
    let src = indoc! {r#"
        Thank you for installing.

        {{- $breaking := "" }}
        {{- if hasKey .Values "resticTimeout" }}
        {{- $breaking = print $breaking "\n\nREMOVED: resticTimeout" }}
        {{- end }}
        {{- range $key, $value := .Values.configMaps }}
        {{- if eq $key "fs-restore-action-config" }}
        {{- if hasKey $value.labels "velero.io/restic" }}
        {{- $breaking = print $breaking "\n\nREMOVED: velero.io/restic label" }}
        {{- end }}
        {{- if $value.data.image }}
        {{- if contains "velero-restic-restore-helper" $value.data.image }}
        {{- $breaking = print $breaking "\n\nREMOVED: restic restore helper image" }}
        {{- end }}
        {{- end }}
        {{- end }}
        {{- end }}
        {{- if $breaking }}
        {{- fail $breaking }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r"
        configMaps: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults render"),
        (
            serde_json::json!({ "resticTimeout": "1h" }),
            false,
            "the removed top-level key aborts",
        ),
        (
            serde_json::json!({ "configMaps": { "fs-restore-action-config": {
                "labels": { "velero.io/pod-volume-restore": "" },
                "data": { "image": "velero/velero-restore-helper:v1.0" },
            } } }),
            true,
            "current label and image forms render",
        ),
        (
            serde_json::json!({ "configMaps": { "fs-restore-action-config": {
                "labels": { "velero.io/restic": "" },
                "data": {},
            } } }),
            false,
            "the legacy restic label aborts",
        ),
        (
            serde_json::json!({ "configMaps": { "fs-restore-action-config": {
                "labels": {},
                "data": { "image": "velero/velero-restic-restore-helper:v1.0" },
            } } }),
            false,
            "the legacy restore-helper image aborts",
        ),
        (
            serde_json::json!({ "configMaps": { "other": {
                "labels": { "velero.io/restic": "" },
                "data": {},
            } } }),
            true,
            "other members escape the key-narrowed gates",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "range-appended accumulator ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A list accumulator made nonempty by guarded `append` calls retains the
/// exact guards when a later length check terminates rendering.
#[test]
fn monotone_list_accumulator_preserves_guarded_fail_condition() {
    let src = indoc! {r#"
        Thank you for installing.

        {{- $removed := list "ebpf" "gvisor" }}
        {{- $found := list }}
        {{- if has .Values.driverKind $removed }}
        {{- $found = append $found (printf "driver.kind=%s" .Values.driverKind) }}
        {{- end }}
        {{- if gt (len $found) 0 }}
        {{- fail (join ", " $found) }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("driverKind: auto\n"));

    for driver_kind in ["auto", "kmod", "modern_ebpf"] {
        let instance = serde_json::json!({ "driverKind": driver_kind });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "supported driver kinds leave the accumulator empty: \
             instance={instance}; schema={schema}"
        );
    }
    for driver_kind in ["ebpf", "gvisor"] {
        let instance = serde_json::json!({ "driverKind": driver_kind });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "removed driver kinds populate the accumulator and reach fail: \
             instance={instance}; schema={schema}"
        );
    }
}

/// a self-guarded `tpl` inside a named helper binds its
/// truthy-implies-string contract at every call site (oauth2-proxy
/// `alphaConfig.configFile` shape).
#[test]
fn helper_internal_self_guarded_tpl_contract_reaches_callers() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: {{ include "app.alpha" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.alpha" -}}
        {{- if .Values.alphaConfig.configFile }}
        {{- tpl .Values.alphaConfig.configFile $ }}
        {{- end }}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r#"
        alphaConfig:
          configFile: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "alphaConfig": { "configFile": "cfg" } }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strings feed tpl and the falsy state skips it: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "alphaConfig": { "configFile": { "a": 1 } } })
        ),
        "a truthy map reaches the helper's tpl and aborts: {schema}"
    );
}

/// literal-returning helper condition: a `tpl` guarded by
/// `eq (include "mode" .) "literal"` binds where the mode helper's OWN
/// branch guards select that literal (oauth2-proxy
/// `legacy-config.content` chain). The mode helper is a pure literal
/// dispatch, so the comparison decodes into the matching arms' branch
/// conditions conjoined with the negations of the arms before them.
#[test]
fn tpl_behind_literal_helper_mode_condition_binds_branch_guards() {
    let src = indoc! {r#"
        {{- if not (has (include "app.mode" .) (list "existing-configmap" "no-config")) -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: {{ include "app.content" . | quote }}
        {{- end -}}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.mode" -}}
        {{- if and .Values.alphaConfig.enabled (not .Values.config.forceLegacyConfig) -}}
        generated-alpha-compatible
        {{- else if .Values.config.existingConfig -}}
        existing-configmap
        {{- else if .Values.config.configFile -}}
        inline-custom
        {{- else if .Values.alphaConfig.enabled -}}
        generated-alpha-compatible
        {{- else if not .Values.config.forceLegacyConfig -}}
        no-config
        {{- else -}}
        generated-legacy
        {{- end -}}
        {{- end -}}

        {{- define "app.content" -}}
        {{- if eq (include "app.mode" .) "inline-custom" -}}
        {{- tpl .Values.config.configFile $ -}}
        {{- end -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r#"
        config:
          existingConfig: ~
          configFile: ""
          forceLegacyConfig: false
        alphaConfig:
          enabled: false
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "config": { "configFile": "cfg" } }),
        serde_json::json!({ "config": { "existingConfig": "cm", "configFile": { "a": 1 } } }),
        serde_json::json!({
            "alphaConfig": { "enabled": true },
            "config": { "forceLegacyConfig": false, "configFile": { "a": 1 } }
        }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strings render, and other modes never reach the tpl: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "alphaConfig": { "enabled": true },
                "config": { "forceLegacyConfig": true, "configFile": { "a": 1 } }
            })
        ),
        "inline-custom mode feeds the map to tpl and aborts: {schema}"
    );
}

/// A type handled by one consumer does not become a converted member-host
/// form for another consumer. Without an ordered object-producing mutation,
/// the unconditional member read still rejects the string form.
#[test]
fn independent_kind_guard_does_not_convert_member_host() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if kindIs "string" .Values.image }}
          raw: {{ .Values.image | quote }}
          {{- end }}
          name: {{ .Values.image.name | quote }}
    "#};
    let values_yaml = indoc! {r"
        image:
          name: app
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "image": { "name": "app" } })),
        "the unconditional member read accepts an object host: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "image": "app:latest" })),
        "a separate string guard cannot make the unconditional member read safe: {schema}"
    );
}

/// A handled input kind cannot attach to a later unconditional member read
/// when the object-producing mutation has an unrelated outer predicate.
#[test]
fn guarded_set_conversion_does_not_escape_its_outer_guard() {
    let src = indoc! {r#"
        {{- include "app.fiximage" .Values.jetstream }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          name: {{ .Values.jetstream.image.name | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.fiximage" -}}
        {{- if .enabled }}
        {{- if kindIs "string" .image }}
        {{- $_ := set . "image" (dict "name" .image) }}
        {{- end }}
        {{- end }}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r"
        jetstream:
          enabled: false
          image:
            name: app
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "jetstream": { "enabled": false, "image": { "name": "app" } }
            })
        ),
        "the unconditional member read accepts an object host: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "jetstream": { "enabled": false, "image": "app" } })
        ),
        "the disabled mutation leaves the raw string unsafe for the member read: {schema}"
    );
}

/// an ordered `set` mutation converts the string image form into the
/// map the later member reads require — the accepted union is EXACTLY
/// string-or-map, and untouched scalars still abort the member read (nack
/// `jsc.fixImage`/`jsc.image` shape). The member-access arm carries the
/// kinds the chart's own `kindIs` dispatch handles, so the converted
/// string form stays accepted while the untouched complement rejects.
#[test]
fn ordered_set_mutation_accepts_converted_and_map_forms() {
    let src = indoc! {r#"
        {{- include "app.fiximage" .Values.jetstream -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: {{ include "app.image" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.fiximage" -}}
        {{- if kindIs "string" .image }}
        {{- $_ := set . "image" (dict "repository" (split ":" .image)._0 "tag" ((split ":" .image)._1 | default "latest")) }}
        {{- end }}
        {{- end -}}

        {{- define "app.image" -}}
        {{- $d := .Values.jetstream.image -}}
        {{- printf "%s:%s" $d.repository (default "latest" $d.tag) -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r"
        jetstream:
          image: nats:2.9
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "jetstream": { "image": "repo:tag" } }),
        serde_json::json!({ "jetstream": { "image": { "repository": "r" } } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the string form converts and the map form reads directly: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "jetstream": { "image": 7 } })),
        "an untouched scalar reaches the member read and aborts: {schema}"
    );
}

/// statically empty subject: `required "msg" nil` is a pure guarded
/// validator — under its branch guards rendering always terminates, so the
/// guard conjunction is a terminal clause (airflow check-values
/// `elasticsearch/opensearch` mutual exclusion; ingress-nginx
/// `required (index (dict) ".")`).
#[test]
fn required_nil_subject_is_a_guarded_terminal_clause() {
    let src = indoc! {r#"
        {{- if and .Values.elasticsearch.enabled .Values.opensearch.enabled }}
        {{ required "You must not set both elasticsearch and opensearch" nil }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        elasticsearch:
          enabled: false
        opensearch:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "elasticsearch": { "enabled": true } }),
        serde_json::json!({ "opensearch": { "enabled": true } }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "outside the guarded branch the validator never runs: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "elasticsearch": { "enabled": true },
                "opensearch": { "enabled": true }
            })
        ),
        "both enabled reaches the required-nil terminal: {schema}"
    );
}

/// ranged member subject: `required` over each member's field
/// survives the conversion pipeline and binds per member (argo-cd
/// `configs.clusterCredentials.*.config` shape).
#[test]
fn required_ranged_member_field_binds_per_member() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $index, $cred := .Values.creds }}
          entry{{ $index }}: {{ required "each entry needs config" $cred.config | quote }}
          {{- end }}
    "#};
    let values_yaml = "creds: ~\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "creds": [{ "config": "c1" }] }),
        serde_json::json!({ "creds": [] }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "complete entries and empty collections render: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "creds": [{ "name": "a" }] })),
        "an entry without `config` hits required and aborts: {schema}"
    );
}

/// A fragment-rendering `required` inside block-scalar text remains a
/// validator even though the rendered value is suppressed into the scalar
/// instead of occupying its own YAML node.
#[test]
fn required_in_suppressed_block_scalar_binds_ranged_member() {
    let src = indoc! {r#"
        {{- range $name, $cred := .Values.creds }}
        apiVersion: v1
        kind: Secret
        metadata:
          name: {{ $name }}
        stringData:
          config: |
            {{- required "each entry needs config" $cred.config | toRawJson | nindent 4 }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("creds: {}\n"));

    for instance in [
        serde_json::json!({ "creds": {} }),
        serde_json::json!({ "creds": { "prod": { "config": { "token": "x" } } } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "empty collections and complete entries render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "creds": { "prod": { "server": "https://example.com" } } })
        ),
        "an entry missing config terminates inside the block scalar: {schema}"
    );
}

/// helper-internal subject: a `required` inside a named helper holds
/// at every call site (kyverno `kyverno.chartVersion` requires
/// `global.templating.version` before `replace`).
#[test]
fn required_inside_helper_reaches_callers() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          version: {{ include "chart.version" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "chart.version" -}}
        {{- required "version is required" .Values.global.version | replace "+" "_" -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r#"
        global:
          version: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "global": { "version": "1.2.3" } })
        ),
        "a nonempty version renders: {schema}"
    );
    for instance in [
        serde_json::json!({ "global": { "version": "" } }),
        serde_json::json!({}),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a Helm-empty version fails the helper's required: \
             instance={instance}; schema={schema}"
        );
    }
}

/// Helper arguments are eagerly evaluated, so validators inside a nested
/// include run even when its result is only used to construct another
/// helper's context.
#[test]
fn required_inside_nested_helper_argument_reaches_callers() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ include "app.outer" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.required" -}}
        {{- if .Values.templating.enabled -}}
        {{- required "version is required" .Values.templating.version -}}
        {{- end -}}
        {{- end -}}

        {{- define "app.collect" -}}
        {{- index . 0 -}}
        {{- end -}}

        {{- define "app.outer" -}}
        {{- template "app.collect" (list (include "app.required" .)) -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {r#"
        templating:
          enabled: false
          version: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "templating": { "enabled": false, "version": "" } }),
        serde_json::json!({ "templating": { "enabled": true, "version": "1.2.3" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a skipped validator and a nonempty version render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "templating": { "enabled": true, "version": "" } })
        ),
        "constructing the outer helper argument evaluates the nested required call: {schema}"
    );
}

/// Eager argument evaluation carries runtime effects, but the argument's
/// value is not rendered merely because a helper received it. A helper that
/// ignores its context must not turn that context into scalar sink evidence.
#[test]
fn ignored_helper_context_does_not_render_argument_value() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ include "app.static" .Values.context | quote }}
          defaulted: {{ include "app.static" (default "fallback" .Values.context) | quote }}
          local: {{ include "app.static" $context | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.static" -}}
        fixed
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("context: {}\n"));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "context": { "nested": true } })
        ),
        "the static helper ignores its context instead of rendering it at the scalar slot: {schema}"
    );
}

/// unrepresentable sentinel, conservative pin: a `required nil`
/// behind a loop-computed local guard must NOT bind — the valid
/// alternative (the matching env entry) stays accepted (airflow broker-url
/// sentinel shape).
#[test]
fn required_nil_behind_loop_local_guard_abstains() {
    let src = indoc! {r#"
        {{- $found := false }}
        {{- range .Values.env }}
        {{- if eq .name "BROKER_URL" }}
        {{- $found = true }}
        {{- end }}
        {{- end }}
        {{- if not $found }}
        {{ required "an env entry named BROKER_URL is required" nil }}
        {{- end }}
    "#};
    let values_yaml = "env: []\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "env": [{ "name": "BROKER_URL" }] })
        ),
        "the loop-derived guard cannot lower; the terminal must abstain \
         rather than reject the valid alternative: {schema}"
    );
}
