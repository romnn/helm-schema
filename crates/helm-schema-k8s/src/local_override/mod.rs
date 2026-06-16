mod provider;

pub(crate) use provider::bundled_local_definition_schema;
pub use provider::{
    LocalSchemaLeaf, LocalSchemaProvider, debug_materialize_schema_for_resource,
    descend_schema_path, descend_schema_path_expanding_leaf,
    descend_schema_path_expanding_leaf_with_root_metadata,
    descend_schema_path_expanding_leaf_with_root_metadata_source,
    descend_schema_path_expanding_leaf_with_source, expand_local_refs,
};
