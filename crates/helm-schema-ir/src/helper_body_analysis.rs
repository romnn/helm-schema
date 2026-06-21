use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_value_from_expr_with_fragment_locals, values_for_helper_arg_with_fragment_locals,
};
use crate::helper_fragment_output_uses::{FragmentOutputUseRuntime, FragmentOutputUseSnapshot};
use crate::helper_runtime_plan::{HelperConditionPlan, HelperRangeRuntimePlan};
use crate::helper_summary::HelperSummary;
use crate::helper_value_analysis::{HelperValueRuntime, HelperValueSnapshot};
use crate::helper_walk_state::{
    FragmentOutputWalkState, HelperRuntimeLocals, HelperValuesWalkState,
};
use crate::node_eval::{
    AssignmentObservation, NodeActionEffectSink, NodeEvalRuntime, eval_template_body,
};
use crate::predicate::Predicate;
use crate::{ContractProvenance, SourceSpan};

pub(crate) struct BoundHelperCallResolution {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) helper_body_dot: Option<AbstractValue>,
    pub(crate) helper_fragment_dot: Option<AbstractValue>,
}

pub(crate) struct ResolveBoundHelperCallParams<'a, 'context> {
    pub(crate) helper_name: &'a str,
    pub(crate) arg: Option<&'a TemplateExpr>,
    pub(crate) outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    pub(crate) current_dot: Option<&'a AbstractValue>,
    pub(crate) fragment_locals: &'a HashMap<String, AbstractValue>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'a HashSet<String>,
}

pub(crate) fn resolve_bound_helper_call(
    params: ResolveBoundHelperCallParams<'_, '_>,
) -> BoundHelperCallResolution {
    let mut binding_seen = params.seen.clone();
    let mut bindings = values_for_helper_arg_with_fragment_locals(
        params.arg,
        params.outer_bindings,
        params.current_dot,
        params.fragment_locals,
        params.context,
        &mut binding_seen,
    );

    let mut dot_seen = params.seen.clone();
    let mut helper_body_dot = params
        .arg
        .and_then(|expr| {
            helper_value_from_expr_with_fragment_locals(
                expr,
                params.fragment_locals,
                params.outer_bindings,
                params.current_dot,
                params.context,
                &mut dot_seen,
            )
        })
        .or_else(|| params.current_dot.cloned());

    let mut helper_fragment_dot = params.arg.and_then(|expr| {
        context_value_from_outer_expr(
            expr,
            Some(params.fragment_locals),
            params.outer_bindings,
            params.current_dot,
        )
    });

    if helper_uses_large_config_arg(params.helper_name) {
        if let Some(binding) = bindings.remove("config") {
            bindings.insert("config".to_string(), abstract_config_binding(binding));
        }
        helper_body_dot = helper_body_dot.map(abstract_config_entry_in_binding);
        helper_fragment_dot = helper_fragment_dot.map(abstract_config_entry_in_binding);
    }

    BoundHelperCallResolution {
        bindings,
        helper_body_dot,
        helper_fragment_dot,
    }
}

fn helper_uses_large_config_arg(name: &str) -> bool {
    name.starts_with("opentelemetry-collector.apply")
}

fn abstract_config_binding(binding: AbstractValue) -> AbstractValue {
    let paths = binding.paths();
    if paths.is_empty() {
        AbstractValue::Top
    } else {
        AbstractValue::PathSet(paths)
    }
}

fn abstract_config_entry_in_binding(binding: AbstractValue) -> AbstractValue {
    match binding {
        AbstractValue::Dict(mut entries) => {
            if let Some(config) = entries.remove("config") {
                entries.insert("config".to_string(), abstract_config_binding(config));
            }
            AbstractValue::Dict(entries)
        }
        other => other,
    }
}

struct ResolvedHelperBody<'a> {
    source: &'a str,
    tree: tree_sitter::Tree,
    provenance: Option<ContractProvenance>,
}

impl<'a> ResolvedHelperBody<'a> {
    fn resolve(name: &str, context: FragmentEvalContext<'a>) -> Option<Self> {
        let source = context.define_bodies.source(name)?;
        let tree = context.define_bodies.tree(name)?;
        let provenance = context
            .define_bodies
            .source_path(name)
            .zip(context.define_bodies.body_offset(name))
            .map(|(source_path, body_offset)| {
                ContractProvenance::new(
                    source_path,
                    SourceSpan::new(body_offset, body_offset + source.len()),
                    vec![name.to_string()],
                )
            });
        Some(Self {
            source,
            tree,
            provenance,
        })
    }

    fn attach_provenance(&self, analysis: &mut HelperSummary) {
        let Some(provenance) = self.provenance.clone() else {
            return;
        };
        analysis.add_provenance_to_outputs(provenance.clone());
        analysis.add_provenance_to_fragment_outputs(provenance.clone());
        analysis.add_provenance_to_dependencies(provenance);
    }
}

#[tracing::instrument(skip_all, fields(helper = name))]
pub(crate) fn interpret_bound_helper_body(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    let Some(body) = ResolvedHelperBody::resolve(name, context) else {
        return HelperSummary::default();
    };
    let mut analysis = HelperSummary::default();
    collect_helper_summary(&body, resolution, context, seen, &mut analysis);
    body.attach_provenance(&mut analysis);

    analysis
}

fn collect_helper_summary(
    body: &ResolvedHelperBody<'_>,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    analysis: &mut HelperSummary,
) {
    let mut value_locals = HelperRuntimeLocals::default();
    let mut fragment_locals = HelperRuntimeLocals::default();
    let mut local_output_meta = HashMap::new();
    let mut fragment_output_uses = Vec::new();
    let mut value_seen = seen.clone();
    let mut fragment_seen = seen.clone();
    let mut helper_values_state = HelperValuesWalkState {
        locals: &mut value_locals,
        local_output_meta: &mut local_output_meta,
        context,
        seen: &mut value_seen,
        analysis,
    };
    let mut fragment_output_state = FragmentOutputWalkState {
        locals: &mut fragment_locals,
        context,
        seen: &mut fragment_seen,
        outputs: &mut fragment_output_uses,
    };
    let value_runtime = HelperValueRuntime::new(
        body.source,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        &mut helper_values_state,
    );
    let fragment_runtime = FragmentOutputUseRuntime::new(
        &body.tree,
        body.source,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        resolution.helper_fragment_dot.as_ref(),
        &mut fragment_output_state,
    );
    let mut runtime = CombinedHelperRuntime {
        value: value_runtime,
        fragment: fragment_runtime,
    };
    eval_template_body(&mut runtime, body.tree.root_node());
    analysis.add_fragment_output_uses(fragment_output_uses);
}

struct CombinedHelperRuntime<'context, 'state> {
    value: HelperValueRuntime<'context, 'state>,
    fragment: FragmentOutputUseRuntime<'context, 'state>,
}

#[derive(Clone)]
struct CombinedHelperSnapshot {
    value: HelperValueSnapshot,
    fragment: FragmentOutputUseSnapshot,
}

struct CombinedHelperConditionPlan {
    value: HelperConditionPlan,
    fragment: HelperConditionPlan,
}

struct CombinedHelperRangePlan {
    value: HelperRangeRuntimePlan,
    fragment: HelperRangeRuntimePlan,
}

impl NodeActionEffectSink for CombinedHelperRuntime<'_, '_> {
    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.value.push_predicate_if_absent(predicate.clone());
        self.fragment.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.value.push_dot_binding(binding.clone());
        self.fragment.push_dot_binding(binding);
    }

    fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.value
            .insert_range_domain(variable.clone(), literals.clone());
        self.fragment.insert_range_domain(variable, literals);
    }

    fn observe_value_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: crate::YamlPath,
        kind: crate::ValueKind,
        extra_guards: &[crate::Guard],
    ) {
        self.value.observe_value_use_with_extra_guards(
            source_expr.clone(),
            path.clone(),
            kind,
            extra_guards,
        );
        self.fragment
            .observe_value_use_with_extra_guards(source_expr, path, kind, extra_guards);
    }
}

impl NodeEvalRuntime for CombinedHelperRuntime<'_, '_> {
    type ScopeSnapshot = CombinedHelperSnapshot;
    type ConditionPlan = CombinedHelperConditionPlan;
    type RangePlan = CombinedHelperRangePlan;

    fn source(&self) -> &str {
        self.value.source()
    }

    fn document_path_for_node(&self, node: tree_sitter::Node<'_>) -> crate::YamlPath {
        self.fragment.document_path_for_node(node)
    }

    fn document_path_for_mapping_entry_indent(
        &self,
        node: tree_sitter::Node<'_>,
        indent: usize,
    ) -> crate::YamlPath {
        self.fragment
            .document_path_for_mapping_entry_indent(node, indent)
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        CombinedHelperSnapshot {
            value: self.value.scope_snapshot(),
            fragment: self.fragment.scope_snapshot(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        self.value.restore_scope(snapshot.value);
        self.fragment.restore_scope(snapshot.fragment);
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        let value_outcomes = outcomes
            .iter()
            .cloned()
            .map(|outcome| outcome.value)
            .collect();
        let fragment_outcomes = outcomes
            .into_iter()
            .map(|outcome| outcome.fragment)
            .collect();
        self.value.join_branch_scopes(&entry.value, value_outcomes);
        self.fragment
            .join_branch_scopes(&entry.fragment, fragment_outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        let value_outcomes = outcomes
            .iter()
            .cloned()
            .map(|outcome| outcome.value)
            .collect();
        let fragment_outcomes = outcomes
            .into_iter()
            .map(|outcome| outcome.fragment)
            .collect();
        self.value.join_range_scopes(&entry.value, value_outcomes);
        self.fragment
            .join_range_scopes(&entry.fragment, fragment_outcomes);
    }

    fn range_iteration_count(&self) -> usize {
        let value_count = self.value.range_iteration_count();
        let fragment_count = self.fragment.range_iteration_count();
        debug_assert_eq!(value_count, fragment_count);
        value_count
    }

    fn enter_range_iteration(&mut self, index: usize) {
        self.value.enter_range_iteration(index);
        self.fragment.enter_range_iteration(index);
    }

    fn exit_range_iteration(&mut self, index: usize) {
        self.value.exit_range_iteration(index);
        self.fragment.exit_range_iteration(index);
    }

    fn enter_no_output(&mut self) {
        self.value.enter_no_output();
        self.fragment.enter_no_output();
    }

    fn exit_no_output(&mut self) {
        self.value.exit_no_output();
        self.fragment.exit_no_output();
    }

    fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        self.value.handle_output_node(node, exprs);
        self.fragment.handle_output_node(node, exprs);
    }

    fn observe_assignment_exprs(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentObservation {
        let value = self.value.observe_assignment_exprs(exprs);
        let fragment = self.fragment.observe_assignment_exprs(exprs);
        match (value, fragment) {
            (AssignmentObservation::LocalMutationApplied, _)
            | (_, AssignmentObservation::LocalMutationApplied) => {
                AssignmentObservation::LocalMutationApplied
            }
            (AssignmentObservation::ExpressionObserved, _)
            | (_, AssignmentObservation::ExpressionObserved) => {
                AssignmentObservation::ExpressionObserved
            }
            _ => AssignmentObservation::Unhandled,
        }
    }

    fn plan_if_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        CombinedHelperConditionPlan {
            value: self.value.plan_if_condition(header),
            fragment: self.fragment.plan_if_condition(header),
        }
    }

    fn activate_if_condition(&mut self, plan: &Self::ConditionPlan) {
        self.value.activate_if_condition(&plan.value);
        self.fragment.activate_if_condition(&plan.fragment);
    }

    fn plan_with_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        CombinedHelperConditionPlan {
            value: self.value.plan_with_condition(header),
            fragment: self.fragment.plan_with_condition(header),
        }
    }

    fn activate_with_condition(&mut self, plan: &Self::ConditionPlan) {
        self.value.activate_with_condition(&plan.value);
        self.fragment.activate_with_condition(&plan.fragment);
    }

    fn activate_condition_alternative(&mut self, plan: &Self::ConditionPlan) {
        self.value.activate_condition_alternative(&plan.value);
        self.fragment.activate_condition_alternative(&plan.fragment);
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&helm_schema_ast::TemplateHeader>,
        current_path: &crate::YamlPath,
    ) -> Self::RangePlan {
        CombinedHelperRangePlan {
            value: self.value.plan_range_action(node, header, current_path),
            fragment: self.fragment.plan_range_action(node, header, current_path),
        }
    }

    fn range_output_path(
        &self,
        node: tree_sitter::Node<'_>,
        current_path: &crate::YamlPath,
        plan: &Self::RangePlan,
    ) -> crate::YamlPath {
        let value_path = self
            .value
            .range_output_path(node, current_path, &plan.value);
        let fragment_path = self
            .fragment
            .range_output_path(node, current_path, &plan.fragment);
        debug_assert_eq!(value_path, fragment_path);
        value_path
    }

    fn activate_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        plan: &Self::RangePlan,
        current_path: &crate::YamlPath,
    ) {
        self.value
            .activate_range_action(node, &plan.value, current_path);
        self.fragment
            .activate_range_action(node, &plan.fragment, current_path);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap, HashSet};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::{DefineIndex, TreeSitterParser};

    use crate::Guard;
    use crate::define_body_cache::DefineBodyCache;
    use crate::fragment_expr_eval::FragmentEvalContext;
    use crate::helper_summary::HelperSummaryCache;

    use super::{BoundHelperCallResolution, interpret_bound_helper_body};

    #[test]
    fn helper_body_summary_preserves_if_else_output_predicates() {
        let source = r#"
            {{- define "serviceAccountName" -}}
            {{- if .Values.signoz.serviceAccount.create -}}
              {{ default (include "fullname" .) .Values.signoz.serviceAccount.name }}
            {{- else -}}
              {{ default "default" .Values.signoz.serviceAccount.name }}
            {{- end -}}
            {{- end -}}
        "#;
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::new(),
            helper_body_dot: None,
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary =
            interpret_bound_helper_body("serviceAccountName", &resolution, context, &mut seen);
        let output_meta = summary.output_path_meta();
        let meta = output_meta
            .get("signoz.serviceAccount.name")
            .expect("service account name output metadata");
        let guard_sets = meta.contract_guard_sets("signoz.serviceAccount.name");

        assert!(
            guard_sets.contains(&vec![
                Guard::Truthy {
                    path: "signoz.serviceAccount.create".to_string(),
                },
                Guard::Default {
                    path: "signoz.serviceAccount.name".to_string(),
                },
            ]),
            "expected create=true output branch; guard_sets={guard_sets:#?}"
        );
        assert!(
            guard_sets.contains(&vec![
                Guard::Not {
                    path: "signoz.serviceAccount.create".to_string(),
                },
                Guard::Default {
                    path: "signoz.serviceAccount.name".to_string(),
                },
            ]),
            "expected create=false output branch; guard_sets={guard_sets:#?}"
        );
        sim_assert_eq!(
            have: summary.type_hints().get("signoz.serviceAccount.name"),
            want: Some(&["string".to_string()].into_iter().collect()),
            "defaulted scalar output should retain string type hint"
        );
        assert!(summary.fragment_output_uses().is_empty());
    }

    #[test]
    fn helper_body_summary_resolves_string_hints_through_local_aliases() {
        let source = r#"
            {{- define "image" -}}
            {{- $repositoryName := .imageRoot.repository -}}
            {{- $tag := .imageRoot.tag | toString -}}
            {{- printf "%s:%s" $repositoryName $tag -}}
            {{- end -}}
        "#;
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::from([(
                "imageRoot".to_string(),
                crate::abstract_value::AbstractValue::ValuesPath("image".to_string()),
            )]),
            helper_body_dot: None,
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary = interpret_bound_helper_body("image", &resolution, context, &mut seen);

        sim_assert_eq!(
            have: summary.type_hints().get("image.repository"),
            want: Some(&BTreeSet::from(["string".to_string()]))
        );
        sim_assert_eq!(
            have: summary.type_hints().get("image.tag"),
            want: Some(&BTreeSet::from(["string".to_string()]))
        );
    }

    #[test]
    fn storage_class_helper_projects_storage_class_name_relative_path() {
        let source = include_str!(
            "../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates/_storage.tpl"
        );
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, source)
            .expect("define source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let resolution = BoundHelperCallResolution {
            bindings: HashMap::from([
                (
                    "persistence".to_string(),
                    crate::abstract_value::AbstractValue::ValuesPath("persistence".to_string()),
                ),
                (
                    "global".to_string(),
                    crate::abstract_value::AbstractValue::ValuesPath("global".to_string()),
                ),
            ]),
            helper_body_dot: Some(crate::abstract_value::AbstractValue::Dict(
                [
                    (
                        "persistence".to_string(),
                        crate::abstract_value::AbstractValue::ValuesPath("persistence".to_string()),
                    ),
                    (
                        "global".to_string(),
                        crate::abstract_value::AbstractValue::ValuesPath("global".to_string()),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
            helper_fragment_dot: None,
        };
        let mut seen = HashSet::new();

        let summary =
            interpret_bound_helper_body("common.storage.class", &resolution, context, &mut seen);
        let outputs = summary.fragment_output_uses();

        assert!(
            outputs.iter().any(|output| {
                output.source_expr == "global.storageClass"
                    && output.relative_path.0 == ["storageClassName".to_string()]
            }),
            "expected global.storageClass to project to storageClassName, got {outputs:#?}"
        );
        assert!(
            outputs.iter().any(|output| {
                output.source_expr == "persistence.storageClass"
                    && output.relative_path.0 == ["storageClassName".to_string()]
            }),
            "expected persistence.storageClass to project to storageClassName, got {outputs:#?}"
        );
    }
}
