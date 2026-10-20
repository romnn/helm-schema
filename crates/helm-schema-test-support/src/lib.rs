//! Corpus artifacts for helm-schema's integration tests: one explicit producer,
//! many named consumers.
//!
//! [`registry`] lists every fixture-backed artifact with its complete recipe.
//! The `corpus_generation` binary runs [`generate::produce`] once over the whole
//! registry and writes a manifest. Tests call [`consume`], which reads a
//! verified artifact from that producer run or, without one, generates the
//! single artifact it needs from the current checkout and compares it with its
//! committed fixture.

use color_eyre::eyre::{self, WrapErr as _};

pub mod cell_matrix;
pub mod consume;
pub mod generate;
pub mod helm;
pub mod machine;
pub mod manifest;
pub mod registry;
pub mod source_digest;

pub use consume::consume;
pub use registry::{ArtifactId, ChartId, IrId, LeanId, PolicyId, TemplateId};

#[cfg(test)]
mod tests;

/// Enables process-wide Perfetto tracing when `HELM_SCHEMA_TRACE_OUTPUT` names a file.
///
/// Call once at the harness entry point, before starting worker threads.
/// With the variable unset, the existing subscriber is left untouched.
/// The parent directory must already exist.
///
/// # Errors
///
/// Returns an error if the file cannot be created or a global subscriber is already set.
pub fn init_perfetto_from_env() -> eyre::Result<()> {
    let Some(path) = std::env::var_os("HELM_SCHEMA_TRACE_OUTPUT") else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(path);
    let file = std::fs::File::create(&path)
        .wrap_err_with(|| format!("create trace {}", path.display()))?;
    tracing::dispatcher::set_global_default(helm_schema::trace::perfetto_dispatch(file))
        .wrap_err("install Perfetto subscriber")
}
