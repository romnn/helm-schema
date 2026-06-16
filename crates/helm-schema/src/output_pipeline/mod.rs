//! Final JSON Schema output pipeline.
//!
//! Everything in this module runs after chart analysis and schema inference.
//! These transforms are output policy only; they must not feed information
//! back into template interpretation.

mod descriptions;
mod format;
mod global_mirror;
mod options;
mod overrides;
mod transforms;

pub use format::write_schema_json;
pub use options::{JsonOutputFormat, OutputPipelineOptions, PolicyInputOptions, ReferenceMode};
pub use overrides::{PolicyInputs, load_policy_inputs};
pub use transforms::apply_schema_output_pipeline;
