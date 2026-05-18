use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum DiagFormat {
    #[default]
    Text,
    Json,
}

#[derive(Args, Debug, Clone)]
pub struct DiagArgs {
    #[arg(long = "diag-format", value_enum, default_value_t = DiagFormat::Text)]
    pub diag_format: DiagFormat,
}
