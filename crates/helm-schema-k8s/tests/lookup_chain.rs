//! Chain-layer behaviour: precedence, ResourceDocMissing handling,
//! PathUnresolved silence, and the "MissingSchema only from chain"
//! invariant.

use helm_schema_core::{
    ApiPresenceQuery, ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath,
};
use helm_schema_k8s::{
    Chain, Diagnostic, DiagnosticSink, K8sSchemaProvider, LookupTraceEntry, LookupTraceOutcome,
    ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

#[derive(Debug)]
struct FakeProvider {
    name: ProviderOrigin,
    behaviour: FakeBehaviour,
    has_resource: bool,
    capability_answers: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone)]
enum FakeBehaviour {
    Found(Value),
    PathUnresolved,
    ResourceDocMissing { path: String, io: String },
    NotOwned,
}

fn provider_use(path: YamlPath, resource: ResourceRef) -> ProviderSchemaUse {
    ProviderSchemaUse {
        value_path: "x".to_string(),
        path,
        kind: ValueKind::Scalar,
        resource,
        is_self_range_collection: false,
    }
}

impl FakeProvider {
    fn new(name: ProviderOrigin, has_resource: bool, behaviour: FakeBehaviour) -> Self {
        Self {
            name,
            behaviour,
            has_resource,
            capability_answers: std::collections::HashMap::new(),
        }
    }

    /// Configure an explicit `Capabilities.APIVersions.Has(api)`
    /// answer. Without one, this provider abstains (returns `None`).
    fn with_capability(mut self, api: &str, has: bool) -> Self {
        self.capability_answers.insert(api.to_string(), has);
        self
    }
}

impl ResourceSchemaOracle for FakeProvider {
    fn schema_fragment_for_resource_path(
        &self,
        _r: &ResourceRef,
        _p: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        match &self.behaviour {
            FakeBehaviour::Found(v) => Some(ProviderSchemaFragment::new(v.clone())),
            _ => None,
        }
    }
}

impl K8sSchemaProvider for FakeProvider {
    fn origin(&self) -> ProviderOrigin {
        self.name
    }

    fn lookup(&self, _r: &ResourceRef, _p: &YamlPath) -> ProviderLookupResult {
        match &self.behaviour {
            FakeBehaviour::Found(v) => ProviderLookupResult::Found {
                schema: ProviderSchemaFragment::new(v.clone()),
                resolved_k8s_version: None,
            },
            FakeBehaviour::PathUnresolved => ProviderLookupResult::PathUnresolved,
            FakeBehaviour::ResourceDocMissing { path, io } => {
                ProviderLookupResult::ResourceDocMissing {
                    io_error: io.clone(),
                    source_path: path.clone(),
                }
            }
            FakeBehaviour::NotOwned => ProviderLookupResult::NotOwned,
        }
    }

    fn has_resource(&self, _r: &ResourceRef) -> bool {
        self.has_resource
    }

    fn capability_has_query_at_primary_version(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.capability_answers
            .get(&query.canonical_helm_literal())
            .copied()
    }
}

fn resource() -> ResourceRef {
    ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    }
}

#[test]
fn chain_precedence_local_over_crd_over_k8s() {
    let local = FakeProvider::new(
        ProviderOrigin::LocalOverride,
        true,
        FakeBehaviour::Found(Value::String("local".to_string())),
    );
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::Found(Value::String("crd".to_string())),
    );
    let k8s = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        true,
        FakeBehaviour::Found(Value::String("k8s".to_string())),
    );
    let chain = Chain::new(vec![Box::new(local), Box::new(crd), Box::new(k8s)]);
    let schema = chain
        .schema_fragment_for_resource_path(&resource(), &YamlPath(Vec::new()))
        .map(|fragment| fragment.into_schema());
    sim_assert_eq!(
        have: schema,
        want: Some(Value::String("local".to_string())),
        "LocalOverride wins precedence; CRD and K8s never consulted"
    );
}

#[test]
fn provider_local_path_unresolved_is_not_missing() {
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::PathUnresolved,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(crd)]).with_diagnostic_sink(diagnostics.clone());
    let schema = chain.schema_fragment_for_resource_path(&resource(), &YamlPath(Vec::new()));
    assert!(
        schema.is_none(),
        "PathUnresolved maps to Resolved with None schema"
    );
    let any_missing = diagnostics
        .snapshot()
        .iter()
        .any(|d| matches!(d, Diagnostic::MissingSchema { .. }));
    assert!(
        !any_missing,
        "PathUnresolved must NOT trigger MissingSchema diagnostic"
    );
}

#[test]
fn chain_emits_missing_schema_only_at_chain_layer() {
    // Every provider returns NotOwned, so the chain emits exactly one
    // MissingSchema (and none of the providers does).
    let p1 = FakeProvider::new(
        ProviderOrigin::LocalOverride,
        false,
        FakeBehaviour::NotOwned,
    );
    let p2 = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        false,
        FakeBehaviour::NotOwned,
    );
    let p3 = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p1), Box::new(p2), Box::new(p3)])
        .with_diagnostic_sink(diagnostics.clone());

    let _ = chain.schema_fragment_for_resource_path(&resource(), &YamlPath(Vec::new()));

    let missing = diagnostics
        .snapshot()
        .into_iter()
        .filter(|d| matches!(d, Diagnostic::MissingSchema { .. }))
        .count();
    sim_assert_eq!(
        have: missing,
        want: 1,
        "exactly one MissingSchema, emitted by the chain layer"
    );
}

#[test]
fn chain_local_override_unreadable_does_not_fall_through() {
    // Local claims the resource (has_resource=true) but its file is
    // unreadable, so the chain emits LocalOverrideUnreadable and returns
    // MissingSchema, never falling through to CRD or K8s.
    let local = FakeProvider::new(
        ProviderOrigin::LocalOverride,
        true,
        FakeBehaviour::ResourceDocMissing {
            path: "/path/to/override.json".to_string(),
            io: "permission denied".to_string(),
        },
    );
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::Found(Value::String("crd-would-have-answered".to_string())),
    );
    let k8s = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        true,
        FakeBehaviour::Found(Value::String("k8s-would-have-answered".to_string())),
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(local), Box::new(crd), Box::new(k8s)])
        .with_diagnostic_sink(diagnostics.clone());

    let schema = chain.schema_fragment_for_resource_path(&resource(), &YamlPath(Vec::new()));
    assert!(
        schema.is_none(),
        "override-unreadable must NOT silently use CRD/K8s schema"
    );

    let snapshot = diagnostics.snapshot();
    let override_unreadable = snapshot
        .iter()
        .find_map(|d| match d {
            Diagnostic::LocalOverrideUnreadable {
                kind,
                override_path,
                io_error,
                ..
            } => Some((kind.clone(), override_path.clone(), io_error.clone())),
            _ => None,
        })
        .expect("LocalOverrideUnreadable must be present");
    sim_assert_eq!(have: override_unreadable.0, want: "ServiceMonitor");
    sim_assert_eq!(have: override_unreadable.1, want: "/path/to/override.json");
    assert!(override_unreadable.2.contains("permission denied"));
    assert!(
        !snapshot
            .iter()
            .any(|diagnostic| matches!(diagnostic, Diagnostic::MissingSchema { .. })),
        "override unreadability is the actionable diagnostic; generic MissingSchema would be noise"
    );
}

#[test]
fn chain_non_override_resource_doc_missing_falls_through() {
    // CRD claims it but its doc is missing, so the chain falls through to K8s.
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::ResourceDocMissing {
            path: String::new(),
            io: "transient".to_string(),
        },
    );
    let k8s = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        true,
        FakeBehaviour::Found(Value::String("k8s-wins".to_string())),
    );
    let chain = Chain::new(vec![Box::new(crd), Box::new(k8s)]);
    let schema = chain
        .schema_fragment_for_resource_path(&resource(), &YamlPath(Vec::new()))
        .map(|fragment| fragment.into_schema());
    sim_assert_eq!(
        have: schema,
        want: Some(Value::String("k8s-wins".to_string())),
        "non-override ResourceDocMissing falls through to next provider"
    );
}

#[test]
fn traced_resolution_records_provider_attempts_until_resolution() {
    let local = FakeProvider::new(
        ProviderOrigin::LocalOverride,
        false,
        FakeBehaviour::NotOwned,
    );
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::ResourceDocMissing {
            path: String::new(),
            io: "transient".to_string(),
        },
    );
    let k8s = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        true,
        FakeBehaviour::Found(Value::String("k8s-wins".to_string())),
    );
    let chain = Chain::new(vec![Box::new(local), Box::new(crd), Box::new(k8s)]);

    let target = resource();
    let traced = chain.resolve_against_chain_traced(&target, &YamlPath(Vec::new()));
    let attempts: Vec<(String, String, ProviderOrigin, LookupTraceOutcome)> = traced
        .trace
        .entries()
        .iter()
        .map(|entry| match entry {
            LookupTraceEntry::ResourceProvider {
                resource,
                provider,
                outcome,
            } => (
                resource.api_version.clone(),
                resource.kind.clone(),
                *provider,
                outcome.clone(),
            ),
            other => panic!("unexpected trace entry: {other:?}"),
        })
        .collect();

    sim_assert_eq!(
        have: attempts,
        want: vec![
            (
                target.api_version.clone(),
                target.kind.clone(),
                ProviderOrigin::LocalOverride,
                LookupTraceOutcome::NotOwned,
            ),
            (
                target.api_version.clone(),
                target.kind.clone(),
                ProviderOrigin::DefaultCatalog,
                LookupTraceOutcome::ResourceDocMissing {
                    source_path: String::new(),
                    io_error: "transient".to_string(),
                },
            ),
            (
                target.api_version.clone(),
                target.kind.clone(),
                ProviderOrigin::KubernetesOpenApi,
                LookupTraceOutcome::Found {
                    resolved_k8s_version: None,
                },
            ),
        ]
    );
}

#[test]
fn traced_capability_query_records_provider_attempts_until_answer() {
    let local = FakeProvider::new(
        ProviderOrigin::LocalOverride,
        false,
        FakeBehaviour::NotOwned,
    );
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        false,
        FakeBehaviour::NotOwned,
    );
    let k8s = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    )
    .with_capability("autoscaling/v2", true);
    let chain = Chain::new(vec![Box::new(local), Box::new(crd), Box::new(k8s)]);
    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");

    let traced = chain.capability_has_query_at_primary_version_traced(&query);
    let attempts: Vec<(ProviderOrigin, Option<bool>)> = traced
        .trace
        .entries()
        .iter()
        .map(|entry| match entry {
            LookupTraceEntry::ApiPresenceProvider { provider, answer } => (*provider, *answer),
            other => panic!("unexpected trace entry: {other:?}"),
        })
        .collect();

    sim_assert_eq!(have: traced.answer, want: Some(true));
    sim_assert_eq!(
        have: attempts,
        want: vec![
            (ProviderOrigin::LocalOverride, None),
            (ProviderOrigin::DefaultCatalog, None),
            (ProviderOrigin::KubernetesOpenApi, Some(true)),
        ]
    );
}

#[test]
fn chain_exposes_provider_kube_version() {
    #[derive(Debug)]
    struct VersionedProvider;

    impl ResourceSchemaOracle for VersionedProvider {
        fn schema_fragment_for_resource_path(
            &self,
            _r: &ResourceRef,
            _p: &YamlPath,
        ) -> Option<ProviderSchemaFragment> {
            None
        }
    }

    impl K8sSchemaProvider for VersionedProvider {
        fn origin(&self) -> ProviderOrigin {
            ProviderOrigin::KubernetesOpenApi
        }

        fn has_resource(&self, _r: &ResourceRef) -> bool {
            false
        }

        fn primary_k8s_version(&self) -> Option<&str> {
            Some("v1.35.0")
        }
    }

    let chain = Chain::new(vec![Box::new(VersionedProvider)]);

    sim_assert_eq!(have: chain.kube_version(), want: Some("v1.35.0"));
}

#[test]
fn local_provider_emits_local_override_origin() {
    // A LocalSchemaProvider configured against a real directory reports
    // its origin as LocalOverride from any chain query.
    use helm_schema_k8s::LocalSchemaProvider;
    let tmp = std::env::temp_dir().join(format!("helm-schema.local-origin.{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let provider = LocalSchemaProvider::new(&tmp);
    sim_assert_eq!(have: provider.origin(), want: ProviderOrigin::LocalOverride);
}

// Multi-candidate iteration in schema_fragment_for_use must not emit MissingSchema for
// speculative misses when a later candidate resolves. The primary apiVersion
// (`policy/v1beta1`) is what the user wrote in the chart; `policy/v1` is the
// ordered candidate added by `ordered_api_versions_for_resource`. Only the
// success-side outcome (Found via a candidate) should be observable from the
// sink.
#[test]
fn chain_schema_fragment_for_use_speculative_misses_do_not_leak_diagnostics() {
    let crd = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        true,
        FakeBehaviour::Found(Value::String("policy/v1beta1-schema".to_string())),
    );
    // Wrap the fake in a "knows-policy/v1beta1-only" stub: it returns
    // NotOwned for any other apiVersion. We use the surrogate API the
    // FakeProvider already exposes. It is content-blind, so any
    // resolve_against_chain call returns Found. We need a slightly
    // smarter fake that only answers for one apiVersion.
    #[derive(Debug)]
    struct OnlyAnswersFor {
        wants: &'static str,
    }
    impl ResourceSchemaOracle for OnlyAnswersFor {
        fn schema_fragment_for_resource_path(
            &self,
            r: &ResourceRef,
            _p: &YamlPath,
        ) -> Option<ProviderSchemaFragment> {
            if r.api_version == self.wants {
                Some(ProviderSchemaFragment::new(Value::String(
                    "hit".to_string(),
                )))
            } else {
                None
            }
        }
    }

    impl K8sSchemaProvider for OnlyAnswersFor {
        fn origin(&self) -> ProviderOrigin {
            ProviderOrigin::DefaultCatalog
        }
        fn lookup(&self, r: &ResourceRef, _p: &YamlPath) -> ProviderLookupResult {
            if r.api_version == self.wants {
                ProviderLookupResult::Found {
                    schema: ProviderSchemaFragment::new(Value::String("hit".to_string())),
                    resolved_k8s_version: None,
                }
            } else {
                ProviderLookupResult::NotOwned
            }
        }
        fn has_resource(&self, r: &ResourceRef) -> bool {
            r.api_version == self.wants
        }
    }
    drop(crd);

    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(OnlyAnswersFor {
        wants: "policy/v1beta1",
    })])
    .with_diagnostic_sink(diagnostics.clone());

    let use_ = provider_use(
        YamlPath(Vec::new()),
        ResourceRef {
            api_version: "policy/v1beta1".to_string(),
            kind: "PodSecurityPolicy".to_string(),
            // `policy/v1` is a speculative candidate; ranking puts it
            // first (stable > beta) so it will be probed before the
            // primary. If the speculative miss leaks, we'd see
            // MissingSchema(policy/v1, ...) in the snapshot.
            api_version_candidates: vec!["policy/v1".to_string()],
            api_version_branches: Vec::new(),
        },
    );
    let schema = chain
        .schema_fragment_for_use(&use_)
        .map(|fragment| fragment.into_schema());
    sim_assert_eq!(
        have: schema,
        want: Some(Value::String("hit".to_string())),
        "the primary `policy/v1beta1` must resolve via fallback"
    );

    let snapshot = diagnostics.snapshot();
    let missing_for_policy_v1: Vec<_> = snapshot
        .iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } if kind == "PodSecurityPolicy" && api_version == "policy/v1" => {
                Some(api_version.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        missing_for_policy_v1.is_empty(),
        "speculative miss for policy/v1 must NOT leak MissingSchema; sink contained: {snapshot:?}"
    );
}

// Multi-branch helper output preserves candidates as ambiguous alternatives
// (empty primary plus N candidates). On total failure, commit_missing_schema
// must emit one MissingSchema per candidate so the user sees the full set of
// apiVersions that were tried, not a single empty-apiVersion attribution.
#[test]
fn chain_commit_missing_schema_emits_per_candidate_when_primary_empty() {
    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    // The IR shape for a helper that resolves to TWO branches:
    // empty primary + both branches in candidates.
    let resource = ResourceRef {
        api_version: String::new(),
        kind: "PodSecurityPolicy".to_string(),
        api_version_candidates: vec!["policy/v1".to_string(), "policy/v1beta1".to_string()],
        api_version_branches: Vec::new(),
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<(String, String)> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 2,
        "exactly one MissingSchema per candidate when primary is empty; got {missing:?}"
    );
    let kinds: std::collections::HashSet<String> = missing.iter().map(|(k, _)| k.clone()).collect();
    let api_versions: std::collections::HashSet<String> =
        missing.iter().map(|(_, v)| v.clone()).collect();
    assert!(kinds.contains("PodSecurityPolicy"));
    assert!(
        api_versions.contains("policy/v1"),
        "policy/v1 must have its own MissingSchema; got {api_versions:?}"
    );
    assert!(
        api_versions.contains("policy/v1beta1"),
        "policy/v1beta1 must have its own MissingSchema; got {api_versions:?}"
    );
}

// When the resource carries typed `api_version_branches`, the chain must emit
// one MissingSchema attributed to the live branch (the branch the chart would
// emit at runtime for the primary K8s version), not one MissingSchema per
// branch.
//
// This test pins the "Has guard false, else branch wins" case: the provider
// tells the chain `Has "policy/v1"` is false for its primary version, so the
// else branch (policy/v1beta1) is the runtime-live branch and gets the
// attribution.
#[test]
fn chain_commit_missing_schema_else_branch_attribution_when_has_is_false() {
    use helm_schema_core::{CapabilityGuard, HelperBranch};

    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    )
    .with_capability("policy/v1", false);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: String::new(),
        kind: "PodSecurityPolicy".to_string(),
        api_version_candidates: vec!["policy/v1".to_string(), "policy/v1beta1".to_string()],
        api_version_branches: vec![
            HelperBranch::with_literals(
                Some(CapabilityGuard::Has {
                    api: "policy/v1".to_string(),
                }),
                vec!["policy/v1".to_string()],
            ),
            HelperBranch::with_literals(None, vec!["policy/v1beta1".to_string()]),
        ],
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<(String, String)> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 1,
        "typed-branch resource must emit exactly ONE MissingSchema (live-branch attribution); got {missing:?}"
    );
    sim_assert_eq!(have: missing[0].0, want: "PodSecurityPolicy");
    sim_assert_eq!(
        have: missing[0].1,
        want: "policy/v1beta1",
        "attribution must be the else branch's literal when the Has guard evaluates false"
    );
}

// Same shape, but the Has guard evaluates true. The if-branch is the
// runtime-live branch and gets the attribution. This is the elasticsearch PSP
// regression: chart structurally emits `apiVersion: policy/v1` in K8s 1.35
// (PDB is at policy/v1), PSP does not exist at policy/v1, and the chain
// surfaces this as a real chart bug rather than silently preferring the else
// branch's policy/v1beta1.
#[test]
fn chain_commit_missing_schema_if_branch_attribution_when_has_is_true() {
    use helm_schema_core::{CapabilityGuard, HelperBranch};

    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    )
    .with_capability("policy/v1", true);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: String::new(),
        kind: "PodSecurityPolicy".to_string(),
        api_version_candidates: vec!["policy/v1".to_string(), "policy/v1beta1".to_string()],
        api_version_branches: vec![
            HelperBranch::with_literals(
                Some(CapabilityGuard::Has {
                    api: "policy/v1".to_string(),
                }),
                vec!["policy/v1".to_string()],
            ),
            HelperBranch::with_literals(None, vec!["policy/v1beta1".to_string()]),
        ],
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<(String, String)> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 1,
        "exactly one MissingSchema; got {missing:?}"
    );
    sim_assert_eq!(
        have: missing[0].1,
        want: "policy/v1",
        "attribution must be the if-branch's literal when Has guard evaluates true; the chart really emits this broken apiVersion"
    );
}

// Nested-branch composition through the live Chain. Outer
// `if Has example.com/v1 then (Nested if Has
// other.example.com/v1 then "b" else "b_legacy") else "y"` with both guards
// live means the chain picks the deepest live literal "b" and attributes
// MissingSchema to it. This tests that the chain's commit_missing_schema path
// uses live_literals (which recurses through Nested) for attribution.
#[test]
fn chain_commit_missing_schema_recurses_through_nested_branch_body() {
    use helm_schema_core::{CapabilityGuard, HelperBranch};

    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    )
    .with_capability("example.com/v1", true)
    .with_capability("other.example.com/v1", true);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let nested = vec![
        HelperBranch::with_literals(
            Some(CapabilityGuard::Has {
                api: "other.example.com/v1".to_string(),
            }),
            vec!["b".to_string()],
        ),
        HelperBranch::with_literals(None, vec!["b_legacy".to_string()]),
    ];
    let resource = ResourceRef {
        api_version: String::new(),
        kind: "SomeKind".to_string(),
        api_version_candidates: vec![],
        api_version_branches: vec![
            HelperBranch::with_nested(
                Some(CapabilityGuard::Has {
                    api: "example.com/v1".to_string(),
                }),
                nested,
            ),
            HelperBranch::with_literals(None, vec!["y".to_string()]),
        ],
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<(String, String)> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 1,
        "exactly one MissingSchema; got {missing:?}"
    );
    sim_assert_eq!(
        have: missing[0].1,
        want: "b",
        "chain must recurse through Nested body to attribute to deepest live literal; got {missing:?}"
    );
}

// Same nested shape, but inner Has B is false. The chain must still commit to
// the outer if-branch's subtree (Has A live) and pick the inner else branch's
// literal, not fall back to the outer else branch.
#[test]
fn chain_recurses_through_nested_picks_inner_else_when_inner_has_false() {
    use helm_schema_core::{CapabilityGuard, HelperBranch};

    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    )
    .with_capability("example.com/v1", true)
    .with_capability("other.example.com/v1", false);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let nested = vec![
        HelperBranch::with_literals(
            Some(CapabilityGuard::Has {
                api: "other.example.com/v1".to_string(),
            }),
            vec!["b".to_string()],
        ),
        HelperBranch::with_literals(None, vec!["b_legacy".to_string()]),
    ];
    let resource = ResourceRef {
        api_version: String::new(),
        kind: "SomeKind".to_string(),
        api_version_candidates: vec![],
        api_version_branches: vec![
            HelperBranch::with_nested(
                Some(CapabilityGuard::Has {
                    api: "example.com/v1".to_string(),
                }),
                nested,
            ),
            HelperBranch::with_literals(None, vec!["y".to_string()]),
        ],
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<String> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema { api_version, .. } => Some(api_version),
            _ => None,
        })
        .collect();
    sim_assert_eq!(have: missing.len(), want: 1);
    sim_assert_eq!(
        have: missing[0],
        want: "b_legacy",
        "inner Has-B false must pick inner else, not fall back to outer else"
    );
}

// When there is no unguarded else branch but typed branches are present (e.g.
// only an `if X` with no else), the chain falls back to the last decoded
// branch's first literal, picking the latest alternative the chart author
// considered rather than per-candidate flooding.
#[test]
fn chain_commit_missing_schema_attributes_to_last_branch_when_no_else() {
    use helm_schema_core::{CapabilityGuard, HelperBranch};

    let p = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        false,
        FakeBehaviour::NotOwned,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: String::new(),
        kind: "PodSecurityPolicy".to_string(),
        api_version_candidates: vec!["policy/v1".to_string()],
        api_version_branches: vec![HelperBranch::with_literals(
            Some(CapabilityGuard::Has {
                api: "policy/v1".to_string(),
            }),
            vec!["policy/v1".to_string()],
        )],
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let missing: Vec<(String, String)> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 1,
        "exactly one MissingSchema; got {missing:?}"
    );
    sim_assert_eq!(have: missing[0].1, want: "policy/v1");
}

// schema_fragment_for_use must not turn an intentional `Resolved { schema: None }`
// (PathUnresolved) into MissingSchema. A provider claimed ownership of the
// resource; the YAML path simply has no schema constraint (e.g. ConfigMap.data.X
// where the spec doesn't constrain free-form data). Emitting
// MissingSchema in that case floods stderr with diagnostics for
// built-in K8s resources whose schemas legitimately don't constrain
// every nested path.
#[test]
fn chain_schema_fragment_for_use_path_unresolved_does_not_leak_missing_schema() {
    // Provider claims ownership (has_resource=true) and returns
    // PathUnresolved (silent gap, not a missing-resource case).
    let provider = FakeProvider::new(
        ProviderOrigin::KubernetesOpenApi,
        true,
        FakeBehaviour::PathUnresolved,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());

    let use_ = provider_use(
        YamlPath(vec!["data".to_string(), "config-blob".to_string()]),
        ResourceRef {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
    );
    let schema = chain.schema_fragment_for_use(&use_);
    assert!(
        schema.is_none(),
        "PathUnresolved must surface as None schema (intentional silence)"
    );

    let snapshot = diagnostics.snapshot();
    let any_missing = snapshot
        .iter()
        .any(|d| matches!(d, Diagnostic::MissingSchema { .. }));
    assert!(
        !any_missing,
        "PathUnresolved on an owned resource must NOT trigger MissingSchema; sink contained: {snapshot:?}"
    );
}

// Variant of the above: multi-candidate iteration, ALL candidates
// resolve to PathUnresolved (each via different apiVersion). No
// MissingSchema must fire. Resolved{None} from any candidate is
// ownership claim, and the chain must honour the silence.
#[test]
fn chain_schema_fragment_for_use_multi_candidate_all_path_unresolved_does_not_leak() {
    #[derive(Debug)]
    struct AlwaysPathUnresolved;
    impl ResourceSchemaOracle for AlwaysPathUnresolved {
        fn schema_fragment_for_resource_path(
            &self,
            _r: &ResourceRef,
            _p: &YamlPath,
        ) -> Option<ProviderSchemaFragment> {
            None
        }
    }

    impl K8sSchemaProvider for AlwaysPathUnresolved {
        fn origin(&self) -> ProviderOrigin {
            ProviderOrigin::KubernetesOpenApi
        }
        fn lookup(&self, _r: &ResourceRef, _p: &YamlPath) -> ProviderLookupResult {
            ProviderLookupResult::PathUnresolved
        }
        fn has_resource(&self, _r: &ResourceRef) -> bool {
            true
        }
    }

    let diagnostics = DiagnosticSink::new();
    let chain =
        Chain::new(vec![Box::new(AlwaysPathUnresolved)]).with_diagnostic_sink(diagnostics.clone());

    let use_ = provider_use(
        YamlPath(vec!["data".to_string()]),
        ResourceRef {
            api_version: "policy/v1beta1".to_string(),
            kind: "PodSecurityPolicy".to_string(),
            api_version_candidates: vec!["policy/v1".to_string()],
            api_version_branches: Vec::new(),
        },
    );
    let _ = chain.schema_fragment_for_use(&use_);

    let missing_count = diagnostics
        .snapshot()
        .into_iter()
        .filter(|d| matches!(d, Diagnostic::MissingSchema { .. }))
        .count();
    sim_assert_eq!(
        have: missing_count,
        want: 0,
        "no MissingSchema must fire when every candidate's outcome is PathUnresolved (ownership claim)"
    );
}

// When every candidate misses, exactly one MissingSchema fires and it carries
// the user-written primary apiVersion, not a speculative candidate.
#[test]
fn chain_schema_fragment_for_use_total_failure_attributes_to_primary() {
    let p = FakeProvider::new(
        ProviderOrigin::DefaultCatalog,
        false,
        FakeBehaviour::NotOwned,
    );
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(p)]).with_diagnostic_sink(diagnostics.clone());

    let use_ = provider_use(
        YamlPath(Vec::new()),
        ResourceRef {
            api_version: "policy/v1beta1".to_string(),
            kind: "PodSecurityPolicy".to_string(),
            api_version_candidates: vec!["policy/v1".to_string()],
            api_version_branches: Vec::new(),
        },
    );
    let _ = chain.schema_fragment_for_use(&use_);

    let missing: Vec<_> = diagnostics
        .snapshot()
        .into_iter()
        .filter_map(|d| match d {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => Some((kind, api_version)),
            _ => None,
        })
        .collect();
    sim_assert_eq!(
        have: missing.len(),
        want: 1,
        "exactly one MissingSchema, not one per candidate; got {missing:?}"
    );
    sim_assert_eq!(
        have: missing[0],
        want: (
            "PodSecurityPolicy".to_string(),
            "policy/v1beta1".to_string()
        ),
        "attribution must be the user-written primary apiVersion, not the speculative candidate"
    );
}
