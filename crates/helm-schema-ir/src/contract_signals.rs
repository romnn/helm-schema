use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{ChartFacts, Guard, ResourceRef, ValueKind, YamlPath};

/// Contract fact that needs a Kubernetes resource schema lookup.
///
/// This is narrower than [`ContractUse`]: schema providers need only the
/// rendered resource/path target, while generator policy also needs the input
/// values path and value-kind domain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderSchemaUse {
    pub value_path: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub resource: ResourceRef,
    pub is_self_range_collection: bool,
}

impl ProviderSchemaUse {
    #[must_use]
    pub fn from_contract_use(contract_use: &ContractUse) -> Option<Self> {
        if contract_use.source_expr.trim().is_empty()
            || contract_use.kind == ValueKind::PartialScalar
            || contract_use.path.0.is_empty()
        {
            return None;
        }
        let resource = contract_use.resource.clone()?;

        Some(Self {
            value_path: contract_use.source_expr.clone(),
            path: contract_use.path.clone(),
            kind: contract_use.kind,
            resource,
            is_self_range_collection: use_is_self_range_collection(contract_use),
        })
    }
}

/// Type-level constraints declared by template guards.
///
/// These are contract facts, not JSON Schema fragments. Schema lowering stays
/// in the generator so the contract layer remains independent from output
/// format policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardConstraint {
    /// `if eq .Values.X "value"` admits the literal value when the branch
    /// renders.
    Eq { value: String },
    /// `if typeIs "<json type>" .Values.X` declares the type accepted by the
    /// branch.
    TypeIs { schema_type: String },
}

/// Kubernetes `metadata.*` field shape referenced by a values path.
///
/// The contract layer records the field category structurally from the
/// rendered document path. JSON Schema lowering remains a generator policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetadataFieldKind {
    /// `metadata.labels` and `metadata.annotations`.
    StringMap,
    /// `metadata.name`.
    Name,
    /// `metadata.namespace`.
    Namespace,
}

/// Path-level facts derived directly from normalized contract claims.
///
/// These are the values paths that downstream schema generation must consider,
/// plus typed guard facts that can be lowered by generator policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractPathSignals {
    pub referenced_value_paths: BTreeSet<String>,
    pub ranged_value_paths: BTreeSet<String>,
    pub value_paths_used_as_fragment: BTreeSet<String>,
    pub partial_scalar_value_paths: BTreeSet<String>,
    pub guard_constraints_by_value_path: BTreeMap<String, Vec<GuardConstraint>>,
    pub metadata_fields_by_value_path: BTreeMap<String, BTreeSet<MetadataFieldKind>>,
}

/// Compatibility signal for the optional `required` schema post-pass.
///
/// The contract layer identifies which paths look like positive guard headers
/// and which paths are ruled out by optional control flow. JSON Schema mutation
/// remains a generator policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequiredInferenceSignals {
    pub positive_header_paths: BTreeSet<String>,
    pub conditionally_optional_paths: BTreeSet<String>,
    pub default_fallback_paths: BTreeSet<String>,
}

/// Contract-derived facts consumed by core values-schema generation.
///
/// This is the typed boundary between static template interpretation and JSON
/// Schema lowering. Optional post-passes can ask for their own projections,
/// but core schema generation should consume this artifact rather than
/// re-reading raw contract claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractSchemaSignals {
    pub chart_facts: ChartFacts,
    pub path_signals: ContractPathSignals,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub nullable_value_paths: BTreeSet<String>,
    pub paths_with_referenced_descendants: BTreeSet<String>,
    pub value_path_facts: BTreeMap<String, ContractValuePathFacts>,
    pub required_inference_signals: RequiredInferenceSignals,
}

/// Schema-generation facts for one input values path.
///
/// This bundles the contract-owned path state that schema lowering needs, so
/// generator code does not have to reconstruct semantic facts from multiple
/// lower-level projections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContractValuePathFacts {
    pub has_referenced_descendants: bool,
    pub used_as_fragment: bool,
    pub is_ranged_source: bool,
    pub is_partial_scalar_value_path: bool,
    pub has_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_self_range_guard_render_use: bool,
    pub is_nullable: bool,
}

fn use_is_self_range_collection(use_: &ContractUse) -> bool {
    use_.guards
        .iter()
        .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr))
        && use_
            .path
            .0
            .last()
            .is_none_or(|segment| !segment.ends_with("[*]"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ContractIr, ContractProjection};

    #[test]
    fn contract_projection_nullable_paths_include_range_only_collection() {
        let projection = ContractProjection::from_contract_uses(vec![ContractUse::new(
            "snapshot".to_string(),
            YamlPath(vec!["data".to_string(), "command".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Range {
                path: "snapshots".to_string(),
            }],
            None,
        )]);

        let nullable_paths = projection.schema_signals().nullable_value_paths;

        assert!(
            nullable_paths.contains("snapshots"),
            "range sources are null-tolerant because Helm treats nil range inputs as empty: {nullable_paths:?}",
        );
        assert!(
            !nullable_paths.contains("snapshot"),
            "range item values should not inherit collection nullability: {nullable_paths:?}",
        );
    }

    #[test]
    fn contract_projection_nullable_paths_require_every_render_use_to_be_tolerant() {
        let path = YamlPath(vec!["metadata".to_string(), "name".to_string()]);
        let projection = ContractProjection::from_contract_uses(vec![
            ContractUse::new(
                "serviceAccount.name".to_string(),
                path.clone(),
                ValueKind::Scalar,
                vec![Guard::Default {
                    path: "serviceAccount.name".to_string(),
                }],
                None,
            ),
            ContractUse::new(
                "serviceAccount.name".to_string(),
                path,
                ValueKind::Scalar,
                Vec::new(),
                None,
            ),
        ]);

        let nullable_paths = projection.schema_signals().nullable_value_paths;

        assert!(
            !nullable_paths.contains("serviceAccount.name"),
            "one guarded render use must not make a bare render site nullable: {nullable_paths:?}",
        );
    }

    #[test]
    fn contract_projection_path_signals_collect_references_and_typed_guard_constraints() {
        let projection = ContractProjection::from_contract_uses(vec![
            ContractUse::new(
                "podLabels".to_string(),
                YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
                ValueKind::Fragment,
                vec![
                    Guard::Eq {
                        path: "mode".to_string(),
                        value: "prod".to_string(),
                    },
                    Guard::TypeIs {
                        path: "extraConfig".to_string(),
                        schema_type: "string".to_string(),
                    },
                    Guard::Range {
                        path: "extraEnv".to_string(),
                    },
                ],
                None,
            ),
            ContractUse::new(
                "image.tag".to_string(),
                YamlPath(vec!["spec".to_string(), "image".to_string()]),
                ValueKind::PartialScalar,
                Vec::new(),
                None,
            ),
            ContractUse::new(
                "podName".to_string(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                Vec::new(),
                None,
            ),
            ContractUse::new(
                "podNamespace".to_string(),
                YamlPath(vec!["metadata".to_string(), "namespace".to_string()]),
                ValueKind::Scalar,
                Vec::new(),
                None,
            ),
            ContractUse::new(
                String::new(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Eq {
                    path: "ignored.guard".to_string(),
                    value: "ignored".to_string(),
                }],
                None,
            ),
        ]);

        let signals = projection.schema_signals().path_signals;

        assert_eq!(
            signals.referenced_value_paths,
            BTreeSet::from([
                "extraConfig".to_string(),
                "extraEnv".to_string(),
                "image.tag".to_string(),
                "mode".to_string(),
                "podLabels".to_string(),
                "podName".to_string(),
                "podNamespace".to_string(),
            ]),
        );
        assert_eq!(
            signals.ranged_value_paths,
            BTreeSet::from(["extraEnv".to_string()]),
        );
        assert_eq!(
            signals.value_paths_used_as_fragment,
            BTreeSet::from(["podLabels".to_string()]),
        );
        assert_eq!(
            signals.partial_scalar_value_paths,
            BTreeSet::from(["image.tag".to_string()]),
        );
        assert_eq!(
            signals.metadata_fields_by_value_path.get("podLabels"),
            Some(&BTreeSet::from([MetadataFieldKind::StringMap])),
        );
        assert_eq!(
            signals.metadata_fields_by_value_path.get("podName"),
            Some(&BTreeSet::from([MetadataFieldKind::Name])),
        );
        assert_eq!(
            signals.metadata_fields_by_value_path.get("podNamespace"),
            Some(&BTreeSet::from([MetadataFieldKind::Namespace])),
        );
        assert_eq!(
            signals.guard_constraints_by_value_path.get("mode"),
            Some(&vec![GuardConstraint::Eq {
                value: "prod".to_string(),
            }]),
        );
        assert_eq!(
            signals.guard_constraints_by_value_path.get("extraConfig"),
            Some(&vec![GuardConstraint::TypeIs {
                schema_type: "string".to_string(),
            }]),
        );
        assert!(
            !signals.referenced_value_paths.contains("ignored.guard"),
            "empty-source compatibility rows should not seed schema paths",
        );
        assert!(
            !signals.metadata_fields_by_value_path.contains_key(""),
            "empty-source compatibility rows should not seed metadata facts",
        );
    }

    #[test]
    fn contract_projection_provider_schema_uses_are_rendered_resource_claims_only() {
        let resource = ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        let projection = ContractProjection::from_contract_uses(vec![
            ContractUse::new(
                "containers".to_string(),
                YamlPath(vec![
                    "spec".to_string(),
                    "template".to_string(),
                    "spec".to_string(),
                    "containers".to_string(),
                ]),
                ValueKind::Fragment,
                Vec::new(),
                Some(resource.clone()),
            ),
            ContractUse::new(
                "ports".to_string(),
                YamlPath(vec!["spec".to_string(), "ports".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Range {
                    path: "ports".to_string(),
                }],
                Some(resource.clone()),
            ),
            ContractUse::new(
                "image.tag".to_string(),
                YamlPath(vec!["spec".to_string(), "image".to_string()]),
                ValueKind::PartialScalar,
                Vec::new(),
                Some(resource.clone()),
            ),
            ContractUse::new(
                "pathless".to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                Vec::new(),
                Some(resource.clone()),
            ),
            ContractUse::new(
                "noResource".to_string(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                Vec::new(),
                None,
            ),
            ContractUse::new(
                String::new(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                Vec::new(),
                Some(resource),
            ),
        ]);

        let requests = projection.schema_signals().provider_schema_uses;

        assert_eq!(requests.len(), 2, "{requests:#?}");
        assert_eq!(requests[0].value_path, "containers");
        assert_eq!(requests[0].kind, ValueKind::Fragment);
        assert!(!requests[0].is_self_range_collection);
        assert_eq!(requests[1].value_path, "ports");
        assert_eq!(requests[1].kind, ValueKind::Scalar);
        assert!(requests[1].is_self_range_collection);
    }

    #[test]
    fn contract_projection_schema_signals_bundle_core_generation_facts() {
        let resource = ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        let projection = ContractProjection::from_contract_uses(vec![
            ContractUse::new(
                "podLabels".to_string(),
                YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
                ValueKind::Fragment,
                Vec::new(),
                Some(resource.clone()),
            ),
            ContractUse::new(
                "serviceAccount.name".to_string(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Default {
                    path: "serviceAccount.name".to_string(),
                }],
                Some(resource),
            ),
        ]);

        let signals = projection.schema_signals();

        assert_eq!(
            signals
                .path_signals
                .metadata_fields_by_value_path
                .get("podLabels"),
            Some(&BTreeSet::from([MetadataFieldKind::StringMap])),
        );
        assert!(
            signals.nullable_value_paths.contains("serviceAccount.name"),
            "default-guarded render use should surface as nullable contract evidence",
        );
        assert!(
            signals
                .paths_with_referenced_descendants
                .contains("serviceAccount"),
            "contract schema signals should own descendant path topology",
        );
        assert!(
            signals
                .chart_facts
                .path_facts
                .get("serviceAccount.name")
                .is_some_and(|fact| fact.has_render_use && fact.all_render_uses_self_guarded),
        );
        assert!(
            signals
                .value_path_facts
                .get("serviceAccount")
                .is_some_and(|fact| fact.has_referenced_descendants),
            "contract value-path facts should own descendant path topology",
        );
        assert!(
            signals
                .value_path_facts
                .get("serviceAccount.name")
                .is_some_and(|fact| fact.has_render_use
                    && fact.all_render_uses_self_guarded
                    && fact.is_nullable),
            "contract value-path facts should bundle nullable render-use evidence",
        );
        assert_eq!(signals.provider_schema_uses.len(), 2);
    }

    #[test]
    fn contract_ir_derives_schema_signals_without_projection_detour() {
        let resource = ResourceRef {
            api_version: "v1".to_string(),
            kind: "ServiceAccount".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            Some(resource.clone()),
        ));
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }],
            Some(resource),
        ));
        contract.push(ContractUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Fragment,
            Vec::new(),
            None,
        ));

        let projection_signals = contract.clone().project().schema_signals();
        let direct_signals = contract.into_schema_signals();

        assert_eq!(direct_signals, projection_signals);
    }

    #[test]
    fn contract_projection_required_inference_signals_are_typed_header_facts() {
        let projection = ContractProjection::from_contract_uses(vec![
            ContractUse::new(
                "feature.enabled".to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                Vec::new(),
                None,
            ),
            ContractUse::new(
                "mode".to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                vec![Guard::Eq {
                    path: "mode".to_string(),
                    value: "strict".to_string(),
                }],
                None,
            ),
            ContractUse::new(
                "optional".to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                vec![Guard::Not {
                    path: "optional".to_string(),
                }],
                None,
            ),
            ContractUse::new(
                "either.primary".to_string(),
                YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Or {
                    paths: vec!["either.primary".to_string(), "either.fallback".to_string()],
                }],
                None,
            ),
            ContractUse::new(
                "ranged".to_string(),
                YamlPath(vec!["spec".to_string(), "ports".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Range {
                    path: "ranged".to_string(),
                }],
                None,
            ),
            ContractUse::new(
                "defaulted".to_string(),
                YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Default {
                    path: "defaulted".to_string(),
                }],
                None,
            ),
        ]);

        let signals = projection.schema_signals().required_inference_signals;

        assert_eq!(
            signals.positive_header_paths,
            BTreeSet::from(["feature.enabled".to_string(), "mode".to_string()])
        );
        assert_eq!(
            signals.conditionally_optional_paths,
            BTreeSet::from([
                "optional".to_string(),
                "either.primary".to_string(),
                "either.fallback".to_string(),
            ])
        );
        assert_eq!(
            signals.default_fallback_paths,
            BTreeSet::from(["defaulted".to_string()])
        );
    }
}
