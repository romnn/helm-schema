use std::path::PathBuf;

use clap::Args;

use helm_schema::output::{
    FetchPolicy, JsonOutputFormat, LoadBudget, OutputPipelineOptions, PolicyInputOptions,
    ReferenceMode,
};

#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Write compact JSON output instead of the default pretty JSON.
    #[arg(long)]
    pub compact: bool,

    /// Remove JSON Schema `description` annotations from the generated output.
    ///
    /// This is schema-aware: properties named `description` remain intact.
    #[arg(long)]
    pub strip_descriptions: bool,

    /// Leave file/URL `$ref` strings in the generated schema as-is.
    ///
    /// By default, external refs are resolved into root-level `$defs` so the
    /// output is self-contained while still sharing referenced schemas.
    #[arg(long, conflicts_with = "inline_refs")]
    pub keep_refs: bool,

    /// Fully inline resolved file/URL `$ref`s instead of writing `$defs`.
    #[arg(long)]
    pub inline_refs: bool,

    /// Deduplicate repeated schema subtrees into root-level `$defs`.
    ///
    /// This is an output-only transform over the final JSON Schema. It does not
    /// participate in Helm template inference.
    #[arg(long)]
    pub minimize: bool,
}

impl OutputArgs {
    pub(crate) fn policy_input_options(&self, fetch_policy: FetchPolicy) -> PolicyInputOptions {
        PolicyInputOptions {
            reference_mode: ReferenceMode::from_flags(self.keep_refs, self.inline_refs),
            fetch_policy,
            load_budget: LoadBudget::default(),
        }
    }

    pub(crate) fn pipeline_options(&self) -> OutputPipelineOptions {
        OutputPipelineOptions {
            reference_mode: ReferenceMode::from_flags(self.keep_refs, self.inline_refs),
            strip_descriptions: self.strip_descriptions,
            minimize: self.minimize,
        }
    }

    pub(crate) fn json_format(&self) -> JsonOutputFormat {
        JsonOutputFormat::from_compact(self.compact)
    }
}
