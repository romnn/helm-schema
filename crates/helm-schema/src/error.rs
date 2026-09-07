use std::path::PathBuf;

/// Errors produced while loading charts, analyzing templates, and emitting schemas.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Virtual-filesystem operation failed.
    #[error("vfs error: {0}")]
    Vfs(#[from] vfs::VfsError),

    /// A chart source used as text is not valid UTF-8.
    #[error("chart source is not valid UTF-8: {path}")]
    NonUtf8ChartSource {
        /// Logical or virtual path of the invalid source.
        path: String,
    },

    /// Prepared chart files were requested for a chart outside the snapshot.
    #[error("loaded chart corpus has no entry for {path}")]
    LoadedChartMissing {
        /// Discovered chart directory missing from the corpus.
        path: String,
    },

    /// Operating-system I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML input could not be decoded.
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Values declarations use keys whose spelling Helm normalizes ambiguously.
    #[error(
        "unquoted YAML 1.1 Boolean-alias keys are not supported in chart declarations:\n{details}"
    )]
    YamlBooleanAliasKeys {
        /// Deterministically ordered file locations and source spellings.
        details: String,
    },

    /// A values declaration could not be structurally inspected for key style.
    #[error("failed to inspect YAML Boolean-alias keys in {path}: {message}")]
    YamlBooleanAliasScan {
        /// Values declaration being inspected.
        path: String,
        /// YAML parser failure.
        message: String,
    },

    /// Multiple dependency declarations target the same values root.
    #[error(
        "duplicate dependency values keys in {path}:\n{details}\neach dependency must own a unique .Values root"
    )]
    DuplicateDependencyValuesKeys {
        /// Manifest containing the duplicate declarations.
        path: String,
        /// Deterministically ordered values keys and their declarations.
        details: String,
    },

    /// Multiple vendored entries have the same internal chart name.
    #[error(
        "duplicate installed dependency names in {path}:\n{details}\nHelm's installed-entry association for duplicate internal names is nondeterministic"
    )]
    DuplicateInstalledDependencyNames {
        /// Vendored charts directory containing the duplicate entries.
        path: String,
        /// Deterministically ordered internal names and entry paths.
        details: String,
    },

    /// JSON input or output could not be decoded or encoded.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// A caller override is not a JSON Schema document root.
    #[error("override schema root in {path} must be an object or boolean, found {kind}")]
    InvalidOverrideRoot {
        /// Override file carrying the invalid root.
        path: PathBuf,
        /// JSON kind found at the document root.
        kind: &'static str,
    },

    /// A final output document is not a JSON Schema root.
    #[error("final schema root must be an object or boolean, found {kind}")]
    InvalidFinalSchemaRoot {
        /// JSON kind found at the document root.
        kind: &'static str,
    },

    /// Helm template source could not be parsed.
    #[error("template parse error: {0}")]
    TemplateParse(#[from] helm_schema_ast::ParseError),

    /// Chart discovery found no analyzable charts.
    #[error("no charts discovered")]
    NoChartsDiscovered,

    /// A chart manifest has no usable chart name.
    #[error("chart name missing for {path}")]
    ChartNameMissing {
        /// Path of the unnamed chart.
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

    /// An emission selection resolves to a contradictory knob matrix.
    #[error("invalid emission policy: {0}")]
    InvalidEmissionPolicy(#[from] helm_schema_gen::InvalidEmissionPolicy),

    /// An explicit config path could not be read.
    #[error("failed to read config {path}: {source}")]
    ConfigRead {
        /// Config file path.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },

    /// A config document is malformed or contains unknown fields.
    #[error("invalid config {path}: {source}")]
    InvalidConfig {
        /// Config file path or packaged-chart member label.
        path: String,
        /// YAML decoding failure.
        #[source]
        source: serde_yaml::Error,
    },

    /// A config document uses a version this binary cannot honor exactly.
    #[error(
        "unsupported config version {found} in {path}; supported range is {supported_min}..={supported_max}; update the config or the helm-schema binary"
    )]
    UnsupportedConfigVersion {
        /// Config file path.
        path: PathBuf,
        /// Version requested by the config.
        found: u64,
        /// Oldest config version honored by this binary.
        supported_min: u64,
        /// Newest config version honored by this binary.
        supported_max: u64,
    },
}

/// Result returned by the schema engine's public operations.
pub type EngineResult<T> = std::result::Result<T, CliError>;
