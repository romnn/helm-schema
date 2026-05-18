use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub compact: bool,

    /// Leave file/URL `$ref` strings in the generated schema as-is.
    /// By default, the final output pass walks the merged schema and
    /// inlines every file `$ref` (and, unless `--offline`, every URL
    /// `$ref`), recursively, with cycle detection.
    #[arg(long)]
    pub keep_refs: bool,
}
