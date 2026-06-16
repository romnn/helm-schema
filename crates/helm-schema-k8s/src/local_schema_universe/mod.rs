mod provider;
mod universe;

pub use provider::{ChartLocalCrdSchemaProvider, debug_materialize_schema_for_resource};
pub use universe::{
    LocalResourceSchema, LocalSchemaUniverse, resource_schemas_from_crd_document,
    resource_schemas_from_crd_document_with_source,
};
