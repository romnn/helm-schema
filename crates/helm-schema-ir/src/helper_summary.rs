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

#[derive(Clone, Debug, Default)]
pub(crate) struct HelperSummary {
    pub(crate) output: BTreeMap<String, HelperOutputMeta>,
    pub(crate) fragment_output: BTreeSet<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) string_output: BTreeSet<String>,
    pub(crate) dependency_paths: BTreeSet<String>,
    pub(crate) dependency_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
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

impl HelperSummary {
    pub(crate) fn extend(&mut self, other: Self) {
        for (path, meta) in other.output {
            self.add_output_meta(path, meta);
        }
        self.fragment_output.extend(
            other
                .fragment_output
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        self.fragment_output_uses.extend(
            other
                .fragment_output_uses
                .into_iter()
                .filter(|output| !output.source_expr.trim().is_empty()),
        );
        self.string_output.extend(other.string_output);
        self.dependency_paths.extend(
            other
                .dependency_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        self.add_dependency_meta_map(other.dependency_meta);
        self.guard_paths.extend(
            other
                .guard_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        for (path, schema_types) in other.type_hints {
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn add_output(
        &mut self,
        path: String,
        predicates: &BTreeSet<Predicate>,
        defaulted: bool,
    ) {
        self.add_output_meta(
            path,
            HelperOutputMeta::with_predicates(predicates, defaulted),
        );
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.output.entry(path).or_default().merge(meta);
    }

    pub(crate) fn add_dependency_meta_map(
        &mut self,
        meta_by_path: BTreeMap<String, HelperOutputMeta>,
    ) {
        for (path, meta) in meta_by_path {
            if path.trim().is_empty() {
                continue;
            }
            self.dependency_paths.insert(path.clone());
            self.dependency_meta.entry(path).or_default().merge(meta);
        }
    }

    pub(crate) fn add_fragment_output_use(
        &mut self,
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) {
        self.fragment_output_uses.push(HelperFragmentOutputUse::new(
            source_expr,
            relative_path,
            kind,
            meta,
        ));
    }

    pub(crate) fn dependency_paths(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self
            .output
            .keys()
            .chain(self.dependency_paths.iter())
            .chain(self.dependency_meta.keys())
            .chain(self.guard_paths.iter())
            .chain(self.fragment_output.iter())
            .chain(self.type_hints.keys())
            .cloned()
            .collect();
        out.extend(
            self.fragment_output_uses
                .iter()
                .filter(|output| !output.source_expr.trim().is_empty())
                .map(|output| output.source_expr.clone()),
        );
        out.retain(|path| !path.trim().is_empty());
        remove_ancestor_paths(out)
    }

    pub(crate) fn condition_paths(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self
            .output
            .keys()
            .chain(self.dependency_meta.keys())
            .chain(self.guard_paths.iter())
            .chain(self.fragment_output.iter())
            .chain(self.type_hints.keys())
            .cloned()
            .collect();
        out.extend(
            self.fragment_output_uses
                .iter()
                .filter(|output| !output.source_expr.trim().is_empty())
                .map(|output| output.source_expr.clone()),
        );
        out.retain(|path| !path.trim().is_empty());
        remove_ancestor_paths(out)
    }

    pub(crate) fn output_meta(&self) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self.output.clone();
        for output in &self.fragment_output_uses {
            if output.source_expr.trim().is_empty() {
                continue;
            }
            out.entry(output.source_expr.clone())
                .or_default()
                .merge_ref(&output.meta);
        }
        for path in &self.fragment_output {
            if path.trim().is_empty() {
                continue;
            }
            out.entry(path.clone()).or_default();
        }
        out
    }

    pub(crate) fn dependency_meta(&self) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self.dependency_meta.clone();
        for (path, meta) in self.output_meta() {
            out.entry(path).or_default().merge(meta);
        }
        out
    }

    pub(crate) fn project_helper_value(self) -> Option<AbstractValue> {
        project_summary_value(self, SummaryProjectionKind::HelperValue)
            .map(|value| value.to_context_value())
    }

    pub(crate) fn project_fragment_value(self) -> Option<AbstractValue> {
        project_summary_value(self, SummaryProjectionKind::FragmentValue)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

#[derive(Clone, Copy)]
enum SummaryProjectionKind {
    HelperValue,
    FragmentValue,
}

fn project_summary_value(
    analysis: HelperSummary,
    kind: SummaryProjectionKind,
) -> Option<AbstractValue> {
    let structured_sources = structured_fragment_sources(&analysis);
    let rendered_sources = rendered_sources(&analysis, &structured_sources);

    let mut values = Vec::new();
    if !analysis.string_output.is_empty() {
        values.push(AbstractValue::StringSet(analysis.string_output));
    }
    for output in analysis.fragment_output_uses {
        values.push(AbstractValue::for_output_path(
            output.source_expr,
            &output.relative_path,
            output.meta,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            values.push(fragment_output_value(source, kind));
        }
    }
    for (source, meta) in analysis.output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            values.push(AbstractValue::OutputSet(
                [(source, meta)].into_iter().collect(),
            ));
        }
    }
    AbstractValue::merge_all(values)
}

fn fragment_output_value(source: String, kind: SummaryProjectionKind) -> AbstractValue {
    match kind {
        SummaryProjectionKind::HelperValue => {
            AbstractValue::PathSet([source].into_iter().collect())
        }
        SummaryProjectionKind::FragmentValue => {
            AbstractValue::OutputSet([(source, Default::default())].into_iter().collect())
        }
    }
}

fn structured_fragment_sources(analysis: &HelperSummary) -> BTreeSet<String> {
    analysis
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect()
}

fn rendered_sources(
    analysis: &HelperSummary,
    structured_sources: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut rendered_sources = structured_sources.clone();
    rendered_sources.extend(analysis.fragment_output.iter().cloned());
    rendered_sources.extend(analysis.output.keys().cloned());
    rendered_sources
}

fn remove_ancestor_paths(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter(|path| !output_path::values_path_has_descendant(path, &paths))
        .cloned()
        .collect()
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
            out.push_str("v");
            append_len_prefixed(out, variable);
        }
        TemplateExpr::Call { function, args } => {
            out.push_str("c");
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
            out.push_str("u");
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
    use std::collections::{BTreeMap, BTreeSet};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::TemplateExpr;

    use super::{HelperOutputMeta, HelperSummary};
    use crate::abstract_value::AbstractValue;
    use crate::predicate::{Predicate, PredicateAtom};
    use crate::template_expr_cache::parse_expr_text;
    use crate::{Guard, ValueKind, YamlPath};

    #[test]
    fn helper_output_meta_projects_predicates_to_contract_guard_sets() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([BTreeSet::from([Predicate::Not(Box::new(
                Predicate::Atom(PredicateAtom::Truthy {
                    path: "feature.enabled".to_string(),
                }),
            ))])]),
            defaulted: true,
            provenance: Vec::new(),
        };

        sim_assert_eq!(
            meta.contract_guard_sets("serviceAccount.name"),
            vec![vec![
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
                    Predicate::Atom(PredicateAtom::Truthy {
                        path: "feature.enabled".to_string(),
                    }),
                    Predicate::Atom(PredicateAtom::Truthy {
                        path: "component.enabled".to_string(),
                    }),
                ]),
                BTreeSet::from([
                    Predicate::Not(Box::new(Predicate::Atom(PredicateAtom::Truthy {
                        path: "feature.enabled".to_string(),
                    }))),
                    Predicate::Atom(PredicateAtom::Truthy {
                        path: "component.enabled".to_string(),
                    }),
                ]),
            ]),
            defaulted: true,
            provenance: Vec::new(),
        };

        sim_assert_eq!(
            meta.contract_guard_sets("serviceAccount.name"),
            vec![
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
        summary.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        );

        sim_assert_eq!(summary.fragment_output_uses.len(), 1);
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
        summary.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            meta.clone(),
        );

        sim_assert_eq!(
            summary.project_helper_value(),
            Some(AbstractValue::Dict(BTreeMap::from([(
                "app".to_string(),
                AbstractValue::OutputSet(BTreeMap::from([("podLabels".to_string(), meta)])),
            )])))
        );
    }

    #[test]
    fn helper_summary_fragment_projection_preserves_structured_output_path() {
        let mut summary = HelperSummary::default();
        summary.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        );

        sim_assert_eq!(
            summary.project_fragment_value(),
            Some(AbstractValue::Dict(BTreeMap::from([(
                "app".to_string(),
                AbstractValue::fragment_output_paths(["podLabels".to_string()]),
            )])))
        );
    }

    #[test]
    fn helper_summary_fragment_projection_merges_scalar_outputs_into_one_output_set() {
        let mut summary = HelperSummary::default();
        summary.add_output_meta("image.repository".to_string(), HelperOutputMeta::default());
        summary.add_output_meta("image.tag".to_string(), HelperOutputMeta::default());
        summary.fragment_output.insert("extraEnv".to_string());

        sim_assert_eq!(
            summary.project_fragment_value(),
            Some(AbstractValue::fragment_output_paths([
                "extraEnv".to_string(),
                "image.repository".to_string(),
                "image.tag".to_string(),
            ]))
        );
    }

    #[test]
    fn structural_exprs_cache_key_is_source_spelling_independent() {
        fn exprs(text: &str) -> Vec<TemplateExpr> {
            parse_expr_text(text)
        }

        sim_assert_eq!(
            super::structural_exprs_cache_key(&exprs("include \"name\" .")),
            super::structural_exprs_cache_key(&exprs("{{ include   \"name\" . }}"))
        );
    }
}
