//! YAML layout and contract projection over frozen value-runtime observations.

mod domain;
mod eval;
mod lower;
mod project;
#[cfg(test)]
#[path = "../tests/fragment_dump.rs"]
mod tests;

pub(crate) use eval::EvaluatedDocument;
pub(crate) use eval::ValueRead;
pub(crate) use eval::project_execution_at_offset;
pub(crate) use project::contract_ir_from_document;
#[cfg(test)]
pub(crate) use tests::dump_document;
