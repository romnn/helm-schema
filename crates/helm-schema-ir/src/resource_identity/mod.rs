//! Resource identity projection for rendered manifest documents.
//!
//! This module owns the path from rendered YAML source bytes to Kubernetes
//! resource identity facts. Keeping the detector, source-span locator, and
//! apiVersion helper-output evaluator behind one boundary makes the future
//! abstract-document identity projection a local replacement.

mod api_version;
mod detector;
mod helper_output;
mod list_envelope;
mod locator;
mod manifest_resource;
mod source_documents;
mod span_collection;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use detector::ResourceIdentityDetector;
pub(crate) use locator::ResourceIdentityIndex;
