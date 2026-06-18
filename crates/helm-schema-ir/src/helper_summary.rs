use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use crate::bound_helper_call_analysis::{
    analyze_bound_helper_call_with_fragment_locals,
    analyze_bound_helper_calls_with_fragment_locals_in_exprs,
};
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_binding::HelperBinding;
use crate::predicate::Predicate;
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<Predicate>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
}

impl HelperOutputMeta {
    pub(crate) fn with_predicates(predicates: &BTreeSet<Predicate>, defaulted: bool) -> Self {
        Self {
            predicates: predicates.clone(),
            defaulted,
            provenance: Vec::new(),
        }
    }

    pub(crate) fn add_predicates(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        self.predicates.extend(predicates);
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

    pub(crate) fn compatibility_guards(&self, source_expr: &str) -> Vec<Guard> {
        let mut guards = Vec::new();
        for predicate in &self.predicates {
            for guard in predicate.compatibility_guards() {
                if !guards.contains(&guard) {
                    guards.push(guard);
                }
            }
        }
        if self.defaulted {
            let default_guard = Guard::Default {
                path: source_expr.to_string(),
            };
            if !guards.contains(&default_guard) {
                guards.push(default_guard);
            }
        }
        guards
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HelperFragmentOutputUse {
    pub(crate) source_expr: String,
    pub(crate) relative_path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) meta: HelperOutputMeta,
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
        self.fragment_output_uses.push(HelperFragmentOutputUse {
            source_expr,
            relative_path,
            kind,
            meta,
        });
    }
}

pub(crate) struct HelperSummaryCache {
    bound_helper_calls: RefCell<BTreeMap<BoundHelperCallsCacheKey, HelperSummary>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallsCacheKey {
    exprs: String,
    current_dot: Option<HelperBinding>,
    root_bindings: BTreeMap<String, HelperBinding>,
    fragment_locals: BTreeMap<String, FragmentBinding>,
}

impl HelperSummaryCache {
    pub(crate) fn new() -> Self {
        Self {
            bound_helper_calls: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn summarize_bound_helper_calls_in_exprs(
        &self,
        exprs: &[helm_schema_ast::TemplateExpr],
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
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

        let root_bindings_key: BTreeMap<String, HelperBinding> = bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, FragmentBinding> = fragment_locals
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

    #[tracing::instrument(skip_all)]
    pub(crate) fn summarize_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&helm_schema_ast::TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        )
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
    use std::collections::BTreeSet;

    use helm_schema_ast::TemplateExpr;

    use super::{HelperOutputMeta, HelperSummary};
    use crate::predicate::{Predicate, PredicateAtom};
    use crate::template_expr_cache::parse_expr_text;
    use crate::{Guard, ValueKind, YamlPath};

    #[test]
    fn helper_output_meta_projects_predicates_at_compatibility_boundary() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::Not(Box::new(Predicate::Atom(
                PredicateAtom::Truthy {
                    path: "feature.enabled".to_string(),
                },
            )))]),
            defaulted: true,
            provenance: Vec::new(),
        };

        assert_eq!(
            meta.compatibility_guards("serviceAccount.name"),
            vec![
                Guard::Not {
                    path: "feature.enabled".to_string(),
                },
                Guard::Default {
                    path: "serviceAccount.name".to_string(),
                },
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

        assert_eq!(summary.fragment_output_uses.len(), 1);
    }

    #[test]
    fn structural_exprs_cache_key_is_source_spelling_independent() {
        fn exprs(text: &str) -> Vec<TemplateExpr> {
            parse_expr_text(text)
        }

        assert_eq!(
            super::structural_exprs_cache_key(&exprs("include \"name\" .")),
            super::structural_exprs_cache_key(&exprs("{{ include   \"name\" . }}"))
        );
    }
}
