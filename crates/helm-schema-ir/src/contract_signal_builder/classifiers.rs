use crate::Guard;
use crate::contract::ContractUse;
use crate::contract_signals::{GuardConstraint, MetadataFieldKind};

pub(super) fn use_is_positive_header(use_: &ContractUse) -> bool {
    !use_.guards.is_empty()
        && use_.guards.iter().all(|guard| match guard {
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

pub(super) fn metadata_field_kind_from_yaml_path(path: &[String]) -> Option<MetadataFieldKind> {
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

pub(super) fn guard_constraint_from_guard(guard: &Guard) -> Option<GuardConstraint> {
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

pub(super) fn use_is_self_guarded(use_: &ContractUse) -> bool {
    if use_.path.0.is_empty() {
        return true;
    }

    use_has_matching_self_guard(use_)
}

pub(super) fn use_is_null_tolerant(use_: &ContractUse) -> bool {
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
