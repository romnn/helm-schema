//! Diagnostic tree rendering for the historical placement assertions.

mod domain;
mod dump;
mod eval;
mod lower;

pub(crate) use dump::dump_document;
pub(crate) use eval::EvaluatedDocument;
pub(crate) use eval::project_execution_at_offset;
