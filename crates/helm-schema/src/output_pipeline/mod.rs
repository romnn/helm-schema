//! Final JSON Schema output pipeline.
//!
//! Everything in this module runs after chart analysis and schema inference.
//! These transforms are output policy only; they must not feed information
//! back into template interpretation.

mod annotation;
mod descriptions;
mod format;
mod options;
mod overrides;
mod reachability;
mod transforms;

pub(crate) use annotation::FinalOutputPolicy;
pub use format::{FinalOutputMetrics, write_schema_json};
pub use options::{
    EmitRequest, JsonOutputFormat, OutputPipelineOptions, PolicyInputOptions, ReferencePolicy,
};
pub(crate) use overrides::{PreparedEmitRequest, prepare_emit_request};
pub(crate) use transforms::apply_schema_output_pipeline;
