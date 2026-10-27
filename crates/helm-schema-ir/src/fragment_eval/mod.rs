//! YAML layout and contract projection over frozen value-runtime observations.

mod domain;
#[cfg(test)]
mod dump;
mod eval;
mod lower;
mod project;

#[cfg(test)]
pub(crate) use dump::dump_document;
pub(crate) use eval::EvaluatedDocument;
pub(crate) use eval::ValueRead;
#[cfg(test)]
pub(crate) use eval::eval_document;
#[cfg(test)]
pub(crate) use eval::project_execution;
pub(crate) use eval::project_execution_at_offset;
pub(crate) use project::contract_ir_from_document;
