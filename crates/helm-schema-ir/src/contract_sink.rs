use std::collections::BTreeSet;

use crate::contract::{ContractIr, ContractUse};
use crate::{ContractProvenance, Guard, ResourceRef, SourceSpan, ValueKind, YamlPath};

/// One output row's lowered contract claims: every emission site in the
/// symbolic walker reduces its row class to this shape, and
/// [`ContractUseContext::emit`] is the single terminal that fans it out to
/// one `ContractUse` per guard set.
///
/// `emit_path: Some(path)` claims a rendered document path and keeps the
/// site's resource scope even when the path is empty; `None` is a pathless
/// claim that drops the resource.
pub(crate) struct EmissionWitness {
    pub(crate) source_expr: String,
    pub(crate) emit_path: Option<YamlPath>,
    pub(crate) kind: ValueKind,
    pub(crate) guard_sets: Vec<Vec<Guard>>,
    pub(crate) provenance: Vec<ContractProvenance>,
    pub(crate) dependency: bool,
}

impl EmissionWitness {
    /// A non-dependency witness with no extra provenance; pass
    /// `vec![Vec::new()]` as `guard_sets` for a single claim without extra
    /// guards.
    pub(crate) fn new(
        source_expr: String,
        emit_path: Option<YamlPath>,
        kind: ValueKind,
        guard_sets: Vec<Vec<Guard>>,
    ) -> Self {
        Self {
            source_expr,
            emit_path,
            kind,
            guard_sets,
            provenance: Vec::new(),
            dependency: false,
        }
    }
}

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

    /// Terminal for all contract emission: pushes one claim per guard set,
    /// routed to the dependency lane when the witness says so. This is where
    /// the ambient projection policy applies: document-path suppression,
    /// pathless partial-scalar normalization, ambient guards, chart-level
    /// default admission, and the site's provenance.
    pub(crate) fn emit(&self, witness: EmissionWitness, contract: &mut ContractIr) {
        let (mut path, resource) = match witness.emit_path {
            Some(path) => (path, self.resource.clone()),
            None => (YamlPath(Vec::new()), None),
        };
        if self.suppress_document_path {
            path = YamlPath(Vec::new());
        }
        let mut kind = witness.kind;
        if kind == ValueKind::PartialScalar && path.0.is_empty() {
            kind = ValueKind::Scalar;
        }
        let provenance = self.provenance_sites(&witness.provenance);
        let chart_default_guard = (!path.0.is_empty()
            && self.chart_value_defaults.contains(&witness.source_expr))
        .then(|| Guard::Default {
            path: witness.source_expr.clone(),
        });
        for extra_guards in &witness.guard_sets {
            let mut guards = self.guards_with(extra_guards);
            if let Some(default_guard) = &chart_default_guard {
                merge_guards(&mut guards, std::slice::from_ref(default_guard));
            }
            let contract_use = ContractUse::with_provenances(
                witness.source_expr.clone(),
                path.clone(),
                kind,
                guards,
                resource.clone(),
                provenance.clone(),
            );
            if witness.dependency {
                contract.push_dependency_use(contract_use);
            } else {
                contract.push(contract_use);
            }
        }
    }

    pub(crate) fn has_ambient_guards(&self) -> bool {
        !self.guards.is_empty()
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
        crate::helper_summary::merge_provenance_sites(&mut provenance, extra_provenance);
        provenance
    }
}

/// Append each guard not already present, preserving existing order.
pub(crate) fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}

#[cfg(test)]
#[path = "tests/contract_sink.rs"]
mod tests;
