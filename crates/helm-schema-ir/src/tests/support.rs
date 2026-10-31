use crate::analysis_db::IrAnalysisDb;
use crate::fragment_eval::project_execution_at_offset;

#[derive(Debug)]
pub(crate) struct EvaluatedDocument {
    pub(crate) contract: crate::contract::ContractIr,
    pub(crate) inspection: super::fragment_inspection::EvaluatedDocument,
}
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
    EvaluatedDocument {
        contract: project_execution_at_offset(source, source_path, 0, db, execution),
        inspection: super::fragment_inspection::project_execution_at_offset(
            source,
            source_path,
            0,
            db,
            execution,
        ),
    }
}

impl std::ops::Deref for EvaluatedDocument {
    type Target = super::fragment_inspection::EvaluatedDocument;
    fn deref(&self) -> &Self::Target {
        &self.inspection
    }
}
