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

pub mod acceptance;
pub mod admission;
pub mod cell_matrix;
pub mod consume;
pub mod generate;
pub mod helm;
pub mod machine;
pub mod manifest;
pub mod registry;
pub mod source_digest;
pub mod value_algebra_diff;

pub use consume::consume;
pub use registry::{ArtifactId, ChartId, IrId, LeanId, PolicyId, TemplateId};

#[cfg(test)]
mod tests;

/// Enables process-wide Perfetto tracing when `HELM_SCHEMA_TRACE_OUTPUT` names a file.
///
/// Call once at the harness entry point, before starting worker threads.
/// With the variable unset, the existing subscriber is left untouched.
/// The parent directory must already exist.
/// `HELM_SCHEMA_TRACE_FILTER` optionally supplies tracing target and level directives.
///
/// # Errors
///
/// Returns an error if the file cannot be created, the filter is invalid or not Unicode,
/// or a global subscriber is already set.
pub fn init_perfetto_from_env() -> eyre::Result<()> {
    let Some(path) = std::env::var_os("HELM_SCHEMA_TRACE_OUTPUT") else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(path);
    let file = std::fs::File::create(&path)
        .wrap_err_with(|| format!("create trace {}", path.display()))?;
    let dispatcher = match std::env::var("HELM_SCHEMA_TRACE_FILTER") {
        Ok(filter) => {
            let targets = filter.parse().wrap_err("parse Perfetto target filter")?;
            helm_schema_dev::trace::perfetto_dispatch_with_targets(file, targets)
        }
        Err(std::env::VarError::NotPresent) => helm_schema_dev::trace::perfetto_dispatch(file),
        Err(err @ std::env::VarError::NotUnicode(_)) => {
            return Err(err).wrap_err("read Perfetto target filter");
        }
    };
    tracing::dispatcher::set_global_default(dispatcher).wrap_err("install Perfetto subscriber")
}
