use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_ast::DefineIndex;

use crate::analysis_db::IrAnalysisDb;
use crate::contract::ContractIr;

/// Reusable state for generating symbolic IR across many templates that
/// share one [`DefineIndex`].
///
/// The context owns exact parse/helper-analysis caches. Reusing it across
/// templates avoids recomputing helper bodies without changing analysis
/// semantics; a cache miss and cache hit return the same structural facts.
#[derive(Clone)]
pub struct SymbolicIrContext {
    inner: Rc<SymbolicIrContextInner>,
}

struct SymbolicIrContextInner {
    analysis_db: IrAnalysisDb,
}

impl SymbolicIrContext {
    #[tracing::instrument(skip_all)]
    pub fn new(defines: &DefineIndex) -> Self {
        Self {
            inner: Rc::new(SymbolicIrContextInner {
                analysis_db: IrAnalysisDb::new(defines),
            }),
        }
    }

    /// Build a context that can execute chart-authored string defaults when
    /// a `tpl` call selects them. The strings are not general inference
    /// evidence; they are consulted only at that explicit execution boundary.
    #[must_use]
    pub fn with_chart_default_strings(
        defines: &DefineIndex,
        chart_default_strings: BTreeMap<String, String>,
    ) -> Self {
        Self::with_policy(defines, chart_default_strings, None)
    }

    /// Build a context carrying analysis policy inputs: the chart-authored
    /// string defaults plus the normalized Kubernetes version
    /// (`.Capabilities.KubeVersion` conditions evaluate against it; `None`
    /// abstains them).
    #[must_use]
    pub fn with_policy(
        defines: &DefineIndex,
        chart_default_strings: BTreeMap<String, String>,
        kubernetes_version: Option<String>,
    ) -> Self {
        Self {
            inner: Rc::new(SymbolicIrContextInner {
                analysis_db: IrAnalysisDb::with_policy(
                    defines,
                    chart_default_strings,
                    kubernetes_version,
                ),
            }),
        }
    }

    /// Generate the opaque contract graph without finalizing it.
    ///
    /// Callers that need to combine, scope, or otherwise transform chart-local
    /// contracts should use this method and derive schema facts with
    /// [`ContractIr::finalize`]. Inspection output can finalize the graph once
    /// and ask the resulting contract for its stable document.
    #[must_use]
    pub fn generate_contract_ir(&self, src: &str) -> ContractIr {
        self.generate_contract_ir_with_provenance(src, None)
    }

    #[must_use]
    pub fn generate_contract_ir_for_source(&self, src: &str, source_path: &str) -> ContractIr {
        self.generate_contract_ir_with_provenance(src, Some(source_path))
    }

    /// Evaluate a template into the abstract fragment domain, reusing this
    /// context's memoized helper analyses.
    #[must_use]
    pub fn eval_document_fragment(&self, src: &str) -> crate::fragment_eval::EvaluatedDocument {
        crate::fragment_eval::eval_document(src, None, &self.inner.analysis_db)
    }

    fn generate_contract_ir_with_provenance(
        &self,
        src: &str,
        source_path: Option<&str>,
    ) -> ContractIr {
        let document =
            crate::fragment_eval::eval_document(src, source_path, &self.inner.analysis_db);
        let mut contract = crate::fragment_eval::contract_ir_from_document(&document);
        for name in &document.values_root_helper_includes {
            contract.extend_values_program_wrappers(
                self.inner
                    .analysis_db
                    .program_wrapper_sentinels(name)
                    .into_iter()
                    .map(|(key, spread)| helm_schema_core::ValuesProgramWrapper {
                        scope_path: String::new(),
                        key,
                        spread,
                    }),
            );
        }
        contract.extend_values_program_wrapper_exclusions(
            document.pre_rewrite_strict_paths.iter().cloned(),
        );
        contract
    }
}
