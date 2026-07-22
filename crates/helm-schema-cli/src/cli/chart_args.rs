use std::path::PathBuf;

use clap::Args;

/// Chart discovery, values composition, and requiredness options.
#[derive(Args, Debug, Clone)]
pub struct ChartArgs {
    /// Excludes chart test templates from analysis.
    #[arg(long)]
    pub exclude_tests: bool,

    /// Omits dependency values beneath their subchart keys.
    #[arg(long)]
    pub no_subchart_values: bool,

    /// Additional values files whose comments should be layered into
    /// schema descriptions. These files are documentation metadata only
    /// and do not contribute type hints or accepted value paths.
    #[arg(short = 'f', long = "values", value_name = "VALUES_FILE")]
    pub values_files: Vec<PathBuf>,

    /// Mark paths used in unconditional template guards
    /// (`if .Values.X`/`eq .Values.X "..."` with no enclosing guard) as
    /// `required` on their parent object. Paths reachable via any
    /// `default <expr> .Values.X` fallback are excluded — the fallback
    /// expression can be a literal (`default "x" .Values.X`), an
    /// identifier (`default .Chart.Name .Values.X`), or a parenthesized
    /// expression (`default (printf "%s" .Y) .Values.X`).
    #[arg(long)]
    pub infer_required: bool,
}
