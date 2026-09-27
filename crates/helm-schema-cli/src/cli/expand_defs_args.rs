use std::path::PathBuf;

use clap::Args;

/// Arguments of `helm-schema expand-defs`.
#[derive(Args, Debug, Clone)]
pub struct ExpandDefsArgs {
    /// The short-to-readable name map written by `shorten --map` or
    /// `--defs-map`.
    #[arg(long, value_name = "PATH")]
    pub map: PathBuf,

    /// Text that mentions short `$defs` keys, such as a Helm log.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Where to write the text with readable names.
    #[arg(value_name = "OUTPUT")]
    pub output: PathBuf,
}
