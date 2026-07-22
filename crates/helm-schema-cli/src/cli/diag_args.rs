use clap::{Args, ValueEnum};

/// Serialization format for runtime diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum DiagFormat {
    /// Human-readable diagnostic text.
    #[default]
    Text,
    /// One structured JSON object per diagnostic.
    Json,
}

/// Runtime diagnostic output options.
#[derive(Args, Debug, Clone)]
pub struct DiagArgs {
    /// Format used for emitted diagnostics.
    #[arg(long = "diag-format", value_enum, default_value_t = DiagFormat::Text)]
    pub diag_format: DiagFormat,
}
