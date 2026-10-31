use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_ast::DefineIndex;

use crate::analysis_db::{IrAnalysisDb, ParsedDefines};
use crate::contract::ContractIr;

/// Initial non-values inputs supplied by the Helm render environment.
///
/// These inputs may decide control flow, but never become `.Values` schema
/// evidence themselves. Static root strings use path segments rather than a
/// dotted spelling so literal map keys containing dots remain unambiguous.
#[derive(Clone, Debug, Default)]
pub struct SymbolicPolicy {
    /// Chart-authored string defaults executed through `tpl`.
    pub chart_default_strings: BTreeMap<String, String>,
    /// Normalized Kubernetes version used by `.Capabilities.KubeVersion`.
    pub kubernetes_version: Option<String>,
    /// Exact initial scalar strings beneath roots such as `.Chart`.
    pub static_root_strings: BTreeMap<Vec<String>, String>,
}

/// One executable template in a shared chart render session.
#[derive(Clone, Copy, Debug)]
pub struct SymbolicTemplate<'a> {
    /// Template text whose expressions execute in the shared heap.
    pub source: &'a str,
    /// Helm's logical name, including the chart instance and `templates/` path.
    pub source_path: &'a str,
}

/// Reusable state for generating symbolic IR across many templates that
/// share one [`DefineIndex`].
///
/// The context shares parsed helper programs and immutable analysis policy.
/// A single-source call creates a fresh heap, while a batch call shares one
/// heap across its templates in Helm execution order.
#[derive(Clone)]
pub struct SymbolicIrContext {
    analysis_db: Rc<IrAnalysisDb>,
}

impl SymbolicIrContext {
    /// Creates a reusable symbolic-analysis context for indexed chart sources.
    #[tracing::instrument(skip_all)]
    pub fn new(defines: &DefineIndex) -> Self {
        Self {
            analysis_db: Rc::new(IrAnalysisDb::new(defines)),
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
        Self::with_policy(
            defines,
            SymbolicPolicy {
                chart_default_strings,
                ..SymbolicPolicy::default()
            },
        )
    }

    /// Builds a context carrying initial render-policy inputs.
    #[must_use]
    pub fn with_policy(defines: &DefineIndex, policy: SymbolicPolicy) -> Self {
        Self {
            analysis_db: Rc::new(IrAnalysisDb::with_policy(defines, policy)),
        }
    }

    /// Builds a context over define programs parsed once for the chart tree.
    #[must_use]
    pub fn with_parsed_policy(defines: &ParsedDefines, policy: SymbolicPolicy) -> Self {
        Self {
            analysis_db: Rc::new(IrAnalysisDb::with_parsed_policy(defines, policy)),
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

    /// Builds contract IR while attaching a logical template source path.
    #[must_use]
    pub fn generate_contract_ir_for_source(&self, src: &str, source_path: &str) -> ContractIr {
        self.generate_contract_ir_with_provenance(src, Some(source_path))
    }

    /// Executes chart templates in Helm order while preserving cross-file mutations.
    ///
    /// Results retain the input order.
    /// Every file gets fresh lexical bindings and a fresh `.Template` object.
    /// A render abort prevents the affected alternative from reaching later files.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn generate_contract_irs_for_sources(
        &self,
        sources: &[SymbolicTemplate<'_>],
    ) -> Vec<ContractIr> {
        let mut contracts: Vec<_> = sources.iter().map(|_| ContractIr::default()).collect();
        let prepared = sources
            .iter()
            .map(|source| {
                self.analysis_db.parsed_source(
                    source.source,
                    Some(source.source_path),
                    source.source_path,
                )
            })
            .collect();
        crate::value::project_documents(
            prepared,
            &self.analysis_db,
            |index, program, execution| {
                let Some(source) = sources.get(index) else {
                    return;
                };
                let (source, path, offset) =
                    program.map_or((source.source, Some(source.source_path), 0), |program| {
                        (
                            program.source(),
                            program.source_path(),
                            program.body_offset(),
                        )
                    });
                let document = crate::fragment_eval::project_execution_at_offset(
                    source,
                    path,
                    offset,
                    &self.analysis_db,
                    execution,
                );
                if let Some(contract) = contracts.get_mut(index) {
                    contract.append(document);
                }
            },
        );
        contracts
    }

    fn generate_contract_ir_with_provenance(
        &self,
        src: &str,
        source_path: Option<&str>,
    ) -> ContractIr {
        let parsed =
            self.analysis_db
                .parsed_source(src, source_path, source_path.unwrap_or("gotpl"));
        let mut contract = ContractIr::default();
        crate::value::project_documents(
            vec![parsed],
            &self.analysis_db,
            |_, program, execution| {
                let (source, path, offset) = program.map_or((src, source_path, 0), |program| {
                    (
                        program.source(),
                        program.source_path(),
                        program.body_offset(),
                    )
                });
                let document = crate::fragment_eval::project_execution_at_offset(
                    source,
                    path,
                    offset,
                    &self.analysis_db,
                    execution,
                );
                contract.append(document);
            },
        );
        contract
    }
}

#[cfg(test)]
#[path = "../tests/symbolic_context.rs"]
mod tests;
