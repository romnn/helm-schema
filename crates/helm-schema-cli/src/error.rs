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
    TemplateParse(#[from] helm_schema_ast::ParseError),

    #[error("no charts discovered")]
    NoChartsDiscovered,

    #[error("subchart name missing for {path}")]
    SubchartNameMissing { path: String },

    #[error("no Chart.yaml found in archive {archive}")]
    NoChartYamlInArchive { archive: String },

    #[error("failed to load CRD catalog from {dir}")]
    CrdCatalogLoad { dir: String },

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
}

pub type CliResult<T> = std::result::Result<T, CliError>;
