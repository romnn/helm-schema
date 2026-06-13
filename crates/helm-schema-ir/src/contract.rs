use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Guard, ResourceRef, ValueKind, ValueUse, YamlPath};

/// Context applied when semantic facts are lowered to compatibility-era
/// contract uses.
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

/// Internal compatibility-era contract use.
///
/// The symbolic interpreter builds these as semantic claims. The public
/// `ValueUse` type is only produced after contract normalization, so later
/// phases can grow richer contract witnesses without teaching the interpreter
/// to construct fixture DTOs directly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ContractUse {
    pub(crate) source_expr: String,
    pub(crate) path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) guards: Vec<Guard>,
    pub(crate) resource: Option<ResourceRef>,
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

    fn into_value_use(self) -> ValueUse {
        ValueUse {
            source_expr: self.source_expr,
            path: self.path,
            kind: self.kind,
            guards: self.guards,
            resource: self.resource,
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

/// Normalized compatibility projection of a contract graph.
///
/// This is the remaining bridge to generator and fixture code that still
/// consumes [`ValueUse`]. Production callers should pass this artifact around
/// instead of owning raw `Vec<ValueUse>` collections directly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractProjection {
    uses: Vec<ValueUse>,
}

impl ContractProjection {
    /// Build a projection from already-projected compatibility DTOs.
    ///
    /// This is for tests and transitional consumers that still construct
    /// [`ValueUse`] rows directly. Interpreter code should produce
    /// [`ContractIr`] and call [`ContractIr::project`] instead.
    pub fn from_value_uses(mut uses: Vec<ValueUse>) -> Self {
        uses.sort();
        uses.dedup();
        Self { uses }
    }

    /// Borrow the normalized compatibility uses.
    pub fn uses(&self) -> &[ValueUse] {
        &self.uses
    }

    /// Consume the projection and return the compatibility DTOs.
    pub fn into_value_uses(self) -> Vec<ValueUse> {
        self.uses
    }
}

/// Opaque guarded contract graph for one template interpretation.
///
/// This is still compatibility-era because its private leaves are `ContractUse`
/// claims that project to [`ValueUse`], but accumulation, path rebasing, and
/// normalization now live behind one contract-layer artifact instead of a raw
/// vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
}

impl ContractIr {
    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.uses.push(contract_use);
    }

    pub(crate) fn extend<I>(&mut self, contract_uses: I)
    where
        I: IntoIterator<Item = ContractUse>,
    {
        self.uses.extend(contract_uses);
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

    /// Normalize claims and project them to a compatibility artifact.
    pub fn project(mut self) -> ContractProjection {
        self.normalize();
        ContractProjection {
            uses: self
                .uses
                .into_iter()
                .map(ContractUse::into_value_use)
                .collect(),
        }
    }

    /// Normalize claims and project them to the compatibility `ValueUse` DTO.
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
}
