use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ChartFacts, Guard, PathFact, ResourceRef, ValueKind, ValueUse, YamlPath};

/// Context applied when semantic facts are lowered to contract claims.
///
/// The interpreter owns the facts. This type owns the common projection policy:
/// ambient guards, render-suppressed paths, partial-scalar normalization, and
/// chart-level default mutations.
pub(crate) struct ContractUseContext<'a> {
    guards: &'a [Guard],
    chart_value_defaults: &'a BTreeSet<String>,
    suppress_document_path: bool,
}

impl<'a> ContractUseContext<'a> {
    pub(crate) fn new(
        guards: &'a [Guard],
        chart_value_defaults: &'a BTreeSet<String>,
        suppress_document_path: bool,
    ) -> Self {
        Self {
            guards,
            chart_value_defaults,
            suppress_document_path,
        }
    }

    pub(crate) fn contract_use(
        &self,
        source_expr: String,
        mut path: YamlPath,
        mut kind: ValueKind,
        extra_guards: &[Guard],
        resource: Option<ResourceRef>,
    ) -> ContractUse {
        if self.suppress_document_path {
            path = YamlPath(Vec::new());
        }
        if kind == ValueKind::PartialScalar && path.0.is_empty() {
            kind = ValueKind::Scalar;
        }

        let mut guards = self.guards_with(extra_guards);
        if !path.0.is_empty() && self.chart_value_defaults.contains(&source_expr) {
            let default_guard = Guard::Default {
                path: source_expr.clone(),
            };
            merge_guards(&mut guards, std::slice::from_ref(&default_guard));
        }

        ContractUse::new(source_expr, path, kind, guards, resource)
    }

    pub(crate) fn pathless_contract_use(
        &self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) -> ContractUse {
        self.contract_use(source_expr, YamlPath(Vec::new()), kind, extra_guards, None)
    }

    fn guards_with(&self, extra_guards: &[Guard]) -> Vec<Guard> {
        let mut guards = self.guards.to_vec();
        merge_guards(&mut guards, extra_guards);
        guards
    }
}

/// A normalized contract claim for one observed values path.
///
/// This is still the migration-era claim shape, but it is owned by the
/// contract layer. [`ValueUse`] remains the serialized fixture DTO at the
/// inspection boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<Guard>,
    pub resource: Option<ResourceRef>,
}

impl ContractUse {
    pub(crate) fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
        }
    }

    fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.source_expr = map(&self.source_expr);
        self.guards = std::mem::take(&mut self.guards)
            .into_iter()
            .map(|guard| guard.map_value_paths(map))
            .collect();
    }
}

impl From<ValueUse> for ContractUse {
    fn from(value_use: ValueUse) -> Self {
        Self {
            source_expr: value_use.source_expr,
            path: value_use.path,
            kind: value_use.kind,
            guards: value_use.guards,
            resource: value_use.resource,
        }
    }
}

impl From<ContractUse> for ValueUse {
    fn from(contract_use: ContractUse) -> Self {
        Self {
            source_expr: contract_use.source_expr,
            path: contract_use.path,
            kind: contract_use.kind,
            guards: contract_use.guards,
            resource: contract_use.resource,
        }
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
}

/// Receives contract claims from node/action interpretation.
///
/// Some helper-summary passes intentionally implement this as a no-op because
/// they collect local helper facts rather than root chart contract claims.
pub(crate) trait ContractUseSink {
    fn emit_contract_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind);

    fn emit_contract_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    );
}

/// Normalized projection of a contract graph.
///
/// Production callers pass this artifact between analysis and schema
/// generation. Fixture and external tooling code can still project it to
/// [`ValueUse`] DTO rows explicitly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractProjection {
    uses: Vec<ContractUse>,
}

impl ContractProjection {
    /// Build a projection from already-projected compatibility DTOs.
    ///
    /// This is for tests and transitional consumers that still construct
    /// [`ValueUse`] rows directly. Interpreter code should produce
    /// [`ContractIr`] and call [`ContractIr::project`] instead.
    pub fn from_value_uses(uses: Vec<ValueUse>) -> Self {
        let mut uses: Vec<ContractUse> = uses.into_iter().map(ContractUse::from).collect();
        uses.sort();
        uses.dedup();
        Self { uses }
    }

    /// Build a normalized projection from contract-layer claims.
    pub fn from_contract_uses(mut uses: Vec<ContractUse>) -> Self {
        uses.sort();
        uses.dedup();
        Self { uses }
    }

    /// Borrow the normalized contract claims.
    pub fn uses(&self) -> &[ContractUse] {
        &self.uses
    }

    /// Consume the projection and return the compatibility DTOs.
    pub fn into_value_uses(self) -> Vec<ValueUse> {
        self.uses.into_iter().map(ValueUse::from).collect()
    }

    /// Derive chart-level path facts from this normalized projection.
    #[must_use]
    pub fn chart_facts(&self) -> ChartFacts {
        derive_chart_facts_from_uses(&self.uses)
    }

    /// Derive path-level reference and guard-constraint facts from this
    /// normalized projection.
    #[must_use]
    pub fn path_signals(&self) -> ContractPathSignals {
        derive_path_signals_from_uses(&self.uses)
    }

    /// Identify value paths for which an explicit `null` default is accepted
    /// by the chart's template control flow.
    ///
    /// A path qualifies when every observed use is null-tolerant and at least
    /// one rendered use provides non-null type evidence. Header-only
    /// guard/binding uses are null-tolerant because Helm evaluates them
    /// against `nil`; rendered uses must sit under a self-guard such as `if`,
    /// `with`, `range`, `eq`, or a structural chart-default mutation for the
    /// same values path.
    #[must_use]
    pub fn nullable_value_paths(&self) -> BTreeSet<String> {
        derive_nullable_value_paths_from_uses(&self.uses)
    }
}

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
}

impl ContractIr {
    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.uses.push(contract_use);
    }

    /// Add a pathless scalar claim for a value path.
    ///
    /// Pathless claims make a value path visible to downstream schema
    /// generation without asserting any rendered Kubernetes field shape.
    pub fn push_pathless_scalar(&mut self, source_expr: impl Into<String>) {
        self.push(ContractUse::new(
            source_expr.into(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
    }

    /// Move all claims from another contract graph into this graph.
    pub fn append(&mut self, mut other: Self) {
        self.uses.append(&mut other.uses);
    }

    /// Rewrite all referenced values paths while preserving rendered YAML paths.
    ///
    /// This is used at chart boundaries where a dependency's `.Values.foo`
    /// contract becomes `.Values.subchart.foo`, while rendered manifest paths
    /// such as `metadata.name` stay unchanged.
    pub fn map_value_paths<F>(&mut self, mut map: F)
    where
        F: FnMut(&str) -> String,
    {
        for contract_use in &mut self.uses {
            contract_use.map_value_paths(&mut map);
        }
    }

    /// Normalize claims and project them to the schema-generation artifact.
    pub fn project(mut self) -> ContractProjection {
        self.normalize();
        ContractProjection { uses: self.uses }
    }

    /// Normalize claims and project them to the fixture `ValueUse` DTO.
    pub fn into_value_uses(self) -> Vec<ValueUse> {
        self.project().into_value_uses()
    }

    fn normalize(&mut self) {
        drop_default_guard_subsumed_duplicates(&mut self.uses);
        drop_values_list_envelope_duplicates(&mut self.uses);
        merge_pathless_resource_variants(&mut self.uses);
        sort_and_dedup_contract_uses(&mut self.uses);
    }
}

fn sort_and_dedup_contract_uses(uses: &mut Vec<ContractUse>) {
    uses.sort_by(|a, b| {
        a.source_expr
            .cmp(&b.source_expr)
            .then_with(|| a.path.0.cmp(&b.path.0))
            .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
            .then_with(|| a.resource.cmp(&b.resource))
            .then_with(|| a.guards.cmp(&b.guards))
    });
    uses.dedup();
}

fn merge_pathless_resource_variants(uses: &mut Vec<ContractUse>) {
    let mut merged = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<(String, ValueKind, Vec<Guard>), usize> =
        BTreeMap::new();

    for contract_use in std::mem::take(uses) {
        if contract_use.path.0.is_empty() {
            let key = (
                contract_use.source_expr.clone(),
                contract_use.kind,
                contract_use.guards.clone(),
            );
            if let Some(index) = pathless_index_by_identity.get(&key).copied() {
                let existing_resource = merged
                    .get(index)
                    .and_then(|existing: &ContractUse| existing.resource.clone());
                match (existing_resource, &contract_use.resource) {
                    (None, Some(_)) => {
                        if let Some(existing) = merged.get_mut(index) {
                            existing.resource = contract_use.resource;
                        }
                        continue;
                    }
                    (None, None) => continue,
                    (Some(existing), Some(resource)) if existing == *resource => continue,
                    (Some(_), None) => continue,
                    (Some(_), Some(_)) => {}
                }
            }
            pathless_index_by_identity.insert(key, merged.len());
        }
        merged.push(contract_use);
    }

    *uses = merged;
}

fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    let defaulted_render_sites: BTreeSet<_> = uses
        .iter()
        .filter(|contract_use| {
            contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr),
            )
        })
        .map(|contract_use| {
            (
                contract_use.source_expr.clone(),
                contract_use.path.clone(),
                contract_use.kind,
                contract_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|contract_use| {
        if contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr),
        ) {
            return true;
        }
        !defaulted_render_sites.contains(&(
            contract_use.source_expr.clone(),
            contract_use.path.clone(),
            contract_use.kind,
            contract_use.resource.clone(),
        ))
    });
}

fn drop_values_list_envelope_duplicates(uses: &mut Vec<ContractUse>) {
    let render_sites: BTreeSet<_> = uses
        .iter()
        .map(|contract_use| {
            (
                contract_use.source_expr.clone(),
                contract_use.path.clone(),
                contract_use.kind,
                contract_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|contract_use| {
        let Some(index) = contract_use
            .path
            .0
            .iter()
            .position(|segment| segment == "values[*]")
        else {
            return true;
        };
        let mut collapsed_path = contract_use.path.clone();
        collapsed_path.0.remove(index);
        !render_sites.contains(&(
            contract_use.source_expr.clone(),
            collapsed_path,
            contract_use.kind,
            contract_use.resource.clone(),
        ))
    });
}

fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}

fn derive_chart_facts_from_uses(uses: &[ContractUse]) -> ChartFacts {
    #[derive(Default)]
    struct Acc {
        has_render_use: bool,
        all_render_uses_self_guarded: bool,
        has_fragment_render: bool,
        has_self_range_guard_render_use: bool,
    }

    fn use_is_self_guarded(use_: &ContractUse) -> bool {
        if use_.path.0.is_empty() {
            return true;
        }

        use_.guards.iter().any(|guard| match guard {
            Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path } => path == &use_.source_expr,
            Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => false,
        })
    }

    let mut by_path: BTreeMap<String, Acc> = BTreeMap::new();
    let mut descendant_paths: BTreeSet<String> = BTreeSet::new();

    for use_ in uses {
        if use_.source_expr.trim().is_empty() {
            for guard in &use_.guards {
                for path in guard.value_paths() {
                    if path.trim().is_empty() {
                        continue;
                    }
                    let acc = by_path.entry(path.to_string()).or_insert_with(|| Acc {
                        all_render_uses_self_guarded: true,
                        ..Acc::default()
                    });
                    if !use_.path.0.is_empty() {
                        acc.has_render_use = true;
                        acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
                        acc.has_self_range_guard_render_use |= matches!(guard, Guard::Range { .. });
                    }
                }
            }
            continue;
        }

        let acc = by_path
            .entry(use_.source_expr.clone())
            .or_insert_with(|| Acc {
                all_render_uses_self_guarded: true,
                ..Acc::default()
            });

        if !use_.path.0.is_empty() {
            acc.has_render_use = true;
            acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
            acc.has_self_range_guard_render_use |= use_
                .guards
                .iter()
                .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr));
            acc.all_render_uses_self_guarded &= use_is_self_guarded(use_);
        }

        for guard in &use_.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() || path == use_.source_expr {
                    continue;
                }
                let acc = by_path.entry(path.to_string()).or_insert_with(|| Acc {
                    all_render_uses_self_guarded: true,
                    ..Acc::default()
                });
                if !use_.path.0.is_empty() {
                    acc.has_render_use = true;
                    acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
                    acc.has_self_range_guard_render_use |= matches!(guard, Guard::Range { .. });
                }
            }
        }

        let mut segments: Vec<&str> = use_
            .source_expr
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            descendant_paths.insert(segments.join("."));
        }
    }

    let path_facts = by_path
        .into_iter()
        .map(|(path, acc)| {
            (
                path.clone(),
                PathFact {
                    has_render_use: acc.has_render_use,
                    all_render_uses_self_guarded: acc.all_render_uses_self_guarded,
                    has_fragment_render: acc.has_fragment_render,
                    descendant_accessed: descendant_paths.contains(&path),
                    has_self_range_guard_render_use: acc.has_self_range_guard_render_use,
                },
            )
        })
        .collect();

    ChartFacts { path_facts }
}

fn derive_path_signals_from_uses(uses: &[ContractUse]) -> ContractPathSignals {
    let mut signals = ContractPathSignals::default();
    for contract_use in uses {
        if contract_use.source_expr.trim().is_empty() {
            continue;
        }

        signals
            .referenced_value_paths
            .insert(contract_use.source_expr.clone());
        if contract_use.kind == ValueKind::Fragment {
            signals
                .value_paths_used_as_fragment
                .insert(contract_use.source_expr.clone());
        }
        if contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty() {
            signals
                .partial_scalar_value_paths
                .insert(contract_use.source_expr.clone());
        }
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                signals.referenced_value_paths.insert(path.to_string());
                if matches!(guard, Guard::Range { .. }) {
                    signals.ranged_value_paths.insert(path.to_string());
                }

                if let Some(constraint) = guard_constraint_from_guard(guard) {
                    signals
                        .guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(constraint);
                }
            }
        }
    }

    signals
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

fn derive_nullable_value_paths_from_uses(uses: &[ContractUse]) -> BTreeSet<String> {
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

    let mut by_path: BTreeMap<&str, NullablePathAccumulator> = BTreeMap::new();
    for contract_use in uses {
        if contract_use.source_expr.trim().is_empty() {
            continue;
        }
        let info = by_path
            .entry(contract_use.source_expr.as_str())
            .or_insert_with(NullablePathAccumulator::new);
        let has_self_range_guard = contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
        );
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
                by_path
                    .entry(path.as_str())
                    .or_insert_with(NullablePathAccumulator::new)
                    .has_render_use = true;
            }
        }
    }
    by_path
        .into_iter()
        .filter_map(|(path, info)| {
            (info.has_render_use && info.all_uses_nullable).then(|| path.to_string())
        })
        .collect()
}

fn use_is_null_tolerant(use_: &ContractUse) -> bool {
    if use_.path.0.is_empty() {
        return true;
    }

    use_.guards.iter().any(|guard| match guard {
        Guard::Truthy { path }
        | Guard::Eq { path, .. }
        | Guard::Range { path }
        | Guard::With { path }
        | Guard::Default { path } => path == &use_.source_expr,
        Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_use_context_attaches_chart_default_only_to_rendered_paths() {
        let guards = Vec::new();
        let chart_value_defaults = BTreeSet::from(["serviceAccount.name".to_string()]);
        let context = ContractUseContext::new(&guards, &chart_value_defaults, false);

        let rendered = context.contract_use(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            &[],
            None,
        );
        assert_eq!(
            rendered.guards,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }]
        );

        let pathless = context.pathless_contract_use(
            "serviceAccount.name".to_string(),
            ValueKind::Scalar,
            &[],
        );
        assert!(pathless.guards.is_empty());
    }

    #[test]
    fn contract_use_context_lowers_pathless_partial_scalar_to_scalar() {
        let guards = Vec::new();
        let chart_value_defaults = BTreeSet::new();
        let context = ContractUseContext::new(&guards, &chart_value_defaults, false);

        let contract_use =
            context.pathless_contract_use("image.tag".to_string(), ValueKind::PartialScalar, &[]);

        assert_eq!(contract_use.kind, ValueKind::Scalar);
    }

    #[test]
    fn contract_ir_finalization_keeps_default_guarded_render_site_over_bare_duplicate() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }],
            None,
        ));

        let value_uses = contract.into_value_uses();

        assert_eq!(value_uses.len(), 1);
        assert_eq!(
            value_uses.first().map(|value_use| &value_use.guards),
            Some(&vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }])
        );
    }

    #[test]
    fn contract_ir_finalization_prefers_resource_claim_for_pathless_duplicate() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "nameOverride".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
        contract.push(ContractUse::new(
            "nameOverride".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            Some(ResourceRef {
                api_version: "v1".to_string(),
                kind: "Service".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }),
        ));

        let value_uses = contract.into_value_uses();

        assert_eq!(value_uses.len(), 1);
        assert_eq!(
            value_uses
                .first()
                .and_then(|value_use| value_use.resource.as_ref())
                .map(|resource| (resource.api_version.as_str(), resource.kind.as_str())),
            Some(("v1", "Service"))
        );
    }

    #[test]
    fn contract_ir_maps_value_paths_without_touching_rendered_yaml_path() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "serviceAccount.enabled".to_string(),
                },
                Guard::Or {
                    paths: vec!["pod.enabled".to_string(), "global.enabled".to_string()],
                },
            ],
            None,
        ));

        contract.map_value_paths(|path| {
            if path.starts_with("global.") {
                path.to_string()
            } else {
                format!("subchart.{path}")
            }
        });

        let value_uses = contract.into_value_uses();
        let value_use = value_uses.first().expect("mapped value use");

        assert_eq!(value_use.source_expr, "subchart.serviceAccount.name");
        assert_eq!(
            value_use.path,
            YamlPath(vec!["metadata".to_string(), "name".to_string()])
        );
        assert_eq!(
            value_use.guards,
            vec![
                Guard::Truthy {
                    path: "subchart.serviceAccount.enabled".to_string(),
                },
                Guard::Or {
                    paths: vec![
                        "subchart.pod.enabled".to_string(),
                        "global.enabled".to_string()
                    ],
                },
            ]
        );
    }

    #[test]
    fn contract_ir_pathless_scalar_seed_projects_without_rendered_path() {
        let mut contract = ContractIr::default();

        contract.push_pathless_scalar("extraConfig");

        let projection = contract.project();
        let value_uses = projection.uses();
        assert_eq!(value_uses.len(), 1);
        assert_eq!(value_uses[0].source_expr, "extraConfig");
        assert_eq!(value_uses[0].path, YamlPath(Vec::new()));
        assert_eq!(value_uses[0].kind, ValueKind::Scalar);
        assert!(value_uses[0].guards.is_empty());
        assert!(value_uses[0].resource.is_none());
    }

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

        let nullable_paths = projection.nullable_value_paths();

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

        let nullable_paths = projection.nullable_value_paths();

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
                String::new(),
                YamlPath(vec!["ignored".to_string()]),
                ValueKind::Scalar,
                vec![Guard::Eq {
                    path: "ignored.guard".to_string(),
                    value: "ignored".to_string(),
                }],
                None,
            ),
        ]);

        let signals = projection.path_signals();

        assert_eq!(
            signals.referenced_value_paths,
            BTreeSet::from([
                "extraConfig".to_string(),
                "extraEnv".to_string(),
                "image.tag".to_string(),
                "mode".to_string(),
                "podLabels".to_string(),
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
    }
}
