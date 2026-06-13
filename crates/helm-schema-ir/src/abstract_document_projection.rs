use crate::contract::{ContractUse, ContractUseContext};
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

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

    pub(crate) fn into_contract_use(self, context: &ContractUseContext<'_>) -> ContractUse {
        match self {
            Self::DocumentUse(use_) => use_.into_contract_use(context),
            Self::HelperUse {
                source_expr,
                kind,
                guards,
            } => context.pathless_contract_use(source_expr, kind, &guards),
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
    fn into_contract_use(self, context: &ContractUseContext<'_>) -> ContractUse {
        context.contract_use(
            self.source_expr,
            self.path,
            self.kind,
            &self.guards,
            self.resource,
        )
    }
}
