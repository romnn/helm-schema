use std::path::PathBuf;

use clap::Args;

use helm_schema::output::JsonOutputFormat;

/// Arguments of `helm-schema shorten`.
#[derive(Args, Debug, Clone)]
pub struct ShortenArgs {
    /// Schema with readable `$defs` names, as `helm-schema` writes it.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Where to write the schema with short `$defs` keys.
    #[arg(value_name = "OUTPUT")]
    pub output: PathBuf,

    /// Write the map from each short key to its readable name here.
    #[arg(long, value_name = "PATH")]
    pub map: Option<PathBuf>,

    /// Write compact JSON output instead of the default pretty JSON.
    #[arg(long)]
    pub compact: bool,
}

impl ShortenArgs {
    pub(crate) fn json_format(&self) -> JsonOutputFormat {
        JsonOutputFormat::from_compact(self.compact)
    }
}
