//! Fragment evaluation: the `Guarded<AbstractFragment>` domain and its
//! interpreter over the `helm-schema-syntax` templated-YAML CST.
//!
//! This is the production frontend: the abstract rendered document is
//! evaluated once, guards stay tree-structured, and the
//! contract graph is a projection over that one artifact
//! (`contract_ir_from_document`). Helper bodies evaluate through the
//! same interpreter into memoized fragment summaries; helper
//! calls splice those summaries at their call sites or consume their value
//! projection inside expressions.

mod assignments;
mod capture_scope;
mod control;
mod domain;
#[cfg(test)]
mod dump;
mod eval;
mod hole_effects;
mod holes;
mod inline_regions;
mod lower;
mod project;
pub(crate) mod summary;

#[cfg(test)]
pub(crate) use domain::{StringPart, TaintPart};
#[cfg(test)]
pub(crate) use dump::dump_document;
#[cfg(test)]
pub(crate) use eval::EvaluatedDocument;

pub(crate) use eval::{BodyEvalFacts, ValueRead, eval_document};
#[cfg(test)]
pub(crate) use lower::over_cap_scalar_taint;
pub(crate) use project::contract_ir_from_document;
