//! Fragment evaluation: the `Guarded<AbstractFragment>` domain and its
//! interpreter over the `helm-schema-syntax` templated-YAML CST.
//!
//! This is the production document frontend (see
//! `plan/unified-frontend-redesign.md`, Stage B): the abstract rendered
//! document is evaluated once, guards stay tree-structured, and the
//! contract graph is a projection over that one artifact
//! ([`contract_ir_from_document`]). Helper bodies still flow through the
//! memoized summary machinery; moving them in-domain is the next stage.

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
    PathCondition, Sequence, SiteFacts, Splice, SpliceMeta, StringPart, TaintPart, and_conditions,
};
pub use dump::dump_document;
pub use eval::{EvaluatedDocument, ValueRead};

pub(crate) use eval::eval_document;
pub(crate) use project::contract_ir_from_document;
