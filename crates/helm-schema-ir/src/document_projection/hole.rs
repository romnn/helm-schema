use crate::contract::ContractUse;
use crate::contract_sink::ContractUseContext;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

use super::hole_context::DocumentHoleContext;

pub(super) struct DocumentHole {
    path: YamlPath,
    kind: ValueKind,
    in_mapping_key: bool,
    entire_scalar_value: bool,
    helper_inlined: bool,
    resource: Option<ResourceRef>,
}

impl DocumentHole {
    pub(super) fn new(hole_context: DocumentHoleContext, helper_inlined: bool) -> Self {
        Self {
            path: hole_context.path,
            kind: hole_context.kind,
            in_mapping_key: hole_context.in_mapping_key,
            entire_scalar_value: hole_context.entire_scalar_value,
            helper_inlined,
            resource: hole_context.resource,
        }
    }

    pub(super) fn path(&self) -> &YamlPath {
        &self.path
    }

    pub(super) fn kind(&self) -> ValueKind {
        self.kind
    }

    pub(super) fn contract_use(
        &self,
        context: &ContractUseContext<'_>,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
    ) -> ContractUse {
        context.contract_use(source_expr, path, kind, &guards, self.resource.clone())
    }

    pub(super) fn direct_value_kind(&self) -> ValueKind {
        if self.kind == ValueKind::Scalar && !self.entire_scalar_value && !self.path.0.is_empty() {
            ValueKind::PartialScalar
        } else {
            self.kind
        }
    }

    pub(super) fn direct_value_path(&self, source_expr: &str) -> YamlPath {
        if source_expr.ends_with(".*") && !self.in_sequence_item() {
            YamlPath(Vec::new())
        } else {
            self.path.clone()
        }
    }

    fn in_sequence_item(&self) -> bool {
        self.path
            .0
            .last()
            .map(std::string::String::as_str)
            .is_some_and(|segment| segment.ends_with("[*]"))
    }

    pub(super) fn can_project_scalar_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && self.entire_scalar_value
    }

    pub(super) fn can_project_fragment_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && self.kind == ValueKind::Fragment
    }

    pub(super) fn can_project_structured_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && (self.kind == ValueKind::Fragment
                || (self.kind == ValueKind::Scalar && self.entire_scalar_value))
    }
}
