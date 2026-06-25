use std::collections::BTreeSet;

use crate::contract::ContractUse;
use crate::{ContractProvenance, Guard, ResourceRef, SourceSpan, ValueKind, YamlPath};

/// Context applied when semantic facts are lowered to contract claims.
///
/// The interpreter owns the facts. This type owns the common projection policy:
/// ambient guards, render-suppressed paths, partial-scalar normalization, and
/// chart-level default mutations.
pub(crate) struct ContractUseContext<'a> {
    guards: &'a [Guard],
    chart_value_defaults: &'a BTreeSet<String>,
    suppress_document_path: bool,
    resource: Option<ResourceRef>,
    site_provenance: Option<ContractProvenance>,
}

impl<'a> ContractUseContext<'a> {
    pub(crate) fn new(
        guards: &'a [Guard],
        chart_value_defaults: &'a BTreeSet<String>,
        suppress_document_path: bool,
        resource: Option<ResourceRef>,
        source_path: Option<&'a str>,
        source_span: Option<SourceSpan>,
        helper_chain: Vec<String>,
    ) -> Self {
        let site_provenance = source_path
            .zip(source_span)
            .map(|(source_path, source_span)| {
                ContractProvenance::new(source_path, source_span, helper_chain)
            });
        Self {
            guards,
            chart_value_defaults,
            suppress_document_path,
            resource,
            site_provenance,
        }
    }

    pub(crate) fn contract_use(
        &self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) -> ContractUse {
        self.contract_use_with_extra_provenance(source_expr, path, kind, extra_guards, &[])
    }

    pub(crate) fn contract_use_with_extra_provenance(
        &self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
        extra_provenance: &[ContractProvenance],
    ) -> ContractUse {
        self.contract_use_with_resource(
            source_expr,
            path,
            kind,
            extra_guards,
            self.resource.clone(),
            extra_provenance,
        )
    }

    fn contract_use_with_resource(
        &self,
        source_expr: String,
        mut path: YamlPath,
        mut kind: ValueKind,
        extra_guards: &[Guard],
        resource: Option<ResourceRef>,
        extra_provenance: &[ContractProvenance],
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

        ContractUse::with_provenances(
            source_expr,
            path,
            kind,
            guards,
            resource,
            self.provenance_sites(extra_provenance),
        )
    }

    pub(crate) fn pathless_contract_use(
        &self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) -> ContractUse {
        self.pathless_contract_use_with_extra_provenance(source_expr, kind, extra_guards, &[])
    }

    pub(crate) fn pathless_contract_use_with_extra_provenance(
        &self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
        extra_provenance: &[ContractProvenance],
    ) -> ContractUse {
        self.contract_use_with_resource(
            source_expr,
            YamlPath(Vec::new()),
            kind,
            extra_guards,
            None,
            extra_provenance,
        )
    }

    fn guards_with(&self, extra_guards: &[Guard]) -> Vec<Guard> {
        let mut guards = self.guards.to_vec();
        merge_guards(&mut guards, extra_guards);
        guards
    }

    fn provenance_sites(&self, extra_provenance: &[ContractProvenance]) -> Vec<ContractProvenance> {
        let mut provenance = Vec::new();
        if let Some(site) = &self.site_provenance {
            provenance.push(site.clone());
        }
        for extra in extra_provenance {
            if !provenance.contains(extra) {
                provenance.push(extra.clone());
            }
        }
        provenance
    }
}

fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}

#[cfg(test)]
#[path = "tests/contract_sink.rs"]
mod tests;
