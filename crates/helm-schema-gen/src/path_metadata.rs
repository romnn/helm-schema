use std::collections::BTreeSet;

use helm_schema_ir::ContractProjection;

use crate::resolve_policy::ResolvePolicy;

pub(crate) struct PathMetadata {
    pub(crate) nullable_paths: BTreeSet<String>,
    pub(crate) paths_with_descendants: BTreeSet<String>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_path_metadata(
    contract_projection: &ContractProjection,
    referenced_value_paths: &BTreeSet<String>,
) -> PathMetadata {
    let resolve_policy = ResolvePolicy::default();
    PathMetadata {
        nullable_paths: resolve_policy.nullable_value_paths(contract_projection),
        paths_with_descendants: collect_paths_with_descendants(referenced_value_paths),
    }
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
