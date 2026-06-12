use std::collections::{BTreeSet, HashMap, HashSet};

use crate::assignment_action_plan::{AssignmentActionPlan, LocalAssignmentPlan};
use crate::binding::FragmentBinding;
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_text};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, parse_helper_assignment,
    range_body_emits_sequence_item_from_source, range_has_destructured_variable_definition,
    range_header_text_from_source, range_iterable_binding,
};
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_template_body};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::value_use_sink::ValueUseSink;
use crate::{ValueKind, YamlPath};

pub(crate) fn collect_bound_fragment_outputs_from_tree(
    node: tree_sitter::Node<'_>,
    source: &str,
    locals: &mut HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    outputs: &mut BTreeSet<String>,
) {
    let mut runtime = FragmentOutputRuntime {
        source,
        locals,
        dot_stack: vec![current_dot.cloned()],
        context,
        seen,
        outputs,
        no_output_depth: 0,
    };
    eval_template_body(&mut runtime, node);
}

struct FragmentOutputRuntime<'context, 'state> {
    source: &'state str,
    locals: &'state mut HashMap<String, FragmentBinding>,
    dot_stack: Vec<Option<FragmentBinding>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    outputs: &'state mut BTreeSet<String>,
    no_output_depth: usize,
}

#[derive(Clone)]
struct FragmentOutputSnapshot {
    locals: HashMap<String, FragmentBinding>,
    dot_stack_len: usize,
}

impl FragmentOutputRuntime<'_, '_> {
    fn current_dot(&self) -> Option<&FragmentBinding> {
        self.dot_stack.last().and_then(Option::as_ref)
    }

    fn apply_assignment_side_effects_from_text(&mut self, text: &str) -> bool {
        let mut seen = self.seen.clone();
        let current_dot = self.current_dot().cloned();
        apply_local_set_mutations(
            text,
            self.locals,
            current_dot.as_ref(),
            self.context,
            &mut seen,
        )
    }

    fn merge_outcomes(&mut self, outcomes: Vec<FragmentOutputSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.locals;
        for outcome in iter {
            locals = merge_fragment_locals(locals, outcome.locals);
        }
        *self.locals = locals;
    }
}

impl ValueUseSink for FragmentOutputRuntime<'_, '_> {
    fn emit_use(&mut self, _source_expr: String, _path: YamlPath, _kind: ValueKind) {}

    fn emit_use_with_extra_guards(
        &mut self,
        _source_expr: String,
        _path: YamlPath,
        _kind: ValueKind,
        _extra_guards: &[crate::Guard],
    ) {
    }
}

impl NodeActionEffectSink for FragmentOutputRuntime<'_, '_> {
    fn apply_get_binding(&mut self, _plan: GetBindingPlan) {}

    fn declare_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        if let Some(binding) = binding {
            self.locals.insert(variable, binding);
        } else {
            self.locals.remove(&variable);
        }
    }

    fn assign_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        self.declare_fragment_binding(variable, binding);
    }

    fn refresh_default_paths(&mut self, _variable: &str, _rhs: &str) {}

    fn refresh_helper_output_meta(&mut self, _variable: String, _rhs: &str) {}

    fn push_predicate_if_absent(&mut self, _predicate: Predicate) {}

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>) {
        self.dot_stack.push(binding);
    }

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}
}

impl NodeEvalRuntime for FragmentOutputRuntime<'_, '_> {
    type ScopeSnapshot = FragmentOutputSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

    fn ingest_text_up_to(&mut self, _end_byte: usize) {}

    fn current_rendered_path(&self) -> YamlPath {
        YamlPath(Vec::new())
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        FragmentOutputSnapshot {
            locals: self.locals.clone(),
            dot_stack_len: self.dot_stack.len(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.locals = snapshot.locals;
        self.dot_stack.truncate(snapshot.dot_stack_len);
    }

    fn enter_local_scope(&mut self) {}

    fn exit_local_scope(&mut self) {}

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.dot_stack.truncate(entry.dot_stack_len);
        self.merge_outcomes(outcomes);
    }

    fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        if self.no_output_depth > 0 {
            return;
        }
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };
        let current_dot = self.current_dot().cloned();
        if let Some(binding) = fragment_binding_from_text(
            text,
            self.locals,
            current_dot.as_ref(),
            self.context,
            self.seen,
        ) {
            self.outputs.extend(FragmentBinding::paths(&binding));
        }
    }

    fn apply_assignment_side_effects(&mut self, text: &str) -> bool {
        self.apply_assignment_side_effects_from_text(text)
    }

    fn plan_assignment_action(&self, text: &str) -> AssignmentActionPlan {
        let local_assignment = parse_helper_assignment(text).and_then(|assignment| {
            let current_dot = self.current_dot().cloned();
            let mut seen = self.seen.clone();
            let fragment_binding = self.context.fragment_binding_from_expr(
                &assignment.rhs_expr,
                self.locals,
                current_dot.as_ref(),
                &mut seen,
            )?;
            Some(LocalAssignmentPlan {
                variable: assignment.variable,
                kind: assignment.kind,
                fragment_binding: Some(fragment_binding),
                rhs: assignment.rhs,
            })
        });

        AssignmentActionPlan {
            get_binding: None,
            local_assignment,
        }
    }

    fn plan_if_condition(&mut self, _header: &str) -> ConditionActionPlan {
        ConditionActionPlan {
            predicate: Predicate::True,
            bound_values: Vec::new(),
            dot_binding: None,
            apply_alternative_predicate: false,
        }
    }

    fn plan_with_condition(&mut self, header: &str) -> ConditionActionPlan {
        let mut seen = self.seen.clone();
        let dot_binding = fragment_binding_from_text(
            header,
            self.locals,
            self.current_dot(),
            self.context,
            &mut seen,
        );
        ConditionActionPlan {
            predicate: Predicate::True,
            bound_values: Vec::new(),
            dot_binding,
            apply_alternative_predicate: false,
        }
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        _current_path: &YamlPath,
    ) -> RangeActionPlan {
        let header = range_header_text_from_source(node, self.source);
        let binding = header.as_deref().and_then(|text| {
            let mut seen = self.seen.clone();
            range_iterable_binding(
                text,
                self.locals,
                self.current_dot(),
                self.context,
                &mut seen,
            )
        });
        if range_has_destructured_variable_definition(node)
            && !range_body_emits_sequence_item_from_source(node, self.source)
            && let Some(binding) = &binding
        {
            self.outputs.extend(FragmentBinding::paths(binding));
        }
        RangeActionPlan {
            header_text: header,
            source_paths: Vec::new(),
            literal_range: None,
            guard_path: YamlPath(Vec::new()),
            emit_header_use: false,
            renders_mapping_entries: false,
            dot_binding: binding.as_ref().and_then(FragmentBinding::item_binding),
            apply_dot_binding: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::define_body_cache::{DefineBodyCache, parse_go_template};
    use crate::helper_summary::HelperSummaryCache;
    use helm_schema_ast::DefineIndex;

    fn collect_outputs(source: &str) -> BTreeSet<String> {
        let defines = DefineIndex::new();
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries);
        let tree = parse_go_template(source).expect("parse helper body");
        let mut locals = HashMap::new();
        let mut seen = HashSet::new();
        let mut outputs = BTreeSet::new();
        collect_bound_fragment_outputs_from_tree(
            tree.root_node(),
            source,
            &mut locals,
            None,
            context,
            &mut seen,
            &mut outputs,
        );
        outputs
    }

    #[test]
    fn fragment_outputs_follow_local_assignment_through_shared_node_eval() {
        let outputs = collect_outputs(
            r#"
{{- $ctx := dict "config" .Values.serviceAccount -}}
{{- $ctx.config.name -}}
"#,
        );

        assert_eq!(outputs, BTreeSet::from(["serviceAccount.name".to_string()]));
    }

    #[test]
    fn fragment_outputs_use_with_body_dot_through_shared_node_eval() {
        let outputs = collect_outputs(
            r#"
{{- with .Values.serviceAccount -}}
{{- .name -}}
{{- end -}}
"#,
        );

        assert_eq!(outputs, BTreeSet::from(["serviceAccount.name".to_string()]));
    }

    #[test]
    fn fragment_outputs_preserve_destructured_range_iterable_path() {
        let outputs = collect_outputs(
            r#"
{{- range $name, $value := .Values.extraLabels -}}
{{ $name }}: {{ $value }}
{{- end -}}
"#,
        );

        assert_eq!(outputs, BTreeSet::from(["extraLabels".to_string()]));
    }
}
