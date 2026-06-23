//! Resource identity projection for rendered manifest documents.
//!
//! This module owns the path from rendered YAML source bytes to Kubernetes
//! resource identity facts. The document-projection tracker consumes this
//! boundary directly, so resource identity stays isolated from the walker and
//! from contract lowering.

mod detector;
mod helper_output;
mod list_envelope;
mod locator;
mod manifest_resource;
mod source_documents;
mod span_collection;
mod state;

pub(crate) use detector::ResourceIdentityDetector;
pub(crate) use locator::ResourceIdentityIndex;
