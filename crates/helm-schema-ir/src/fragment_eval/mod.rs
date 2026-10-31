//! Contract lowering from executed YAML ownership and frozen value relationships.

mod eval;

#[cfg(test)]
pub(crate) use crate::tests::fragment_inspection::dump_document;
pub(crate) use eval::{ValueRead, project_execution_at_offset};
