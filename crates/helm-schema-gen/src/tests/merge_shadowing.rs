//! Per-key shadowing of ordered `merge` layers at provider sinks: with
//! destination-first `merge preferred legacy`, a legacy member reaches the
//! rendered slot only where the preferred layer lacks that key.

use indoc::indoc;

use super::{parse_ir, parse_ir_with_helpers, schema_accepts_instance, schema_for_values_yaml};

/// The velero shape: the deprecated `securityContext` merges beneath
/// `podSecurityContext` into a Deployment's pod security context. A legacy
/// member is typed exactly where the preferred object does not supply it,
/// the preferred layer keeps its whole payload typing under its own
/// truthiness, and custom legacy keys stay open.
#[test]
fn shadowed_merge_layer_binds_members_only_where_unshadowed() {
    let src = indoc! {r"
        {{- $ctx := merge (.Values.podSecurityContext | default dict) (.Values.securityContext | default dict) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              {{- with $ctx }}
              securityContext:
                {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: main
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("podSecurityContext: {}\nsecurityContext: {}\n"),
    );
    for (instance, want) in [
        // An active legacy member reaches the rendered slot and must type.
        (
            serde_json::json!({ "securityContext": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({ "securityContext": { "runAsUser": 1000 } }),
            true,
        ),
        // The preferred layer supplies the key, so the same malformed
        // legacy member is shadowed and never rendered.
        (
            serde_json::json!({
                "podSecurityContext": { "runAsUser": 1000 },
                "securityContext": { "runAsUser": { "bad": true } }
            }),
            true,
        ),
        // The preferred layer's own members always win and always type.
        (
            serde_json::json!({ "podSecurityContext": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({ "podSecurityContext": { "runAsUser": 1000 } }),
            true,
        ),
        // Keys outside the provider payload stay open.
        (
            serde_json::json!({ "securityContext": { "customExtra": "x" } }),
            true,
        ),
        (serde_json::json!({}), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "per-key merge shadowing scopes the legacy layer's typing: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Member selection over `mergeOverwrite` layers keeps the layered
/// precedence (mergo recurses into nested maps with the same override
/// order), so a `pick`ed member of the merged dict still resolves each
/// layer's path — kyverno's `featuresOverride.logging` reaches the helper's
/// member reads instead of vanishing behind the base layer. The base
/// layer's declared-map typing stays: the scalar-base-fully-shadowed lane
/// is a documented declared-default policy limitation.
#[test]
fn merged_member_projection_reaches_both_layers() {
    let src = indoc! {r#"
        {{- $picked := pick (mergeOverwrite (deepCopy .Values.features) .Values.ctrl.featuresOverride) "logging" }}
        {{- $flags := list -}}
        {{- with $picked.logging -}}
          {{- $flags = append $flags (print "--loggingFormat=" .format) -}}
          {{- $flags = append $flags (print "--v=" .verbosity) -}}
        {{- end -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: busybox
              args:
                {{- range $flags }}
                - {{ . }}
                {{- end }}
    "#};
    let values = "features:\n  logging:\n    format: text\n    verbosity: 2\nctrl:\n  featuresOverride: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values));
    for (instance, want, label) in [
        (
            serde_json::json!({ "features": { "logging": { "format": "json", "verbosity": 4 } } }),
            true,
            "map base logging",
        ),
        (
            serde_json::json!({
                "ctrl": { "featuresOverride": { "logging": { "format": "json", "verbosity": 4 } } }
            }),
            true,
            "map override logging",
        ),
        (
            serde_json::json!({ "features": { "logging": 5 } }),
            false,
            "scalar base unshadowed",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "merged member projection {label}: instance={instance}; schema={schema}"
        );
    }
}

/// `mergeOverwrite` has the opposite precedence — later arguments win — so
/// the layer roles flip: the SECOND path becomes the preferred layer and
/// the first is typed only where the second lacks the key.
#[test]
fn merge_overwrite_reverses_layer_precedence() {
    let src = indoc! {r"
        {{- $ctx := mergeOverwrite (.Values.legacy | default dict) (.Values.preferred | default dict) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              {{- with $ctx }}
              securityContext:
                {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: main
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("legacy: {}\npreferred: {}\n"));
    for (instance, want) in [
        (
            serde_json::json!({ "legacy": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({
                "preferred": { "runAsUser": 1000 },
                "legacy": { "runAsUser": { "bad": true } }
            }),
            true,
        ),
        (
            serde_json::json!({ "preferred": { "runAsUser": { "bad": true } } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "mergeOverwrite flips which layer is shadowed: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The KPS rule-annotation shape: dig- and default-derived locals merged
/// over a fresh-dict destination reach a dynamic-member provider slot
/// (`metadata.annotations`). The empty literal destination drops out of
/// the layer order, `if $merged` decodes to the layers' disjunction, the
/// TOP layer types its members under its own truthiness, and the shadowed
/// layer types only where the top layer is Helm-empty — the per-key
/// correlation over dynamic names stays open (documented F93 bound).
#[test]
fn fresh_dict_merge_layers_type_dynamic_members_with_shadow_refinement() {
    let src = indoc! {r#"
        {{- $ruleAnnotations := dig "MyAlert" (dict) .Values.additionalRuleAnnotations }}
        {{- $groupAnnotations := default (dict) .Values.additionalRuleGroupAnnotations.mygroup }}
        {{- $additionalAnnotations := mergeOverwrite (dict) $groupAnnotations $ruleAnnotations }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- if $additionalAnnotations }}
          annotations:
        {{ toYaml $additionalAnnotations | indent 4 }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("additionalRuleAnnotations: {}\nadditionalRuleGroupAnnotations: {}\n"),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "additionalRuleAnnotations": { "MyAlert": { "foo": 7 } } }),
            false,
        ),
        (
            serde_json::json!({ "additionalRuleAnnotations": { "MyAlert": { "foo": "x" } } }),
            true,
        ),
        (
            serde_json::json!({ "additionalRuleGroupAnnotations": { "mygroup": { "foo": 7 } } }),
            false,
        ),
        (
            serde_json::json!({ "additionalRuleGroupAnnotations": { "mygroup": { "foo": "x" } } }),
            true,
        ),
        // Shadowed corner: the rule layer supplies the same key with a
        // string, so the group's numeric member never renders.
        (
            serde_json::json!({
                "additionalRuleGroupAnnotations": { "mygroup": { "foo": 7 } },
                "additionalRuleAnnotations": { "MyAlert": { "foo": "x" } }
            }),
            true,
        ),
        // A numeric group member beside an unrelated-key rule map abstains
        // (documented widening: per-key correlation over dynamic names).
        (
            serde_json::json!({
                "additionalRuleGroupAnnotations": { "mygroup": { "foo": 7 } },
                "additionalRuleAnnotations": { "MyAlert": { "bar": "x" } }
            }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "fresh-dict merge layers bind dynamic-member payloads per layer: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The airflow scope-list candidate helper (range over `(list LOCAL $)`
/// with `hasKey`+truthy checks and `break`) resolves to guarded values
/// candidates, so an include splice at a provider slot types each
/// candidate's members. Pins the DECODED half of the F80 securityContext
/// lane: the real chart still abstains behind the nil-scrubbed celery
/// merge layer, which is the recorded residual.
#[test]
fn candidate_selection_helper_binds_provider_payload_through_scope_list() {
    let helpers = indoc! {r#"
        {{- define "test.podSecurityContext" }}
          {{- $ := last . }}
          {{- $result := dict }}
          {{- range . }}
            {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod }}
              {{- $result = .securityContexts.pod }}
              {{- break }}
            {{- end }}
            {{- if and (hasKey . "securityContext") .securityContext }}
              {{- $result = .securityContext }}
              {{- break }}
            {{- end }}
          {{- end }}
          {{- if $result }}
            {{- toYaml $result | print }}
          {{- else }}
        runAsUser: {{ $.uid }}
          {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- $securityContext := include "test.podSecurityContext" (list .Values.workers .Values) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              securityContext: {{ $securityContext | nindent 8 }}
              containers:
                - name: main
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(
            "uid: 50000\nworkers:\n  securityContexts: {}\n  securityContext: {}\nsecurityContexts: {}\nsecurityContext: {}\n",
        ),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": "oops" } } } }),
            false,
        ),
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": 50000 } } } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "scope-list candidate selection keeps provider member typing: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The airflow worker chain end-to-end: `removeNilFields` scrubs the
/// celery overrides, the hand-rolled `workersMergeValues` deep-merge
/// helper layers them over the workers map, and the candidate-selection
/// helper binds the merged `securityContexts.pod` to the provider's pod
/// securityContext. Each layer types its own payload exactly where it
/// supplies the rendered member — the fully-shadowed corner stays open —
/// and the scrubbed layer's members admit null (the scrub drops them
/// before the sink renders).
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn nil_scrubbed_merge_helper_layers_bind_candidate_provider_payloads() {
    let helpers = indoc! {r#"
        {{- define "test.podSecurityContext" }}
          {{- $ := last . }}
          {{- $result := dict }}
          {{- range . }}
            {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod }}
              {{- $result = .securityContexts.pod }}
              {{- break }}
            {{- end }}
            {{- if and (hasKey . "securityContext") .securityContext }}
              {{- $result = .securityContext }}
              {{- break }}
            {{- end }}
          {{- end }}
          {{- if $result }}
            {{- toYaml $result | print }}
          {{- else }}
        runAsUser: {{ $.uid }}
          {{- end }}
        {{- end }}
        {{- define "removeNilFields" -}}
          {{- $newValues := dict -}}
          {{- range $key, $val := . -}}
            {{- if kindIs "map" $val -}}
              {{- $nested := include "removeNilFields" $val | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $newValues $key $nested -}}
              {{- end -}}
            {{- else if not (kindIs "invalid" $val) -}}
              {{- $_ := set $newValues $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $newValues -}}
        {{- end -}}
        {{- define "workersMergeValues" -}}
          {{- $inputMap := index . 0 -}}
          {{- $overwriteMap := index . 1 -}}
          {{- $sectionName := index . 2 -}}
          {{- $orBoolean := index . 3 -}}
          {{- $outputMap := dict -}}
          {{- $fullOverwrite := list "annotations" "podAnnotations" "securityContext" "resources" "nodeSelector" "affinity" "labels" -}}
          {{- range $key, $val := $inputMap -}}
            {{- if and (hasKey $overwriteMap $key) (has $key $fullOverwrite) -}}
              {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
            {{- else if and (hasKey $overwriteMap $key) (kindIs "map" $val) -}}
              {{- $nested := include "workersMergeValues" (list $val (get $overwriteMap $key) $key $orBoolean) | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $outputMap $key $nested -}}
              {{- end -}}
            {{- else if and (hasKey $overwriteMap $key) (not (and (kindIs "slice" (get $overwriteMap $key)) (eq (len (get $overwriteMap $key)) 0))) -}}
              {{- if and (kindIs "bool" $val) (has $sectionName $orBoolean) -}}
                {{- $_ := set $outputMap $key (or $val (get $overwriteMap $key)) -}}
              {{- else -}}
                {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
              {{- end -}}
            {{- else -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- range $key, $val := $overwriteMap -}}
            {{- if not (hasKey $inputMap $key) -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $outputMap -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- $filteredCelery := include "removeNilFields" .Values.workers.celery | fromYaml -}}
        {{- $workers := include "workersMergeValues" (list .Values.workers $filteredCelery "" list) | fromYaml -}}
        {{- $securityContext := include "test.podSecurityContext" (list $workers .Values) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              securityContext: {{ $securityContext | nindent 8 }}
              containers:
                - name: main
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(
            "uid: 50000\nworkers:\n  securityContexts: {}\n  celery:\n    securityContexts: {}\nsecurityContexts: {}\nsecurityContext: {}\n",
        ),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "workers": {
                "securityContexts": { "pod": { "runAsUser": "oops" } },
                "celery": { "securityContexts": { "pod": { "runAsUser": 50000 } } } } }),
            true,
        ),
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": "oops" } } } }),
            false,
        ),
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": 50000 } } } }),
            true,
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": "oops" } } } } }),
            false,
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": 50000 } } } } }),
            true,
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": null, "fsGroup": 5 } } } } }),
            true,
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": null } } } } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "scrubbed merge-helper layer polarity: instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The airflow worker chain THROUGH the per-set reroot: each
/// `workers.celery.sets[]` entry merges over the celery-merged workers map,
/// `set $globals.Values "workers" $workers` rebinds the values context,
/// and the with-body's `.Values.workers` selectors feed the
/// candidate-selection helper. The layered identities must survive the
/// reroot — string `runAsUser` rejects through the base and celery layers
/// — while the per-set member kinds keep their round-8/17 capture arms
/// (scalar `labels` in a set terminates the merge recursion).
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn rerooted_worker_set_merges_keep_layered_provider_payloads() {
    let helpers = indoc! {r#"
        {{- define "test.podSecurityContext" }}
          {{- $ := last . }}
          {{- $result := dict }}
          {{- range . }}
            {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod }}
              {{- $result = .securityContexts.pod }}
              {{- break }}
            {{- end }}
            {{- if and (hasKey . "securityContext") .securityContext }}
              {{- $result = .securityContext }}
              {{- break }}
            {{- end }}
          {{- end }}
          {{- if $result }}
            {{- toYaml $result | print }}
          {{- else }}
        runAsUser: {{ $.uid }}
        fsGroup: {{ $.gid }}
          {{- end }}
        {{- end }}
        {{- define "removeNilFields" -}}
          {{- $newValues := dict -}}
          {{- range $key, $val := . -}}
            {{- if kindIs "map" $val -}}
              {{- $nested := include "removeNilFields" $val | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $newValues $key $nested -}}
              {{- end -}}
            {{- else if not (kindIs "invalid" $val) -}}
              {{- $_ := set $newValues $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $newValues -}}
        {{- end -}}
        {{- define "workersMergeValues" -}}
          {{- $inputMap := index . 0 -}}
          {{- $overwriteMap := index . 1 -}}
          {{- $sectionName := index . 2 -}}
          {{- $orBoolean := index . 3 -}}
          {{- $outputMap := dict -}}
          {{- $fullOverwrite := list "annotations" "podAnnotations" "securityContext" "resources" "nodeSelector" "affinity" "labels" -}}
          {{- range $key, $val := $inputMap -}}
            {{- if and (hasKey $overwriteMap $key) (has $key $fullOverwrite) -}}
              {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
            {{- else if and (hasKey $overwriteMap $key) (kindIs "map" $val) -}}
              {{- $nested := include "workersMergeValues" (list $val (get $overwriteMap $key) $key $orBoolean) | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $outputMap $key $nested -}}
              {{- end -}}
            {{- else if and (hasKey $overwriteMap $key) (not (and (kindIs "slice" (get $overwriteMap $key)) (eq (len (get $overwriteMap $key)) 0))) -}}
              {{- if and (kindIs "bool" $val) (has $sectionName $orBoolean) -}}
                {{- $_ := set $outputMap $key (or $val (get $overwriteMap $key)) -}}
              {{- else -}}
                {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
              {{- end -}}
            {{- else -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- range $key, $val := $overwriteMap -}}
            {{- if not (hasKey $inputMap $key) -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $outputMap -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- $globals := deepCopy . -}}
        {{- $filteredCelery := include "removeNilFields" .Values.workers.celery | fromYaml -}}
        {{- $mergedWorkers := include "workersMergeValues" (list .Values.workers $filteredCelery "" list) | fromYaml -}}
        {{- $_ := unset $mergedWorkers "celery" -}}
        {{- $workerSets := .Values.workers.celery.sets | default list -}}
        {{- range $workerSet := $workerSets -}}
        {{- $workers := include "workersMergeValues" (list $mergedWorkers $workerSet "" list) | fromYaml -}}
        {{- $_ := set $globals.Values "workers" $workers -}}
        {{- with $globals -}}
        {{- if or (contains "CeleryExecutor" .Values.executor) (contains "CeleryKubernetesExecutor" .Values.executor) }}
        {{- $securityContext := include "test.podSecurityContext" (list .Values.workers .Values) }}
        ---
        apiVersion: apps/v1
        kind: {{ if .Values.workers.persistence.enabled }}StatefulSet{{ else }}Deployment{{ end }}
        metadata:
          name: test-{{ $workerSet.name }}
          labels:
        {{- if or .Values.labels .Values.workers.labels }}
          {{- mustMerge .Values.workers.labels .Values.labels | toYaml | nindent 4 }}
        {{- end }}
        spec:
          template:
            spec:
              securityContext: {{ $securityContext | nindent 8 }}
              containers:
                - name: main
        {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {r#"
            uid: 50000
            gid: 0
            executor: "CeleryExecutor"
            securityContext: {}
            securityContexts:
              pod: {}
            workers:
              securityContext: {}
              securityContexts:
                pod: {}
                container: {}
              labels: {}
              persistence:
                enabled: false
              celery:
                enableDefault: true
                sets: []
                securityContext: {}
                securityContexts:
                  pod: {}
                  container: {}
        "#}),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": "oops" } } } }),
            false,
            "base-layer string runAsUser reaches the rendered pod context",
        ),
        (
            serde_json::json!({ "workers": { "securityContexts": { "pod": { "runAsUser": 50000 } } } }),
            true,
            "base-layer integer runAsUser renders",
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": "oops" } } } } }),
            false,
            "celery-layer string runAsUser reaches the rendered pod context",
        ),
        (
            serde_json::json!({ "workers": { "celery": { "securityContexts": { "pod": { "runAsUser": null } } } } }),
            true,
            "the scrub drops null members before the sink",
        ),
        (
            serde_json::json!({ "workers": {
                "securityContexts": { "pod": { "runAsUser": "oops" } },
                "celery": { "securityContexts": { "pod": { "runAsUser": 50000 } } } } }),
            true,
            "the celery layer shadows the base member",
        ),
        (
            serde_json::json!({ "workers": { "celery": { "sets": [
                { "name": "heavy", "labels": "oops" }
            ] } } }),
            false,
            "scalar labels in a worker set terminates the merge recursion",
        ),
        (
            serde_json::json!({ "workers": { "celery": { "sets": [
                { "name": "heavy", "labels": { "tier": "heavy" } }
            ] } } }),
            true,
            "map labels override renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "rerooted worker-set layers ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The per-set merge consumed DIRECTLY (no reroot): layering a
/// `sets[]` range member over the celery-scrubbed workers base keeps the
/// base's layered identity — the scrub marker survives on the set-free
/// operand — so the candidate-selection payload types exactly.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn per_set_merge_layers_bind_without_the_reroot() {
    let helpers = indoc! {r#"
        {{- define "test.podSecurityContext" }}
          {{- $ := last . }}
          {{- $result := dict }}
          {{- range . }}
            {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod }}
              {{- $result = .securityContexts.pod }}
              {{- break }}
            {{- end }}
          {{- end }}
          {{- if $result }}
            {{- toYaml $result | print }}
          {{- else }}
        runAsUser: {{ $.uid }}
          {{- end }}
        {{- end }}
        {{- define "removeNilFields" -}}
          {{- $newValues := dict -}}
          {{- range $key, $val := . -}}
            {{- if kindIs "map" $val -}}
              {{- $nested := include "removeNilFields" $val | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $newValues $key $nested -}}
              {{- end -}}
            {{- else if not (kindIs "invalid" $val) -}}
              {{- $_ := set $newValues $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $newValues -}}
        {{- end -}}
        {{- define "workersMergeValues" -}}
          {{- $inputMap := index . 0 -}}
          {{- $overwriteMap := index . 1 -}}
          {{- $sectionName := index . 2 -}}
          {{- $orBoolean := index . 3 -}}
          {{- $outputMap := dict -}}
          {{- $fullOverwrite := list "labels" "securityContext" "resources" -}}
          {{- range $key, $val := $inputMap -}}
            {{- if and (hasKey $overwriteMap $key) (has $key $fullOverwrite) -}}
              {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
            {{- else if and (hasKey $overwriteMap $key) (kindIs "map" $val) -}}
              {{- $nested := include "workersMergeValues" (list $val (get $overwriteMap $key) $key $orBoolean) | fromYaml -}}
              {{- if gt (len $nested) 0 -}}
                {{- $_ := set $outputMap $key $nested -}}
              {{- end -}}
            {{- else if and (hasKey $overwriteMap $key) (not (and (kindIs "slice" (get $overwriteMap $key)) (eq (len (get $overwriteMap $key)) 0))) -}}
              {{- $_ := set $outputMap $key (get $overwriteMap $key) -}}
            {{- else -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- range $key, $val := $overwriteMap -}}
            {{- if not (hasKey $inputMap $key) -}}
              {{- $_ := set $outputMap $key $val -}}
            {{- end -}}
          {{- end -}}
          {{- toYaml $outputMap -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- $filteredCelery := include "removeNilFields" .Values.workers.celery | fromYaml -}}
        {{- $mergedWorkers := include "workersMergeValues" (list .Values.workers $filteredCelery "" list) | fromYaml -}}
        {{- $_ := unset $mergedWorkers "celery" -}}
        {{- range $workerSet := .Values.workers.celery.sets -}}
        {{- $workers := include "workersMergeValues" (list $mergedWorkers $workerSet "" list) | fromYaml -}}
        {{- $securityContext := include "test.podSecurityContext" (list $workers $.Values) }}
        ---
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test-{{ $workerSet.name }}
        spec:
          template:
            spec:
              securityContext: {{ $securityContext | nindent 8 }}
              containers:
                - name: main
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(
            "uid: 50000\nworkers:\n  securityContexts: {}\n  celery:\n    sets: []\n    securityContexts: {}\n",
        ),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({ "workers": {
                "securityContexts": { "pod": { "runAsUser": "oops" } },
                "celery": { "sets": [ { "name": "a" } ] } } }),
            false,
            "base-layer string runAsUser",
        ),
        (
            serde_json::json!({ "workers": {
                "securityContexts": { "pod": { "runAsUser": 50000 } },
                "celery": { "sets": [ { "name": "a" } ] } } }),
            true,
            "base-layer integer runAsUser",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "per-set direct consumption ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The kube-prometheus-stack annotation-override shape: a `hasKey` gate
/// over `mergeOverwrite (dict) $group $rule` whose group layer is a
/// `default (dict)` selection chain. The chain's empty-dict tail drops
/// out of the presence decode — selection only reaches the tail when the
/// raw path is falsy, and a present key makes it truthy — so the gate
/// reads presence from both annotation paths exactly and the composed
/// `runbook_url` splice keeps its array rejection where neither layer
/// supplies the key (an array breaks the rendered YAML; helm aborts).
#[test]
fn selection_chain_merge_layers_keep_the_has_key_gated_splice() {
    let src = indoc! {r#"
        {{- if .Values.defaultRules.create }}
        apiVersion: monitoring.coreos.com/v1
        kind: PrometheusRule
        metadata:
          name: test
        spec:
          groups:
          - name: alertmanager.rules
            rules:
            - alert: AlertmanagerFailedReload
              annotations:
        {{- $ruleAnnotations := dig "AlertmanagerFailedReload" (dict) .Values.defaultRules.additionalRuleAnnotations }}
        {{- $groupAnnotations := default (dict) .Values.defaultRules.additionalRuleGroupAnnotations.alertmanager }}
        {{- $additionalAnnotations := mergeOverwrite (dict) $groupAnnotations $ruleAnnotations }}
        {{- if $additionalAnnotations }}
        {{ toYaml $additionalAnnotations | indent 8 }}
        {{- end }}
                {{- if not (hasKey $additionalAnnotations "runbook_url") }}
                runbook_url: {{ .Values.defaultRules.runbookUrl }}/alertmanager/alertmanagerfailedreload
                {{- end }}
              expr: vector(1)
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {r#"
            defaultRules:
              create: true
              additionalRuleAnnotations: {}
              additionalRuleGroupAnnotations:
                alertmanager: {}
              runbookUrl: "https://runbooks.example/runbooks"
        "#}),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({ "defaultRules": { "create": true, "runbookUrl": [] } }),
            false,
            "live array splice",
        ),
        (
            serde_json::json!({ "defaultRules": { "create": true, "runbookUrl": "https://x" } }),
            true,
            "live string splice",
        ),
        // A non-collection scalar renders as scalar text inside the
        // composed line; only arrays break the rendered YAML.
        (
            serde_json::json!({ "defaultRules": { "create": true, "runbookUrl": 7 } }),
            true,
            "live integer splice",
        ),
        (
            serde_json::json!({ "defaultRules": {
                "create": true,
                "runbookUrl": [],
                "additionalRuleGroupAnnotations": { "alertmanager": { "runbook_url": "x" } } } }),
            true,
            "group annotation shadows the splice",
        ),
        (
            serde_json::json!({ "defaultRules": {
                "create": true,
                "runbookUrl": [],
                "additionalRuleAnnotations": { "AlertmanagerFailedReload": { "runbook_url": "x" } } } }),
            true,
            "rule annotation shadows the splice",
        ),
        (
            serde_json::json!({ "defaultRules": { "create": false, "runbookUrl": [] } }),
            true,
            "dormant rule document",
        ),
        (serde_json::json!({}), true, "empty document"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "selection-chain merge layer presence ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}
