//! Fragment evaluation: the `Guarded<AbstractFragment>` domain and its
//! interpreter over the `helm-schema-syntax` templated-YAML CST.
//!
//! This is the production frontend: the abstract rendered document is
//! evaluated once, guards stay tree-structured, and the
//! contract graph is a projection over that one artifact
//! ([`contract_ir_from_document`]). Helper bodies evaluate through the
//! same interpreter into memoized fragment summaries ([`summary`]); helper
//! calls splice those summaries at their call sites or consume their value
//! projection inside expressions.

mod assignments;
mod control;
mod domain;
mod dump;
mod eval;
mod files;
mod hole_effects;
mod holes;
mod inline_regions;
mod lower;
mod project;
pub(crate) mod summary;

pub use domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Opaque,
    PathCondition, Sequence, SiteFacts, Splice, SpliceMeta, StringPart, TaintPart, and_conditions,
};
pub use dump::dump_document;
pub use eval::{EvaluatedDocument, ValueRead};

pub(crate) use eval::{BodyEvalFacts, eval_document};
pub(crate) use project::contract_ir_from_document;
