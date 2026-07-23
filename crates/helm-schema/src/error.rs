use std::path::PathBuf;

/// Errors produced while loading charts, analyzing templates, and emitting schemas.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Virtual-filesystem operation failed.
    #[error("vfs error: {0}")]
    Vfs(#[from] vfs::VfsError),

    /// Operating-system I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML input could not be decoded.
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON input or output could not be decoded or encoded.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Helm template source could not be parsed.
    #[error("template parse error: {0}")]
    TemplateParse(#[from] helm_schema_ast::ParseError),

    /// Chart discovery found no analyzable charts.
    #[error("no charts discovered")]
    NoChartsDiscovered,

    /// A discovered subchart path has no usable chart name.
    #[error("subchart name missing for {path}")]
    SubchartNameMissing {
        /// Path of the unnamed subchart.
        path: String,
    },

    /// An archive does not contain a chart manifest.
    #[error("no Chart.yaml found in archive {archive}")]
    NoChartYamlInArchive {
        /// Path or identifier of the archive.
        archive: String,
    },

    /// The output directory could not be created.
    #[error("failed to create output directory {path}")]
    CreateOutputDir {
        /// Directory creation target.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },

    /// A generated schema could not be written.
    #[error("failed to write output {path}")]
    WriteOutput {
        /// Output file that could not be written.
        path: PathBuf,
        /// Underlying filesystem failure.
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

    /// A self-contained schema could not be produced from external references.
    #[error("$ref bundling failed: {0}")]
    RefBundling(String),

    /// A local filesystem path cannot be represented as a file URI.
    #[error("filesystem path cannot be represented as a file URI: {path}")]
    InvalidFileUriPath {
        /// Filesystem path that could not be encoded.
        path: PathBuf,
    },

    /// A file URI cannot be represented as a local filesystem path.
    #[error("file URI cannot be represented as a local filesystem path: {uri}")]
    InvalidFileUri {
        /// File URI that could not be decoded.
        uri: String,
    },

    /// One loaded document exceeded the configured byte budget.
    #[error("load budget exceeded for {subject} (limit {limit_bytes} bytes)")]
    LoadBudgetExceeded {
        /// Document or archive member being loaded.
        subject: String,
        /// Maximum permitted byte count.
        limit_bytes: usize,
    },

    /// An archive or directory exceeded the configured entry budget.
    #[error("load budget exceeded for {subject} (limit {limit_entries} entries)")]
    LoadEntryBudgetExceeded {
        /// Archive or directory being enumerated.
        subject: String,
        /// Maximum permitted entry count.
        limit_entries: usize,
    },

    /// An archive member would escape its extraction root.
    #[error("unsafe archive entry path {entry_path} in {archive}")]
    UnsafeArchiveEntryPath {
        /// Archive containing the unsafe member.
        archive: String,
        /// Untrusted member path rejected by validation.
        entry_path: String,
    },

    /// Mutually-exclusive CLI flags or otherwise-invalid combination
    /// detected after `clap` parsing succeeded.
    #[error("invalid CLI options: {0}")]
    CliValidation(String),
}

/// Result returned by the schema engine's public operations.
pub type EngineResult<T> = std::result::Result<T, CliError>;
