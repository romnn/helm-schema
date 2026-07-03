//! Manifest resource spans: byte ranges of one template source that belong
//! to a detected Kubernetes resource, with the path prefix List-envelope
//! items strip from emitted paths.

use helm_schema_core::ResourceRef;

#[derive(Clone, Debug)]
pub struct ResourceSpan {
    pub start: usize,
    pub end: usize,
    pub resource: ResourceRef,
    pub path_prefix: Vec<String>,
}
