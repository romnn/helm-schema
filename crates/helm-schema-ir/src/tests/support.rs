use crate::analysis_db::IrAnalysisDb;
use crate::fragment_eval::{EvaluatedDocument, project_execution_at_offset};
use crate::value::DocumentExecution;

#[tracing::instrument(skip_all)]
pub(crate) fn eval_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> EvaluatedDocument {
    let execution = crate::value::tests::execute_document(source, source_path, db);
    project_execution(source, source_path, db, &execution)
}

pub(crate) fn project_execution(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
    execution: &DocumentExecution,
) -> EvaluatedDocument {
    project_execution_at_offset(source, source_path, 0, db, execution)
}
