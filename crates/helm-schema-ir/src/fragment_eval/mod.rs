//! Stage-B fragment evaluation: the `Guarded<AbstractFragment>` domain and
//! its interpreter over the `helm-schema-syntax` templated-YAML CST.
//!
//! This module is built *beside* the current three-runtime pipeline (see
//! `plan/unified-frontend-redesign.md`, Stage B): the abstract rendered
//! document is evaluated once, guards stay tree-structured, and value uses
//! are projections over that one artifact. Nothing in the existing pipeline
//! consumes this yet; the differential harness in this crate's integration
//! tests compares its projections against the current `ContractIr` rows per
//! corpus fixture.

mod control;
mod domain;
mod dump;
mod eval;
mod files;
mod holes;
mod lower;
mod project;

pub use domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Opaque,
    PathCondition, Sequence, Splice, SpliceMeta, StringPart, and_conditions,
};
pub use dump::dump_document;
pub use eval::{EvaluatedDocument, ValueRead};
pub use project::{FragmentValueUse, document_value_uses};

pub(crate) use eval::eval_document;
