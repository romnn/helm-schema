mod provider;
mod universe;

pub use provider::ChartLocalCrdSchemaProvider;
pub(crate) use universe::ResourceDocKey;
pub use universe::{
    LocalResourceSchema, LocalSchemaUniverse, resource_schemas_from_crd_document_with_source,
};
