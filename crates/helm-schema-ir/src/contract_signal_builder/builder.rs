use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, ProviderSchemaUse, ValueKind, contract::ContractUse};
use helm_schema_core::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay, ContractFailImplication,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, FailValueRequirement, MetadataFieldKind, Predicate,
};

#[tracing::instrument(skip_all)]
#[expect(
    clippy::too_many_arguments,
    reason = "each parameter is one interpreter fact channel; a struct would               mirror the same nine fields without adding an invariant"
)]
pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &BTreeMap<String, BTreeSet<String>>,
    guarded_type_hints: &BTreeMap<String, BTreeSet<String>>,
    shape_erased_value_paths: &BTreeSet<String>,
    string_contract_value_paths: &BTreeSet<String>,
    direct_range_source_paths: &BTreeSet<String>,
    destructured_range_source_paths: &BTreeSet<String>,
    fail_conditions: &[crate::eval_effect::FailCapture],
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    let mut terminal_clauses = Vec::new();
    for contract_use in uses {
        record_contract_use(
            &mut paths,
            contract_use,
            direct_range_source_paths,
            destructured_range_source_paths,
        );
    }
    for capture in fail_conditions {
        record_fail_conjunction(
            &mut paths,
            &mut terminal_clauses,
            capture,
            direct_range_source_paths,
        );
    }
    for value_path in dependency_values_root_fragments {
        if !value_path.trim().is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.facts.record_facts(ContractValuePathFacts {
                accepted_values_root_fragment: true,
                accepted_dependency_values_root_fragment: true,
                ..ContractValuePathFacts::default()
            });
        }
    }
    // A path the chart consumes through a total stringification tolerates
    // any input type, even when the flow is too indirect for a placed row
    // (vault's `set . "csiEnabled" (eq (.Values.csi.enabled | toString)
    // "true")`); the fact carries the same serialized dominance a
    // stringified render does.
    for value_path in shape_erased_value_paths {
        if value_path.trim().is_empty() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.referenced = true;
        acc.facts.facts.used_as_serialized = true;
    }
    for value_path in string_contract_value_paths {
        if value_path.trim().is_empty() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.facts.facts.has_string_contract = true;
    }
    for (value_path, schema_types) in type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.type_hints.extend(schema_types);
        }
    }
    // Guarded hints hold only where their branches render: they type the
    // path's conditional overlays but never the unconditional base.
    for (value_path, schema_types) in guarded_type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.guarded_type_hints.extend(schema_types);
        }
    }
    finish_schema_signals(paths, terminal_clauses)
}

#[derive(Default)]
struct ContractPathAccumulator {
    referenced: bool,
    guard_predicates: Vec<ConditionalGuard>,
    facts: PathSchemaFactsAccumulator,
    requiredness: ContractRequirednessEvidence,
    /// Sink typing from guarded rows: binds at the path level only while no
    /// serialized use proves the wider contract (the overlay branches keep
    /// their own copies either way).
    guarded_provider_schema_uses: Vec<ProviderSchemaUse>,
    guarded_metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    type_hints: BTreeSet<String>,
    /// Hints observed only under branch predicates: overlay typing only.
    guarded_type_hints: BTreeSet<String>,
    conditional_overlay_branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    has_unconditional_overlay_peer: bool,
    saw_unsupported_overlay: bool,
    fail_implications: Vec<ContractFailImplication>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathSchemaFactsAccumulator {
    metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    facts: ContractValuePathFacts,
    all_uses_nullable: bool,
}

impl Default for PathSchemaFactsAccumulator {
    fn default() -> Self {
        Self {
            metadata_field_kinds: BTreeSet::new(),
            provider_schema_uses: Vec::new(),
            facts: ContractValuePathFacts {
                all_render_uses_self_guarded: true,
                ..ContractValuePathFacts::default()
            },
            all_uses_nullable: true,
        }
    }
}

impl PathSchemaFactsAccumulator {
    fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    fn record_metadata_field_kind(&mut self, field_kind: Option<MetadataFieldKind>) {
        if let Some(field_kind) = field_kind {
            self.metadata_field_kinds.insert(field_kind);
        }
    }

    fn record_facts(&mut self, facts: ContractValuePathFacts) {
        self.facts.used_as_fragment |= facts.used_as_fragment;
        self.facts.used_as_serialized |= facts.used_as_serialized;
        self.facts.has_string_contract |= facts.has_string_contract;
        self.facts.used_as_pathless_fragment |= facts.used_as_pathless_fragment;
        self.facts.accepted_values_root_fragment |= facts.accepted_values_root_fragment;
        self.facts.accepted_dependency_values_root_fragment |=
            facts.accepted_dependency_values_root_fragment;
        self.facts.is_ranged_source |= facts.is_ranged_source;
        self.facts.is_direct_ranged_source |= facts.is_direct_ranged_source;
        self.facts.has_destructured_range_use |= facts.has_destructured_range_use;
        self.facts.is_partial_scalar_value_path |= facts.is_partial_scalar_value_path;
        self.facts.is_nullable |= facts.is_nullable;
        self.facts.merge_render_use_facts(facts);
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn merge_union(&mut self, other: Self) {
        for provider_schema_use in other.provider_schema_uses {
            self.record_provider_schema_use(provider_schema_use);
        }
        self.metadata_field_kinds.extend(other.metadata_field_kinds);
        self.record_facts(other.facts);
        self.all_uses_nullable &= other.all_uses_nullable;
    }

    fn facts(
        &self,
        has_referenced_descendants: bool,
        has_item_descendants: bool,
        has_structured_item_descendants: bool,
    ) -> ContractValuePathFacts {
        let mut facts = self.facts;
        facts.has_referenced_descendants = has_referenced_descendants;
        facts.has_item_descendants = has_item_descendants;
        facts.has_structured_item_descendants = has_structured_item_descendants;
        facts.is_nullable &= self.all_uses_nullable;
        facts
    }

    fn conditional_overlay_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let facts = self.facts(
            global_facts.has_referenced_descendants,
            global_facts.has_item_descendants,
            global_facts.has_structured_item_descendants,
        );
        // A runtime string contract recorded by this branch's own rows
        // types the branch; mutually exclusive branches that render the
        // path without the contract stay unaffected.
        let mut type_hints = type_hints;
        if facts.has_string_contract {
            type_hints.insert("string".to_string());
        }
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

/// The subset of a path's observed type hints compatible with an overlay
/// branch's own type partition. A positive `TypeIs(T)` key keeps only `T`;
/// a negated one drops `T`; foreign guards leave the hints untouched.
fn partition_compatible_hints(
    hints: &BTreeSet<String>,
    guards: &[ConditionalGuard],
    value_path: &str,
) -> BTreeSet<String> {
    let mut compatible = hints.clone();
    for guard in guards {
        match guard {
            ConditionalGuard::TypeIs { path, schema_type } if path == value_path => {
                compatible.retain(|hint| hint == schema_type);
            }
            ConditionalGuard::Not(inner) => {
                if let ConditionalGuard::TypeIs { path, schema_type } = inner.as_ref()
                    && path == value_path
                {
                    compatible.retain(|hint| hint != schema_type);
                }
            }
            _ => {}
        }
    }
    compatible
}

fn record_contract_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    direct_range_source_paths: &BTreeSet<String>,
    destructured_range_source_paths: &BTreeSet<String>,
) {
    let conjunctions = contract_use
        .condition
        .disjuncts()
        .iter()
        .map(|conjunction| conjunction.iter().cloned().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for predicates in conjunctions {
        record_contract_use_conjunction(
            paths,
            contract_use,
            &predicates,
            direct_range_source_paths,
            destructured_range_source_paths,
        );
    }
}

fn record_contract_use_conjunction(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    predicates: &[Predicate],
    direct_range_source_paths: &BTreeSet<String>,
    destructured_range_source_paths: &BTreeSet<String>,
) {
    let has_source = !contract_use.source_expr.trim().is_empty();
    let path_is_empty = contract_use.path.0.is_empty();
    let range_guard_paths = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let self_range_guarded = range_guard_paths.contains(contract_use.source_expr.as_str());
    let has_matching_self_guard = predicates
        .iter()
        .any(|predicate| predicate_is_self_guarding(predicate, &contract_use.source_expr));
    let pathless_self_default_guarded = path_is_empty
        && predicates.iter().any(|predicate| {
            matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr)
        });

    // A row dispatched by a type test on its own path belongs to a
    // type-switch (`if eq (typeOf .Values.x) "string" … else …`): values of
    // unmatched types render nothing, which is valid, so the dispatch arms
    // must not close the path to the union of their tested types, and an
    // arm's sink typing holds only for its tested type, never path-wide.
    let type_dispatched = has_source
        && predicates
            .iter()
            .any(|predicate| predicate_tests_source_type(predicate, &contract_use.source_expr));
    // The catch-all COMPLEMENT arm of a type dispatch (every self-type
    // test negated: a plain `else`) executes for every unmatched type, so
    // its structural placement types that whole domain — scoped to the
    // branch key the partition rides. Arms with a positive self-type test
    // stay suppressed: their sink typing joins the union through the
    // dispatch guard predicates instead, and a `tpl`-style string arm's
    // placement says nothing about the raw value.
    let complement_dispatched = type_dispatched
        && predicates.iter().all(|predicate| {
            !predicate_tests_source_type(predicate, &contract_use.source_expr)
                || matches!(
                    predicate,
                    Predicate::Not(inner)
                        if predicate_tests_source_type(inner, &contract_use.source_expr)
                )
        });

    if has_source {
        let mut facts = ContractValuePathFacts {
            used_as_fragment: contract_use.kind == ValueKind::Fragment,
            used_as_serialized: contract_use.kind == ValueKind::Serialized || type_dispatched,
            has_string_contract: contract_use.has_string_contract && !type_dispatched,
            used_as_pathless_fragment: contract_use.kind == ValueKind::Fragment && path_is_empty,
            is_partial_scalar_value_path: contract_use.kind == ValueKind::PartialScalar
                && !path_is_empty,
            is_nullable: !path_is_empty
                || self_range_guarded
                || contract_use.kind == ValueKind::Fragment
                || pathless_self_default_guarded,
            ..ContractValuePathFacts::default()
        };
        if !path_is_empty {
            facts.record_render_use(self_range_guarded, Some(has_matching_self_guard));
            facts.has_unconditional_render_use = predicates.is_empty();
        }

        let positive_header = contract_use.kind == ValueKind::Scalar
            && path_is_empty
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                predicate_is_positive_header(predicate, &contract_use.source_expr)
            });
        // A serialized splice renders text the sink cannot type back onto
        // the input, so it contributes no metadata field kind either.
        let metadata_field_kind = if contract_use.kind == ValueKind::Serialized || type_dispatched {
            None
        } else {
            metadata_field_kind_from_yaml_path(&contract_use.path.0)
        };
        let acc = path_accumulator(paths, &contract_use.source_expr);
        acc.requiredness.is_positive_header |= positive_header;
        // An UNCONDITIONAL string-contract row types the path itself;
        // a conditional one types only its own overlay branch (the branch
        // facts carry it there).
        if contract_use.has_string_contract && predicates.is_empty() {
            acc.type_hints.insert("string".to_string());
        }
        acc.record_source_use(
            facts,
            path_is_empty || has_matching_self_guard,
            lowerable_conditional_guard_set(contract_use, predicates),
            (!type_dispatched || complement_dispatched)
                .then(|| provider_schema_use(contract_use, self_range_guarded))
                .flatten(),
            metadata_field_kind,
        );
    }

    for path in predicates
        .iter()
        .flat_map(Predicate::conditionally_optional_paths)
    {
        path_accumulator(paths, &path)
            .requiredness
            .is_conditionally_optional = true;
    }
    for path in predicates.iter().filter_map(|predicate| match predicate {
        Predicate::Guard(Guard::Default { path }) => Some(path),
        _ => None,
    }) {
        path_accumulator(paths, path)
            .requiredness
            .has_default_fallback = true;
    }
    if has_source {
        for predicate in conditional_guard_predicates(predicates) {
            for path in predicate.value_paths() {
                let acc = path_accumulator(paths, &path);
                if !acc.guard_predicates.contains(&predicate) {
                    acc.guard_predicates.push(predicate.clone());
                }
            }
        }
    }
    for path in predicates.iter().flat_map(Predicate::value_paths) {
        if has_source && path == contract_use.source_expr.as_str() {
            continue;
        }
        let acc = path_accumulator(paths, &path);
        acc.referenced |= has_source;
        if !path_is_empty {
            let mut facts = ContractValuePathFacts::default();
            facts.record_render_use(range_guard_paths.contains(&path), None);
            acc.facts.record_facts(facts);
        }
    }
    if has_source {
        for path in range_guard_paths {
            // No render-use flags ride along here, so record_facts leaves the
            // accumulator's self-guarded default untouched.
            let facts = ContractValuePathFacts {
                is_ranged_source: true,
                is_direct_ranged_source: direct_range_source_paths.contains(&path),
                has_destructured_range_use: destructured_range_source_paths.contains(&path),
                is_nullable: true,
                ..ContractValuePathFacts::default()
            };
            path_accumulator(paths, &path).facts.record_facts(facts);
            if direct_range_source_paths.contains(&path) {
                if let Some(parent) = path.strip_suffix(".*")
                    && !path_contains_wildcard(parent)
                {
                    // A nested range over a MEMBER identity (`range
                    // $values` where `$values` holds each member of a
                    // directly ranged map): every member must itself be
                    // rangeable, or the inner range aborts rendering.
                    record_member_range_requirement(paths, parent, predicates);
                } else {
                    record_guarded_range_read(
                        paths,
                        &path,
                        predicates,
                        destructured_range_source_paths.contains(&path),
                    );
                }
            }
        }
    }
}

/// A `range` read under foreign conditions bounds an ITERABLE requirement
/// to those conditions: Go's `range` iterates collections and skips nil but
/// fails template rendering on scalars, so inside the guarded branch the
/// ranged path must be a collection. The branch stays render-free; overlay
/// lowering recognizes that shape and emits the iterable domain.
/// Unconditional ranges keep their pre-existing unconstrained lowering.
fn record_guarded_range_read(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    ranged_path: &str,
    predicates: &[Predicate],
    destructured: bool,
) {
    if path_contains_wildcard(ranged_path) {
        return;
    }
    let mut guards = Vec::new();
    for predicate in predicates {
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path }) if path == ranged_path
        ) {
            continue;
        }
        // A range nested inside the path's own type dispatch
        // (`if typeIs "[]interface {}" .Values.x` around `range .Values.x`)
        // iterates only when the test matches, so the test stays IN the
        // branch key: unmatched types skip the arm entirely while matched
        // collections still receive the branch's item typing.
        if predicate_is_self_type_partition(predicate, ranged_path) {
            let Some(guard) = predicate_to_guard(predicate, None) else {
                return;
            };
            guards.push(guard);
            continue;
        }
        if extend_lowerable_predicate(predicate, ranged_path, &mut guards).is_none() {
            return;
        }
    }
    guards.sort();
    guards.dedup();
    if guards.is_empty() {
        return;
    }
    let branch = path_accumulator(paths, ranged_path)
        .conditional_overlay_branches
        .entry(guards)
        .or_default();
    branch.facts.is_nullable = true;
    branch.record_facts(ContractValuePathFacts {
        is_ranged_source: true,
        // A two-variable range cannot iterate integers; the branch's
        // iterable domain must follow the parsed binding arity (F58).
        has_destructured_range_use: destructured,
        is_nullable: true,
        ..ContractValuePathFacts::default()
    });
}

/// Lower one `fail` conjunction into a path requirement: rendering aborts
/// whenever the conjunction holds, so valid inputs must falsify the failing
/// TEST wherever the OUTER guards hold. Conjunctions whose test cannot be
/// negated structurally are skipped (truthy-fallback predicates approximate
/// undecodable conditions and must never be negated).
fn record_fail_conjunction(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    capture: &crate::eval_effect::FailCapture,
    direct_range_source_paths: &BTreeSet<String>,
) {
    // An approximate enclosing condition with NO resolvable paths (the
    // empty marker) could gate anything, and a `$local` name leaking into
    // predicate paths means the condition lowering lost the real subject:
    // both make negation unsound for the whole capture.
    if capture.approximate_condition_paths.contains("") {
        return;
    }
    if capture
        .conjunction
        .iter()
        .flat_map(Predicate::value_paths)
        .any(|path| path.starts_with('$'))
    {
        return;
    }
    // A multi-path `with` header (`with (coalesce a b)`) contributes its
    // EXACT disjunction plus one `With` row marker per path; the markers
    // annotate rows, and reading them as conjuncts would narrow the
    // failure to "every path set" when the disjunction alone is the
    // condition. Drop a marker whenever a disjunction over its path is
    // present.
    let or_covered: BTreeSet<&str> = capture
        .conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Or(items) => Some(items.iter().filter_map(|item| match item {
                Predicate::Guard(Guard::Truthy { path } | Guard::With { path }) => {
                    Some(path.as_str())
                }
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect();
    let conjunction: Vec<Predicate> = capture
        .conjunction
        .iter()
        .filter(|predicate| {
            !matches!(
                predicate,
                Predicate::Guard(Guard::With { path }) if or_covered.contains(path.as_str())
            )
        })
        .cloned()
        .collect();
    let conjunction = &conjunction;
    let ranged: Vec<&str> = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    if ranged.len() > 1 {
        return;
    }
    let ranged = ranged.first().copied();
    // A range over a DERIVED iterable (`range until (int .Values.n)`) has
    // no member identity on the underlying path; only direct ranges carry
    // per-member requirements. Helper-scope directness rides the capture.
    if let Some(path) = ranged
        && !direct_range_source_paths.contains(path)
        && !capture.direct_ranged_paths.contains(path)
    {
        return;
    }
    let member_scope = ranged.map(|path| format!("{path}.*"));

    let mut outer_guards = Vec::new();
    let mut requirements = Vec::new();
    let mut test_paths: BTreeSet<String> = BTreeSet::new();
    for predicate in conjunction {
        if matches!(predicate, Predicate::Guard(Guard::Range { .. })) {
            continue;
        }
        let paths_of = predicate.value_paths();
        // A conjunct is part of the failing TEST when it scopes to the
        // ranged member (or, without a range, to a single path) AND its
        // negation states an enforceable requirement; everything else is
        // an outer condition of the arm.
        let test_scope = match &member_scope {
            Some(scope) => (!paths_of.is_empty()
                && paths_of
                    .iter()
                    .all(|path| path == scope || path.starts_with(&format!("{scope}."))))
            .then(|| scope.clone()),
            None => (paths_of.len() == 1 && predicate_is_negatable_test(predicate))
                .then(|| paths_of.iter().next().cloned().unwrap_or_default()),
        };
        let required = test_scope
            .as_ref()
            .and_then(|scope| requirements_from_negation(predicate, scope))
            .filter(|required| !required.is_empty());
        match (test_scope, required) {
            (Some(scope), Some(mut required)) => {
                requirements.append(&mut required);
                test_paths.insert(scope);
            }
            // A member-scoped conjunct whose negation cannot be stated
            // poisons the member test: the requirement would be missing a
            // dimension of the real condition.
            (Some(_), None) if member_scope.is_some() => return,
            _ => {
                // The residue outside the failing test: lower what encodes
                // and keep the requirement for the rest (a validator's
                // strictness survives an unencodable outer guard as a
                // bounded approximation; rendering truly aborts inside it).
                if let Some(guard) = predicate_to_guard(predicate, None) {
                    outer_guards.push(guard);
                }
            }
        }
    }
    if requirements.is_empty() || test_paths.len() != 1 {
        // No single-path test survived. When the WHOLE conjunction lowers
        // to conditional guards — mutual exclusions and other cross-path
        // validator formulas — it becomes a document-level terminal
        // clause: no valid values document may satisfy all of it. Ranged
        // captures have member semantics no root clause can express, and
        // an approximate enclosing condition would make the clause fire
        // too widely.
        if ranged.is_none()
            && capture.approximate_condition_paths.is_empty()
            && !conjunction.is_empty()
        {
            let clause = conjunction
                .iter()
                .map(|predicate| predicate_to_guard(predicate, None))
                .collect::<Option<Vec<_>>>();
            if let Some(mut clause) = clause {
                clause.sort();
                clause.dedup();
                if !clause.is_empty() && !terminal_clauses.contains(&clause) {
                    terminal_clauses.push(clause);
                }
            }
        }
        return;
    }
    let target = match ranged {
        Some(path) => path.to_string(),
        None => {
            let Some(path) = test_paths.into_iter().next() else {
                return;
            };
            path
        }
    };
    if path_contains_wildcard(&target) {
        return;
    }
    // An approximately-lowered enclosing condition that touches the tested
    // path itself partitions the very domain being negated (kyverno's
    // `eq (int .replicas) 0` inner check): negating without it would
    // manufacture requirements. Foreign approximations only widen where
    // the requirement applies, the bounded direction validators accept.
    if capture.approximate_condition_paths.iter().any(|path| {
        path == &target
            || helm_schema_core::values_path_is_descendant(path, &target)
            || helm_schema_core::values_path_is_descendant(&target, path)
    }) {
        return;
    }
    requirements.sort();
    requirements.dedup();
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        per_member: ranged.is_some(),
        requirements,
    };
    let acc = path_accumulator(paths, &target);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

/// Whether a conjunct is a structurally negatable failing test. Positive
/// truthiness is excluded: the condition lowering falls back to truthy
/// approximations for conditions it cannot decode, and negating an
/// approximation would manufacture requirements the chart never stated.
fn predicate_is_negatable_test(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Not(inner) => !matches!(inner.as_ref(), Predicate::Guard(Guard::Range { .. })),
        Predicate::Guard(Guard::TypeIs { .. } | Guard::Absent { .. }) => true,
        Predicate::Or(items) => items.iter().all(predicate_is_negatable_test),
        _ => false,
    }
}

/// Requirements implied by the NEGATION of a failing test: the negation
/// must hold for the value at `scope` (a member scope `p.*` or the path
/// itself).
fn requirements_from_negation(
    predicate: &Predicate,
    scope: &str,
) -> Option<Vec<FailValueRequirement>> {
    match predicate {
        Predicate::Not(inner) => requirements_from_holding(inner, scope),
        // Negating a disjunction: every arm's negation must hold.
        Predicate::Or(items) => {
            let mut requirements = Vec::new();
            for item in items {
                requirements.append(&mut requirements_from_negation(item, scope)?);
            }
            Some(requirements)
        }
        Predicate::Guard(Guard::TypeIs { path, schema_type }) if path == scope => {
            Some(vec![FailValueRequirement::NotSchemaType(
                schema_type.clone(),
            )])
        }
        Predicate::Guard(Guard::Absent { path }) => {
            let member = path.strip_prefix(&format!("{scope}."))?;
            (!member.contains('.'))
                .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
        }
        // Dropping an equality arm weakens the requirement (fewer values
        // rejected), which is the safe direction: `required` emptiness
        // tests carry `= null` / `= ""` arms whose negations have no
        // member-schema spelling.
        Predicate::Guard(Guard::Eq { .. }) => Some(Vec::new()),
        _ => None,
    }
}

/// Requirements implied by a predicate HOLDING for the value at `scope`.
fn requirements_from_holding(
    predicate: &Predicate,
    scope: &str,
) -> Option<Vec<FailValueRequirement>> {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, schema_type }) if path == scope => {
            Some(vec![FailValueRequirement::SchemaType(schema_type.clone())])
        }
        // The tested value's own truthiness (`and $v (kindIs "string" $v)`):
        // the type requirement carries the substance; truthiness (rejecting
        // "" or 0) stays unmodeled as a bounded approximation.
        Predicate::Guard(Guard::Truthy { path }) if path == scope => Some(Vec::new()),
        Predicate::Guard(Guard::Truthy { path }) => {
            let member = path.strip_prefix(&format!("{scope}."))?;
            (!member.contains('.'))
                .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
        }
        Predicate::And(items) => {
            let mut requirements = Vec::new();
            for item in items {
                requirements.append(&mut requirements_from_holding(item, scope)?);
            }
            Some(requirements)
        }
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Guard(Guard::Absent { path }) => {
                let member = path.strip_prefix(&format!("{scope}."))?;
                (!member.contains('.'))
                    .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
            }
            _ => requirements_from_negation(inner, scope),
        },
        _ => None,
    }
}

/// A directly ranged path whose LITERAL members another template reads is
/// object-shaped whenever it is truthy: field access on a non-object
/// aborts rendering, while an empty (falsy) collection skips the member
/// templates. This closes the union bypass where the range's array
/// alternative would admit values the member reads reject.
fn record_ranged_member_read_implications(paths: &mut BTreeMap<String, ContractPathAccumulator>) {
    let ranged: Vec<String> = paths
        .iter()
        .filter(|(_, acc)| acc.facts.facts.is_direct_ranged_source)
        .map(|(path, _)| path.clone())
        .collect();
    for path in ranged {
        let member_prefix = format!("{path}.");
        let has_literal_member_reads = paths.iter().any(|(candidate, acc)| {
            acc.referenced
                && candidate
                    .strip_prefix(&member_prefix)
                    .is_some_and(|member| !member.starts_with('*'))
        });
        if !has_literal_member_reads {
            continue;
        }
        let implication = ContractFailImplication {
            outer_guards: vec![ConditionalGuard::Truthy { path: path.clone() }],
            per_member: false,
            requirements: vec![FailValueRequirement::SchemaType("object".to_string())],
        };
        let acc = path_accumulator(paths, &path);
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

fn finish_schema_signals(
    mut paths: BTreeMap<String, ContractPathAccumulator>,
    mut terminal_clauses: Vec<Vec<ConditionalGuard>>,
) -> ContractSchemaSignals {
    record_ranged_member_read_implications(&mut paths);
    let referenced_paths = paths
        .iter()
        .filter_map(|(path, acc)| acc.referenced.then_some(path.clone()))
        .collect();
    let (
        paths_with_referenced_descendants,
        paths_with_item_descendants,
        paths_with_structured_item_descendants,
    ) = collect_paths_with_descendants(&referenced_paths);
    for path in &paths_with_referenced_descendants {
        path_accumulator(&mut paths, path);
    }

    let schema_evidence_by_value_path = paths
        .into_iter()
        .map(|(value_path, acc)| {
            let has_descendants = paths_with_referenced_descendants.contains(&value_path);
            let has_item_descendants = paths_with_item_descendants.contains(&value_path);
            let has_structured_item_descendants =
                paths_with_structured_item_descendants.contains(&value_path);
            let evidence = acc.into_schema_evidence(
                value_path.clone(),
                has_descendants,
                has_item_descendants,
                has_structured_item_descendants,
            );
            (value_path, evidence)
        })
        .collect();
    terminal_clauses.sort();
    terminal_clauses.dedup();
    ContractSchemaSignals::new(schema_evidence_by_value_path, terminal_clauses)
}

fn path_accumulator<'a>(
    paths: &'a mut BTreeMap<String, ContractPathAccumulator>,
    path: &str,
) -> &'a mut ContractPathAccumulator {
    paths.entry(path.to_string()).or_default()
}

impl ContractPathAccumulator {
    fn record_source_use(
        &mut self,
        facts: ContractValuePathFacts,
        source_null_tolerant: bool,
        lowerable_guards: Option<Vec<ConditionalGuard>>,
        provider_schema_use: Option<ProviderSchemaUse>,
        metadata_field_kind: Option<MetadataFieldKind>,
    ) {
        self.referenced = true;
        self.facts.record_facts(facts);
        let row_forms_overlay_branch = facts.has_render_use
            && !facts.has_unconditional_render_use
            && lowerable_guards.is_some();
        if row_forms_overlay_branch {
            // A guarded row's sink typing rides its overlay branch; whether
            // it also binds at the path level is decided once the path's
            // serialized uses are known (see `into_schema_evidence`).
            if let Some(provider_use) = provider_schema_use.clone() {
                self.guarded_provider_schema_uses.push(provider_use);
            }
            if let Some(field_kind) = metadata_field_kind {
                self.guarded_metadata_field_kinds.insert(field_kind);
            }
        } else {
            if let Some(provider_use) = provider_schema_use.clone() {
                self.facts.record_provider_schema_use(provider_use);
            }
            self.facts.record_metadata_field_kind(metadata_field_kind);
        }
        if facts.has_render_use {
            if facts.has_unconditional_render_use {
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = lowerable_guards {
                let branch = self.conditional_overlay_branches.entry(guards).or_default();
                branch.facts.is_nullable = true;
                branch.record_nullable_observation(source_null_tolerant);
                branch.record_metadata_field_kind(metadata_field_kind);
                branch.record_facts(facts);

                if let Some(provider_schema_use) = provider_schema_use {
                    branch.record_provider_schema_use(provider_schema_use);
                }
            } else {
                self.saw_unsupported_overlay = true;
            }
        }
        self.facts.record_nullable_observation(source_null_tolerant);
    }

    fn into_schema_evidence(
        self,
        value_path: String,
        has_referenced_descendants: bool,
        has_item_descendants: bool,
        has_structured_item_descendants: bool,
    ) -> ContractPathSchemaEvidence {
        let facts = self.facts.facts(
            has_referenced_descendants,
            has_item_descendants,
            has_structured_item_descendants,
        );
        let ContractPathAccumulator {
            referenced,
            guard_predicates,
            facts: mut path_facts,
            requiredness,
            type_hints,
            guarded_type_hints,
            guarded_provider_schema_uses,
            guarded_metadata_field_kinds,
            conditional_overlay_branches,
            mut has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            mut fail_implications,
        } = self;
        if !facts.used_as_serialized {
            for provider_use in guarded_provider_schema_uses {
                path_facts.record_provider_schema_use(provider_use);
            }
            path_facts
                .metadata_field_kinds
                .extend(guarded_metadata_field_kinds);
        }
        let overlay_type_hints: BTreeSet<String> = type_hints
            .iter()
            .chain(guarded_type_hints.iter())
            .cloned()
            .collect();
        let mut evidence_groups: Vec<(PathSchemaFactsAccumulator, Vec<Vec<ConditionalGuard>>)> =
            Vec::new();
        for (guards, branch) in conditional_overlay_branches {
            if let Some((_, guard_sets)) = evidence_groups
                .iter_mut()
                .find(|(evidence, _)| evidence == &branch)
            {
                guard_sets.push(guards);
            } else {
                evidence_groups.push((branch, vec![guards]));
            }
        }
        let mut conditional_overlay_branches: BTreeMap<
            Vec<ConditionalGuard>,
            PathSchemaFactsAccumulator,
        > = BTreeMap::new();
        for (branch, guard_sets) in evidence_groups {
            for guards in
                helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(guard_sets)
            {
                if guards.is_empty() {
                    has_unconditional_overlay_peer = true;
                    continue;
                }
                match conditional_overlay_branches.entry(guards) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().merge_union(branch.clone());
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(branch.clone());
                    }
                }
            }
        }
        // An overlay whose guards could not be lowered poisons every overlay
        // for the path: emitting only the lowerable branches would understate
        // the conditional shape.
        let conditional_overlays = if saw_unsupported_overlay {
            Vec::new()
        } else {
            conditional_overlay_branches
                .into_iter()
                .map(|(guards, branch)| {
                    // A branch keyed on the path's own type partition hosts
                    // only the hints compatible with that partition: the
                    // map arm's object hint must never type the slice arm's
                    // `then` (and vice versa), or a live arm becomes
                    // internally contradictory.
                    let branch_hints = partition_compatible_hints(
                        &overlay_type_hints,
                        &guards,
                        value_path.as_str(),
                    );
                    ConditionalPathOverlay {
                        guards,
                        evidence: branch.conditional_overlay_evidence(facts, branch_hints),
                        preserve_base_schema: has_unconditional_overlay_peer,
                    }
                })
                .collect()
        };
        // Branch-scoped hints ride the overlays' evidence copies. When no
        // overlay can host them (none lowered, or an unsupported guard
        // poisoned them), dropping them would lose the evidence entirely,
        // so they degrade to path-level typing instead.
        let (type_hints, guarded_type_hints) = if conditional_overlays.is_empty() {
            (
                type_hints.union(&guarded_type_hints).cloned().collect(),
                BTreeSet::new(),
            )
        } else {
            (type_hints, guarded_type_hints)
        };
        fail_implications.sort();
        fail_implications.dedup();
        ContractPathSchemaEvidence {
            value_path,
            is_referenced_value_path: referenced,
            facts,
            guard_predicates,
            metadata_field_kinds: path_facts.metadata_field_kinds,
            type_hints,
            guarded_type_hints,
            provider_schema_uses: path_facts.provider_schema_uses,
            requiredness,
            conditional_overlays,
            fail_implications,
        }
    }
}

fn metadata_field_kind_from_yaml_path(path: &[String]) -> Option<MetadataFieldKind> {
    if path.get(path.len().checked_sub(2)?)?.as_str() != "metadata" {
        return None;
    }

    match path.last()?.as_str() {
        "labels" | "annotations" => Some(MetadataFieldKind::StringMap),
        "name" => Some(MetadataFieldKind::Name),
        "namespace" => Some(MetadataFieldKind::Namespace),
        _ => None,
    }
}

fn conditional_guard_predicates(predicates: &[Predicate]) -> Vec<ConditionalGuard> {
    let mut guards = predicates
        .iter()
        .filter_map(|predicate| predicate_to_guard(predicate, None))
        .collect::<Vec<_>>();
    guards.sort();
    guards.dedup();
    guards
}

fn lowerable_conditional_guard_set(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<Vec<ConditionalGuard>> {
    if path_contains_wildcard(&contract_use.source_expr) {
        return None;
    }

    let mut guards = Vec::new();
    for predicate in predicates {
        // The row's own iteration (`range .Values.x` around a render of
        // `.Values.x` itself) is how the row fires, not a foreign
        // condition; the overlay keys on the residual conjuncts. A range
        // over a DIFFERENT path stays unlowerable.
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path }) if path == &contract_use.source_expr
        ) {
            continue;
        }
        extend_lowerable_predicate(predicate, &contract_use.source_expr, &mut guards)?;
    }
    guards.sort();
    guards.dedup();
    Some(guards)
}

fn provider_schema_use(
    contract_use: &ContractUse,
    self_range_guarded: bool,
) -> Option<ProviderSchemaUse> {
    if contract_use.source_expr.trim().is_empty()
        || matches!(
            contract_use.kind,
            ValueKind::PartialScalar | ValueKind::Serialized
        )
        || contract_use.path.0.is_empty()
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        resource,
        is_self_range_collection: self_range_guarded
            && contract_use
                .path
                .0
                .last()
                .is_none_or(|segment| !segment.ends_with("[*]")),
    })
}

fn predicate_to_guard(
    predicate: &Predicate,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    match predicate {
        Predicate::True | Predicate::False => None,
        Predicate::Guard(guard) => guard_to_conditional_guard(guard, target_value_path),
        Predicate::Not(inner) => Some(ConditionalGuard::Not(Box::new(predicate_to_guard(
            inner,
            target_value_path,
        )?))),
        Predicate::And(predicates) => {
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        Predicate::Or(predicates) => {
            // Inside a disjunction a guard on the target itself is
            // load-bearing (`or .Values.other (and .Values.self .flag)`),
            // unlike a top-level self conjunct (the row's own firing
            // condition), so arms encode their paths literally.
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, None))
                .collect::<Option<Vec<_>>>()?;
            if guards
                .iter()
                .flat_map(ConditionalGuard::value_paths)
                .any(|path| path_contains_wildcard(&path))
            {
                return None;
            }
            guards.sort();
            guards.dedup();
            (target_value_path.is_some() || !guards.is_empty())
                .then_some(ConditionalGuard::AnyOf(guards))
        }
    }
}

fn extend_lowerable_predicate(
    predicate: &Predicate,
    target_value_path: &str,
    out: &mut Vec<ConditionalGuard>,
) -> Option<()> {
    match predicate {
        Predicate::True | Predicate::False => return None,
        Predicate::Guard(Guard::With { .. }) => {}
        Predicate::And(predicates) => {
            for predicate in predicates {
                extend_lowerable_predicate(predicate, target_value_path, out)?;
            }
        }
        Predicate::Guard(Guard::Range { .. }) => return None,
        Predicate::Guard(Guard::Default { path }) if path == target_value_path => {}
        // The row's own truthiness is nullability evidence (captured as
        // source null-tolerance), not a conditional shape over *other*
        // paths; like the self-default and self-negation arms it must not
        // poison the foreign overlay keys. Root-to-leaf guard stacks put it
        // on every `with .Values.x`-wrapped render since the fragment
        // interpreter landed.
        Predicate::Guard(Guard::Truthy { path }) if path == target_value_path => {}
        // Self-negation carries the branch's own-arm exclusion, not a
        // conditional shape over *other* paths; the overlay keys stay on the
        // foreign conditions.
        Predicate::Not(inner)
            if matches!(
                inner.as_ref(),
                Predicate::Guard(Guard::Truthy { path }) if path == target_value_path
            ) => {}
        // A type test on the row's own path (also negated or a disjunction
        // of such tests) partitions its domain (a type-switch arm). The
        // partition is load-bearing: the arm's sink typing holds only for
        // its tested types, and an executing complement arm's requirements
        // hold exactly for the untested ones — so it stays ON the overlay
        // key rather than leaking the arm's shape over the whole domain.
        other if predicate_is_self_type_partition(other, target_value_path) => {
            out.push(predicate_to_guard(other, Some(target_value_path))?);
        }
        other => {
            out.push(predicate_to_guard(other, Some(target_value_path))?);
        }
    }
    Some(())
}

fn guard_to_conditional_guard(
    guard: &Guard,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    let path = |path: &str| match target_value_path {
        Some(target_value_path) => lowerable_guard_path(path, target_value_path),
        None => Some(path.to_string()),
    };

    match guard {
        Guard::Truthy { path: value_path } => Some(ConditionalGuard::Truthy {
            path: path(value_path)?,
        }),
        Guard::With { path: value_path } if target_value_path.is_none() => {
            Some(ConditionalGuard::With {
                path: path(value_path)?,
            })
        }
        Guard::Eq {
            path: value_path,
            value,
        } => Some(ConditionalGuard::Eq {
            path: path(value_path)?,
            value: value.clone(),
        }),
        Guard::NotEq {
            path: value_path,
            value,
        } => Some(ConditionalGuard::NotEq {
            path: path(value_path)?,
            value: value.clone(),
        }),
        Guard::Absent { path: value_path } => Some(ConditionalGuard::Absent {
            path: path(value_path)?,
        }),
        Guard::TypeIs {
            path: value_path,
            schema_type,
        } => {
            // A type test on the TARGET itself is load-bearing dispatch
            // structure (the `else` of `if typeIs "string" x` scopes an
            // object overlay to non-strings); only truthiness self-guards
            // are the row's own firing condition and stay stripped.
            let path = if target_value_path == Some(value_path.as_str()) {
                (!path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::TypeIs {
                path,
                schema_type: schema_type.clone(),
            })
        }
        Guard::NotTypeIs {
            path: value_path,
            schema_type,
        } => {
            // The dispatch complement is load-bearing on the target for the
            // same reason as the positive test above.
            let path = if target_value_path == Some(value_path.as_str()) {
                (!path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::Not(Box::new(ConditionalGuard::TypeIs {
                path,
                schema_type: schema_type.clone(),
            })))
        }
        Guard::Range { .. }
        | Guard::With { .. }
        | Guard::Default { .. }
        | Guard::Not { .. }
        | Guard::Or { .. }
        | Guard::AnyOf { .. } => None,
    }
}

fn predicate_is_self_guarding(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(
            Guard::Truthy { path }
                | Guard::Eq { path, .. }
                | Guard::Range { path }
                | Guard::With { path }
                | Guard::Default { path }
        ) if path == source_expr
    )
}

/// A nested range over each member of `parent` (`p.*` ranged): members
/// must be rangeable wherever the outer conditions hold.
fn record_member_range_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    parent: &str,
    predicates: &[Predicate],
) {
    let mut outer_guards = Vec::new();
    for predicate in predicates {
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path }) if path == parent || path == &format!("{parent}.*")
        ) {
            continue;
        }
        if let Some(guard) = predicate_to_guard(predicate, None) {
            outer_guards.push(guard);
        }
    }
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        per_member: true,
        requirements: vec![FailValueRequirement::Iterable {
            allow_integer: false,
        }],
    };
    let acc = path_accumulator(paths, parent);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

/// Whether a conjunct tests the TYPE of `source_expr`, positively or under
/// negation.
fn predicate_tests_source_type(predicate: &Predicate, source_expr: &str) -> bool {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, .. }) => path == source_expr,
        Predicate::Not(inner) => predicate_tests_source_type(inner, source_expr),
        Predicate::And(items) | Predicate::Or(items) => items
            .iter()
            .any(|item| predicate_tests_source_type(item, source_expr)),
        Predicate::True | Predicate::False | Predicate::Guard(_) => false,
    }
}

/// Whether every leaf of `predicate` is a type test on `target_value_path`
/// itself: such a predicate partitions the row's own domain instead of
/// conditioning it on other paths.
fn predicate_is_self_type_partition(predicate: &Predicate, target_value_path: &str) -> bool {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, .. }) => path == target_value_path,
        Predicate::Not(inner) => predicate_is_self_type_partition(inner, target_value_path),
        Predicate::And(items) | Predicate::Or(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| predicate_is_self_type_partition(item, target_value_path))
        }
        Predicate::True | Predicate::False | Predicate::Guard(_) => false,
    }
}

fn predicate_is_positive_header(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::TypeIs { path, .. }) if path == source_expr
    )
}

fn lowerable_guard_path(path: &str, target_value_path: &str) -> Option<String> {
    (!path_contains_wildcard(path) && path != target_value_path).then(|| path.to_string())
}

fn path_contains_wildcard(path: &str) -> bool {
    helm_schema_core::split_value_path(path)
        .iter()
        .any(|segment| segment == "*")
}

/// All strict ancestors of the referenced paths, the subset whose
/// descendant continues through a `*` item segment (a ranged collection's
/// element rows, as opposed to a literal member read), and the subset
/// whose `*` descendant continues INTO element structure (`p.*.field`) —
/// a bare `p.*` value row proves no LIST shape, since `range` iterates
/// maps too.
fn collect_paths_with_descendants(
    paths: &BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    let mut ancestors = BTreeSet::new();
    let mut item_ancestors = BTreeSet::new();
    let mut structured_item_ancestors = BTreeSet::new();
    for path in paths {
        let segments = helm_schema_core::split_value_path(path);
        for prefix_len in 1..segments.len() {
            let ancestor = helm_schema_core::join_value_path(&segments[..prefix_len]);
            if segments[prefix_len] == "*" {
                item_ancestors.insert(ancestor.clone());
                if prefix_len + 1 < segments.len() {
                    structured_item_ancestors.insert(ancestor.clone());
                }
            }
            ancestors.insert(ancestor);
        }
    }
    (ancestors, item_ancestors, structured_item_ancestors)
}
