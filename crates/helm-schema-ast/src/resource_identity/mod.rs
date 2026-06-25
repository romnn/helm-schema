mod detector;
mod list_envelope;
mod manifest_resource;
mod source_documents;
mod span_collection;
mod state;

pub use detector::ResourceIdentityDetector;
pub(crate) use span_collection::{ResourceSpan, collect_resource_spans};
