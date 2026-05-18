use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct ChartArgs {
    #[arg(long)]
    pub exclude_tests: bool,

    #[arg(long)]
    pub no_subchart_values: bool,

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
