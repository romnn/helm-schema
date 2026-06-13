use std::collections::BTreeSet;

use crate::contract::ContractUse;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

#[derive(Clone)]
pub(crate) struct AbstractDocumentProjectionContext {
    guards: Vec<Guard>,
    chart_value_defaults: BTreeSet<String>,
    suppress_document_path: bool,
}

impl AbstractDocumentProjectionContext {
    pub(crate) fn new(
        guards: Vec<Guard>,
        chart_value_defaults: BTreeSet<String>,
        suppress_document_path: bool,
    ) -> Self {
        Self {
            guards,
            chart_value_defaults,
            suppress_document_path,
        }
    }
}

pub(crate) enum AbstractDocumentProjection {
    DocumentUse(AbstractDocumentUse),
    HelperUse {
        source_expr: String,
        kind: ValueKind,
        guards: Vec<Guard>,
    },
}

impl AbstractDocumentProjection {
    pub(crate) fn document_use(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self::DocumentUse(AbstractDocumentUse {
            source_expr,
            path,
            kind,
            guards,
            resource,
        })
    }

    pub(crate) fn helper_use(source_expr: String, kind: ValueKind, guards: Vec<Guard>) -> Self {
        Self::HelperUse {
            source_expr,
            kind,
            guards,
        }
    }

    pub(crate) fn with_context(mut self, context: &AbstractDocumentProjectionContext) -> Self {
        match &mut self {
            Self::DocumentUse(use_) => use_.apply_context(context),
            Self::HelperUse { guards, .. } => {
                *guards = guards_with_context(&context.guards, guards);
            }
        }
        self
    }

    pub(crate) fn into_contract_use(self) -> ContractUse {
        match self {
            Self::DocumentUse(use_) => use_.into_contract_use(),
            Self::HelperUse {
                source_expr,
                kind,
                guards,
            } => ContractUse::new(
                source_expr,
                YamlPath(Vec::new()),
                if kind == ValueKind::PartialScalar {
                    ValueKind::Scalar
                } else {
                    kind
                },
                guards,
                None,
            ),
        }
    }
}

pub(crate) struct AbstractDocumentUse {
    source_expr: String,
    path: YamlPath,
    kind: ValueKind,
    guards: Vec<Guard>,
    resource: Option<ResourceRef>,
}

impl AbstractDocumentUse {
    fn apply_context(&mut self, context: &AbstractDocumentProjectionContext) {
        if context.suppress_document_path {
            self.path = YamlPath(Vec::new());
        }
        if self.kind == ValueKind::PartialScalar && self.path.0.is_empty() {
            self.kind = ValueKind::Scalar;
        }
        self.guards = guards_with_context(&context.guards, &self.guards);
        if !self.path.0.is_empty() && context.chart_value_defaults.contains(&self.source_expr) {
            let default_guard = Guard::Default {
                path: self.source_expr.clone(),
            };
            if !self.guards.contains(&default_guard) {
                self.guards.push(default_guard);
            }
        }
    }

    fn into_contract_use(self) -> ContractUse {
        ContractUse::new(
            self.source_expr,
            self.path,
            self.kind,
            self.guards,
            self.resource,
        )
    }
}

fn guards_with_context(context_guards: &[Guard], extra_guards: &[Guard]) -> Vec<Guard> {
    let mut guards = context_guards.to_vec();
    merge_guards(&mut guards, extra_guards);
    guards
}

fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}
