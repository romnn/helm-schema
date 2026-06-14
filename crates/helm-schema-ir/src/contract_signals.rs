use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{ChartFacts, Guard, PathFact, ResourceRef, ValueKind, YamlPath};

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

pub(crate) fn derive_schema_signals_from_uses(uses: &[ContractUse]) -> ContractSchemaSignals {
    let mut builder = ContractSchemaSignalBuilder::default();
    for contract_use in uses {
        builder.record(contract_use);
    }
    builder.finish()
}

#[derive(Default)]
struct ContractSchemaSignalBuilder {
    chart_facts_by_path: BTreeMap<String, ChartPathAccumulator>,
    chart_descendant_paths: BTreeSet<String>,
    path_signals: ContractPathSignals,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    nullable_by_path: BTreeMap<String, NullablePathAccumulator>,
    required_inference_signals: RequiredInferenceSignals,
}

struct ChartPathAccumulator {
    has_render_use: bool,
    all_render_uses_self_guarded: bool,
    has_fragment_render: bool,
    has_self_range_guard_render_use: bool,
}

impl ChartPathAccumulator {
    fn new() -> Self {
        Self {
            has_render_use: false,
            all_render_uses_self_guarded: true,
            has_fragment_render: false,
            has_self_range_guard_render_use: false,
        }
    }
}

struct NullablePathAccumulator {
    has_render_use: bool,
    all_uses_nullable: bool,
}

impl NullablePathAccumulator {
    fn new() -> Self {
        Self {
            has_render_use: false,
            all_uses_nullable: true,
        }
    }
}

impl ContractSchemaSignalBuilder {
    fn record(&mut self, contract_use: &ContractUse) {
        self.record_provider_schema_use(contract_use);
        self.record_chart_facts(contract_use);
        self.record_path_signals(contract_use);
        self.record_nullable_path(contract_use);
        self.record_required_inference_signals(contract_use);
    }

    fn finish(self) -> ContractSchemaSignals {
        let chart_descendant_paths = self.chart_descendant_paths;
        let path_facts: BTreeMap<String, PathFact> = self
            .chart_facts_by_path
            .into_iter()
            .map(|(path, acc)| {
                (
                    path.clone(),
                    PathFact {
                        has_render_use: acc.has_render_use,
                        all_render_uses_self_guarded: acc.all_render_uses_self_guarded,
                        has_fragment_render: acc.has_fragment_render,
                        descendant_accessed: chart_descendant_paths.contains(&path),
                        has_self_range_guard_render_use: acc.has_self_range_guard_render_use,
                    },
                )
            })
            .collect();
        let paths_with_referenced_descendants =
            collect_paths_with_descendants(&self.path_signals.referenced_value_paths);
        let nullable_value_paths = self
            .nullable_by_path
            .into_iter()
            .filter_map(|(path, acc)| (acc.has_render_use && acc.all_uses_nullable).then_some(path))
            .collect();
        let value_path_facts = build_contract_value_path_facts(
            &path_facts,
            &self.path_signals,
            &nullable_value_paths,
            &paths_with_referenced_descendants,
        );

        ContractSchemaSignals {
            chart_facts: ChartFacts { path_facts },
            path_signals: self.path_signals,
            provider_schema_uses: self.provider_schema_uses,
            nullable_value_paths,
            paths_with_referenced_descendants,
            value_path_facts,
            required_inference_signals: self.required_inference_signals,
        }
    }

    fn record_provider_schema_use(&mut self, contract_use: &ContractUse) {
        if let Some(provider_use) = ProviderSchemaUse::from_contract_use(contract_use) {
            self.provider_schema_uses.push(provider_use);
        }
    }

    fn record_chart_facts(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            self.record_empty_source_chart_facts(contract_use);
            return;
        }

        self.chart_accumulator(&contract_use.source_expr);
        if !contract_use.path.0.is_empty() {
            let self_range_guarded = contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
            );
            self.record_chart_render_use(
                &contract_use.source_expr,
                contract_use,
                self_range_guarded,
                Some(use_is_self_guarded(contract_use)),
            );
        }

        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() || path == contract_use.source_expr {
                    continue;
                }
                self.chart_accumulator(path);
                if !contract_use.path.0.is_empty() {
                    self.record_chart_render_use(
                        path,
                        contract_use,
                        matches!(guard, Guard::Range { .. }),
                        None,
                    );
                }
            }
        }

        let mut segments: Vec<&str> = contract_use
            .source_expr
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            self.chart_descendant_paths.insert(segments.join("."));
        }
    }

    fn record_empty_source_chart_facts(&mut self, contract_use: &ContractUse) {
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.chart_accumulator(path);
                if !contract_use.path.0.is_empty() {
                    self.record_chart_render_use(
                        path,
                        contract_use,
                        matches!(guard, Guard::Range { .. }),
                        None,
                    );
                }
            }
        }
    }

    fn chart_accumulator(&mut self, path: &str) -> &mut ChartPathAccumulator {
        self.chart_facts_by_path
            .entry(path.to_string())
            .or_insert_with(ChartPathAccumulator::new)
    }

    fn record_chart_render_use(
        &mut self,
        path: &str,
        contract_use: &ContractUse,
        range_guarded: bool,
        self_guarded: Option<bool>,
    ) {
        let acc = self.chart_accumulator(path);
        acc.has_render_use = true;
        acc.has_fragment_render |= contract_use.kind == ValueKind::Fragment;
        acc.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            acc.all_render_uses_self_guarded &= self_guarded;
        }
    }

    fn record_path_signals(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        self.path_signals
            .referenced_value_paths
            .insert(contract_use.source_expr.clone());
        if contract_use.kind == ValueKind::Fragment {
            self.path_signals
                .value_paths_used_as_fragment
                .insert(contract_use.source_expr.clone());
        }
        if contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty() {
            self.path_signals
                .partial_scalar_value_paths
                .insert(contract_use.source_expr.clone());
        }
        if let Some(field_kind) = metadata_field_kind_from_yaml_path(&contract_use.path.0) {
            self.path_signals
                .metadata_fields_by_value_path
                .entry(contract_use.source_expr.clone())
                .or_default()
                .insert(field_kind);
        }
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.path_signals
                    .referenced_value_paths
                    .insert(path.to_string());
                if matches!(guard, Guard::Range { .. }) {
                    self.path_signals
                        .ranged_value_paths
                        .insert(path.to_string());
                }

                if let Some(constraint) = guard_constraint_from_guard(guard) {
                    self.path_signals
                        .guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(constraint);
                }
            }
        }
    }

    fn record_nullable_path(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let has_self_range_guard = contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
        );
        let info = self.nullable_accumulator(&contract_use.source_expr);
        if !contract_use.path.0.is_empty()
            || has_self_range_guard
            || contract_use.kind == ValueKind::Fragment
        {
            info.has_render_use = true;
        }
        info.all_uses_nullable &= use_is_null_tolerant(contract_use);

        for guard in &contract_use.guards {
            if let Guard::Range { path } = guard
                && !path.trim().is_empty()
            {
                self.nullable_accumulator(path).has_render_use = true;
            }
        }
    }

    fn nullable_accumulator(&mut self, path: &str) -> &mut NullablePathAccumulator {
        self.nullable_by_path
            .entry(path.to_string())
            .or_insert_with(NullablePathAccumulator::new)
    }

    fn record_required_inference_signals(&mut self, contract_use: &ContractUse) {
        for guard in &contract_use.guards {
            match guard {
                Guard::Not { path } => {
                    self.required_inference_signals
                        .conditionally_optional_paths
                        .insert(path.clone());
                }
                Guard::Or { paths } => {
                    self.required_inference_signals
                        .conditionally_optional_paths
                        .extend(paths.iter().cloned());
                }
                Guard::Default { path } => {
                    self.required_inference_signals
                        .default_fallback_paths
                        .insert(path.clone());
                }
                Guard::Truthy { .. }
                | Guard::Eq { .. }
                | Guard::Range { .. }
                | Guard::With { .. }
                | Guard::TypeIs { .. } => {}
            }
        }

        if contract_use.kind == ValueKind::Scalar
            && contract_use.path.0.is_empty()
            && !contract_use.source_expr.trim().is_empty()
            && use_is_positive_header(contract_use)
        {
            self.required_inference_signals
                .positive_header_paths
                .insert(contract_use.source_expr.clone());
        }
    }
}

fn build_contract_value_path_facts(
    path_facts: &BTreeMap<String, PathFact>,
    path_signals: &ContractPathSignals,
    nullable_value_paths: &BTreeSet<String>,
    paths_with_referenced_descendants: &BTreeSet<String>,
) -> BTreeMap<String, ContractValuePathFacts> {
    let mut paths = BTreeSet::new();
    paths.extend(path_facts.keys().cloned());
    paths.extend(path_signals.referenced_value_paths.iter().cloned());
    paths.extend(path_signals.ranged_value_paths.iter().cloned());
    paths.extend(path_signals.value_paths_used_as_fragment.iter().cloned());
    paths.extend(path_signals.partial_scalar_value_paths.iter().cloned());
    paths.extend(path_signals.guard_constraints_by_value_path.keys().cloned());
    paths.extend(path_signals.metadata_fields_by_value_path.keys().cloned());
    paths.extend(nullable_value_paths.iter().cloned());
    paths.extend(paths_with_referenced_descendants.iter().cloned());

    paths
        .into_iter()
        .map(|path| {
            let chart_fact = path_facts.get(&path).cloned().unwrap_or_default();
            (
                path.clone(),
                ContractValuePathFacts {
                    has_referenced_descendants: paths_with_referenced_descendants.contains(&path),
                    used_as_fragment: path_signals.value_paths_used_as_fragment.contains(&path),
                    is_ranged_source: path_signals.ranged_value_paths.contains(&path),
                    is_partial_scalar_value_path: path_signals
                        .partial_scalar_value_paths
                        .contains(&path),
                    has_render_use: chart_fact.has_render_use,
                    all_render_uses_self_guarded: chart_fact.all_render_uses_self_guarded,
                    has_self_range_guard_render_use: chart_fact.has_self_range_guard_render_use,
                    is_nullable: nullable_value_paths.contains(&path),
                },
            )
        })
        .collect()
}

fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
}

fn use_is_positive_header(use_: &ContractUse) -> bool {
    use_.guards.is_empty()
        || use_.guards.iter().all(|guard| match guard {
            Guard::Truthy { path } | Guard::Eq { path, .. } | Guard::TypeIs { path, .. } => {
                path == &use_.source_expr
            }
            Guard::Not { .. }
            | Guard::Or { .. }
            | Guard::Range { .. }
            | Guard::With { .. }
            | Guard::Default { .. } => false,
        })
}

fn metadata_field_kind_from_yaml_path(path: &[String]) -> Option<MetadataFieldKind> {
    let last = path.last()?.as_str();
    let prev = path.get(path.len().checked_sub(2)?)?.as_str();
    if prev != "metadata" {
        return None;
    }

    match last {
        "labels" | "annotations" => Some(MetadataFieldKind::StringMap),
        "name" => Some(MetadataFieldKind::Name),
        "namespace" => Some(MetadataFieldKind::Namespace),
        _ => None,
    }
}

fn guard_constraint_from_guard(guard: &Guard) -> Option<GuardConstraint> {
    match guard {
        Guard::Eq { value, .. } => Some(GuardConstraint::Eq {
            value: value.clone(),
        }),
        Guard::TypeIs { schema_type, .. } => Some(GuardConstraint::TypeIs {
            schema_type: schema_type.clone(),
        }),
        Guard::Truthy { .. }
        | Guard::Not { .. }
        | Guard::Or { .. }
        | Guard::Range { .. }
        | Guard::With { .. }
        | Guard::Default { .. } => None,
    }
}

fn use_is_self_guarded(use_: &ContractUse) -> bool {
    if use_.path.0.is_empty() {
        return true;
    }

    use_has_matching_self_guard(use_)
}

fn use_is_null_tolerant(use_: &ContractUse) -> bool {
    if use_.path.0.is_empty() {
        return true;
    }

    use_has_matching_self_guard(use_)
}

fn use_has_matching_self_guard(use_: &ContractUse) -> bool {
    use_.guards.iter().any(|guard| match guard {
        Guard::Truthy { path }
        | Guard::Eq { path, .. }
        | Guard::Range { path }
        | Guard::With { path }
        | Guard::Default { path } => path == &use_.source_expr,
        Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => false,
    })
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
