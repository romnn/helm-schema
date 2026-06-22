use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use crate::abstract_value::AbstractValue;
use crate::bound_helper_call_analysis::{
    analyze_bound_helper_call_with_fragment_locals,
    analyze_bound_helper_calls_with_fragment_locals_in_exprs,
};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::output_path;
use crate::predicate::Predicate;
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
}

impl HelperOutputMeta {
    pub(crate) fn with_predicates(predicates: &BTreeSet<Predicate>, defaulted: bool) -> Self {
        let mut predicate_branches = BTreeSet::new();
        if !predicates.is_empty() {
            predicate_branches.insert(predicates.clone());
        }
        Self {
            predicates: predicate_branches,
            defaulted,
            provenance: Vec::new(),
        }
    }

    pub(crate) fn add_predicates(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        let predicates = predicates.into_iter().collect::<Vec<_>>();
        if predicates.is_empty() {
            return;
        }
        if self.predicates.is_empty() {
            self.predicates.insert(BTreeSet::new());
        }
        let branches = std::mem::take(&mut self.predicates);
        self.predicates = branches
            .into_iter()
            .map(|mut branch| {
                branch.extend(predicates.iter().cloned());
                branch
            })
            .collect();
    }

    pub(crate) fn with_additional_predicates(mut self, predicates: &BTreeSet<Predicate>) -> Self {
        self.add_predicates(predicates.iter().cloned());
        self
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.predicates.extend(other.predicates);
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance);
    }

    pub(crate) fn merge_ref(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance.iter().cloned());
    }

    pub(crate) fn add_provenance_site(&mut self, provenance: ContractProvenance) {
        self.merge_provenance(std::iter::once(provenance));
    }

    fn merge_provenance(&mut self, incoming: impl IntoIterator<Item = ContractProvenance>) {
        for provenance in incoming {
            if !self.provenance.contains(&provenance) {
                self.provenance.push(provenance);
            }
        }
    }

    pub(crate) fn contract_guard_sets(&self, source_expr: &str) -> Vec<Vec<Guard>> {
        let predicate_branches = if self.predicates.is_empty() {
            vec![BTreeSet::new()]
        } else {
            self.predicates.iter().cloned().collect::<Vec<_>>()
        };
        let mut guard_sets = Vec::new();
        for predicate_branch in predicate_branches {
            let mut guards =
                Predicate::contract_guard_stack(&predicate_branch.into_iter().collect::<Vec<_>>());
            if self.defaulted {
                let default_guard = Guard::Default {
                    path: source_expr.to_string(),
                };
                if !guards.contains(&default_guard) {
                    guards.push(default_guard);
                }
            }
            if !guard_sets.contains(&guards) {
                guard_sets.push(guards);
            }
        }
        guard_sets
    }
}

pub(crate) fn insert_type_hint(
    hints: &mut BTreeMap<String, BTreeSet<String>>,
    path: String,
    schema_type: &str,
) {
    if path.trim().is_empty() {
        return;
    }
    hints
        .entry(path)
        .or_default()
        .insert(schema_type.to_string());
}

#[derive(Clone, Debug)]
pub(crate) struct HelperFragmentOutputUse {
    pub(crate) source_expr: String,
    pub(crate) relative_path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) encoded: bool,
    pub(crate) meta: HelperOutputMeta,
}

impl HelperFragmentOutputUse {
    pub(crate) fn new(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) -> Self {
        Self {
            source_expr,
            relative_path,
            kind,
            encoded: false,
            meta,
        }
    }

    pub(crate) fn with_encoding(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        encoded: bool,
        meta: HelperOutputMeta,
    ) -> Self {
        Self {
            source_expr,
            relative_path,
            kind,
            encoded,
            meta,
        }
    }
}

#[derive(Clone, Debug)]
struct HelperStructuredOutput {
    relative_path: YamlPath,
    kind: ValueKind,
    encoded: bool,
    meta: HelperOutputMeta,
}

impl HelperStructuredOutput {
    fn from_output_use(output: HelperFragmentOutputUse) -> Self {
        Self {
            relative_path: output.relative_path,
            kind: output.kind,
            encoded: output.encoded,
            meta: output.meta,
        }
    }

    fn into_output_use(self, source_expr: String) -> HelperFragmentOutputUse {
        HelperFragmentOutputUse {
            source_expr,
            relative_path: self.relative_path,
            kind: self.kind,
            encoded: self.encoded,
            meta: self.meta,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HelperSummary {
    pub(crate) string_output: BTreeSet<String>,
    path_facts: BTreeMap<String, HelperPathFacts>,
    pub(crate) suppress_roots: BTreeSet<String>,
    /// Values-rooted paths that a helper body structurally declares as
    /// null-tolerant via a `set OPERAND "KEY" (OPERAND.KEY | default V)`
    /// mutation. Distinct from `defaulted`, which represents local
    /// `(X | default V)` expressions including condition fallbacks.
    ///
    /// Only explicit set-mutation defaults count here, because that is
    /// the chart writer asserting that this path gets normalized before
    /// later reads in the same render flow.
    pub(crate) chart_defaults: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HelperPathMetaRole {
    Output,
    Dependency,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HelperPathFacts {
    meta_by_role: BTreeMap<HelperPathMetaRole, HelperOutputMeta>,
    pub(crate) guard: bool,
    pub(crate) type_hints: BTreeSet<String>,
    structured_outputs: Vec<HelperStructuredOutput>,
}

impl HelperPathFacts {
    fn merge(&mut self, other: Self) {
        for (role, meta) in other.meta_by_role {
            self.meta_by_role.entry(role).or_default().merge(meta);
        }
        self.guard |= other.guard;
        self.type_hints.extend(other.type_hints);
        self.structured_outputs.extend(other.structured_outputs);
    }

    pub(crate) fn is_dependency_relevant(&self) -> bool {
        self.has_meta_role(HelperPathMetaRole::Output)
            || self.has_meta_role(HelperPathMetaRole::Dependency)
            || self.guard
            || !self.type_hints.is_empty()
            || !self.structured_outputs.is_empty()
    }

    fn merge_meta(&mut self, role: HelperPathMetaRole, meta: HelperOutputMeta) {
        self.meta_by_role.entry(role).or_default().merge(meta);
    }

    fn ensure_meta(&mut self, role: HelperPathMetaRole) {
        self.meta_by_role.entry(role).or_default();
    }

    fn has_meta_role(&self, role: HelperPathMetaRole) -> bool {
        self.meta_by_role.contains_key(&role)
    }

    pub(crate) fn output_meta(&self) -> Option<&HelperOutputMeta> {
        self.meta_by_role.get(&HelperPathMetaRole::Output)
    }

    pub(crate) fn dependency_meta(&self) -> Option<&HelperOutputMeta> {
        self.meta_by_role.get(&HelperPathMetaRole::Dependency)
    }

    pub(crate) fn is_guard(&self) -> bool {
        self.guard
    }

    pub(crate) fn type_hints(&self) -> &BTreeSet<String> {
        &self.type_hints
    }

    pub(crate) fn fragment_output_uses(&self, source_expr: &str) -> Vec<HelperFragmentOutputUse> {
        self.structured_outputs
            .iter()
            .cloned()
            .map(|output| output.into_output_use(source_expr.to_string()))
            .collect()
    }

    pub(crate) fn has_fragment_output_uses(&self) -> bool {
        !self.structured_outputs.is_empty()
    }
}

impl HelperSummary {
    pub(crate) fn extend(&mut self, other: Self) {
        self.merge_path_facts(other.path_facts);
        self.string_output.extend(other.string_output);
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.merge_output_meta(path, meta);
    }

    pub(crate) fn merge_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        self.merge_path_meta(path, HelperPathMetaRole::Output, meta);
    }

    pub(crate) fn add_dependency_meta_map(
        &mut self,
        meta_by_path: BTreeMap<String, HelperOutputMeta>,
    ) {
        for (path, meta) in meta_by_path {
            self.merge_dependency_meta(path, meta);
        }
    }

    pub(crate) fn add_dependency_path(&mut self, path: String) {
        if !path.trim().is_empty() {
            self.path_facts
                .entry(path)
                .or_default()
                .ensure_meta(HelperPathMetaRole::Dependency);
        }
    }

    pub(crate) fn merge_dependency_meta(&mut self, path: String, meta: HelperOutputMeta) {
        self.merge_path_meta(path, HelperPathMetaRole::Dependency, meta);
    }

    pub(crate) fn add_guard_path(&mut self, path: String) {
        if !path.trim().is_empty() {
            self.path_facts.entry(path).or_default().guard = true;
        }
    }

    pub(crate) fn add_fragment_output_use(&mut self, output: HelperFragmentOutputUse) {
        if output.source_expr.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(output.source_expr.clone())
            .or_default()
            .structured_outputs
            .push(HelperStructuredOutput::from_output_use(output));
    }

    pub(crate) fn add_fragment_output_uses(&mut self, mut outputs: Vec<HelperFragmentOutputUse>) {
        outputs.retain(|output| {
            output.kind == ValueKind::Fragment || !output.relative_path.0.is_empty()
        });
        let structured_sources: BTreeSet<String> = outputs
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        for source in &structured_sources {
            self.remove_output_path(source);
        }
        for output in outputs {
            self.add_fragment_output_use(output);
        }
    }

    pub(crate) fn add_type_hints(&mut self, hints: BTreeMap<String, BTreeSet<String>>) {
        for (path, schema_types) in hints {
            self.merge_type_hints(path, schema_types);
        }
    }

    pub(crate) fn merge_type_hints(&mut self, path: String, schema_types: BTreeSet<String>) {
        if path.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(path)
            .or_default()
            .type_hints
            .extend(schema_types);
    }

    pub(crate) fn has_document_value_facts(&self) -> bool {
        self.path_facts
            .values()
            .any(HelperPathFacts::is_dependency_relevant)
    }

    pub(crate) fn add_provenance_to_outputs(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            if let Some(meta) = facts.meta_by_role.get_mut(&HelperPathMetaRole::Output) {
                meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn add_provenance_to_dependencies(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            if let Some(meta) = facts.meta_by_role.get_mut(&HelperPathMetaRole::Dependency) {
                meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn add_provenance_to_fragment_outputs(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            for output in &mut facts.structured_outputs {
                output.meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn remove_output_path(&mut self, path: &str) {
        if let Some(facts) = self.path_facts.get_mut(path) {
            facts.meta_by_role.remove(&HelperPathMetaRole::Output);
        }
    }

    pub(crate) fn path_facts(&self) -> impl Iterator<Item = (&str, &HelperPathFacts)> {
        self.path_facts
            .iter()
            .map(|(path, facts)| (path.as_str(), facts))
    }

    pub(crate) fn structured_fragment_sources(&self) -> BTreeSet<String> {
        self.path_facts()
            .filter(|(_path, facts)| !facts.structured_outputs.is_empty())
            .map(|(path, _facts)| path.to_string())
            .collect()
    }

    pub(crate) fn rendered_sources(&self) -> BTreeSet<String> {
        let mut rendered_sources = self.structured_fragment_sources();
        rendered_sources.extend(
            self.path_facts()
                .filter(|(_path, facts)| facts.output_meta().is_some())
                .map(|(path, _facts)| path.to_string()),
        );
        rendered_sources
    }

    pub(crate) fn take_chart_value_defaults(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.chart_defaults)
    }

    pub(crate) fn mark_suppressed_roots_for_bound_outputs(
        &mut self,
        bindings: &HashMap<String, AbstractValue>,
    ) {
        let rendered_sources: BTreeSet<String> = self
            .path_facts
            .iter()
            .filter_map(|(path, facts)| {
                (facts.has_meta_role(HelperPathMetaRole::Output) || facts.guard)
                    .then_some(path.clone())
            })
            .collect();
        for binding in bindings.values() {
            let AbstractValue::ValuesPath(root) = binding else {
                continue;
            };
            if output_path::values_path_has_descendant(root, &rendered_sources) {
                self.suppress_roots.insert(root.clone());
            }
        }
    }

    fn merge_path_facts(&mut self, path_facts: BTreeMap<String, HelperPathFacts>) {
        for (path, facts) in path_facts {
            if path.trim().is_empty() {
                continue;
            }
            self.path_facts.entry(path).or_default().merge(facts);
        }
    }

    fn merge_path_meta(&mut self, path: String, role: HelperPathMetaRole, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(path)
            .or_default()
            .merge_meta(role, meta);
    }

    pub(crate) fn project_helper_value(self) -> Option<AbstractValue> {
        project_summary_value(self).map(|value| value.to_context_value())
    }

    pub(crate) fn project_fragment_value(self) -> Option<AbstractValue> {
        project_summary_value(self)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

fn project_summary_value(analysis: HelperSummary) -> Option<AbstractValue> {
    let structured_sources = analysis.structured_fragment_sources();
    let rendered_sources = analysis.rendered_sources();

    let mut values = Vec::new();
    if !analysis.string_output.is_empty() {
        values.push(AbstractValue::StringSet(analysis.string_output.clone()));
    }
    for (path, facts) in analysis.path_facts() {
        for output in facts.fragment_output_uses(path) {
            values.push(AbstractValue::for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        if let Some(meta) = facts.output_meta()
            && !structured_sources.contains(path)
            && !output_path::values_path_has_descendant(path, &rendered_sources)
        {
            values.push(AbstractValue::OutputSet(
                [(path.to_string(), meta.clone())].into_iter().collect(),
            ));
        }
    }
    AbstractValue::merge_all(values)
}

pub(crate) struct HelperSummaryCache {
    bound_helper_calls: RefCell<BTreeMap<BoundHelperCallsCacheKey, HelperSummary>>,
    bound_helper_call: RefCell<BTreeMap<BoundHelperCallCacheKey, HelperSummary>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallsCacheKey {
    exprs: String,
    current_dot: Option<AbstractValue>,
    root_bindings: BTreeMap<String, AbstractValue>,
    fragment_locals: BTreeMap<String, AbstractValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallCacheKey {
    name: String,
    arg: String,
    current_dot: Option<AbstractValue>,
    outer_bindings: BTreeMap<String, AbstractValue>,
    fragment_locals: BTreeMap<String, AbstractValue>,
    seen: BTreeSet<String>,
}

impl HelperSummaryCache {
    pub(crate) fn new() -> Self {
        Self {
            bound_helper_calls: RefCell::new(BTreeMap::new()),
            bound_helper_call: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn summarize_bound_helper_calls_in_exprs(
        &self,
        exprs: &[helm_schema_ast::TemplateExpr],
        bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        fragment_locals: &HashMap<String, AbstractValue>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        if !seen.is_empty() {
            return analyze_bound_helper_calls_with_fragment_locals_in_exprs(
                exprs,
                bindings,
                current_dot,
                fragment_locals,
                context,
                seen,
            );
        }

        let root_bindings_key: BTreeMap<String, AbstractValue> = bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, AbstractValue> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallsCacheKey {
            exprs: structural_exprs_cache_key(exprs),
            current_dot: current_dot.cloned(),
            root_bindings: root_bindings_key,
            fragment_locals: fragment_locals_key,
        };

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            return cached.clone();
        }

        let summary = analyze_bound_helper_calls_with_fragment_locals_in_exprs(
            exprs,
            bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        );
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, summary.clone());
        summary
    }

    #[tracing::instrument(skip_all, fields(helper = name))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn summarize_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&helm_schema_ast::TemplateExpr>,
        outer_bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        fragment_locals: &HashMap<String, AbstractValue>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        let outer_bindings_key: BTreeMap<String, AbstractValue> = outer_bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, AbstractValue> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallCacheKey {
            name: name.to_string(),
            arg: structural_optional_expr_cache_key(arg),
            current_dot: current_dot.cloned(),
            outer_bindings: outer_bindings_key,
            fragment_locals: fragment_locals_key,
            seen: seen.iter().cloned().collect(),
        };

        if let Some(cached) = self.bound_helper_call.borrow().get(&key) {
            return cached.clone();
        }

        let summary = analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        );
        self.bound_helper_call
            .borrow_mut()
            .insert(key, summary.clone());
        summary
    }
}

fn structural_exprs_cache_key(exprs: &[helm_schema_ast::TemplateExpr]) -> String {
    let mut out = String::new();
    let _ = write!(out, "n{}|", exprs.len());
    for expr in exprs {
        append_structural_expr_key(&mut out, expr);
    }
    out
}

fn structural_optional_expr_cache_key(expr: Option<&helm_schema_ast::TemplateExpr>) -> String {
    let mut out = String::new();
    match expr {
        Some(expr) => append_structural_expr_key(&mut out, expr),
        None => out.push('n'),
    }
    out
}

fn append_structural_expr_key(out: &mut String, expr: &helm_schema_ast::TemplateExpr) {
    use helm_schema_ast::{Literal, TemplateExpr};

    match expr {
        TemplateExpr::Literal(Literal::String(value)) => {
            out.push_str("ls");
            append_len_prefixed(out, value);
        }
        TemplateExpr::Literal(Literal::RawString(value)) => {
            out.push_str("lr");
            append_len_prefixed(out, value);
        }
        TemplateExpr::Literal(Literal::Int(value)) => {
            let _ = write!(out, "li{value}|");
        }
        TemplateExpr::Literal(Literal::Float(value)) => {
            let _ = write!(out, "lf{:016x}|", value.to_bits());
        }
        TemplateExpr::Literal(Literal::Bool(value)) => {
            let _ = write!(out, "lb{}|", u8::from(*value));
        }
        TemplateExpr::Literal(Literal::Nil) => out.push_str("ln|"),
        TemplateExpr::Field(path) => {
            out.push_str("f[");
            append_string_list(out, path);
            out.push(']');
        }
        TemplateExpr::Selector { operand, path } => {
            out.push_str("s(");
            append_structural_expr_key(out, operand);
            out.push('[');
            append_string_list(out, path);
            out.push_str("])");
        }
        TemplateExpr::Variable(variable) => {
            out.push('v');
            append_len_prefixed(out, variable);
        }
        TemplateExpr::Call { function, args } => {
            out.push('c');
            append_len_prefixed(out, function);
            out.push('(');
            for arg in args {
                append_structural_expr_key(out, arg);
            }
            out.push(')');
        }
        TemplateExpr::Pipeline(stages) => {
            out.push_str("p(");
            for stage in stages {
                append_structural_expr_key(out, stage);
            }
            out.push(')');
        }
        TemplateExpr::Parenthesized(inner) => {
            out.push_str("q(");
            append_structural_expr_key(out, inner);
            out.push(')');
        }
        TemplateExpr::VariableDefinition { name, value } => {
            out.push_str("vd");
            append_len_prefixed(out, name);
            append_structural_expr_key(out, value);
        }
        TemplateExpr::Assignment { name, value } => {
            out.push_str("as");
            append_len_prefixed(out, name);
            append_structural_expr_key(out, value);
        }
        TemplateExpr::Unknown(value) => {
            out.push('u');
            append_len_prefixed(out, value);
        }
    }
}

fn append_string_list(out: &mut String, values: &[String]) {
    let _ = write!(out, "{}:", values.len());
    for value in values {
        append_len_prefixed(out, value);
    }
}

fn append_len_prefixed(out: &mut String, value: &str) {
    let _ = write!(out, "{}:{value}|", value.len());
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::TemplateExpr;

    use super::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
    use crate::abstract_value::AbstractValue;
    use crate::predicate::Predicate;
    use crate::template_expr_cache::parse_expr_text;
    use crate::{Guard, ValueKind, YamlPath};

    #[test]
    fn helper_output_meta_projects_predicates_to_contract_guard_sets() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([BTreeSet::from([Predicate::Not(Box::new(
                Predicate::truthy_path("feature.enabled"),
            ))])]),
            defaulted: true,
            provenance: Vec::new(),
        };

        sim_assert_eq!(
            have: meta.contract_guard_sets("serviceAccount.name"),
            want: vec![vec![
                Guard::Not {
                    path: "feature.enabled".to_string(),
                },
                Guard::Default {
                    path: "serviceAccount.name".to_string(),
                },
            ]]
        );
    }

    #[test]
    fn helper_output_meta_preserves_alternative_guard_sets() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([
                BTreeSet::from([
                    Predicate::truthy_path("feature.enabled"),
                    Predicate::truthy_path("component.enabled"),
                ]),
                BTreeSet::from([
                    Predicate::truthy_path("feature.enabled").negated(),
                    Predicate::truthy_path("component.enabled"),
                ]),
            ]),
            defaulted: true,
            provenance: Vec::new(),
        };

        sim_assert_eq!(
            have: meta.contract_guard_sets("serviceAccount.name"),
            want: vec![
                vec![
                    Guard::Truthy {
                        path: "component.enabled".to_string(),
                    },
                    Guard::Truthy {
                        path: "feature.enabled".to_string(),
                    },
                    Guard::Default {
                        path: "serviceAccount.name".to_string(),
                    },
                ],
                vec![
                    Guard::Truthy {
                        path: "component.enabled".to_string(),
                    },
                    Guard::Not {
                        path: "feature.enabled".to_string(),
                    },
                    Guard::Default {
                        path: "serviceAccount.name".to_string(),
                    },
                ],
            ]
        );
    }

    #[test]
    fn helper_summary_merges_fragment_output_uses() {
        let mut summary = HelperSummary::default();
        summary.add_fragment_output_use(HelperFragmentOutputUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        ));

        let outputs = summary
            .path_facts()
            .flat_map(|(path, facts)| facts.fragment_output_uses(path))
            .collect::<Vec<_>>();

        sim_assert_eq!(have: outputs.len(), want: 1);
    }

    #[test]
    fn helper_summary_helper_projection_preserves_structured_output_metadata() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
                "enabled".to_string(),
            )])]),
            defaulted: true,
            provenance: Vec::new(),
        };
        let mut summary = HelperSummary::default();
        summary.add_fragment_output_use(HelperFragmentOutputUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            meta.clone(),
        ));

        sim_assert_eq!(
            have: summary.project_helper_value(),
            want: Some(AbstractValue::Dict(BTreeMap::from([(
                "app".to_string(),
                AbstractValue::OutputSet(BTreeMap::from([("podLabels".to_string(), meta)])),
            )])))
        );
    }

    #[test]
    fn helper_summary_fragment_projection_preserves_structured_output_path() {
        let mut summary = HelperSummary::default();
        summary.add_fragment_output_use(HelperFragmentOutputUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        ));

        sim_assert_eq!(
            have: summary.project_fragment_value(),
            want: Some(AbstractValue::Dict(BTreeMap::from([(
                "app".to_string(),
                AbstractValue::output_paths(["podLabels".to_string()]),
            )])))
        );
    }

    #[test]
    fn helper_summary_fragment_projection_merges_scalar_outputs_into_one_output_set() {
        let mut summary = HelperSummary::default();
        summary.add_output_meta("image.repository".to_string(), HelperOutputMeta::default());
        summary.add_output_meta("image.tag".to_string(), HelperOutputMeta::default());

        sim_assert_eq!(
            have: summary.project_fragment_value(),
            want: Some(AbstractValue::output_paths([
                "image.repository".to_string(),
                "image.tag".to_string(),
            ]))
        );
    }

    #[test]
    fn suppresses_bound_root_when_helper_outputs_descendant_path() {
        let mut analysis = HelperSummary::default();
        analysis.add_output_meta(
            "serviceAccount.name".to_string(),
            HelperOutputMeta::default(),
        );
        let bindings = HashMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )]);

        analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

        sim_assert_eq!(
            have: analysis.suppress_roots,
            want: BTreeSet::from(["serviceAccount".to_string()])
        );
    }

    #[test]
    fn does_not_suppress_bound_root_for_exact_root_output() {
        let mut analysis = HelperSummary::default();
        analysis.add_output_meta("serviceAccount".to_string(), HelperOutputMeta::default());
        let bindings = HashMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )]);

        analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

        assert!(analysis.suppress_roots.is_empty());
    }

    #[test]
    fn structural_exprs_cache_key_is_source_spelling_independent() {
        fn exprs(text: &str) -> Vec<TemplateExpr> {
            parse_expr_text(text)
        }

        sim_assert_eq!(
            have: super::structural_exprs_cache_key(&exprs("include \"name\" .")),
            want: super::structural_exprs_cache_key(&exprs("{{ include   \"name\" . }}"))
        );
    }
}
