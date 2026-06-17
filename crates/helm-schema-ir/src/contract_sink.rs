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
    source_path: Option<&'a str>,
    source_span: Option<SourceSpan>,
}

impl<'a> ContractUseContext<'a> {
    pub(crate) fn new(
        guards: &'a [Guard],
        chart_value_defaults: &'a BTreeSet<String>,
        suppress_document_path: bool,
        source_path: Option<&'a str>,
        source_span: Option<SourceSpan>,
    ) -> Self {
        Self {
            guards,
            chart_value_defaults,
            suppress_document_path,
            source_path,
            source_span,
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

        ContractUse::with_provenance(source_expr, path, kind, guards, resource, self.provenance())
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

    fn provenance(&self) -> Option<ContractProvenance> {
        Some(ContractProvenance::new(
            self.source_path?,
            self.source_span?,
        ))
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
        let context = ContractUseContext::new(&guards, &chart_value_defaults, false, None, None);

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
        let context = ContractUseContext::new(&guards, &chart_value_defaults, false, None, None);

        let contract_use =
            context.pathless_contract_use("image.tag".to_string(), ValueKind::PartialScalar, &[]);

        assert_eq!(contract_use.kind, ValueKind::Scalar);
    }
}
