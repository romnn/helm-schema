use std::collections::BTreeMap;

use crate::contract::ContractUse;
use crate::contract_signals::{
    ContractPathSignals, ContractSchemaSignals, RequiredInferenceSignals,
};
use crate::provider_schema_use::{ProviderSchemaUse, from_contract_use};
use crate::{Guard, ValueKind};

use super::classifiers::{
    guard_constraint_from_guard, metadata_field_kind_from_yaml_path, use_is_null_tolerant,
    use_is_positive_header, use_is_self_guarded,
};
use super::value_path_facts::{
    RenderPathFacts, build_contract_value_path_facts, collect_paths_with_descendants,
};

pub(crate) fn derive_schema_signals_from_uses(uses: &[ContractUse]) -> ContractSchemaSignals {
    let mut builder = ContractSchemaSignalBuilder::default();
    for contract_use in uses {
        builder.record(contract_use);
    }
    builder.finish()
}

#[derive(Default)]
struct ContractSchemaSignalBuilder {
    render_facts_by_path: BTreeMap<String, RenderPathFacts>,
    path_signals: ContractPathSignals,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    nullable_by_path: BTreeMap<String, NullablePathAccumulator>,
    required_inference_signals: RequiredInferenceSignals,
}

struct NullablePathAccumulator {
    has_render_use: bool,
    all_uses_nullable: bool,
}

impl Default for NullablePathAccumulator {
    fn default() -> Self {
        Self {
            has_render_use: false,
            all_uses_nullable: true,
        }
    }
}

impl ContractSchemaSignalBuilder {
    fn record(&mut self, contract_use: &ContractUse) {
        self.record_provider_schema_use(contract_use);
        self.record_render_facts(contract_use);
        self.record_path_signals(contract_use);
        self.record_nullable_path(contract_use);
        self.record_required_inference_signals(contract_use);
    }

    fn finish(self) -> ContractSchemaSignals {
        let paths_with_referenced_descendants =
            collect_paths_with_descendants(&self.path_signals.referenced_value_paths);
        let nullable_value_paths = self
            .nullable_by_path
            .into_iter()
            .filter_map(|(path, acc)| (acc.has_render_use && acc.all_uses_nullable).then_some(path))
            .collect();
        let value_path_facts = build_contract_value_path_facts(
            &self.render_facts_by_path,
            &self.path_signals,
            &nullable_value_paths,
            &paths_with_referenced_descendants,
        );

        ContractSchemaSignals {
            path_signals: self.path_signals,
            provider_schema_uses: self.provider_schema_uses,
            nullable_value_paths,
            paths_with_referenced_descendants,
            value_path_facts,
            required_inference_signals: self.required_inference_signals,
        }
    }

    fn record_provider_schema_use(&mut self, contract_use: &ContractUse) {
        if let Some(provider_use) = from_contract_use(contract_use) {
            self.provider_schema_uses.push(provider_use);
        }
    }

    fn record_render_facts(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            self.record_empty_source_render_facts(contract_use);
            return;
        }

        self.render_path_facts(&contract_use.source_expr);
        if !contract_use.path.0.is_empty() {
            let self_range_guarded = contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
            );
            self.record_render_use(
                &contract_use.source_expr,
                self_range_guarded,
                Some(use_is_self_guarded(contract_use)),
            );
        }

        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() || path == contract_use.source_expr {
                    continue;
                }
                self.render_path_facts(path);
                if !contract_use.path.0.is_empty() {
                    self.record_render_use(path, matches!(guard, Guard::Range { .. }), None);
                }
            }
        }
    }

    fn record_empty_source_render_facts(&mut self, contract_use: &ContractUse) {
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.render_path_facts(path);
                if !contract_use.path.0.is_empty() {
                    self.record_render_use(path, matches!(guard, Guard::Range { .. }), None);
                }
            }
        }
    }

    fn render_path_facts(&mut self, path: &str) -> &mut RenderPathFacts {
        self.render_facts_by_path
            .entry(path.to_string())
            .or_default()
    }

    fn record_render_use(&mut self, path: &str, range_guarded: bool, self_guarded: Option<bool>) {
        let acc = self.render_path_facts(path);
        acc.has_render_use = true;
        acc.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            acc.all_render_uses_self_guarded &= self_guarded;
        }
    }

    fn record_path_signals(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        self.path_signals
            .referenced_value_paths
            .insert(contract_use.source_expr.clone());
        if contract_use.kind == ValueKind::Fragment {
            self.path_signals
                .value_paths_used_as_fragment
                .insert(contract_use.source_expr.clone());
        }
        if contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty() {
            self.path_signals
                .partial_scalar_value_paths
                .insert(contract_use.source_expr.clone());
        }
        if let Some(field_kind) = metadata_field_kind_from_yaml_path(&contract_use.path.0) {
            self.path_signals
                .metadata_fields_by_value_path
                .entry(contract_use.source_expr.clone())
                .or_default()
                .insert(field_kind);
        }
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.path_signals
                    .referenced_value_paths
                    .insert(path.to_string());
                if matches!(guard, Guard::Range { .. }) {
                    self.path_signals
                        .ranged_value_paths
                        .insert(path.to_string());
                }

                if let Some(constraint) = guard_constraint_from_guard(guard) {
                    self.path_signals
                        .guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(constraint);
                }
            }
        }
    }

    fn record_nullable_path(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let has_self_range_guard = contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
        );
        let info = self.nullable_accumulator(&contract_use.source_expr);
        if !contract_use.path.0.is_empty()
            || has_self_range_guard
            || contract_use.kind == ValueKind::Fragment
        {
            info.has_render_use = true;
        }
        info.all_uses_nullable &= use_is_null_tolerant(contract_use);

        for guard in &contract_use.guards {
            if let Guard::Range { path } = guard
                && !path.trim().is_empty()
            {
                self.nullable_accumulator(path).has_render_use = true;
            }
        }
    }

    fn nullable_accumulator(&mut self, path: &str) -> &mut NullablePathAccumulator {
        self.nullable_by_path.entry(path.to_string()).or_default()
    }

    fn record_required_inference_signals(&mut self, contract_use: &ContractUse) {
        for guard in &contract_use.guards {
            match guard {
                Guard::Not { path } => {
                    self.required_inference_signals
                        .conditionally_optional_paths
                        .insert(path.clone());
                }
                Guard::Or { paths } => {
                    self.required_inference_signals
                        .conditionally_optional_paths
                        .extend(paths.iter().cloned());
                }
                Guard::Default { path } => {
                    self.required_inference_signals
                        .default_fallback_paths
                        .insert(path.clone());
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
            self.required_inference_signals
                .positive_header_paths
                .insert(contract_use.source_expr.clone());
        }
    }
}
