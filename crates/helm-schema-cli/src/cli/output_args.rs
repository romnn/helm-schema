use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Write compact JSON output instead of the default pretty JSON.
    #[arg(long)]
    pub compact: bool,

    /// Leave file/URL `$ref` strings in the generated schema as-is.
    /// By default, the final output pass walks the merged schema and
    /// inlines every file `$ref` (and, unless `--offline`, every URL
    /// `$ref`), recursively, with cycle detection.
    #[arg(long)]
    pub keep_refs: bool,

    /// Deduplicate repeated schema subtrees into root-level `$defs`.
    ///
    /// This is an output-only transform over the final JSON Schema. It does not
    /// participate in Helm template inference.
    #[arg(long)]
    pub minimize: bool,
}
