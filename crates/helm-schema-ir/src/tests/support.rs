use test_util::prelude::sim_assert_eq;

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
    let mut document = EvaluatedDocument {
        contract: crate::contract::ContractIr::default(),
        inspection: super::fragment_inspection::EvaluatedDocument::default(),
    };
    with_execution(source, source_path, db, |execution| {
        document = project_execution(source, source_path, db, execution);
    });
    document
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

/// Inspect the same sealed root-program view used by production contract lowering.
pub(crate) fn with_execution(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
    mut inspect: impl FnMut(&DocumentExecution),
) {
    let source = db.parsed_source(source, source_path, source_path.unwrap_or("gotpl"));
    let mut visited = 0;
    crate::value::project_documents(vec![source], db, |_, _, execution| {
        visited += 1;
        inspect(execution);
    });
    sim_assert_eq!(have: visited, want: 1, "a single-source test must inspect exactly one program");
}

/// A strict consumer must retain both the selected source and its complete execution guard.
pub(crate) fn assert_string_consumption(
    execution: &DocumentExecution,
    path: &str,
    condition: helm_schema_core::Predicate,
) {
    let path = helm_schema_core::ValuesPath::parse(path);
    let condition = condition.normalize_boolean();
    assert!(
        execution
            .observed_facts()
            .consumptions
            .iter()
            .any(|consumption| {
                consumption.path == path
                    && consumption.test_condition() == condition
                    && matches!(&consumption.domain, crate::value::ConsumptionDomain::Known {
                preimage: helm_schema_core::FailValueRequirement::SchemaTypeEvenNull(schema_type),
                requires_presence: true,
            } if schema_type == "string")
            }),
        "missing selected string requirement for {path:?}: {:?}",
        execution.observed_facts().consumptions
    );
}

/// Inspect the full owned document alongside its production-exported requirements.
pub(crate) fn eval_complete_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> EvaluatedDocument {
    let mut document = EvaluatedDocument {
        contract: crate::contract::ContractIr::default(),
        inspection: super::fragment_inspection::EvaluatedDocument::default(),
    };
    with_execution(source, source_path, db, |execution| {
        document = EvaluatedDocument {
            contract: project_execution_at_offset(source, source_path, 0, db, execution),
            inspection: super::fragment_inspection::project_complete_execution_at_offset(
                source_path,
                0,
                db,
                execution,
            ),
        };
    });
    document
}
