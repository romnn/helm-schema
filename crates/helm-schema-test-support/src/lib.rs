//! Corpus artifacts for helm-schema's integration tests: one explicit producer,
//! many named consumers.
//!
//! [`registry`] lists every fixture-backed artifact with its complete recipe.
//! The `corpus_generation` binary runs [`generate::produce`] once over the whole
//! registry and writes a manifest. Tests call [`consume`], which reads a
//! verified artifact from that producer run or, without one, generates the
//! single artifact it needs from the current checkout and compares it with its
//! committed fixture.

pub mod consume;
pub mod generate;
pub mod machine;
pub mod manifest;
pub mod registry;
pub mod source_digest;

pub use consume::consume;
pub use registry::{ArtifactId, ChartId, IrId, LeanId, PolicyId, TemplateId};
