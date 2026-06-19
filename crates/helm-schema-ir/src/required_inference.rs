//! Static-analysis primitives consumed solely by the heuristic
//! `--infer-required` feature in `helm-schema-gen` /
//! `helm-schema-cli`.
//!
//! Lives in its own module so the entire required-inference feature can
//! be removed cleanly. Nothing in `helm_schema_ir`'s core schema-lowering
//! artifact depends on anything here.

use std::collections::BTreeSet;

use crate::contract::ContractUse;
use crate::{Guard, ValueKind};

/// Contract-derived compatibility facts for the optional `required` schema
/// post-pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequiredInferenceSignals {
    pub positive_header_paths: BTreeSet<String>,
    pub conditionally_optional_paths: BTreeSet<String>,
    pub default_fallback_paths: BTreeSet<String>,
}

pub(crate) fn derive_required_inference_signals(uses: &[ContractUse]) -> RequiredInferenceSignals {
    let mut signals = RequiredInferenceSignals::default();

    for contract_use in uses {
        for guard in &contract_use.guards {
            match guard {
                Guard::Not { path } => {
                    signals.conditionally_optional_paths.insert(path.clone());
                }
                Guard::Absent { path } => {
                    signals.conditionally_optional_paths.insert(path.clone());
                }
                Guard::NotEq { path, .. } => {
                    signals.conditionally_optional_paths.insert(path.clone());
                }
                Guard::Or { paths } => {
                    signals
                        .conditionally_optional_paths
                        .extend(paths.iter().cloned());
                }
                Guard::AnyOf { alternatives } => {
                    for alternative in alternatives {
                        for guard in alternative {
                            signals
                                .conditionally_optional_paths
                                .extend(guard.value_paths().into_iter().map(str::to_string));
                        }
                    }
                }
                Guard::Default { path } => {
                    signals.default_fallback_paths.insert(path.clone());
                }
                Guard::Truthy { .. }
                | Guard::Eq { .. }
                | Guard::Range { .. }
                | Guard::With { .. }
                | Guard::TypeIs { .. } => {}
            }
        }

        if contract_use.kind == ValueKind::Scalar
            && contract_use.path.0.is_empty()
            && !contract_use.source_expr.trim().is_empty()
            && use_is_positive_header(contract_use)
        {
            signals
                .positive_header_paths
                .insert(contract_use.source_expr.clone());
        }
    }

    signals
}

fn use_is_positive_header(use_: &ContractUse) -> bool {
    !use_.guards.is_empty()
        && use_.guards.iter().all(|guard| match guard {
            Guard::Truthy { path } | Guard::Eq { path, .. } | Guard::TypeIs { path, .. } => {
                path == &use_.source_expr
            }
            Guard::Not { .. }
            | Guard::NotEq { .. }
            | Guard::Absent { .. }
            | Guard::Or { .. }
            | Guard::AnyOf { .. }
            | Guard::Range { .. }
            | Guard::With { .. }
            | Guard::Default { .. } => false,
        })
}
