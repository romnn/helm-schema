use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("vfs error: {0}")]
    Vfs(#[from] vfs::VfsError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("template parse error: {0}")]
    TemplateParse(#[from] helm_schema_engine::parse::ParseError),

    #[error("no charts discovered")]
    NoChartsDiscovered,

    #[error("subchart name missing for {path}")]
    SubchartNameMissing { path: String },

    #[error("no Chart.yaml found in archive {archive}")]
    NoChartYamlInArchive { archive: String },

    #[error("failed to create output directory {path}")]
    CreateOutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write output {path}")]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Wraps any failure surfaced by the `jsonschema` / `referencing`
    /// full-inlining pass: file-not-found, JSON parse error, malformed
    /// URI, pointer-to-nowhere, missing anchor, etc. The wrapped variant
    /// carries the structured cause so callers can pattern-match on the
    /// underlying problem (e.g. `Unretrievable { uri, source }` vs
    /// `PointerToNowhere { pointer }`) rather than parsing a string.
    #[error("$ref resolution failed: {0}")]
    Referencing(#[from] jsonschema::ReferencingError),

    #[error("$ref bundling failed: {0}")]
    RefBundling(String),

    /// Mutually-exclusive CLI flags or otherwise-invalid combination
    /// detected after `clap` parsing succeeded.
    #[error("invalid CLI options: {0}")]
    CliValidation(String),
}

pub type CliResult<T> = std::result::Result<T, CliError>;
