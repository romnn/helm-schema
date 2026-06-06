//! IR-level regression tests for the typed-AST extractors in
//! `walker.rs`. Unlike the unit tests on the extractors themselves
//! (which feed text directly into `parse_condition` /
//! `extract_values_paths`), these run the full
//! [`SymbolicIrGenerator`] pipeline against minimal in-memory chart
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

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{Guard, IrGenerator, SymbolicIrGenerator, ValueUse};
use indoc::indoc;

fn generate(template: &str, helpers: &str) -> Vec<ValueUse> {
    let mut idx = DefineIndex::new();
    if !helpers.is_empty() {
        idx.add_file_source("helpers.tpl", helpers);
        idx.add_source(&TreeSitterParser, helpers)
            .expect("helpers parse");
    }
    let ast = TreeSitterParser.parse(template).expect("template parse");
    SymbolicIrGenerator.generate(template, &ast, &idx)
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
        value: value.to_string(),
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

    let map_uses: Vec<&ValueUse> = ir.iter().filter(|u| u.source_expr == "map").collect();
    assert_eq!(
        map_uses.len(),
        2,
        "expected a header scalar use plus a fragment use for `map`; \
         got: {ir:#?}",
    );

    let header_use = map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0.is_empty() && use_.kind == helm_schema_ir::ValueKind::Scalar)
        .expect("header scalar use present");

    // The header scalar use is unconditional: SymbolicIrGenerator
    // emits it BEFORE pushing the Truthy(map) guard, so the use
    // itself must have no guards. A regression that attached
    // Truthy(map) here would double-count `map` against itself.
    assert!(
        header_use.guards.is_empty(),
        "the destructured range header's source use must be unconditional, \
         but got guards: {:?}",
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

    let fallback_map_uses: Vec<&ValueUse> = ir
        .iter()
        .filter(|u| u.source_expr == "fallbackMap")
        .collect();
    assert_eq!(
        fallback_map_uses.len(),
        2,
        "expected a header scalar use plus a fragment use for `fallbackMap`; got: {ir:#?}",
    );

    let header_use = fallback_map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0.is_empty() && use_.kind == helm_schema_ir::ValueKind::Scalar)
        .expect("header scalar use present");
    // The header scalar use is unconditional — same contract as the
    // bare destructured header above.
    assert!(
        header_use.guards.is_empty(),
        "destructured range header use must be unconditional, but got \
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
    // body uses — the exact contract `SymbolicIrGenerator` requires.
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
    let suffix_uses: Vec<&ValueUse> = ir.iter().filter(|u| u.source_expr == "suffix").collect();
    assert_eq!(
        suffix_uses.len(),
        1,
        "expected exactly one `suffix` use inside range body; got: {ir:#?}",
    );
    assert!(
        suffix_uses[0].guards.contains(&range_guard("themap")),
        "expected `suffix` use guarded by Range(themap); got: {:?}",
        suffix_uses[0].guards,
    );

    // `fallback` is OUTSIDE the range — must NOT carry that guard.
    let fallback_uses: Vec<&ValueUse> = ir.iter().filter(|u| u.source_expr == "fallback").collect();
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
        parent_use.guards.is_empty(),
        "the collection-level range header use must stay unconditional; got: {:?}",
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

    let target_uses: Vec<&ValueUse> = ir
        .iter()
        .filter(|u| u.source_expr == "deeplyNested.fieldName")
        .collect();
    assert_eq!(
        target_uses.len(),
        1,
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

    let namespace_uses: Vec<&ValueUse> =
        ir.iter().filter(|u| u.source_expr == "namespace").collect();
    assert_eq!(
        namespace_uses.len(),
        1,
        "expected exactly one `namespace` use; got: {ir:#?}",
    );
    let path = &namespace_uses[0].path.0;
    assert!(
        path.contains(&"namespaceSelector".to_string()),
        "quoted YAML key path should still descend through namespaceSelector; got: {path:?}",
    );
    assert_eq!(
        path.last().map(String::as_str),
        Some("kubernetes.io/metadata.name"),
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
    // Same context idiom inside an `if` condition — the typed walker's
    // parse_condition path must still surface the inner field as an
    // IR use (the IR emits the condition's value reference as a use
    // even though the condition itself doesn't get a self-guard).
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

    let flag_uses: Vec<&ValueUse> = ir
        .iter()
        .filter(|u| u.source_expr == "featureFlag")
        .collect();
    assert_eq!(
        flag_uses.len(),
        1,
        "expected exactly one `featureFlag` use from the helper-context \
         condition; got: {ir:#?}",
    );
    // The condition's source use is unconditional — the Truthy guard
    // is pushed AFTER emission, for body uses (here the body is plain
    // text, so there's nothing to gate). A regression that attached a
    // self-guard would fail here.
    assert!(
        flag_uses[0].guards.is_empty(),
        "helper-context condition source use must be unconditional, but \
         got guards: {:?}",
        flag_uses[0].guards,
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
    let fake_uses: Vec<&ValueUse> = ir.iter().filter(|u| u.source_expr == "fake").collect();
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
    let payload_uses: Vec<&ValueUse> = ir.iter().filter(|u| u.source_expr == "payload").collect();
    assert_eq!(
        payload_uses.len(),
        1,
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
