use std::path::PathBuf;

use clap::{Args, ValueEnum};
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, RootPolicy};

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

    /// Omits the generated `additionalProperties: false` at the chart values
    /// root, so the schema no longer reports root keys that no template reads
    /// (a misspelled top-level key then passes validation).
    ///
    /// Only the root closure changes: nested structural and Kubernetes
    /// resource closures, member contracts, and requirements are kept.
    /// Dependency instance roots are open carriers under either policy.
    #[arg(long)]
    pub open_root: bool,

    /// Selects whether a chart-declared default asserts its type.
    ///
    /// `assert` rejects a value whose scalar type or container kind differs
    /// from the declared default where no other evidence governs the path.
    /// `annotate` keeps the declared default as documentation (property
    /// names, descriptions, `default`) and lets only template and resource
    /// evidence constrain the value.
    #[arg(long, value_enum, default_value_t = DeclaredTypesArg::Assert)]
    pub declared_types: DeclaredTypesArg,
}

/// Roles of a chart-declared default accepted by the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum DeclaredTypesArg {
    /// A declared default asserts its type.
    #[default]
    Assert,
    /// A declared default documents a value without asserting its type.
    Annotate,
}

impl ChartArgs {
    /// The authoring policy these arguments select.
    #[must_use]
    pub const fn authoring_policy(&self) -> AuthoringPolicy {
        AuthoringPolicy {
            root: if self.open_root {
                RootPolicy::Open
            } else {
                RootPolicy::Closed
            },
            declared_types: match self.declared_types {
                DeclaredTypesArg::Assert => DeclaredTypes::Assert,
                DeclaredTypesArg::Annotate => DeclaredTypes::Annotate,
            },
        }
    }
}

#[cfg(test)]
#[path = "../tests/chart_args.rs"]
mod tests;
