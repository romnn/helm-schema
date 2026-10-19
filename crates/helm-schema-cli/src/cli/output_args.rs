use std::path::PathBuf;

use clap::{Args, ValueEnum};

use helm_schema::output::{
    DefinitionNames, EmitRequest, JsonOutputFormat, OutputPipelineOptions, ReferencePolicy,
};

/// How readable output names the `$defs` entries helm-schema creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum DefsNames {
    /// The schema path the definition's content comes from, such as
    /// `k8s/io.k8s.api.core.v1.Probe` or `values/web.securityContext`.
    #[default]
    Source,
    /// The schema path of the definition's first reference.
    Destination,
}

impl From<DefsNames> for DefinitionNames {
    fn from(names: DefsNames) -> Self {
        match names {
            DefsNames::Source => Self::Source,
            DefsNames::Destination => Self::Destination,
        }
    }
}

/// JSON whitespace and definition names used for generated schema output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputLayout {
    /// Pretty JSON unless it exceeds Helm's file limit, then compact JSON.
    /// Shorten names when safe.
    #[default]
    Auto,
    /// Always pretty JSON with readable definition names.
    Pretty,
    /// Always compact JSON; shorten definition names when safe.
    Compact,
}

/// Destination, serialization, reference, and minimization options.
#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// Optional output file; standard output is used when absent.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output layout; auto keeps pretty JSON until Helm's 5 MiB chart-file limit.
    ///
    /// Compact output shortens names only when every reference is a resolvable local pointer
    /// and no anchors, dynamic references, or nested identifiers prevent a safe rename.
    /// External refs retained by --keep-refs also prevent renaming.
    /// Oversized output is always written with a warning, for use by other validators.
    #[arg(long, value_enum, default_value_t = OutputLayout::Auto, conflicts_with_all = ["compact", "shorten_defs"])]
    pub output_layout: OutputLayout,

    /// Deprecated: write compact JSON while keeping readable definition names.
    ///
    /// Use --output-layout compact for compact JSON and short definition names when safe.
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

    /// Keep repeated schema subtrees inline instead of interning them into
    /// root-level `$defs`.
    ///
    /// Interning is an output-only transform over the final JSON Schema (it
    /// does not participate in Helm template inference) and is on by default:
    /// branch-scoped arms repeat large guard fragments, so interning keeps
    /// big charts far below Helm's 5 MiB chart-file limit and speeds up every
    /// downstream validator.
    #[arg(long = "no-minimize", action = clap::ArgAction::SetFalse)]
    pub minimize: bool,

    /// How to name the `$defs` entries helm-schema creates.
    #[arg(long, value_enum, default_value_t = DefsNames::Source)]
    pub defs_names: DefsNames,

    /// Deprecated: shorten `$defs` names, keeping pretty JSON unless --compact is set.
    ///
    /// Use --output-layout compact for compact JSON and short definition names when safe.
    #[arg(long)]
    pub shorten_defs: bool,

    /// Write the short-to-readable `$defs` name map; empty when shortening abstains.
    #[arg(long, value_name = "PATH")]
    pub defs_map: Option<PathBuf>,
}

impl OutputArgs {
    pub(crate) fn emit_request(&self) -> EmitRequest {
        EmitRequest {
            reference_policy: ReferencePolicy::from_flags(self.keep_refs, self.inline_refs),
            output: OutputPipelineOptions {
                strip_descriptions: self.strip_descriptions,
                minimize: self.minimize,
                definition_names: self.defs_names.into(),
            },
        }
    }

    pub(crate) fn json_format(&self) -> JsonOutputFormat {
        JsonOutputFormat::from_compact(self.compact || self.output_layout == OutputLayout::Compact)
    }
}
