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

use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractUse, Guard, GuardValue, SymbolicIrContext, ValueKind};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[path = "../src/tests/guard_oracle.rs"]
mod guard_oracle;

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

fn has_source(uses: &[ContractUse], path: &str) -> bool {
    let path = helm_schema_core::ValuesPath::parse(path);
    uses.iter().any(|use_| use_.source_expr == path)
}

fn truthy(p: &str) -> Guard {
    Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse(p),
    }
}

fn eq(p: &str, value: &str) -> Guard {
    Guard::Eq {
        path: helm_schema_core::ValuesPath::parse(p),
        value: GuardValue::string(value),
    }
}

fn not(p: &str) -> Guard {
    Guard::Not {
        path: helm_schema_core::ValuesPath::parse(p),
    }
}

/// Rendered rows only: pathless rows are read coverage, not placements.
fn rendered<'a>(uses: &'a [ContractUse], path: &str) -> Vec<&'a ContractUse> {
    let path = helm_schema_core::ValuesPath::parse(path);
    uses.iter()
        .filter(|use_| use_.source_expr == path && !use_.path.0.is_empty())
        .collect()
}

/// A row's guard disjunction as one predicate.
fn condition(use_: &ContractUse) -> helm_schema_core::Predicate {
    guard_oracle::dnf_predicate(&use_.condition)
}

/// Whether the row executes only where `guard` holds.
fn implies(use_: &ContractUse, guard: &Guard) -> bool {
    condition(use_).exactly_implies(&helm_schema_core::Predicate::from(guard.clone()))
}

/// The rows together render exactly under `selection` on every document of the leaf product,
/// and `selection` reproduces Helm's verdicts on the recorded documents.
fn assert_covers(
    rows: &[&ContractUse],
    selection: &[Guard],
    leaves: &[Option<serde_json::Value>],
    helm: &[(serde_json::Value, bool)],
) -> eyre::Result<()> {
    let want = guard_oracle::conjunction(selection);
    guard_oracle::assert_equivalent(
        &helm_schema_core::Predicate::Or(rows.iter().map(|use_| condition(use_)).collect()),
        &want,
        leaves,
    )?;
    guard_oracle::assert_helm_verdicts(&want, helm)
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
    let rendered_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "name".to_string()])
        .map(|use_| use_.source_expr.encode())
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
    let password_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.encode())
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
    let value_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "value".to_string()])
        .map(|use_| use_.source_expr.encode())
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
    let password_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.encode())
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
    let password_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.encode())
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

/// A loop-carried write guarded by a nested helper's output keeps every
/// candidate the loop can select, so the outer read still resolves both keys.
///
/// This is the minimal form of `split_path_helper_resolves_key_selected_by_helper`:
/// no `splitList`, no reversal, one member read per candidate. Its
/// near-neighbours — an unguarded loop write, an `if` outside a loop, and a
/// loop write guarded by a values path — all resolve both candidates, so this
/// pins the boundary at "the guard is an evaluated helper output".
#[test]
fn loop_write_guarded_by_helper_output_keeps_each_candidate_key() {
    let helpers = indoc! {r#"
        {{- define "test.read" -}}
        {{- index $.context.Values .key -}}
        {{- end -}}

        {{- define "test.pickGuardedByHelper" -}}
        {{- $key := "alpha" -}}
        {{- range (list "alpha" "beta") -}}
        {{- $probe := include "test.read" (dict "key" . "context" $.context) -}}
        {{- if $probe -}}
        {{- $key = . -}}
        {{- end -}}
        {{- end -}}
        {{- printf "%s" $key -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        {{- $key := include "test.pickGuardedByHelper" (dict "context" $) }}
        {{- $password := include "test.read" (dict "key" $key "context" $) }}
        apiVersion: v1
        kind: Secret
        data:
          password: {{ $password | quote }}
    "#};

    let ir = generate(template, helpers);
    let password_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.encode())
        .collect();

    sim_assert_eq!(
        have: password_sources,
        want: std::collections::BTreeSet::from(["alpha".to_string(), "beta".to_string()]),
        "a helper-output guard must not erase the loop's candidate keys: {ir:#?}"
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
    let password_sources: std::collections::BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "password".to_string()])
        .map(|use_| use_.source_expr.encode())
        .collect();

    sim_assert_eq!(
        have: password_sources,
        want: std::collections::BTreeSet::from([
            "auth.password".to_string(),
            "global.auth.password".to_string(),
        ]),
        "helper-selected path strings must resolve only to nested Values paths: {ir:#?}"
    );
}

#[test]
fn else_branch_uses_inherit_negated_if_guard_without_leaking() -> eyre::Result<()> {
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

    // Each branch's rendered rows execute under that branch's guard; the rows after the branch
    // execute under neither.
    let primary_uses = rendered(&ir, "primary");
    assert!(
        !primary_uses.is_empty(),
        "expected a then-branch use; got {ir:#?}"
    );
    for use_ in &primary_uses {
        sim_assert_eq!(have: use_.path.0, want: vec!["data".to_string(), "primary".to_string()]);
        assert!(
            implies(use_, &truthy("enabled")),
            "then-branch use should inherit Truthy(enabled); got {:?}",
            use_.condition,
        );
    }

    let fallback_uses = rendered(&ir, "fallback");
    assert!(
        !fallback_uses.is_empty(),
        "expected an else-branch use; got {ir:#?}"
    );
    for use_ in &fallback_uses {
        sim_assert_eq!(have: use_.path.0, want: vec!["data".to_string(), "fallback".to_string()]);
        assert!(
            implies(use_, &not("enabled")),
            "else-branch use should inherit Not(enabled); got {:?}",
            use_.condition,
        );
    }

    let after_uses = rendered(&ir, "after");
    assert!(
        !after_uses.is_empty(),
        "expected a post-branch use; got {ir:#?}"
    );
    for use_ in &after_uses {
        sim_assert_eq!(have: use_.path.0, want: vec!["data".to_string(), "after".to_string()]);
        assert!(
            !implies(use_, &truthy("enabled")) && !implies(use_, &not("enabled")),
            "branch guards must not leak to following nodes; got {:?}",
            use_.condition,
        );
    }
    // Coverage: each branch's rows together render exactly under its guard, and the rows after the
    // branch render always (transfer/probes/rw-coverage.txt `else-*`).
    let leaves = guard_oracle::representatives(&[]);
    let enabled =
        serde_json::json!({"enabled": true, "primary": "p", "fallback": "f", "after": "a"});
    let disabled =
        serde_json::json!({"enabled": false, "primary": "p", "fallback": "f", "after": "a"});
    assert_covers(
        &primary_uses,
        &[truthy("enabled")],
        &leaves,
        &[(enabled.clone(), true), (disabled.clone(), false)],
    )?;
    assert_covers(
        &fallback_uses,
        &[not("enabled")],
        &leaves,
        &[(enabled.clone(), false), (disabled.clone(), true)],
    )?;
    assert_covers(
        &after_uses,
        &[],
        &leaves,
        &[(enabled, true), (disabled, true)],
    )
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
          {{- $storageClass := (.Values.global).storageClass | default .Values.persistence.storageClass | default (.Values.global).defaultStorageClass | default "" }}
          {{- if $storageClass }}
          storageClassName: {{ $storageClass }}
          {{- end }}
    "#};
    // The trims keep `storageClassName` on its own line. With the former `-}}` trims Helm joins it
    // onto the storage scalar and fails to parse whenever the alias is truthy
    // (transfer/probes/t3-storage-class-trim.txt), so no placement could exist.

    let ir = generate(template, "");
    let marker = Guard::Default {
        path: helm_schema_core::ValuesPath::parse("global.storageClass"),
    };
    assert!(
        rendered(&ir, "global.storageClass").iter().any(|use_| {
            use_.path.0 == ["spec".to_string(), "storageClassName".to_string()]
                && implies(use_, &truthy("global.storageClass"))
                && !use_.condition.guard_conjunctions().is_empty()
                && use_
                    .condition
                    .guard_conjunctions()
                    .iter()
                    .all(|conjunction| conjunction.contains(&marker))
        }),
        "expected a rendered `storageClassName` use for global.storageClass carrying both Truthy and Default guards; got {ir:#?}",
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
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("deeplyNested.fieldName"))
        .collect();
    // Read coverage and rendered scalar alternatives retain the same source.
    let mut lanes: Vec<_> = target_uses
        .iter()
        .map(|use_| (use_.kind, use_.path.0.clone()))
        .collect();
    // The lanes form a multiset; their emission order carries no meaning.
    lanes.sort();
    let mut want = vec![
        (ValueKind::Serialized, Vec::<String>::new()),
        (ValueKind::WidenedDependency, Vec::<String>::new()),
        (ValueKind::WidenedRegion, Vec::<String>::new()),
        (
            ValueKind::Scalar,
            vec!["data".to_string(), "field".to_string()],
        ),
        (
            ValueKind::GoFormattedMap,
            vec!["data".to_string(), "field".to_string()],
        ),
        (
            ValueKind::GoFormattedSequence,
            vec!["data".to_string(), "field".to_string()],
        ),
    ];
    want.sort();
    sim_assert_eq!(
        have: lanes,
        want: want,
        "helper-context source coverage and all rendered alternatives: {ir:#?}",
    );

    // The loose path search must NOT additionally surface phantom
    // paths from the `.context.*` operand prefix — `context` is a
    // dict key, not a `.Values.*` reference.
    let phantoms: Vec<String> = ir
        .iter()
        .map(|u| u.source_expr.encode())
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

    // Every rendered `namespace` row lands at the one concrete leaf; pathless rows are coverage.
    let namespace_uses = rendered(&ir, "namespace");
    assert!(
        !namespace_uses.is_empty(),
        "expected a rendered `namespace` use; got: {ir:#?}",
    );
    for use_ in namespace_uses {
        let path = &use_.path.0;
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
        has_source(&ir, "ingress.className"),
        "expected helper body to resolve .config.className through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("ingress.className")
                && use_.path.0 == ["spec".to_string(), "ingressClassName".to_string()]
                && use_.kind == helm_schema_ir::ValueKind::Scalar
        }),
        "expected helper body to attach ingress.className to spec.ingressClassName: {ir:#?}",
    );
    assert!(
        has_source(&ir, "ingress.annotations"),
        "expected helper body to resolve .config.annotations through current with-bound dot: {ir:#?}",
    );
    assert!(
        has_source(&ir, "ingress.tls"),
        "expected helper body to resolve .config.tls through current with-bound dot: {ir:#?}",
    );
    assert!(
        has_source(&ir, "ingress.hosts.*.host"),
        "expected helper body to resolve .config.hosts[*].host through current with-bound dot: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == helm_schema_core::ValuesPath::parse("ingress.hosts")
                && use_.path.0 == ["spec".to_string(), "rules".to_string()])
        }),
        "the hosts input should not be projected onto the full rendered IngressRule shape: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == helm_schema_core::ValuesPath::parse("ingress.hosts.*.paths")
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
        has_source(&ir, "service.port"),
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
            use_.source_expr == helm_schema_core::ValuesPath::parse("admintools.podAnnotations")
                && use_.path.0 == annotations_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected admintools.podAnnotations to stay attached to metadata.annotations: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("additionalAnnotations")
                && use_.path.0 == annotations_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected additionalAnnotations to stay attached to metadata.annotations: {ir:#?}",
    );

    let labels_path = ["metadata".to_string(), "labels".to_string()];
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("admintools.podLabels")
                && use_.path.0 == labels_path
                && use_.kind == helm_schema_ir::ValueKind::Fragment
        }),
        "expected admintools.podLabels to stay attached to metadata.labels: {ir:#?}",
    );
    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("additionalLabels")
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
        // `common.tplvalues.render` renders ANY input kind (strings through
        // `tpl`, everything else through `toYaml`), so the placed row keeps
        // structural attachment without a structured-input shape claim.
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == helm_schema_core::ValuesPath::parse(source_expr)
                    && use_.path.0 == labels_path
                    && use_.kind == helm_schema_ir::ValueKind::YamlSerialized
            }),
            "expected {source_expr} to stay attached to metadata.labels: {ir:#?}",
        );
        assert!(
            ir.iter().all(|use_| {
                !(use_.source_expr == helm_schema_core::ValuesPath::parse(source_expr)
                    && use_.path.0.contains(&"values[*]".to_string()))
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
        .filter(|use_| use_.source_expr == helm_schema_core::ValuesPath::parse("podAnnotations"))
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
    let template = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            {{- with .Values.service }}
            - port: {{ .ports.http.port }}
            {{- end }}
    "};

    let ir = generate(template, "");

    assert!(
        ir.iter().any(|use_| use_.source_expr
            == helm_schema_core::ValuesPath::parse("service.ports.http.port")),
        "expected the full rewritten path to surface; got: {ir:#?}",
    );
    assert!(
        ir.iter()
            .all(|use_| use_.source_expr != helm_schema_core::ValuesPath::parse("service.port")),
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
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("featureFlag"))
        .collect();
    sim_assert_eq!(
        have: flag_uses.len(),
        want: 1,
        "expected exactly one `featureFlag` use from the helper-context \
         condition; got: {ir:#?}",
    );
    // Evaluating the condition reads its source before selecting a branch.
    sim_assert_eq!(
        have: flag_uses[0].single_guard_conjunction(),
        want: Vec::<Guard>::new(),
        "the helper-context condition source is read unconditionally",
    );

    // Same absence guarantee as the unconditional helper-context test:
    // the `.context` prefix must not contaminate the IR.
    let phantoms: Vec<String> = ir
        .iter()
        .map(|u| u.source_expr.encode())
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
    let template = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{ .Values.account.name }}.json: |
            {}
    "};

    let ir = generate(template, "");

    assert!(
        ir.iter().any(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("account.name")
                && use_.path.0.is_empty()
        }),
        "expected mapping-key interpolation to surface account.name as a pathless scalar use: {ir:#?}",
    );
    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == helm_schema_core::ValuesPath::parse("account.name")
                && use_.path.0 == ["data".to_string()])
        }),
        "mapping-key interpolation must not project account.name onto ConfigMap.data: {ir:#?}",
    );
}

#[test]
fn inline_scalar_sequence_item_with_mixed_template_gaps_keeps_output_path() {
    let template = indoc! {r"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
                - --image={{- if .Values.image.registry -}}{{ .Values.image.registry }}/{{- end -}}{{ .Values.image.repository }}{{- if .Values.image.digest -}}@{{ .Values.image.digest }}{{- end -}}
    "};

    let ir = generate(template, "");

    for source_expr in ["image.registry", "image.repository", "image.digest"] {
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == helm_schema_core::ValuesPath::parse(source_expr)
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
    let template = indoc! {r"
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
    "};

    let ir = generate(template, "");

    for source_expr in ["image.registry", "image.repository", "image.digest"] {
        assert!(
            ir.iter().any(|use_| {
                use_.source_expr == helm_schema_core::ValuesPath::parse(source_expr)
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
fn eq_condition_with_string_literal_containing_dot_values_does_not_phantom() -> eyre::Result<()> {
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
    let fake_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("fake"))
        .collect();
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
    let payload_uses = rendered(&ir, "payload");
    // Coverage over the comparison's defined domain: Helm's `eq` aborts on a non-string mode
    // (transfer/probes/rw-coverage.txt `eq-int`), so only nil, absent and strings compare.
    let comparable = guard_oracle::representatives(&[".Values.fake"])
        .into_iter()
        .filter(|leaf| {
            leaf.as_ref()
                .is_none_or(|leaf| leaf.is_null() || leaf.is_string())
        })
        .collect::<Vec<_>>();
    assert_covers(
        &payload_uses,
        &[eq("mode", ".Values.fake")],
        &comparable,
        &[
            (
                serde_json::json!({"mode": ".Values.fake", "payload": "x"}),
                true,
            ),
            (serde_json::json!({"mode": "other", "payload": "x"}), false),
        ],
    )?;
    assert!(
        !payload_uses.is_empty(),
        "expected a rendered `payload` use inside the if-body; got: {ir:#?}",
    );
    for use_ in payload_uses {
        let payload_guards = &use_.condition;
        assert!(
            implies(use_, &eq("mode", ".Values.fake")),
            "expected `payload` body use guarded by Eq(mode, \".Values.fake\"); \
             got: {payload_guards:?}",
        );
        assert!(
            use_.condition
                .guard_conjunctions()
                .iter()
                .all(|conjunction| !conjunction.contains(&truthy("mode"))),
            "`payload` use must NOT carry Truthy(mode) — that would mean the \
             eq classification fell through and the old contamination shape \
             re-emerged; got: {payload_guards:?}",
        );
        assert!(
            !condition(use_)
                .value_paths()
                .contains(&helm_schema_core::ValuesPath::parse("fake")),
            "`payload` use must NOT carry Truthy(fake) — `fake` is a string \
             payload, not a real Values reference; got: {payload_guards:?}",
        );
    }

    // `mode` must surface as a use (referenced by the condition).
    assert!(
        ir.iter()
            .any(|u| u.source_expr == helm_schema_core::ValuesPath::parse("mode")),
        "`.Values.mode` did not surface as an IR use; got: {ir:#?}",
    );
    Ok(())
}
