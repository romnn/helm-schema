//! Absence in the COALESCED values document reads nil, whoever declared
//! the key.
//!
//! Helm validates `values.schema.json` against the coalesced document —
//! the same one the templates render from — so every chart's and every
//! subchart's declared defaults are already applied there. A key missing
//! from it is missing because an explicit `null` deleted it, and helm
//! v4.2.3 carries that deletion through the subchart's own coalesce stage
//! (`coalesceValues` deletes the key against the subchart's default instead
//! of restoring it). The generator used to model a dependency-declared key
//! as "refilled at the subchart stage", which made every subchart-scoped
//! nil-deref arm null-only — unreachable, because a declared key can never
//! survive as a literal null — and made a deleted subchart flag read as its
//! declared truthy default.
//!
//! Every row below is adjudicated against `helm template` v4.2.3 with
//! `--kube-version 1.29.0`; the coalesced document each row validates is
//! the one helm actually produces for the quoted `--set`.
//!
//! The schema must also pass `helm lint`, which validates the root values
//! with the user's overrides but without dependency defaults. A subchart arm
//! whose gate an override can switch on while the key it dereferences comes
//! only from the subchart's defaults fails that document, so the lint gate
//! withdraws it: such a deleted key is accepted although Helm aborts.

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions, SchemaProfile};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const KID_CHART_YAML: &str = indoc! {"
    apiVersion: v2
    name: kid
    version: 0.1.0
"};

const KID_VALUES_YAML: &str = indoc! {"
    flag: true
    grp:
      enabled: true
"};

/// Dereferences `grp` only under the `flag` gate, so a deleted `grp`
/// aborts while a deleted `flag` skips the body entirely.
const KID_TEMPLATE: &str = indoc! {r"
    {{- if .Values.flag }}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: kid-cm
    data:
      v: {{ .Values.grp.enabled | quote }}
    {{- end }}
"};

const ROOT_VALUES_YAML: &str = indoc! {"
    rootGrp:
      enabled: true
"};

const ROOT_TEMPLATE: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: root-cm
    data:
      v: {{ .Values.rootGrp.enabled | quote }}
"};

fn schema_for(files: &[(&str, &str)]) -> eyre::Result<serde_json::Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    for (path, contents) in files {
        test_util::write(&chart_dir.join(path)?, contents)?;
    }
    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        authoring: helm_schema::generation::AuthoringPolicy::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };
    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}

/// Root chart plus one subchart, with `dependencies:` spelled by the
/// caller so the alias and `condition:` rows reuse the same bodies.
fn umbrella_schema(
    root_chart_yaml: &str,
    root_values_yaml: &str,
) -> eyre::Result<serde_json::Value> {
    schema_for(&[
        ("Chart.yaml", root_chart_yaml),
        ("values.yaml", root_values_yaml),
        ("templates/cm.yaml", ROOT_TEMPLATE),
        ("charts/kid/Chart.yaml", KID_CHART_YAML),
        ("charts/kid/values.yaml", KID_VALUES_YAML),
        ("charts/kid/templates/cm.yaml", KID_TEMPLATE),
    ])
}

const PLAIN_ROOT_CHART_YAML: &str = indoc! {"
    apiVersion: v2
    name: f23min
    version: 0.1.0
"};

/// `kid.flag` is truthy. The gate carries NO "flag absent" disjunct: a
/// deleted flag reads nil, which is Helm-falsy.
fn flag_truthy_fragment() -> serde_json::Value {
    serde_json::json!({
        "properties": { "flag": { "$ref": "#/$defs/t" } },
        "required": ["flag"],
        "type": "object",
    })
}

/// `<root>.<key>` was null-deleted: BOTH spellings, where the missing one
/// used to be dropped for a subchart-declared key.
fn key_deleted_fragment(key: &str) -> serde_json::Value {
    serde_json::json!({
        "anyOf": [
            {
                "not": {
                    "properties": { key: {} },
                    "required": [key],
                    "type": "object",
                },
            },
            {
                "properties": { key: { "enum": [null] } },
                "required": [key],
                "type": "object",
            },
        ],
    })
}

/// The whole emitted schema for the minimal umbrella, so the subchart arm's
/// shape is pinned rather than sampled.
fn expected_minimal_umbrella_schema() -> serde_json::Value {
    serde_json::json!({
        "$defs": {
            "t": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" },
                ],
            },
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "properties": { "kid": flag_truthy_fragment() },
                    "required": ["kid"],
                    "type": "object",
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": { "grp": { "type": "object" } },
                        },
                    },
                },
            },
            {
                "additionalProperties": {},
                "properties": { "kid": { "type": ["null", "object"] } },
            },
            {
                // Root-scope control: this scope already carried both
                // spellings, and the change leaves it alone.
                "if": key_deleted_fragment("rootGrp"),
                "then": false,
            },
        ],
        "properties": {
            "global": {},
            // No `flag ∧ grp deleted → false` arm under `kid`: `helm lint -f
            // {kid: {flag: true}}` validates `kid` without the subchart's
            // `grp` and fails it while `helm template` renders (Helm
            // v4.2.3), so the lint gate withdraws it.
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "flag": { "type": "boolean" },
                    "global": { "additionalProperties": {}, "type": "object" },
                    "grp": {
                        "additionalProperties": {},
                        "properties": { "enabled": {} },
                    },
                },
                "type": "object",
            },
            "rootGrp": {
                "additionalProperties": {},
                "properties": { "enabled": {} },
                "type": "object",
            },
        },
        "type": "object",
    })
}

#[test]
fn minimal_umbrella_schema_encodes_coalesced_absence() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    sim_assert_eq!(
        have: umbrella_schema(PLAIN_ROOT_CHART_YAML, ROOT_VALUES_YAML)?,
        want: expected_minimal_umbrella_schema(),
    );
    Ok(())
}

/// Rows adjudicated against helm v4.2.3 on the same chart. `helm` renders
/// or aborts as noted; the schema must agree.
#[test]
fn subchart_scoped_absence_matches_helm() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = umbrella_schema(PLAIN_ROOT_CHART_YAML, ROOT_VALUES_YAML)?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            // `--set kid.grp=null`: the subchart's own `grp` default does
            // NOT come back, and helm aborts with "nil pointer evaluating
            // interface {}.enabled" at charts/kid/templates/cm.yaml.
            serde_json::json!({
                "kid": { "flag": true, "global": {} },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted subchart-declared key reads nil and aborts, yet the lint gate withdraws the arm",
        ),
        (
            // `--set kid.flag=null --set kid.grp=notamap`: the deleted flag
            // reads nil, the body is skipped, and helm renders even though
            // `grp` is a bare string.
            serde_json::json!({
                "kid": { "global": {}, "grp": "notamap" },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted subchart flag is falsy and skips its body",
        ),
        (
            // Root-scope control: `--set rootGrp=null` aborts, and this
            // scope was already encoded correctly.
            serde_json::json!({
                "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
            }),
            false,
            "a deleted root-owned key aborts",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// An aliased dependency occupies its ALIAS values root, and absence there
/// behaves exactly as under the chart's own name — the encoding never
/// consults a chart name.
#[test]
fn aliased_dependency_root_shares_the_absence_semantics() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = umbrella_schema(
        indoc! {"
            apiVersion: v2
            name: f23alias
            version: 0.1.0
            dependencies:
              - name: kid
                version: 0.1.0
                alias: kidalias
        "},
        ROOT_VALUES_YAML,
    )?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "kidalias": { "flag": true, "global": {}, "grp": { "enabled": true } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            // `--set kidalias.grp=null` aborts at
            // charts/kidalias/templates/cm.yaml.
            serde_json::json!({
                "kidalias": { "flag": true, "global": {} },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted key under the alias root reads nil and aborts, yet the lint gate withdraws the arm",
        ),
        (
            // `--set kidalias.flag=null --set kidalias.grp=notamap` renders.
            serde_json::json!({
                "kidalias": { "global": {}, "grp": "notamap" },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted flag under the alias root is falsy",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// A dependency gated by `condition:` keeps the same absence semantics
/// while it is enabled, and its dormant arm still accepts a document that
/// switched it off.
#[test]
fn conditional_dependency_keeps_coalesced_absence_semantics() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = umbrella_schema(
        indoc! {"
            apiVersion: v2
            name: f23cond
            version: 0.1.0
            dependencies:
              - name: kid
                version: 0.1.0
                condition: kid.enabled
        "},
        indoc! {"
            kid:
              enabled: true
            rootGrp:
              enabled: true
        "},
    )?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "kid": {
                    "enabled": true, "flag": true, "global": {}, "grp": { "enabled": true },
                },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            // `--set kid.grp=null` with the dependency enabled aborts.
            serde_json::json!({
                "kid": { "enabled": true, "flag": true, "global": {} },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted key under an enabled conditional dependency aborts, yet the lint gate withdraws the arm",
        ),
        (
            // `--set kid.enabled=false` drops the dependency before
            // coalescing, so its defaults never enter the document and
            // nothing renders from it.
            serde_json::json!({
                "kid": { "enabled": false },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the disabled dependency renders nothing",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// The one refill that survives: a merge INSIDE the templates runs after
/// coalescing, so it restores what the document dropped. Deleting the merge
/// source as well leaves nothing to restore and aborts.
#[test]
fn a_template_time_merge_still_refills_a_deleted_key() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = schema_for(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: f23runtime
                version: 0.1.0
            "},
        ),
        (
            "values.yaml",
            indoc! {"
                _defaults:
                  grp:
                    enabled: true
                grp:
                  enabled: true
            "},
        ),
        (
            "templates/cm.yaml",
            indoc! {r#"
                {{- $_ := set $ "Values" (mustMergeOverwrite (deepCopy .Values._defaults) .Values) -}}
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: runtime-cm
                data:
                  v: {{ .Values.grp.enabled | quote }}
            "#},
        ),
    ])?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "_defaults": { "grp": { "enabled": true } },
                "grp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            // `--set grp=null`: the render-time merge puts `grp` back.
            serde_json::json!({ "_defaults": { "grp": { "enabled": true } } }),
            true,
            "the merge refills the deleted key",
        ),
        (
            // `--set grp=null --set _defaults.grp=null`: nothing left to
            // merge, so the read aborts.
            serde_json::json!({ "_defaults": {} }),
            false,
            "deleting the merge source too leaves nil",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// Absence at a dependency's `condition:` path means ENABLED, not falsy.
///
/// Helm resolves `condition:` against the coalesced document and falls back
/// to the dependency's default (`enabled`) when no chart declares the path,
/// so the carve-out runs in the opposite direction from every other row in
/// this file: a missing key reads nil everywhere EXCEPT here. Verified on
/// `charts/c1abs`, whose `condition: kid.enabled` path nothing declares —
/// helm still coalesces the subchart's values in and still aborts on
/// `--set kid.grp=null`.
#[test]
fn an_undeclared_condition_path_leaves_the_dependency_enabled() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = umbrella_schema(
        indoc! {"
            apiVersion: v2
            name: c1abs
            version: 0.1.0
            dependencies:
              - name: kid
                version: 0.1.0
                condition: kid.enabled
        "},
        ROOT_VALUES_YAML,
    )?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render with the dependency enabled",
        ),
        (
            // `--set kid.grp=null` aborts at charts/kid/templates/cm.yaml,
            // which only happens because the undeclared condition path left
            // the dependency ENABLED.
            serde_json::json!({
                "kid": { "flag": true, "global": {} },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the enabled dependency still aborts on its deleted key, yet the lint gate withdraws the arm",
        ),
        (
            // `--set kid.flag=null` renders: the gate is falsy.
            serde_json::json!({
                "kid": { "global": {}, "grp": { "enabled": true } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "a deleted flag skips the body",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// A subchart that switches ITSELF off deletes its whole values scope
/// before the validated document exists.
///
/// The subchart's own `enabled: false` composes into the parent table, the
/// `condition:` reads it, and helm then drops the dependency — so the
/// coalesced document carries no `kid` root at all and renders. Verified on
/// `charts/c2off`: defaults coalesce to `{"rootGrp":{"enabled":true}}`.
#[test]
fn a_self_disabled_dependency_leaves_no_scope_to_validate() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = schema_for(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: c2off
                version: 0.1.0
                dependencies:
                  - name: kid
                    version: 0.1.0
                    condition: kid.enabled
            "},
        ),
        ("values.yaml", ROOT_VALUES_YAML),
        ("templates/cm.yaml", ROOT_TEMPLATE),
        ("charts/kid/Chart.yaml", KID_CHART_YAML),
        (
            "charts/kid/values.yaml",
            indoc! {"
                enabled: false
                flag: true
                grp:
                  enabled: true
            "},
        ),
        ("charts/kid/templates/cm.yaml", KID_TEMPLATE),
    ])?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            // The defaults: helm drops the dependency, so no `kid` root.
            serde_json::json!({ "rootGrp": { "enabled": true } }),
            true,
            "the self-disabled dependency leaves no scope",
        ),
        (
            // `--set kid.enabled=true` brings the whole scope back.
            serde_json::json!({
                "kid": {
                    "enabled": true, "flag": true, "global": {}, "grp": { "enabled": true },
                },
                "rootGrp": { "enabled": true },
            }),
            true,
            "re-enabling restores the full scope",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

/// Deleting a ROOT `global.X` leaves the subchart's own copy intact.
///
/// Helm propagates globals into each dependency's table, and a subchart
/// that declares the same global keeps its own copy through the deletion.
/// Verified on `charts/c3glob`: `--set global.gg=null` coalesces to
/// `{"global":{},"kid":{"flag":true,"global":{"gg":{"enabled":true}}},…}`
/// and RENDERS, while `--set kid.global.gg=null` aborts.
#[test]
fn deleting_a_root_global_leaves_the_subchart_copy() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = schema_for(&[
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: c3glob
                version: 0.1.0
            "},
        ),
        (
            "values.yaml",
            indoc! {"
                global:
                  gg:
                    enabled: true
                rootGrp:
                  enabled: true
            "},
        ),
        ("templates/cm.yaml", ROOT_TEMPLATE),
        ("charts/kid/Chart.yaml", KID_CHART_YAML),
        (
            "charts/kid/values.yaml",
            indoc! {"
                global:
                  gg:
                    enabled: true
                flag: true
            "},
        ),
        (
            "charts/kid/templates/cm.yaml",
            indoc! {r"
                {{- if .Values.flag }}
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: kid-cm
                data:
                  v: {{ .Values.global.gg.enabled | quote }}
                {{- end }}
            "},
        ),
    ])?;
    let validator = jsonschema::validator_for(&schema)?;

    for (instance, want, label) in [
        (
            serde_json::json!({
                "global": { "gg": { "enabled": true } },
                "kid": { "flag": true, "global": { "gg": { "enabled": true } } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the coalesced defaults render",
        ),
        (
            // `--set global.gg=null`: the subchart's copy survives, renders.
            serde_json::json!({
                "global": {},
                "kid": { "flag": true, "global": { "gg": { "enabled": true } } },
                "rootGrp": { "enabled": true },
            }),
            true,
            "the subchart's own global copy survives the root deletion",
        ),
        (
            // `--set kid.global.gg=null`: the copy the subchart reads is
            // gone, and helm aborts.
            serde_json::json!({
                "global": { "gg": { "enabled": true } },
                "kid": { "flag": true, "global": {} },
                "rootGrp": { "enabled": true },
            }),
            true,
            "deleting the subchart's copy aborts, yet the lint gate withdraws the arm",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}; want={want}; errors={:?}",
            validator
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}
