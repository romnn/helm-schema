//! IR-level regression tests for the typed-AST value-path and condition
//! extractors. Unlike the unit tests on the extractors themselves (which feed
//! text directly into `parse_condition` / `extract_values_paths`), these run the full
//! [`SymbolicIrContext`] projection pipeline against minimal in-memory chart
//! fixtures so we lock in the behaviour at the surface every
//! downstream consumer actually sees.
//!
//! Each test pins a specific shape that the typed-AST extractors must
//! preserve from the previous regex implementation:
//!   - destructuring range headers (`range $k, $v := .Values.map`)
//!     contribute their range expression to the IR (and propagate a
//!     Truthy guard to uses inside the body).
//!   - helper-context chains (`.context.Values.X` inside an included
//!     helper) emit a value use for `X` even though `Values` is the
//!     second selector segment, not the root.

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractUse, Guard, GuardValue, SymbolicIrContext};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn generate(template: &str, helpers: &str) -> Vec<ContractUse> {
    let mut idx = DefineIndex::new();
    if !helpers.is_empty() {
        idx.add_file_source("helpers.tpl", helpers);
    }
    SymbolicIrContext::new(&idx)
        .generate_contract_ir(template)
        .finalize()
        .uses()
        .to_vec()
}

fn truthy(p: &str) -> Guard {
    Guard::Truthy {
        path: p.to_string(),
    }
}

fn range_guard(p: &str) -> Guard {
    Guard::Range {
        path: p.to_string(),
    }
}

fn eq(p: &str, value: &str) -> Guard {
    Guard::Eq {
        path: p.to_string(),
        value: GuardValue::string(value),
    }
}

fn not(p: &str) -> Guard {
    Guard::Not {
        path: p.to_string(),
    }
}

#[test]
fn destructuring_range_header_emits_value_use_for_range_expression() {
    // `{{ range $k, $v := .Values.map }}` — the destructured `$k`/`$v`
    // variables are noise, but `.Values.map` is a real reference that
    // must surface in the IR. The header itself is unconditional, so
    // the use carries no guards; uses inside the body inherit a
    // Truthy guard on `map` (covered by
    // `range_body_uses_inherit_truthy_guard_on_destructured_source`).
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- range $k, $v := .Values.map }}
          {{ $k }}: "{{ $v }}"
        {{- end }}
    "#};

    let ir = generate(template, "");

    let map_uses: Vec<&ContractUse> = ir.iter().filter(|u| u.source_expr == "map").collect();
    sim_assert_eq!(
        have: map_uses.len(),
        want: 2,
        "expected a header scalar use plus a fragment use for `map`; \
         got: {ir:#?}",
    );

    let header_use = map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0.is_empty() && use_.kind == helm_schema_ir::ValueKind::Scalar)
        .expect("header scalar use present");

    assert!(
        header_use.guards == [range_guard("map")],
        "the destructured range header's source use should carry only Range(map); \
         got guards: {:?}",
        header_use.guards,
    );

    let fragment_use = map_uses
        .iter()
        .copied()
        .find(|use_| {
            use_.path.0 == ["data".to_string()] && use_.kind == helm_schema_ir::ValueKind::Fragment
        })
        .expect("range fragment use present");
    assert!(
        fragment_use.guards.contains(&range_guard("map")),
        "the fragment use should inherit Range(map); got guards: {:?}",
        fragment_use.guards,
    );
}

#[test]
fn destructuring_range_header_with_helper_call_inside_range_expression() {
    // The range expression itself can be a function call wrapping the
    // Values reference. The IR must still surface every `.Values.*`
    // reference reachable through it — the typed AST recurses into
    // call arguments just like the old regex matched anywhere.
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- range $k, $v := default (dict) .Values.fallbackMap }}
          {{ $k }}: "{{ $v }}"
        {{- end }}
    "#};

    let ir = generate(template, "");

    let fallback_map_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == "fallbackMap")
        .collect();
    sim_assert_eq!(
        have: fallback_map_uses.len(),
        want: 2,
        "expected a header scalar use plus a fragment use for `fallbackMap`; got: {ir:#?}",
    );

    let header_use = fallback_map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0.is_empty() && use_.kind == helm_schema_ir::ValueKind::Scalar)
        .expect("header scalar use present");
    assert!(
        header_use.guards == [range_guard("fallbackMap")],
        "destructured range header use should carry only Range(fallbackMap), but got \
         guards: {:?}",
        header_use.guards,
    );

    let fragment_use = fallback_map_uses
        .iter()
        .copied()
        .find(|use_| {
            use_.path.0 == ["data".to_string()] && use_.kind == helm_schema_ir::ValueKind::Fragment
        })
        .expect("range fragment use present");
    assert!(
        fragment_use.guards.contains(&range_guard("fallbackMap")),
        "the fragment use should inherit Range(fallbackMap); got guards: {:?}",
        fragment_use.guards,
    );

    // No phantom paths from misparsing `default`, `(dict)`, etc.
    let phantoms: Vec<&str> = ir
        .iter()
        .map(|u| u.source_expr.as_str())
        .filter(|s| matches!(*s, "dict" | "default"))
        .collect();
    assert!(
        phantoms.is_empty(),
        "no `default`/`dict` builtin should surface as a `.Values.*` path; \
         got: {phantoms:?}",
    );
}

#[test]
fn range_body_uses_inherit_truthy_guard_on_destructured_source() {
    // Inside the destructured range body, any `.Values.X` reference
    // must carry a Truthy guard on the destructured source (`themap`).
    // This proves the guard pushed by the range header propagates into
    // body uses — the exact contract `SymbolicIrContext` requires.
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          fallback: "{{ .Values.fallback }}"
        {{- range $k, $v := .Values.themap }}
          {{ $k }}: "{{ $v }}-{{ $.Values.suffix }}"
        {{- end }}
    "#};

    let ir = generate(template, "");

    // `suffix` is referenced INSIDE the range body — must carry the
    // Truthy(themap) guard. The IR shape for this fixture is precise
    // enough to pin exactly one use; a regression that emitted a
    // second wrongly-guarded duplicate would fail this assertion.
    let suffix_uses: Vec<&ContractUse> = ir.iter().filter(|u| u.source_expr == "suffix").collect();
    sim_assert_eq!(
        have: suffix_uses.len(),
        want: 1,
        "expected exactly one `suffix` use inside range body; got: {ir:#?}",
    );
    assert!(
        suffix_uses[0].guards.contains(&range_guard("themap")),
        "expected `suffix` use guarded by Range(themap); got: {:?}",
        suffix_uses[0].guards,
    );

    // `fallback` is OUTSIDE the range — must NOT carry that guard.
    let fallback_uses: Vec<&ContractUse> =
        ir.iter().filter(|u| u.source_expr == "fallback").collect();
    assert!(
        !fallback_uses.is_empty(),
        "expected `fallback` use outside range; got: {ir:#?}",
    );
    assert!(
        fallback_uses
            .iter()
            .all(|u| !u.guards.contains(&truthy("themap"))),
        "`fallback` use outside the range body must NOT inherit the \
         range's Truthy guard; got: {fallback_uses:#?}",
    );
}

#[test]
fn branch_assignment_to_outer_local_survives_as_choice_after_if() {
    let template = indoc! {r#"
        {{- $name := .Values.primary }}
        {{- if .Values.useFallback }}
        {{- $name = .Values.fallback }}
        {{- else }}
        {{- $name = .Values.secondary }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          name: "{{ $name }}"
    "#};

    let ir = generate(template, "");
    let rendered_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "name".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        rendered_sources.contains("fallback"),
        "then-branch assignment to outer local must be visible after the if: {ir:#?}"
    );
    assert!(
        rendered_sources.contains("secondary"),
        "else-branch assignment to outer local must be visible after the if: {ir:#?}"
    );
    assert!(
        !rendered_sources.contains("primary"),
        "both branches overwrite the outer local, so the pre-branch value should not remain: {ir:#?}"
    );
}

#[test]
fn scalar_helper_output_assigned_to_local_keeps_value_source() {
    let helpers = indoc! {r#"
        {{- define "test.fullname" -}}
        {{- .Values.fullnameOverride | default "app" -}}
        {{- end -}}

        {{- define "test.password" -}}
        {{- $password := "" -}}
        {{- $secretData := (lookup "v1" "Secret" "default" .secret).data -}}
        {{- if $secretData -}}
        {{- $password = index $secretData .key -}}
        {{- else if .defaultValue -}}
        {{- $password = .defaultValue | toString -}}
        {{- end -}}
        {{- printf "%s" $password -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $password := include "test.password" (dict "secret" (include "test.fullname" .) "key" .Values.auth.secretKey "defaultValue" .Values.auth.password "context" $) }}
        apiVersion: v1
        kind: Secret
        data:
          password: {{ $password | quote }}
    "#};

    let ir = generate(template, helpers);
    let password_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        password_sources.contains("auth.password"),
        "helper scalar output assigned to a local must preserve its rendered value source: {ir:#?}"
    );
    assert!(
        !password_sources.contains("auth.secretKey")
            && !password_sources.contains("fullnameOverride"),
        "lookup secret/key inputs are dependencies of the helper call, not the rendered password value: {ir:#?}"
    );
}

#[test]
fn helper_analysis_ignores_nested_define_bodies_but_keeps_outer_output() {
    let helpers = indoc! {r#"
        {{- define "test.outer" -}}
        {{- define "test.inner" -}}
        {{- .Values.shouldNotLeak -}}
        {{- end -}}
        {{- .Values.actual -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          value: {{ include "test.outer" . | quote }}
    "#};

    let ir = generate(template, helpers);
    let value_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "value".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        value_sources.contains("actual"),
        "outer helper output should survive nested helper definitions: {ir:#?}"
    );
    assert!(
        !value_sources.contains("shouldNotLeak"),
        "nested define bodies must stay suppressed during outer helper analysis: {ir:#?}"
    );
}

#[test]
fn split_path_helper_resolves_dynamic_values_indexing() {
    let helpers = indoc! {r#"
        {{- define "test.getValueFromKey" -}}
        {{- $splitKey := splitList "." .key -}}
        {{- $value := "" -}}
        {{- $latestObj := $.context.Values -}}
        {{- range $splitKey -}}
        {{- $value = (index $latestObj .) -}}
        {{- $latestObj = $value -}}
        {{- end -}}
        {{- printf "%v" (default "" $value) -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $password := include "test.getValueFromKey" (dict "key" "auth.password" "context" $) }}
        apiVersion: v1
        kind: Secret
        data:
          password: {{ $password | quote }}
    "#};

    let ir = generate(template, helpers);
    let password_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        password_sources.contains("auth.password"),
        "splitList/range/index helper should resolve string path keys into Values paths: {ir:#?}"
    );
}

#[test]
fn split_path_helper_resolves_multisegment_key_to_leaf_only() {
    let helpers = indoc! {r#"
        {{- define "test.getValueFromKey" -}}
        {{- $splitKey := splitList "." .key -}}
        {{- $value := "" -}}
        {{- $latestObj := $.context.Values -}}
        {{- range $splitKey -}}
        {{- $value = (index $latestObj .) -}}
        {{- $latestObj = $value -}}
        {{- end -}}
        {{- printf "%v" (default "" $value) -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $password := include "test.getValueFromKey" (dict "key" "global.auth.password" "context" $) }}
        apiVersion: v1
        kind: Secret
        data:
          password: {{ $password | quote }}
    "#};

    let ir = generate(template, helpers);
    let password_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        password_sources.contains("global.auth.password"),
        "splitList/range/index helper should resolve the final leaf path: {ir:#?}"
    );
    assert!(
        !password_sources.contains("global") && !password_sources.contains("global.auth"),
        "intermediate traversal prefixes are interpreter state, not rendered values: {ir:#?}"
    );
}

#[test]
fn split_path_helper_resolves_key_selected_by_helper() {
    let helpers = indoc! {r#"
        {{- define "test.getValueFromKey" -}}
        {{- $splitKey := splitList "." .key -}}
        {{- $value := "" -}}
        {{- $latestObj := $.context.Values -}}
        {{- range $splitKey -}}
        {{- $value = (index $latestObj .) -}}
        {{- $latestObj = $value -}}
        {{- end -}}
        {{- printf "%v" (default "" $value) -}}
        {{- end -}}

        {{- define "test.getKeyFromList" -}}
        {{- $key := first .keys -}}
        {{- $reverseKeys := reverse .keys -}}
        {{- range $reverseKeys -}}
        {{- $value := include "test.getValueFromKey" (dict "key" . "context" $.context) -}}
        {{- if $value -}}
        {{- $key = . -}}
        {{- end -}}
        {{- end -}}
        {{- printf "%s" $key -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $key := include "test.getKeyFromList" (dict "keys" (list "global.auth.password" "auth.password") "context" $) }}
        {{- $password := include "test.getValueFromKey" (dict "key" $key "context" $) }}
        apiVersion: v1
        kind: Secret
        data:
          password: {{ $password | quote }}
    "#};

    let ir = generate(template, helpers);
    let password_sources: std::collections::BTreeSet<&str> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.as_str())
        .collect();

    assert!(
        password_sources.contains("auth.password")
            && password_sources.contains("global.auth.password"),
        "helper-selected string keys should resolve into Values paths: {ir:#?}"
    );
}

#[test]
fn else_branch_uses_inherit_negated_if_guard_without_leaking() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- if .Values.enabled }}
          primary: "{{ .Values.primary }}"
        {{- else }}
          fallback: "{{ .Values.fallback }}"
        {{- end }}
          after: "{{ .Values.after }}"
    "#};

    let ir = generate(template, "");

    let primary_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|use_| use_.source_expr == "primary")
        .collect();
    sim_assert_eq!(
        have: primary_uses.len(),
        want: 1,
        "expected exactly one then-branch use; got {ir:#?}",
    );
    assert!(
        primary_uses[0].guards.contains(&truthy("enabled")),
        "then-branch use should inherit Truthy(enabled); got {:?}",
        primary_uses[0].guards,
    );

    let fallback_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|use_| use_.source_expr == "fallback")
        .collect();
    sim_assert_eq!(
        have: fallback_uses.len(),
        want: 1,
        "expected exactly one else-branch use; got {ir:#?}",
    );
    assert!(
        fallback_uses[0].guards.contains(&not("enabled")),
        "else-branch use should inherit Not(enabled); got {:?}",
        fallback_uses[0].guards,
    );

    let after_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|use_| use_.source_expr == "after")
        .collect();
    sim_assert_eq!(
        have: after_uses.len(),
        want: 1,
        "expected exactly one post-branch use; got {ir:#?}",
    );
    assert!(
        !after_uses[0].guards.contains(&truthy("enabled"))
            && !after_uses[0].guards.contains(&not("enabled")),
        "branch guards must not leak to following nodes; got {:?}",
        after_uses[0].guards,
    );
}

#[test]
fn local_storage_class_alias_emits_guarded_leaf_use() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        spec:
          resources:
            requests:
              storage: {{ .Values.persistence.size | quote }}
          {{- $storageClass := (.Values.global).storageClass | default .Values.persistence.storageClass | default (.Values.global).defaultStorageClass | default "" -}}
          {{- if $storageClass -}}
          storageClassName: {{ $storageClass }}
          {{- end -}}
    "#};

    let ir = generate(template, "");
    let matching_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|use_| use_.source_expr == "global.storageClass")
        .collect();
    assert!(
        matching_uses.iter().any(|use_| {
            use_.path.0 == ["spec".to_string(), "storageClassName".to_string()]
                && use_.guards.contains(&truthy("global.storageClass"))
                && use_.guards.contains(&Guard::Default {
                    path: "global.storageClass".to_string(),
                })
        }),
        "expected a rendered `storageClassName` use for global.storageClass carrying both Truthy and Default guards; got {ir:#?}",
    );
}

#[test]
fn scalar_item_range_keeps_parent_collection_path() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        metadata:
          name: test
        spec:
          accessModes:
          {{- range .Values.accessModes }}
            - {{ . | quote }}
          {{- end }}
    "#};

    let ir = generate(template, "");

    let parent_use = ir
        .iter()
        .find(|use_| {
            use_.source_expr == "accessModes"
                && use_.path.0 == ["spec".to_string(), "accessModes".to_string()]
                && use_.kind == helm_schema_ir::ValueKind::Scalar
        })
        .expect("scalar-item range should keep a collection-level parent use");
    assert!(
        parent_use.guards == [range_guard("accessModes")],
        "the collection-level range header use should carry only Range(accessModes); got: {:?}",
        parent_use.guards,
    );

    let item_use = ir
        .iter()
        .find(|use_| {
            use_.source_expr == "accessModes.*"
                && use_.path.0 == ["spec".to_string(), "accessModes[*]".to_string()]
                && use_.kind == helm_schema_ir::ValueKind::Scalar
        })
        .expect("scalar-item range should still emit per-item uses");
    assert!(
        item_use.guards.contains(&range_guard("accessModes")),
        "the item use should inherit Range(accessModes); got: {:?}",
        item_use.guards,
    );
}

#[test]
fn scalar_range_wrapped_into_object_item_stays_on_leaf_path() {
    let template = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
          {{- range .Values.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                {{- range .paths }}
                  - path: {{ . | quote }}
                    pathType: Prefix
                    backend:
                      service:
                        name: app
                        port:
                          number: 80
                {{- end }}
          {{- end }}
    "#};

    let ir = generate(template, "");

    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == "hosts.*.paths"
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "rules[*]".to_string(),
                        "http".to_string(),
                        "paths".to_string(),
                    ])
        }),
        "the scalar input list should not be projected onto the output object array: {ir:#?}",
    );

    let leaf_use = ir
        .iter()
        .find(|use_| {
            use_.source_expr == "hosts.*.paths.*" && use_.kind == helm_schema_ir::ValueKind::Scalar
        })
        .expect("scalar path item should still surface as a value use");
    assert!(
        leaf_use.guards.contains(&range_guard("hosts"))
            && leaf_use.guards.contains(&range_guard("hosts.*.paths")),
        "the wrapped scalar item use should retain both enclosing range guards; got: {:?}",
        leaf_use.guards,
    );
}

#[test]
fn helper_context_chain_dot_context_values_path_surfaces_as_use() {
    // Chart-helper idiom: caller passes the root context as a named
    // key in a dict (`(dict "context" .)`), and the helper body
    // accesses `.context.Values.X`. The typed-AST extractor's
    // "loose" path search must locate `Values` mid-chain (matching
    // the old regex which matched anywhere) so the schema generator
    // doesn't lose visibility on values referenced this way.
    let helpers = indoc! {r#"
        {{- define "common.field" -}}
        {{- .context.Values.deeplyNested.fieldName -}}
        {{- end -}}
    "#};

    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          field: {{ include "common.field" (dict "context" .) }}
    "#};

    let ir = generate(template, helpers);

    let target_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == "deeplyNested.fieldName")
        .collect();
    sim_assert_eq!(
        have: target_uses.len(),
        want: 1,
        "expected exactly one `deeplyNested.fieldName` use; got: {ir:#?}",
    );

    // The loose path search must NOT additionally surface phantom
    // paths from the `.context.*` operand prefix — `context` is a
    // dict key, not a `.Values.*` reference.
    let phantoms: Vec<&str> = ir
        .iter()
        .map(|u| u.source_expr.as_str())
        .filter(|s| matches!(s.split('.').next(), Some("context" | "Values")))
        .collect();
    assert!(
        phantoms.is_empty(),
        "no `.context.*` or `.Values.*`-prefixed paths should surface from \
         the helper-context chain; got: {phantoms:?}",
    );
}

#[test]
fn quoted_yaml_key_keeps_concrete_leaf_path() {
    let template = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: NetworkPolicy
        metadata:
          name: test
        spec:
          ingress:
            - from:
                - namespaceSelector:
                    matchLabels:
                      "kubernetes.io/metadata.name": "{{ .Values.namespace }}"
    "#};

    let ir = generate(template, "");

    let namespace_uses: Vec<&ContractUse> =
        ir.iter().filter(|u| u.source_expr == "namespace").collect();
    sim_assert_eq!(
        have: namespace_uses.len(),
        want: 1,
        "expected exactly one `namespace` use; got: {ir:#?}",
    );
    let path = &namespace_uses[0].path.0;
    assert!(
        path.contains(&"namespaceSelector".to_string()),
        "quoted YAML key path should still descend through namespaceSelector; got: {path:?}",
    );
    sim_assert_eq!(
        have: path.last().map(String::as_str),
        want: Some("kubernetes.io/metadata.name"),
        "quoted YAML key should become the concrete leaf path segment; got: {path:?}",
    );
}

#[test]
fn exact_helper_dict_dot_arg_uses_current_with_binding() {
    let helpers = indoc! {r#"
        {{- define "common.ingress" -}}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
          {{- with .config.annotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
          {{- if .config.tls }}
          tls:
            {{- range .config.tls }}
            - secretName: {{ .secretName }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .config.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    backend:
                      service:
                        port:
                          number: {{ $.ctx.Values.service.port }}
                  {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- with .Values.ingress -}}
        {{- if .enabled -}}
        {{ include "common.ingress" (dict "ctx" $ "config" .) }}
        {{- end -}}
        {{- end -}}
    "#};

    let ir = generate(template, helpers);
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!("{ir:#?}");
    }

    assert!(
        ir.iter()
            .any(|use_| use_.source_expr == "ingress.className"),
        "expected helper body to resolve .config.className through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == "ingress.className"
                && use_.path.0 == ["spec".to_string(), "ingressClassName".to_string()]
                && use_.kind == helm_schema_ir::ValueKind::Scalar
        }),
        "expected helper body to attach ingress.className to spec.ingressClassName: {ir:#?}",
    );
    assert!(
        ir.iter()
            .any(|use_| use_.source_expr == "ingress.annotations"),
        "expected helper body to resolve .config.annotations through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| use_.source_expr == "ingress.tls"),
        "expected helper body to resolve .config.tls through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter()
            .any(|use_| use_.source_expr == "ingress.hosts.*.host"),
        "expected helper body to resolve .config.hosts[*].host through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == "ingress.hosts"
                && use_.path.0 == ["spec".to_string(), "rules".to_string()])
        }),
        "the hosts input should not be projected onto the full rendered IngressRule shape: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == "ingress.hosts.*.paths"
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "rules[*]".to_string(),
                        "http".to_string(),
                        "paths".to_string(),
                    ])
        }),
        "the nested paths input should not be projected onto the rendered http.paths collection: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| use_.source_expr == "service.port"),
        "expected helper body to keep $.ctx.Values.service.port rooted to the caller context: {ir:#?}",
    );
}

#[test]
fn list_bound_helper_fragment_keeps_metadata_map_paths() {
    let helpers = indoc! {r#"
        {{- define "temporal.resourceAnnotations" -}}
        {{- $global := index . 0 -}}
        {{- $scope := index . 1 -}}
        {{- $resourceType := index . 2 -}}
        {{- $component := "server" -}}
        {{- if (or (eq $scope "admintools") (eq $scope "web")) -}}
        {{- $component = $scope -}}
        {{- end -}}
        {{- with $resourceType -}}
        {{- $resourceTypeKey := printf "%sAnnotations" . -}}
        {{- $componentAnnotations := (index $global.Values $component $resourceTypeKey) -}}
        {{- $scopeAnnotations := dict -}}
        {{- if hasKey (index $global.Values $component) $scope -}}
        {{- $scopeAnnotations = (index $global.Values $component $scope $resourceTypeKey) -}}
        {{- end -}}
        {{- $resourceAnnotations := merge $scopeAnnotations $componentAnnotations -}}
        {{- range $annotation_name, $annotation_value := $resourceAnnotations }}
        {{ $annotation_name }}: {{ $annotation_value | quote }}
        {{- end -}}
        {{- end -}}
        {{- range $annotation_name, $annotation_value := $global.Values.additionalAnnotations }}
        {{ $annotation_name }}: {{ $annotation_value | quote }}
        {{- end -}}
        {{- end -}}

        {{- define "temporal.resourceLabels" -}}
        {{- $global := index . 0 -}}
        {{- $scope := index . 1 -}}
        {{- $resourceType := index . 2 -}}
        {{- $component := "server" -}}
        {{- if (or (eq $scope "admintools") (eq $scope "web")) -}}
        {{- $component = $scope -}}
        {{- end -}}
        {{- with $resourceType -}}
        {{- $resourceTypeKey := printf "%sLabels" . -}}
        {{- $componentLabels := (index $global.Values $component $resourceTypeKey) -}}
        {{- $scopeLabels := dict -}}
        {{- if hasKey (index $global.Values $component) $scope -}}
        {{- $scopeLabels = (index $global.Values $component $scope $resourceTypeKey) -}}
        {{- end -}}
        {{- $resourceLabels := merge $scopeLabels $componentLabels -}}
        {{- range $label_name, $label_value := $resourceLabels }}
        {{ $label_name }}: {{ $label_value | quote }}
        {{- end -}}
        {{- end -}}
        {{- range $label_name, $label_value := $global.Values.additionalLabels }}
        {{ $label_name }}: {{ $label_value | quote }}
        {{- end -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
          annotations:
            {{- include "temporal.resourceAnnotations" (list $ "admintools" "pod") | nindent 4 }}
          labels:
            {{- include "temporal.resourceLabels" (list $ "admintools" "pod") | nindent 4 }}
    "#};

    let ir = generate(template, helpers);
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!("{ir:#?}");
    }

    let annotations_path = ["metadata".to_string(), "annotations".to_string()];
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == "admintools.podAnnotations"
                && use_.path.0 == annotations_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected admintools.podAnnotations to stay attached to metadata.annotations: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == "additionalAnnotations"
                && use_.path.0 == annotations_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected additionalAnnotations to stay attached to metadata.annotations: {ir:#?}",
    );

    let labels_path = ["metadata".to_string(), "labels".to_string()];
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == "admintools.podLabels"
                && use_.path.0 == labels_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected admintools.podLabels to stay attached to metadata.labels: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == "additionalLabels"
                && use_.path.0 == labels_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected additionalLabels to stay attached to metadata.labels: {ir:#?}",
    );
}

#[test]
fn bitnami_tplvalues_merge_list_items_stay_on_labels_path() {
    let helpers = indoc! {r#"
        {{- define "common.tplvalues.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
        {{- if contains "{{" (toJson .value) }}
          {{- if .scope }}
              {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
          {{- else }}
            {{- tpl $value .context }}
          {{- end }}
        {{- else }}
            {{- $value }}
        {{- end }}
        {{- end -}}

        {{- define "common.tplvalues.merge" -}}
        {{- $dst := dict -}}
        {{- range .values -}}
        {{- $dst = include "common.tplvalues.render" (dict "value" . "context" $.context "scope" $.scope) | fromYaml | merge $dst -}}
        {{- end -}}
        {{ $dst | toYaml }}
        {{- end -}}

        {{- define "common.names.name" -}}demo{{- end -}}
        {{- define "common.names.chart" -}}demo{{- end -}}

        {{- define "common.labels.standard" -}}
        {{- $default := dict "app.kubernetes.io/name" (include "common.names.name" .context) "helm.sh/chart" (include "common.names.chart" .context) -}}
        {{ template "common.tplvalues.merge" (dict "values" (list .customLabels $default) "context" .context) }}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $podLabels := include "common.tplvalues.merge" (dict "values" (list .Values.podLabels .Values.commonLabels) "context" .) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          labels: {{- include "common.labels.standard" (dict "customLabels" $podLabels "context" .) | nindent 4 }}
    "#};

    let ir = generate(template, helpers);
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!("{ir:#?}");
    }

    let labels_path = ["metadata".to_string(), "labels".to_string()];
    for source_expr in ["podLabels", "commonLabels"] {
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == source_expr
                    && use_.path.0 == labels_path
                    && use_.kind == helm_schema_ir::ValueKind::Fragment
            }),
            "expected {source_expr} to stay attached to metadata.labels: {ir:#?}",
        );
        assert!(
            ir.iter().all(|use_| {
                !(use_.source_expr == source_expr && use_.path.0.contains(&"values[*]".to_string()))
            }),
            "{source_expr} must not be projected through the helper argument list envelope: {ir:#?}",
        );
    }
}

#[test]
fn conditional_annotations_fragment_stays_under_annotations_path() {
    let template = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          selector:
            matchLabels:
              app: demo
          template:
            metadata:
              annotations:
                checksum/secret: {{ "abc" | quote }}
            {{- if .Values.podAnnotations }}
        {{ toYaml .Values.podAnnotations | indent 8 }}
            {{- end }}
            spec:
              containers:
                - name: demo
                  image: nginx
    "#};

    let ir = generate(template, "");
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!("{ir:#?}");
    }

    let pod_annotations: Vec<&ContractUse> = ir
        .iter()
        .filter(|use_| use_.source_expr == "podAnnotations")
        .collect();
    assert!(
        pod_annotations.iter().any(|use_| {
            use_.path.0
                == [
                    "spec".to_string(),
                    "template".to_string(),
                    "metadata".to_string(),
                    "annotations".to_string(),
                ]
        }),
        "podAnnotations should stay attached to metadata.annotations, got {pod_annotations:#?}",
    );
}

#[test]
fn with_rewritten_selector_chain_does_not_emit_parent_suffix_path() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            {{- with .Values.service }}
            - port: {{ .ports.http.port }}
            {{- end }}
    "#};

    let ir = generate(template, "");

    assert!(
        ir.iter()
            .any(|use_| use_.source_expr == "service.ports.http.port"),
        "expected the full rewritten path to surface; got: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| use_.source_expr != "service.port"),
        "rewritten selector chain should not leak a parent-suffix path; got: {ir:#?}",
    );
}

#[test]
fn helper_context_chain_in_condition_surfaces_referenced_value() {
    // Same context idiom inside an `if` condition — the fragment
    // interpreter's condition decoding must still surface the inner
    // field as an IR use.
    let helpers = indoc! {r#"
        {{- define "common.enabled" -}}
        {{- if .context.Values.featureFlag -}}
        on
        {{- end -}}
        {{- end -}}
    "#};

    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          state: {{ include "common.enabled" (dict "context" .) }}
    "#};

    let ir = generate(template, helpers);

    let flag_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == "featureFlag")
        .collect();
    sim_assert_eq!(
        have: flag_uses.len(),
        want: 1,
        "expected exactly one `featureFlag` use from the helper-context \
         condition; got: {ir:#?}",
    );
    // The condition's source use carries its own decoded condition, the
    // same convention document-level condition reads always had (the
    // summary lane used to flatten these to unconditional reads).
    sim_assert_eq!(
        have: flag_uses[0].guards.clone(),
        want: vec![helm_schema_ir::Guard::Truthy {
            path: "featureFlag".to_string()
        }],
        "helper-context condition source use keeps its own condition",
    );

    // Same absence guarantee as the unconditional helper-context test:
    // the `.context` prefix must not contaminate the IR.
    let phantoms: Vec<&str> = ir
        .iter()
        .map(|u| u.source_expr.as_str())
        .filter(|s| matches!(s.split('.').next(), Some("context" | "Values")))
        .collect();
    assert!(
        phantoms.is_empty(),
        "no `.context.*` or `.Values.*`-prefixed paths should surface from \
         the helper-context condition; got: {phantoms:?}",
    );
}

#[test]
fn template_action_used_in_mapping_key_does_not_project_to_parent_value_path() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{ .Values.account.name }}.json: |
            {}
    "#};

    let ir = generate(template, "");

    assert!(
        ir.iter()
            .any(|use_| { use_.source_expr == "account.name" && use_.path.0.is_empty() }),
        "expected mapping-key interpolation to surface account.name as a pathless scalar use: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == "account.name" && use_.path.0 == ["data".to_string()])
        }),
        "mapping-key interpolation must not project account.name onto ConfigMap.data: {ir:#?}",
    );
}

#[test]
fn inline_scalar_sequence_item_with_mixed_template_gaps_keeps_output_path() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
                - --image={{- if .Values.image.registry -}}{{ .Values.image.registry }}/{{- end -}}{{ .Values.image.repository }}{{- if .Values.image.digest -}}@{{ .Values.image.digest }}{{- end -}}
    "#};

    let ir = generate(template, "");

    for source_expr in ["image.registry", "image.repository", "image.digest"] {
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == source_expr
                    && use_.path.0
                        == [
                            "spec".to_string(),
                            "containers[*]".to_string(),
                            "args[*]".to_string(),
                        ]
            }),
            "expected {source_expr} to stay attached to the args[*] scalar path despite mixed template gaps: {ir:#?}",
        );
    }
}

#[test]
fn with_bound_inline_scalar_sequence_item_with_mixed_template_gaps_keeps_output_path() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
                {{- with .Values.image }}
                - --image={{- if .registry -}}{{ .registry }}/{{- end -}}{{ .repository }}{{- if .digest -}}@{{ .digest }}{{- end -}}
                {{- end }}
    "#};

    let ir = generate(template, "");

    for source_expr in ["image.registry", "image.repository", "image.digest"] {
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == source_expr
                    && use_.path.0
                        == [
                            "spec".to_string(),
                            "containers[*]".to_string(),
                            "args[*]".to_string(),
                        ]
            }),
            "expected {source_expr} to stay attached to the args[*] scalar path inside a with-bound mixed-gap line: {ir:#?}",
        );
    }
}

#[test]
fn eq_condition_with_string_literal_containing_dot_values_does_not_phantom() {
    // The reviewer's original example: `eq .Values.X ".Values.fake"`.
    // Old regex would have extracted both `X` and `fake` from the
    // condition text, fallen past the `eq` branch in parse_condition,
    // and produced two spurious Truthy guards. New typed walker sees
    // the second arg as a `Literal::String` and correctly classifies
    // as `Eq { path: "X", value: ".Values.fake" }`.
    //
    // The body references `.Values.payload` so we can inspect the
    // propagated guard — only that body use carries the condition's
    // guard, and it must be an `Eq` (not a pair of Truthy(mode),
    // Truthy(fake)).
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- if eq .Values.mode ".Values.fake" }}
          payload: "{{ .Values.payload }}"
        {{- end }}
    "#};

    let ir = generate(template, "");

    // No `fake` source_expr should appear anywhere in the IR — it
    // lives inside a string literal, not a real `.Values.fake`
    // reference.
    let fake_uses: Vec<&ContractUse> = ir.iter().filter(|u| u.source_expr == "fake").collect();
    assert!(
        fake_uses.is_empty(),
        "string-literal payload `.Values.fake` leaked into IR as a use: \
         {fake_uses:#?}",
    );

    // The `payload` body use must carry the `Eq` guard from the
    // condition — proving the typed walker classified `eq` correctly.
    // Tighten beyond "contains the Eq": there must be no `Truthy(mode)`
    // or `Truthy(fake)` either, which is what the OLD regex pipeline
    // would have emitted from the contamination fall-through.
    let payload_uses: Vec<&ContractUse> =
        ir.iter().filter(|u| u.source_expr == "payload").collect();
    sim_assert_eq!(
        have: payload_uses.len(),
        want: 1,
        "expected exactly one `payload` use inside the if-body; got: {ir:#?}",
    );
    let payload_guards = &payload_uses[0].guards;
    assert!(
        payload_guards.contains(&eq("mode", ".Values.fake")),
        "expected `payload` body use guarded by Eq(mode, \".Values.fake\"); \
         got: {payload_guards:?}",
    );
    assert!(
        !payload_guards.contains(&truthy("mode")),
        "`payload` use must NOT carry Truthy(mode) — that would mean the \
         eq classification fell through and the old contamination shape \
         re-emerged; got: {payload_guards:?}",
    );
    assert!(
        !payload_guards.contains(&truthy("fake")),
        "`payload` use must NOT carry Truthy(fake) — `fake` is a string \
         payload, not a real Values reference; got: {payload_guards:?}",
    );

    // `mode` must surface as a use (referenced by the condition).
    assert!(
        ir.iter().any(|u| u.source_expr == "mode"),
        "`.Values.mode` did not surface as an IR use; got: {ir:#?}",
    );
}
