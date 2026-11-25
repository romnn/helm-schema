use std::borrow::Cow;
use std::collections::HashMap;

use super::{
    BTreeMap, BTreeSet, ConditionalGuard, ConditionalPathOverlay, EmissionClass, EmissionReport,
    GuardValue, LoweredConjunct, NestedGuardScope, PathSchemaResolver, ResolvedPathSchema,
    SchemaDocument, SchemaNode, Value, YamlValue, common_prefix_len, evaluate_guard_set_on_values,
    guard_encodes_fully,
};
use helm_schema_core::{
    ContractRequirementImplication, GuardConjunction, RequirementCondition,
    RequirementConditionBuilder, RequirementConditionNode, RequirementDecision, ValuesPath,
};

use crate::condition_encoding::{AbsenceDefaults, ConditionFragmentCache, EncodedConditionPlan};
use crate::emission_policy::EmissionOrigin;
use crate::schema_tree::{CanonicalConstraintApplication, CanonicalConstraintOutcome};
use crate::terminal_conditions::{
    ProofGuardRole, TerminalContext, TerminalEmissions, partition_requirement,
};
use crate::uncoalesced_root::{
    GuardEncoding, LintGuardPaths, LintRequirement, UncoalescedRootGate,
};
use crate::values_yaml::yaml_value_at_values_path;

pub(super) fn resolve_overlay_target_schema(
    target_value_path: &ValuesPath,
    overlay: &ConditionalPathOverlay,
    provider_resolutions: &crate::provider_resolution::ProviderSchemaResolutions,
) -> (
    ResolvedPathSchema,
    crate::generation_decisions::PathResolution,
) {
    let evidence = overlay.evidence.as_path_evidence();
    PathSchemaResolver::resolve_single_path_evidence(
        target_value_path,
        &evidence,
        provider_resolutions,
    )
}

pub(super) fn partition_guard_scopes(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Option<(Vec<ConditionalGuard>, Vec<NestedGuardScope>)> {
    let mut outer_guards = Vec::new();
    let mut nested: BTreeMap<Vec<String>, Vec<ConditionalGuard>> = BTreeMap::new();

    for guard in guards {
        let mut member_anchor = None;
        let mut saw_document_path = false;
        for path in guard.value_paths() {
            let segments = path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
            let Some(last_wildcard) = segments.iter().rposition(|segment| segment == "*") else {
                saw_document_path = true;
                continue;
            };
            let anchor = segments.get(..=last_wildcard)?.to_vec();
            if !target_segments.starts_with(&anchor)
                || member_anchor
                    .as_ref()
                    .is_some_and(|existing| existing != &anchor)
            {
                return None;
            }
            member_anchor = Some(anchor);
        }

        let Some(member_anchor) = member_anchor else {
            outer_guards.push(guard.clone());
            continue;
        };
        // One Boolean guard cannot read both a ranged member and an outer
        // document path from inside that member: JSON Schema has no upward
        // navigation. Expression lowering keeps ordinary conjunctions as
        // separate guards, so only genuinely inseparable formulas abstain.
        if saw_document_path {
            return None;
        }
        nested.entry(member_anchor).or_default().push(guard.clone());
    }

    let mut nested_guard_scopes = nested
        .into_iter()
        .map(|(ancestor_segments, guards)| NestedGuardScope {
            ancestor_segments,
            guards,
        })
        .collect::<Vec<_>>();
    nested_guard_scopes.sort_by_key(|scope| scope.ancestor_segments.len());
    if nested_guard_scopes.windows(2).any(|scopes| {
        let [outer, inner] = scopes else {
            return false;
        };
        !inner
            .ancestor_segments
            .starts_with(&outer.ancestor_segments)
    }) {
        return None;
    }

    Some((outer_guards, nested_guard_scopes))
}

pub(super) fn conditional_ancestor_segments(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Vec<String> {
    let mut shared_prefix = target_segments.to_vec();
    for guard in guards {
        for guard_path in guard.value_paths() {
            let guard_path = guard_path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
            shared_prefix.truncate(common_prefix_len(&shared_prefix, &guard_path));
        }
    }
    shared_prefix
}

pub(super) fn guards_supported_for_conditional_lowering(
    guards: &[ConditionalGuard],
    resolved_by_path: &BTreeMap<&ValuesPath, &ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    guards_supported_with_self_path(guards, None, resolved_by_path, values_yaml_doc)
}

/// Fail-implication guard support is more permissive than overlay guard
/// support on TWO axes, both bounded by the arm-only shape (an implication
/// adds an `if guards then requirement` arm and never contributes rows or
/// base structure, so a guard that never fires costs nothing):
/// - a truthy guard over the implication's OWN target path is the
///   capture's structurally derived test subject (`if truthy(x) then x is
///   a string`), not a decoded ambient condition, so the fabricated-path
///   concern does not apply to it even when the chart never declares it;
/// - truthy guards over other undeclared-but-resolved paths lower
///   type-generically: the requirement is a hard render failure, and a
///   fabricated guard path merely leaves the arm inactive.
fn implication_guards_supported<'guard>(
    guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
    target_value_path: &ValuesPath,
    resolved_by_path: &BTreeMap<&ValuesPath, &ResolvedPathSchema>,
) -> bool {
    let mut guards = guards.into_iter().peekable();
    guards.peek().is_some()
        && guards.all(|guard| match guard {
            // The values ROOT owns no resolved path entry, but its
            // truthiness encodes at the document node itself. Rejecting it
            // here would drop the WHOLE implication — every sibling arm
            // with it — because an unsupported guard is fatal to the
            // any-of, which is how a root-scoped `with` cost nats the
            // member-host typing of the five hosts it navigates.
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                path.segments().next().is_none()
                    || path == target_value_path
                    || resolved_by_path.contains_key(path)
            }
            ConditionalGuard::Decision(_)
            | ConditionalGuard::Requirement(_)
            | ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::MatchesValue {
                path: _,
                pattern: _,
            }
            | ConditionalGuard::CollectionPattern {
                path: _,
                domain: _,
                quantifier: _,
                pattern: _,
            }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => true,
            ConditionalGuard::Not(inner) => implication_guards_supported(
                std::iter::once(inner.as_ref()),
                target_value_path,
                resolved_by_path,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                implication_guards_supported(guards.iter(), target_value_path, resolved_by_path)
            }
        })
}

pub(super) fn admit_implication<'a>(
    implication: &'a ContractRequirementImplication,
    target: &ValuesPath,
    resolved: &BTreeMap<&ValuesPath, &ResolvedPathSchema>,
) -> Option<Cow<'a, ContractRequirementImplication>> {
    let guards = &implication.outer_guards;
    let supports = |guard: &ConditionalGuard| {
        implication_guards_supported(std::iter::once(guard), target, resolved)
    };
    if !guards.iter().all(supports) {
        return None;
    }
    if requirement_guard(guards).is_none() {
        return Some(Cow::Borrowed(implication));
    }
    let mut changed = false;
    let mut admitted = Vec::new();
    for guard in guards {
        if let ConditionalGuard::Requirement(graph) = guard
            && let RequirementAdmission::Narrowed(narrowed) =
                supported_requirement(graph, supports)?
        {
            changed = true;
            admitted.push(ConditionalGuard::Requirement(narrowed));
        } else {
            admitted.push(guard.clone());
        }
    }
    if !changed {
        return Some(Cow::Borrowed(implication));
    }
    let mut updated = implication.clone();
    updated.outer_guards = admitted.into();
    Some(Cow::Owned(updated))
}

enum RequirementAdmission {
    Unchanged,
    Narrowed(RequirementCondition),
}

#[derive(Clone, Copy)]
pub(super) struct ImplicationScope {
    pub(super) has_guarded_proof: bool,
    pub(super) any_self_truthy: bool,
    pub(super) any_self_presence: bool,
    pub(super) all_self_presence: bool,
}

impl ImplicationScope {
    const UNCONDITIONAL: Self = Self {
        has_guarded_proof: false,
        any_self_truthy: false,
        any_self_presence: false,
        all_self_presence: false,
    };
    const IMPOSSIBLE: Self = Self {
        all_self_presence: true,
        ..Self::UNCONDITIONAL
    };

    fn guard(guard: &ConditionalGuard, target: &ValuesPath) -> Self {
        let presence = guard.is_self_presence_for(target);
        Self {
            has_guarded_proof: true,
            any_self_truthy: guard.is_self_truthy_for(target),
            any_self_presence: presence,
            all_self_presence: presence,
        }
    }

    fn and(self, other: Self) -> Self {
        Self {
            has_guarded_proof: self.has_guarded_proof || other.has_guarded_proof,
            any_self_truthy: self.any_self_truthy || other.any_self_truthy,
            any_self_presence: self.any_self_presence || other.any_self_presence,
            all_self_presence: self.all_self_presence || other.all_self_presence,
        }
    }

    fn or(self, other: Self) -> Self {
        Self {
            has_guarded_proof: self.has_guarded_proof || other.has_guarded_proof,
            any_self_truthy: self.any_self_truthy || other.any_self_truthy,
            any_self_presence: self.any_self_presence || other.any_self_presence,
            all_self_presence: self.all_self_presence && other.all_self_presence,
        }
    }
}

pub(super) fn implication_scope(
    guards: &GuardConjunction,
    target: &ValuesPath,
) -> ImplicationScope {
    guards
        .iter()
        .fold(ImplicationScope::UNCONDITIONAL, |scope, guard| {
            let guard_scope = match guard {
                ConditionalGuard::Requirement(graph) => {
                    requirement_implication_scope(graph, target)
                        .unwrap_or(ImplicationScope::UNCONDITIONAL)
                }
                _ => ImplicationScope::guard(guard, target),
            };
            scope.and(guard_scope)
        })
}

fn requirement_implication_scope(
    graph: &RequirementCondition,
    target: &ValuesPath,
) -> Option<ImplicationScope> {
    let mut scopes = HashMap::<usize, ImplicationScope>::new();
    for id in graph.reachable_ids() {
        let scope = match graph.node(id)? {
            RequirementConditionNode::False => ImplicationScope::IMPOSSIBLE,
            RequirementConditionNode::True => ImplicationScope::UNCONDITIONAL,
            RequirementConditionNode::Guard(guard) => ImplicationScope::guard(guard, target),
            RequirementConditionNode::And(children) => children
                .iter()
                .try_fold(ImplicationScope::UNCONDITIONAL, |scope, child| {
                    Some(scope.and(*scopes.get(child)?))
                })?,
            RequirementConditionNode::Or(children) => children
                .iter()
                .try_fold(ImplicationScope::IMPOSSIBLE, |scope, child| {
                    Some(scope.or(*scopes.get(child)?))
                })?,
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => {
                let yes = if *when_true == RequirementConditionBuilder::FALSE {
                    ImplicationScope::IMPOSSIBLE
                } else {
                    scopes
                        .get(when_true)?
                        .and(ImplicationScope::guard(test, target))
                };
                let no = if *when_false == RequirementConditionBuilder::FALSE {
                    ImplicationScope::IMPOSSIBLE
                } else {
                    scopes
                        .get(when_false)?
                        .and(negative_implication_scope(test, target))
                };
                yes.or(no)
            }
        };
        scopes.insert(id, scope);
    }
    scopes.get(&graph.root()).copied()
}

fn negative_implication_scope(test: &ConditionalGuard, target: &ValuesPath) -> ImplicationScope {
    let negative = ConditionalGuard::Not(std::sync::Arc::new(test.clone()));
    if let ConditionalGuard::Decision(graph) = test
        && let [
            RequirementDecision {
                test,
                when_true,
                when_false,
            },
        ] = graph.decisions()
    {
        match (*when_true, *when_false) {
            (0, 1) => return ImplicationScope::guard(test, target),
            (1, 0) => {
                return ImplicationScope::guard(
                    &ConditionalGuard::Not(std::sync::Arc::new(test.clone())),
                    target,
                );
            }
            _ => {}
        }
    }
    ImplicationScope::guard(&negative, target)
}

fn supported_requirement(
    graph: &RequirementCondition,
    supports: impl Fn(&ConditionalGuard) -> bool,
) -> Option<RequirementAdmission> {
    let order = graph.reachable_ids();
    if order.iter().all(|&id| match graph.node(id) {
        Some(RequirementConditionNode::Guard(guard)) => supports(guard),
        _ => true,
    }) {
        return Some(RequirementAdmission::Unchanged);
    }
    let mut builder = RequirementConditionBuilder::default();
    let mut mapped = HashMap::new();
    for id in order {
        let root = match graph.node(id)? {
            RequirementConditionNode::True => RequirementConditionBuilder::TRUE,
            RequirementConditionNode::Guard(guard) if supports(guard) => {
                builder.guard(guard.clone())
            }
            RequirementConditionNode::False | RequirementConditionNode::Guard(_) => {
                RequirementConditionBuilder::FALSE
            }
            RequirementConditionNode::And(children) => builder.and(
                children
                    .iter()
                    .map(|child| mapped.get(child).copied())
                    .collect::<Option<Vec<_>>>()?,
            ),
            RequirementConditionNode::Or(children) => builder.union(
                children
                    .iter()
                    .map(|child| mapped.get(child).copied())
                    .collect::<Option<Vec<_>>>()?,
            ),
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => builder.split(
                test.clone(),
                *mapped.get(when_true)?,
                *mapped.get(when_false)?,
            ),
        };
        mapped.insert(id, root);
    }
    let root = *mapped.get(&graph.root())?;
    (root != RequirementConditionBuilder::FALSE)
        .then(|| RequirementAdmission::Narrowed(builder.freeze().root(root, false)))
}

fn guards_supported_with_self_path(
    guards: &[ConditionalGuard],
    self_path: Option<&ValuesPath>,
    resolved_by_path: &BTreeMap<&ValuesPath, &ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            // The truthiness condition encoding is type-generic (const true,
            // non-zero number, non-empty string/array/object). Approximate
            // lookups never reach conditional overlays, so every resolved
            // guard path here is structural evidence even when values.yaml
            // does not declare the finite member (literal-dict range keys).
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                self_path.is_some_and(|self_path| path == self_path)
                    || yaml_value_at_values_path(values_yaml_doc, path).is_some()
                    || resolved_by_path.contains_key(path)
            }
            ConditionalGuard::Decision(_)
            | ConditionalGuard::Requirement(_)
            | ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::MatchesValue {
                path: _,
                pattern: _,
            }
            | ConditionalGuard::CollectionPattern {
                path: _,
                domain: _,
                quantifier: _,
                pattern: _,
            }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_with_self_path(
                std::slice::from_ref(inner),
                self_path,
                resolved_by_path,
                values_yaml_doc,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                guards_supported_with_self_path(
                    guards,
                    self_path,
                    resolved_by_path,
                    values_yaml_doc,
                )
            }
        })
}

/// Lowers terminating validator formulas into `if <guards> then false` clauses.
///
/// No valid values document satisfies all of a clause's guards.
/// Clauses anchor at their shared ancestor, or at the root when a missing ancestor
/// supplies no coalesced instance for the lint comparison.
/// Any unencodable guard skips the whole clause: a partially encoded `if` would
/// reject documents the validator never terminates.
#[tracing::instrument(skip_all, fields(guarded_default_sources = signals.guarded_values_default_sources().len()))]
pub(crate) fn append_terminal_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[&GuardConjunction],
    signals: &helm_schema_core::ContractSchemaSignals,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
    predicate_memo: &helm_schema_core::PredicateMemo,
    gate: &mut UncoalescedRootGate<'_>,
) {
    let prepared = prepare_terminal_clauses(clauses, documents, gate);
    append_values_default_source_absence_clauses(
        root_schema,
        &prepared,
        signals,
        documents,
        dependency_roots,
        predicate_memo,
        gate,
    );
    append_guarded_values_default_source_clauses(
        root_schema,
        &prepared,
        signals,
        documents,
        dependency_roots,
        predicate_memo,
        gate,
    );
    append_deleted_dependency_root_clauses(
        root_schema,
        &prepared,
        documents,
        dependency_roots,
        predicate_memo,
        gate,
    );
    // Availability probes must not reserve names in the emitted condition definitions.
    let mut encodings = ConditionFragmentCache::default();
    let mut paths = crate::terminal_conditions::TerminalPathCache::default();
    let mut emissions = TerminalEmissions::default();
    let rows_span = tracing::info_span!("prepare_terminal_rows", rows = clauses.len());
    let rows_scope = rows_span.enter();
    for (row_index, entry) in prepared.iter().enumerate() {
        if row_index.is_multiple_of(4_096)
            && tracing::enabled!(target: "helm_schema_gen::counters", tracing::Level::INFO)
        {
            emissions.record_storage();
            gate.record_storage();
        }
        let guard_row = entry.guards.as_ref();
        let guards = guard_row;
        let (_, values_yaml_doc, absence) =
            entry.condition_context(documents, dependency_roots, predicate_memo);
        let (shared_ancestor, all_vacuous) = entry.context.as_ref().map_or_else(
            || {
                (
                    paths.ancestor(guards, values_yaml_doc),
                    guards.iter().all(guard_holds_vacuously),
                )
            },
            |context| {
                (
                    context.ancestor(values_yaml_doc),
                    !context.requires_presence(),
                )
            },
        );
        // Keep present values attributed to their nearest shared object.
        // A separate root clause evaluates the original formula only while
        // that object is missing; retaining every guard matters because a
        // negated presence test can make the formula false there.
        let split_vacuous_ancestor = all_vacuous
            && !shared_ancestor.is_empty()
            && !shared_ancestor.iter().any(|segment| segment == "*");
        let ancestor_segments = if split_vacuous_ancestor {
            shared_ancestor.clone()
        } else if all_vacuous {
            Vec::new()
        } else {
            shared_ancestor
        };
        if !encodings.conjunction_encodes_fully(
            guards,
            &ancestor_segments,
            values_yaml_doc,
            absence,
        ) {
            continue;
        }
        append_terminal_clause(
            &mut emissions,
            gate,
            guard_row,
            &ancestor_segments,
            values_yaml_doc,
            absence,
        );
        if split_vacuous_ancestor {
            append_missing_ancestor_clause(
                &mut emissions,
                gate,
                &mut encodings,
                guard_row,
                &ancestor_segments,
                values_yaml_doc,
                absence,
            );
        }
    }
    drop(rows_scope);
    drop(rows_span);
    emissions.append(root_schema, gate);
}

fn append_missing_ancestor_clause(
    emissions: &mut TerminalEmissions,
    gate: &mut UncoalescedRootGate<'_>,
    encodings: &mut ConditionFragmentCache,
    guards: &GuardConjunction,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    absence: AbsenceDefaults<'_>,
) {
    // Runtime defaults provide the effective subtree when the ancestor is absent.
    // A definitively false guard makes the companion unreachable.
    if evaluate_guard_set_on_values(guards, absence.runtime_defaults) == Some(false) {
        return;
    }
    let path = ValuesPath::from_segments(
        ancestor_segments
            .iter()
            .map(|segment| helm_schema_core::Segment::from_encoded_component(segment)),
    );
    let mut missing_ancestor_guards = guards.clone();
    missing_ancestor_guards.push(ConditionalGuard::Absent { path });
    if encodings.conjunction_encodes_fully(&missing_ancestor_guards, &[], values_yaml_doc, absence)
    {
        append_terminal_clause(
            emissions,
            gate,
            &missing_ancestor_guards,
            &[],
            values_yaml_doc,
            absence,
        );
    }
}

struct PreparedTerminalClause<'a> {
    guards: Cow<'a, GuardConjunction>,
    context: Option<TerminalContext>,
    default_document: GuardDefaultDocument,
}

impl PreparedTerminalClause<'_> {
    fn condition_context<'a>(
        &self,
        documents: &'a crate::emission_plan::RootValuesDocuments,
        dependency_roots: &'a BTreeSet<Vec<String>>,
        predicate_memo: &helm_schema_core::PredicateMemo,
    ) -> (
        Option<usize>,
        &'a YamlValue,
        crate::condition_encoding::AbsenceDefaults<'a>,
    ) {
        self.default_document.condition_context(
            self.guards.as_ref(),
            documents,
            dependency_roots,
            predicate_memo,
        )
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum GuardDefaultDocument {
    #[default]
    Derive,
    Selected(Option<usize>),
}

impl GuardDefaultDocument {
    fn condition_context<'a>(
        self,
        guards: &GuardConjunction,
        documents: &'a crate::emission_plan::RootValuesDocuments,
        dependency_roots: &'a BTreeSet<Vec<String>>,
        predicate_memo: &helm_schema_core::PredicateMemo,
    ) -> (
        Option<usize>,
        &'a YamlValue,
        crate::condition_encoding::AbsenceDefaults<'a>,
    ) {
        match self {
            Self::Selected(index) => documents.condition_context_at(index, dependency_roots),
            Self::Derive => documents.condition_context(guards, dependency_roots, predicate_memo),
        }
    }
}

struct RequirementDefaultRegion {
    condition: RequirementCondition,
    default_document: GuardDefaultDocument,
}

fn requirement_default_regions(
    guards: &GuardConjunction,
    graph: &RequirementCondition,
    documents: &crate::emission_plan::RootValuesDocuments,
) -> Vec<RequirementDefaultRegion> {
    use crate::emission_plan::GuardedDefaultsSelection;

    let activations = documents.guarded_activations();
    if activations.is_empty() {
        return vec![RequirementDefaultRegion {
            condition: graph.clone(),
            default_document: GuardDefaultDocument::Selected(None),
        }];
    }
    let extras = helm_schema_core::Predicate::all(
        guards_outside_requirement(guards, graph)
            .map(ConditionalGuard::predicate)
            .collect(),
    );
    let Some(regions) =
        crate::requirement_activation::partition_activation(graph, &extras, &activations)
    else {
        return Vec::new();
    };
    regions
        .into_iter()
        .filter_map(|region| {
            match documents.selected_guarded_defaults(&region.active, &region.unknown) {
                GuardedDefaultsSelection::Selected(index) => Some(RequirementDefaultRegion {
                    condition: region.condition,
                    default_document: GuardDefaultDocument::Selected(index),
                }),
                GuardedDefaultsSelection::Unresolved => {
                    let _bound = tracing::debug_span!(
                    target: "helm_schema_gen::counters", "requirement_default_context_unresolved",
                ).entered();
                    None
                }
            }
        })
        .collect()
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct TerminalRequirementContext {
    shape: TerminalContext,
    lint: LintGuardPaths,
}

impl TerminalRequirementContext {
    fn join(&self, other: &Self) -> Self {
        Self {
            shape: self.shape.join(&other.shape),
            lint: self.lint.join(&other.lint),
        }
    }
}

fn prepare_terminal_clauses<'a>(
    clauses: &[&'a GuardConjunction],
    documents: &crate::emission_plan::RootValuesDocuments,
    gate: &UncoalescedRootGate<'_>,
) -> Vec<PreparedTerminalClause<'a>> {
    let mut prepared = Vec::new();
    let observe = |guard: &ConditionalGuard, role: ProofGuardRole| TerminalRequirementContext {
        shape: TerminalContext::of_guard(guard, !guard_holds_vacuously(guard), role),
        lint: gate.lint_guard_paths(guard),
    };
    for &clause in clauses {
        let Some(graph) = requirement_guard(clause) else {
            prepared.push(PreparedTerminalClause {
                guards: Cow::Borrowed(clause),
                context: None,
                default_document: GuardDefaultDocument::Derive,
            });
            continue;
        };
        // A sharing barrier applies to the original proof, including every successor.
        if !gate.supports_scope_guards(clause) {
            continue;
        }
        let extra_context = guards_outside_requirement(clause, graph)
            .fold(TerminalRequirementContext::default(), |context, guard| {
                context.join(&observe(guard, ProofGuardRole::Leaf))
            });
        for region in requirement_default_regions(clause, graph, documents) {
            let Some(partitions) =
                partition_requirement(&region.condition, observe, TerminalRequirementContext::join)
            else {
                continue;
            };
            for partition in partitions {
                prepared.push(PreparedTerminalClause {
                    guards: Cow::Owned(with_requirement_partition(
                        clause,
                        graph,
                        partition.condition,
                    )),
                    context: Some(partition.context.shape.join(&extra_context.shape)),
                    default_document: region.default_document,
                });
            }
        }
    }
    prepared
}

fn requirement_guard(clause: &GuardConjunction) -> Option<&RequirementCondition> {
    clause.iter().find_map(|guard| match guard {
        ConditionalGuard::Requirement(graph) => Some(graph),
        _ => None,
    })
}

fn guards_outside_requirement<'a>(
    clause: &'a GuardConjunction,
    graph: &RequirementCondition,
) -> impl Iterator<Item = &'a ConditionalGuard> {
    clause.iter().filter(move |guard| {
        !matches!(guard, ConditionalGuard::Requirement(other) if std::ptr::eq(graph, other))
    })
}

fn with_requirement_partition(
    clause: &GuardConjunction,
    graph: &RequirementCondition,
    condition: RequirementCondition,
) -> GuardConjunction {
    let guard = match condition.node(condition.root()) {
        Some(RequirementConditionNode::True) => None,
        Some(RequirementConditionNode::Guard(guard)) => Some(guard.clone()),
        _ => Some(ConditionalGuard::Requirement(condition)),
    };
    guards_outside_requirement(clause, graph)
        .cloned()
        .chain(guard)
        .collect::<Vec<_>>()
        .into()
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ScopedGuardRow {
    guards: GuardConjunction,
    default_document: GuardDefaultDocument,
}

fn conditional_guard_rows(
    guards: GuardConjunction,
    documents: &crate::emission_plan::RootValuesDocuments,
    gate: &UncoalescedRootGate<'_>,
) -> Vec<ScopedGuardRow> {
    let Some(graph) = requirement_guard(&guards) else {
        return vec![ScopedGuardRow {
            guards,
            default_document: GuardDefaultDocument::Derive,
        }];
    };
    if !gate.supports_scope_guards(&guards) {
        return Vec::new();
    }
    let observe = |guard: &ConditionalGuard, _role| gate.lint_guard_paths(guard);
    let mut rows = Vec::new();
    for region in requirement_default_regions(&guards, graph, documents) {
        let Some(partitions) =
            partition_requirement(&region.condition, observe, LintGuardPaths::join)
        else {
            continue;
        };
        rows.extend(partitions.into_iter().map(|partition| ScopedGuardRow {
            guards: with_requirement_partition(&guards, graph, partition.condition),
            default_document: region.default_document,
        }));
    }
    rows
}

/// A deleted dependency values root is not the document minus a key: helm
/// recreates the table from the SUBCHART's own values, so a clause whose
/// guards all hold against that refill terminates every document missing the
/// root — the half a clause anchored inside the root cannot reach. One clause
/// per root states it.
#[tracing::instrument(skip_all)]
fn append_deleted_dependency_root_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[PreparedTerminalClause<'_>],
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
    predicate_memo: &helm_schema_core::PredicateMemo,
    gate: &mut UncoalescedRootGate<'_>,
) {
    let mut deleted_roots = BTreeSet::new();
    let mut paths = crate::terminal_conditions::TerminalPathCache::default();
    for entry in clauses {
        let guards = entry.guards.as_ref();
        let (_, _, absence) = entry.condition_context(documents, dependency_roots, predicate_memo);
        if let Some(root) = paths.deleted_dependency_root(guards, absence) {
            deleted_roots.insert(root);
        }
    }
    for root in deleted_roots {
        let Some(condition) = crate::condition_encoding::dependency_root_gone_condition(root)
        else {
            continue;
        };
        let guards = [ConditionalGuard::Absent {
            path: ValuesPath::from_segments(
                root.iter()
                    .map(|segment| helm_schema_core::Segment::from_encoded_component(segment)),
            ),
        }];
        let (_, values_yaml_doc, absence) =
            documents.condition_context(&guards, dependency_roots, predicate_memo);
        append_unless_withdrawn(
            root_schema,
            gate,
            &[],
            &guards,
            condition,
            GuardEncoding {
                values_yaml_doc,
                absence,
                polarity: crate::condition_encoding::ConditionPolarity::Narrow,
            },
        );
    }
}

#[tracing::instrument(skip_all)]
fn append_values_default_source_absence_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[PreparedTerminalClause<'_>],
    signals: &helm_schema_core::ContractSchemaSignals,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
    predicate_memo: &helm_schema_core::PredicateMemo,
    gate: &mut UncoalescedRootGate<'_>,
) {
    for entry in clauses {
        let guards = entry.guards.as_ref();
        if guards.len() != 1 {
            continue;
        }
        let Some(ConditionalGuard::Absent { path }) = guards.iter().next() else {
            continue;
        };
        let Some(source_path) =
            unique_values_default_source_path(path, signals.values_default_sources())
        else {
            continue;
        };
        let Some(target_absent) = crate::condition_encoding::input_path_absent_condition(path)
        else {
            continue;
        };
        let Some(source_absent) =
            crate::condition_encoding::input_path_absent_condition(&source_path)
        else {
            continue;
        };
        let (_, values_yaml_doc, absence) =
            entry.condition_context(documents, dependency_roots, predicate_memo);
        append_unless_withdrawn(
            root_schema,
            gate,
            &[],
            &[
                ConditionalGuard::Absent { path: path.clone() },
                ConditionalGuard::Absent { path: source_path },
            ],
            SchemaNode::all_of(vec![target_absent, source_absent]),
            GuardEncoding {
                values_yaml_doc,
                absence,
                polarity: crate::condition_encoding::ConditionPolarity::Narrow,
            },
        );
    }
}

#[tracing::instrument(skip_all)]
fn append_guarded_values_default_source_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[PreparedTerminalClause<'_>],
    signals: &helm_schema_core::ContractSchemaSignals,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
    predicate_memo: &helm_schema_core::PredicateMemo,
    gate: &mut UncoalescedRootGate<'_>,
) {
    for entry in clauses {
        let guards = entry.guards.as_ref();
        let absent_paths = guards
            .iter()
            .filter_map(|guard| match guard {
                ConditionalGuard::Absent { path } => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        if absent_paths.is_empty() {
            continue;
        }
        for fact in signals.guarded_values_default_sources() {
            // An unrelated row cannot produce a default-source absence clause.
            // Prove that eligibility before retaining a whole-row implication query.
            let Some((path, source_path)) = absent_paths.iter().find_map(|&path| {
                source_path_for_effective(path, &fact.source).map(|source| (path, source))
            }) else {
                continue;
            };
            let Some(target_absent) = crate::condition_encoding::input_path_absent_condition(path)
            else {
                continue;
            };
            let Some(guards) =
                active_source_guards(entry, documents, &fact.outer_guards, predicate_memo)
            else {
                continue;
            };
            let Some(source_absent) =
                crate::condition_encoding::input_path_absent_condition(&source_path)
            else {
                continue;
            };
            let remaining_guards = guards
                .iter()
                .filter(|guard| *guard != &ConditionalGuard::Absent { path: path.clone() })
                .cloned()
                .collect::<Vec<_>>();
            let (_, values_yaml_doc, absence) =
                entry.condition_context(documents, dependency_roots, predicate_memo);
            let mut conditions = gate.condition_clauses(
                &remaining_guards,
                &[],
                values_yaml_doc,
                absence,
                crate::condition_encoding::ConditionPolarity::Narrow,
            );
            conditions.push(target_absent);
            conditions.push(source_absent);
            let mut judged_guards = remaining_guards;
            judged_guards.push(ConditionalGuard::Absent { path: path.clone() });
            judged_guards.push(ConditionalGuard::Absent { path: source_path });
            append_unless_withdrawn(
                root_schema,
                gate,
                &[],
                &judged_guards,
                SchemaNode::all_of(conditions),
                GuardEncoding {
                    values_yaml_doc,
                    absence,
                    polarity: crate::condition_encoding::ConditionPolarity::Narrow,
                },
            );
        }
    }
}

fn active_source_guards<'a>(
    entry: &'a PreparedTerminalClause<'_>,
    documents: &crate::emission_plan::RootValuesDocuments,
    source_activation: &[ConditionalGuard],
    predicate_memo: &helm_schema_core::PredicateMemo,
) -> Option<Cow<'a, GuardConjunction>> {
    let guards = entry.guards.as_ref();
    if let GuardDefaultDocument::Selected(Some(selected)) = entry.default_document
        && documents.guarded_source_index(source_activation) == Some(selected)
    {
        return Some(Cow::Borrowed(guards));
    }
    let activation = helm_schema_core::Predicate::all(
        source_activation
            .iter()
            .map(ConditionalGuard::predicate)
            .collect(),
    );
    if let Some(graph) = requirement_guard(guards) {
        let extras = helm_schema_core::Predicate::all(
            guards_outside_requirement(guards, graph)
                .map(ConditionalGuard::predicate)
                .collect(),
        );
        let source = crate::requirement_activation::GuardedActivation {
            predicate: activation,
            guard_count: source_activation.len(),
        };
        let active =
            crate::requirement_activation::partition_activation(graph, &extras, &[source])?
                .into_iter()
                .find(|region| region.active.contains(&0))?;
        return Some(Cow::Owned(with_requirement_partition(
            guards,
            graph,
            active.condition,
        )));
    }
    let predicate =
        helm_schema_core::Predicate::all(guards.iter().map(ConditionalGuard::predicate).collect());
    predicate_memo
        .exactly_implies(&predicate, &activation)
        .then_some(Cow::Borrowed(guards))
}

fn source_path_for_effective(
    effective_path: &ValuesPath,
    source: &helm_schema_core::ValuesDefaultSource,
) -> Option<ValuesPath> {
    let target_path = source.target_path.clone();
    if effective_path != &target_path && !effective_path.is_descendant_of(&target_path) {
        return None;
    }
    let mut source_segments = source.source_path.segments().cloned().collect::<Vec<_>>();
    source_segments.extend(
        effective_path
            .segments()
            .skip(target_path.segments().len())
            .cloned(),
    );
    Some(ValuesPath::from_segments(source_segments))
}

fn unique_values_default_source_path(
    effective_path: &ValuesPath,
    sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
) -> Option<ValuesPath> {
    let mut source_paths = sources
        .iter()
        .filter_map(|source| {
            let target_path = source.target_path.clone();
            if effective_path != &target_path && !effective_path.is_descendant_of(&target_path) {
                return None;
            }
            let mut source_segments = source.source_path.segments().cloned().collect::<Vec<_>>();
            source_segments.extend(
                effective_path
                    .segments()
                    .skip(target_path.segments().len())
                    .cloned(),
            );
            Some(ValuesPath::from_segments(source_segments))
        })
        .collect::<BTreeSet<_>>();
    if source_paths.len() == 1 {
        source_paths.pop_first()
    } else {
        None
    }
}

/// Whether the guard can be satisfied with its path (or an ancestor)
/// absent from the document.
fn guard_holds_vacuously(guard: &ConditionalGuard) -> bool {
    match guard {
        ConditionalGuard::Truthy { .. }
        | ConditionalGuard::With { .. }
        | ConditionalGuard::TypeIs { .. }
        | ConditionalGuard::MatchesPattern { .. }
        | ConditionalGuard::MatchesValue {
            path: _,
            pattern: _,
        }
        | ConditionalGuard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | ConditionalGuard::IntGt { .. }
        | ConditionalGuard::IntLt { .. }
        | ConditionalGuard::HasKey { .. }
        | ConditionalGuard::ContainsMemberEquals { .. }
        | ConditionalGuard::ContainsTruthyMember { .. }
        | ConditionalGuard::ContainsEquals { .. }
        | ConditionalGuard::MinMembers { .. } => false,
        ConditionalGuard::Eq { value, .. } => matches!(value, GuardValue::Null),
        ConditionalGuard::Decision(_)
        | ConditionalGuard::Requirement(_)
        | ConditionalGuard::NotEq { .. }
        | ConditionalGuard::Absent { .. }
        | ConditionalGuard::AtMostOneMember { .. }
        | ConditionalGuard::Not(_) => true,
        ConditionalGuard::AllOf(inner) => inner.iter().all(guard_holds_vacuously),
        ConditionalGuard::AnyOf(inner) => inner.iter().any(guard_holds_vacuously),
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConditionalHostPreparation {
    relaxed_host_paths: BTreeSet<Vec<String>>,
}

impl ConditionalHostPreparation {
    pub(crate) fn apply(&self, root_schema: &mut SchemaDocument) {
        for path in &self.relaxed_host_paths {
            root_schema.relax_host_object_type(path);
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn prepare_conditional_hosts(
    conditionals: &[LoweredConjunct],
) -> ConditionalHostPreparation {
    let mut preparation = ConditionalHostPreparation::default();
    // Nil-safe member hosts drop the structural `type: object` their
    // descendants materialized. This is policy-free support: every
    // projection starts from the same relaxed base, whether or not it keeps
    // the presence-guarded arm carrying the exact contract.
    for conditional in conditionals {
        if conditional.carrier.relax_untyped_host && !conditional.schema.is_empty_schema() {
            let mut segments = conditional.carrier.ancestor_segments.clone();
            segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
            preparation.relaxed_host_paths.insert(segments);
        }
    }
    preparation
}

#[tracing::instrument(skip_all, fields(conditionals = conditionals.len(), guard_groups = tracing::field::Empty, content_groups = tracing::field::Empty, max_content_guards = tracing::field::Empty))]
#[expect(
    clippy::too_many_lines,
    reason = "keeping conditional grouping and emission together makes the equivalence rewrites auditable"
)]
pub(crate) fn append_selected_constraints(
    root_schema: &mut SchemaDocument,
    conditionals: Vec<LoweredConjunct>,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
    report: &mut EmissionReport,
    predicate_memo: &helm_schema_core::PredicateMemo,
    gate: &mut UncoalescedRootGate<'_>,
) {
    // Conditionals sharing one guard set and scope conjoin into one if/then:
    // `allOf [{if G then A}, {if G then B}]` is `{if G then A ∧ B}`, and the
    // repeated `if` blocks dominate emitted size on charts with many guarded
    // blocks. Distinct targets merge disjointly; a leaf collision falls back
    // to its own conditional.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct GuardGroupKey {
        ancestor_segments: Vec<String>,
        row: ScopedGuardRow,
    }
    #[expect(
        clippy::mutable_key_type,
        reason = "the guard prefix OnceLock memoizes the canonical order of immutable guards"
    )]
    let mut grouped: BTreeMap<GuardGroupKey, Vec<LoweredConjunct>> = BTreeMap::new();
    let grouping_span = tracing::info_span!(
        "group_conditional_rows",
        groups = tracing::field::Empty,
        allocated_slots = tracing::field::Empty,
        row_bytes = std::mem::size_of::<LoweredConjunct>(),
    )
    .entered();
    for conditional in conditionals {
        // Schema-less conditionals carry base ownership established by a
        // transform or by a separate implication that already emits the
        // complete runtime domain; they have no schema arm to append.
        if conditional.schema.is_empty_schema() {
            if matches!(conditional.class, EmissionClass::Mandatory) {
                report.mandatory_outcomes.redundant += 1;
            }
            continue;
        }
        let mut rows = conditional_guard_rows(conditional.shared_outer_guards(), documents, gate);
        let Some(last) = rows.pop() else {
            continue;
        };
        let ancestor_segments = conditional.carrier.ancestor_segments.clone();
        for row in rows {
            grouped
                .entry(GuardGroupKey {
                    ancestor_segments: ancestor_segments.clone(),
                    row,
                })
                .or_default()
                .push(conditional.clone());
        }
        grouped
            .entry(GuardGroupKey {
                ancestor_segments,
                row: last,
            })
            .or_default()
            .push(conditional);
    }
    grouping_span.record("groups", grouped.len());
    grouping_span.record(
        "allocated_slots",
        grouped.values().map(Vec::capacity).sum::<usize>(),
    );
    drop(grouping_span);
    tracing::Span::current().record("guard_groups", grouped.len());
    struct ContentGroup {
        fragment: Value,
        guard_sets: Vec<ScopedGuardRow>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut by_content: BTreeMap<(Vec<String>, String), ContentGroup> = BTreeMap::new();
    let mut insert_content = |ancestor: &[String], fragment: Value, row, facts, mandatory_facts| {
        let content = by_content
            .entry((
                ancestor.to_vec(),
                helm_schema_json_schema_walk::canonical_json_string(&fragment),
            ))
            .or_insert_with(|| ContentGroup {
                fragment,
                guard_sets: Vec::new(),
                facts: 0,
                mandatory_facts: 0,
            });
        content.guard_sets.push(row);
        content.facts += facts;
        content.mandatory_facts += mandatory_facts;
    };
    for (
        GuardGroupKey {
            ancestor_segments,
            row:
                ScopedGuardRow {
                    guards: guard_row,
                    default_document,
                },
        },
        group,
    ) in grouped
    {
        let (_, values_yaml_doc, absence) = default_document.condition_context(
            &guard_row,
            documents,
            dependency_roots,
            predicate_memo,
        );
        let encoding = GuardEncoding {
            values_yaml_doc,
            absence,
            polarity: crate::condition_encoding::ConditionPolarity::Widen,
        };
        let mut merged: Option<(Value, usize, usize)> = None;
        let mut separate = Vec::new();
        // Each conditional is judged against the lint documents before
        // grouping, so one withdrawn fact never takes its siblings along.
        let mut guard_condition = None;
        for conditional in group {
            let mandatory = usize::from(matches!(conditional.class, EmissionClass::Mandatory));
            if !gate.supports_scope_guards(&guard_row)
                || !gate.requirements_encode_fully(&guard_row, &ancestor_segments, encoding)
                || conditional
                    .nested_guard_scopes()
                    .iter()
                    .any(|scope| !gate.supports_scope_guards(&scope.guards))
            {
                let _abstention = tracing::debug_span!(
                    target: "helm_schema_gen::counters", "condition_scope_encoding_abstention",
                )
                .entered();
                report.mandatory_outcomes.fallback += mandatory;
                continue;
            }
            let Some(fragment) =
                build_scoped_target_fragment(&conditional, values_yaml_doc, absence, gate)
            else {
                report.mandatory_outcomes.fallback += mandatory;
                continue;
            };
            if let Some(graph) = requirement_guard(&guard_row) {
                let judged = if conditional.carrier.relax_untyped_host {
                    vec![LintRequirement {
                        guards: guard_row.clone(),
                        then_value: fragment,
                    }]
                } else {
                    gate.lint_requirement_partitions(
                        &ancestor_segments,
                        &guard_row,
                        graph,
                        fragment,
                        encoding,
                    )
                };
                for LintRequirement { guards, then_value } in judged {
                    insert_content(
                        &ancestor_segments,
                        then_value,
                        ScopedGuardRow {
                            guards,
                            default_document,
                        },
                        1,
                        mandatory,
                    );
                }
                continue;
            }
            let condition = guard_condition.get_or_insert_with(|| {
                gate.planned_conjunction(
                    &guard_row,
                    &ancestor_segments,
                    values_yaml_doc,
                    absence,
                    crate::condition_encoding::ConditionPolarity::Widen,
                )
            });
            // A nil-safe host is already scoped by its actual entry domain.
            // Strict hasKey presence includes null; a second non-null gate would lose its failure.
            let kept = if conditional.carrier.relax_untyped_host {
                Some(fragment)
            } else {
                gate.lint_safe_planned_then(
                    &ancestor_segments,
                    &guard_row,
                    condition,
                    fragment,
                    encoding,
                )
            };
            let Some(fragment) = kept else {
                report.mandatory_outcomes.fallback += mandatory;
                continue;
            };
            if canonicalize_mandatory_constraint(root_schema, &conditional, &fragment, report) {
                continue;
            }
            match &mut merged {
                None => merged = Some((fragment, 1, mandatory)),
                Some((target, facts, mandatory_facts)) => {
                    match merge_disjoint_property_fragment(target, fragment) {
                        Ok(()) => {
                            *facts += 1;
                            *mandatory_facts += mandatory;
                        }
                        Err(fragment) => separate.push((fragment, 1, mandatory)),
                    }
                }
            }
        }
        for (fragment, facts, mandatory_facts) in merged.into_iter().chain(separate) {
            // Conditionals with identical content under one scope disjoin
            // their guards: `if G1 then X` and `if G2 then X` is
            // `if anyOf [G1, G2] then X`, and X (often a repeated provider
            // schema) is the dominant emitted size.
            insert_content(
                &ancestor_segments,
                fragment,
                ScopedGuardRow {
                    guards: guard_row.clone(),
                    default_document,
                },
                facts,
                mandatory_facts,
            );
        }
    }
    tracing::Span::current().record("content_groups", by_content.len());
    tracing::Span::current().record(
        "max_content_guards",
        by_content
            .values()
            .map(|group| group.guard_sets.len())
            .max()
            .unwrap_or_default(),
    );
    gate.finish_conditional_lint();
    // Arms sharing one scope and one encoded condition conjoin their
    // contents: `if C then A` beside `if C then B` is `if C then A ∧ B`,
    // and the repeated condition trees dominate emitted size on charts
    // whose lanes share a few big gates (temporal's per-service config).
    // Coalesced arms keep the FIRST occurrence's position so unaffected
    // documents keep their emission order; trivially-true fragments have
    // no if-block to save and land as their own conjuncts unchanged.
    struct PendingEmission {
        ancestor_segments: Vec<String>,
        condition: EncodedConditionPlan,
        contents: Vec<SchemaNode>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut emissions = Vec::<PendingEmission>::new();
    #[expect(
        clippy::mutable_key_type,
        reason = "condition OnceLocks memoize canonical bytes and lengths derived from immutable schema fragments"
    )]
    let mut emission_index: BTreeMap<(Vec<String>, EncodedConditionPlan), usize> = BTreeMap::new();
    for ((ancestor_segments, _), group) in by_content {
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded requirement implication).
        if group.guard_sets.iter().any(|row| row.guards.is_empty()) {
            emissions.push(PendingEmission {
                ancestor_segments,
                condition: EncodedConditionPlan::any_of(Vec::new()),
                contents: vec![SchemaNode::foreign(group.fragment)],
                facts: group.facts,
                mandatory_facts: group.mandatory_facts,
            });
            continue;
        }
        let guard_sets = if group.guard_sets.iter().any(|row| {
            row.default_document != GuardDefaultDocument::Derive
                || requirement_guard(&row.guards).is_some()
        }) {
            group.guard_sets
        } else {
            helm_schema_core::GuardDnf::normalize_shared_conditional_guard_disjunction(
                group.guard_sets.into_iter().map(|row| row.guards),
            )
            .into_iter()
            .map(|guards| ScopedGuardRow {
                guards,
                default_document: GuardDefaultDocument::Derive,
            })
            .collect()
        };
        let conditions = guard_sets
            .into_iter()
            .map(
                |ScopedGuardRow {
                     guards,
                     default_document,
                 }| {
                    let (_, values_yaml_doc, absence) = default_document.condition_context(
                        &guards,
                        documents,
                        dependency_roots,
                        predicate_memo,
                    );
                    gate.planned_conjunction(
                        &guards,
                        &ancestor_segments,
                        values_yaml_doc,
                        absence,
                        crate::condition_encoding::ConditionPolarity::Widen,
                    )
                },
            )
            .collect();
        let condition = EncodedConditionPlan::any_of(conditions);
        let key = (ancestor_segments.clone(), condition.clone());
        match emission_index.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                if let Some(emission) = emissions.get_mut(*entry.get()) {
                    emission.contents.push(SchemaNode::foreign(group.fragment));
                    emission.facts += group.facts;
                    emission.mandatory_facts += group.mandatory_facts;
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(emissions.len());
                emissions.push(PendingEmission {
                    ancestor_segments,
                    condition,
                    contents: vec![SchemaNode::foreign(group.fragment)],
                    facts: group.facts,
                    mandatory_facts: group.mandatory_facts,
                });
            }
        }
    }
    for mut emission in emissions {
        let content = if emission.contents.len() == 1 {
            emission.contents.remove(0)
        } else {
            SchemaNode::all_of(emission.contents)
        };
        report.carriers.grouping_fan_in = report.carriers.grouping_fan_in.max(emission.facts);
        report.mandatory_outcomes.fallback += emission.mandatory_facts;
        gate.append_planned_conditional(
            root_schema,
            &emission.ancestor_segments,
            &emission.condition,
            content,
        );
    }
}

/// Folds only the lint-checked constraint into an existing property slot.
fn canonicalize_mandatory_constraint(
    document: &mut SchemaDocument,
    conjunct: &LoweredConjunct,
    checked: &Value,
    report: &mut EmissionReport,
) -> bool {
    if !matches!(conjunct.class, EmissionClass::Mandatory) {
        return false;
    }
    // A lint-conditioned fragment must retain its presence guard.
    if checked
        != &build_target_fragment(
            &conjunct.carrier.relative_target_segments,
            conjunct.schema.clone(),
        )
        .into_value()
    {
        report.canonicalization.fallback += 1;
        return false;
    }
    let mut target_segments = conjunct.carrier.ancestor_segments.clone();
    target_segments.extend(conjunct.carrier.relative_target_segments.iter().cloned());
    let is_object_host = conjunct.schema.is_exact_object_type_schema();
    let canonical_object_host = is_object_host
        && conjunct.carrier.ancestor_segments.is_empty()
        && matches!(
            conjunct.origin,
            EmissionOrigin::RequirementImplication | EmissionOrigin::Backprojection
        );
    let outcome = if !is_object_host || canonical_object_host {
        document.canonicalize_constraint_at_path(&target_segments, &conjunct.schema)
    } else {
        CanonicalConstraintOutcome::NotApplicable
    };
    match outcome {
        CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted) => {
            report.canonicalization.applied += 1;
            report.mandatory_outcomes.emitted += 1;
            true
        }
        CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant) => {
            report.canonicalization.redundant += 1;
            report.mandatory_outcomes.redundant += 1;
            true
        }
        CanonicalConstraintOutcome::NotApplicable => {
            report.canonicalization.fallback += 1;
            false
        }
    }
}

fn build_scoped_target_fragment(
    conditional: &LoweredConjunct,
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
    gate: &UncoalescedRootGate<'_>,
) -> Option<Value> {
    let mut target_segments = conditional.carrier.ancestor_segments.clone();
    target_segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
    let mut current_anchor = target_segments;
    let mut content = conditional.schema.clone();

    for scope in conditional.nested_guard_scopes().iter().rev() {
        let relative = current_anchor.strip_prefix(scope.ancestor_segments.as_slice())?;
        let then_schema = build_target_fragment(relative, content);
        let encoding = GuardEncoding {
            values_yaml_doc,
            absence,
            polarity: crate::condition_encoding::ConditionPolarity::Widen,
        };
        if !gate.requirements_encode_fully(&scope.guards, &scope.ancestor_segments, encoding)
            || !scope.guards.iter().all(|guard| {
                guard_encodes_fully(guard, &scope.ancestor_segments, values_yaml_doc, absence)
            })
        {
            return None;
        }
        let condition = SchemaNode::all_of(gate.condition_clauses(
            &scope.guards,
            &scope.ancestor_segments,
            values_yaml_doc,
            absence,
            crate::condition_encoding::ConditionPolarity::Widen,
        ));
        let condition = condition.into_value();
        content = if crate::schema_model::is_empty_schema(&condition) {
            then_schema
        } else {
            SchemaNode::foreign(serde_json::json!({
                "if": condition,
                "then": then_schema.into_value(),
            }))
        };
        current_anchor = scope.ancestor_segments.clone();
    }

    let relative = current_anchor.strip_prefix(conditional.carrier.ancestor_segments.as_slice())?;
    Some(build_target_fragment(relative, content).into_value())
}

/// Merges `incoming` into `target` when both are plain `properties` object
/// fragments whose leaves do not collide.
/// Returns `incoming` on collision, leaving `target` unchanged.
fn merge_disjoint_property_fragment(target: &mut Value, incoming: Value) -> Result<(), Value> {
    fn mergeable(target: &Value, incoming: &Value) -> bool {
        let (Some(target), Some(incoming)) = (target.as_object(), incoming.as_object()) else {
            return false;
        };
        let plain_object = |node: &serde_json::Map<String, Value>| {
            node.keys().all(|key| key == "properties" || key == "type")
                && node.get("type").and_then(Value::as_str) == Some("object")
        };
        if !plain_object(target) || !plain_object(incoming) {
            return false;
        }
        let (Some(Value::Object(target_props)), Some(Value::Object(incoming_props))) =
            (target.get("properties"), incoming.get("properties"))
        else {
            return false;
        };
        incoming_props.iter().all(|(key, value)| {
            target_props
                .get(key)
                .is_none_or(|existing| mergeable(existing, value))
        })
    }
    fn merge(target: &mut Value, incoming: Value) {
        let Value::Object(mut incoming_object) = incoming else {
            return;
        };
        let Some(Value::Object(incoming_props)) = incoming_object.remove("properties") else {
            return;
        };
        let Some(target_props) = target
            .as_object_mut()
            .and_then(|object| object.get_mut("properties"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        for (key, value) in incoming_props {
            match target_props.get_mut(&key) {
                Some(existing) => merge(existing, value),
                None => {
                    target_props.insert(key, value);
                }
            }
        }
    }
    if !mergeable(target, &incoming) {
        return Err(incoming);
    }
    merge(target, incoming);
    Ok(())
}

fn build_target_fragment(path_segments: &[String], leaf_schema: SchemaNode) -> SchemaNode {
    let Some((head, tail)) = path_segments.split_first() else {
        return leaf_schema;
    };

    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_target_fragment(tail, leaf_schema)
    };
    let head = match helm_schema_core::Segment::from_encoded_component(head) {
        helm_schema_core::Segment::EachMember => {
            return SchemaNode::foreign(serde_json::json!({
                "additionalProperties": child.clone().into_value(),
                "items": child.into_value(),
            }));
        }
        helm_schema_core::Segment::Literal(head) => head,
    };
    // The carrier must claim nothing about the ancestor values themselves: a
    // `with`-chain skips falsy ancestors entirely, so the arm has to hold
    // vacuously there. `properties` descent alone already encodes "when this
    // member exists on an object, the leaf requirement applies"; asserting
    // `type: object` on the carrier would reject the skipped falsy states.
    SchemaNode::untyped_member_host().property(head, child)
}

/// Appends `if <guards> then false` at `ancestor_segments` unless the gate
/// withdraws it.
#[tracing::instrument(skip_all, level = "debug")]
fn append_terminal_clause(
    emissions: &mut crate::terminal_conditions::TerminalEmissions,
    gate: &mut UncoalescedRootGate<'_>,
    guard_row: &GuardConjunction,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
) {
    if !gate.supports_scope_guards(guard_row) {
        let _abstention = tracing::debug_span!(
            target: "helm_schema_gen::counters", "terminal_scope_encoding_abstention",
        )
        .entered();
        return;
    }
    let encoding = GuardEncoding {
        values_yaml_doc,
        absence,
        polarity: crate::condition_encoding::ConditionPolarity::Narrow,
    };
    if !gate.requirements_encode_fully(guard_row, ancestor_segments, encoding) {
        return;
    }
    if let Some(graph) = requirement_guard(guard_row) {
        for LintRequirement { guards, then_value } in gate.lint_requirement_partitions(
            ancestor_segments,
            guard_row,
            graph,
            Value::Bool(false),
            encoding,
        ) {
            let planned = gate.planned_conjunction(
                &guards,
                ancestor_segments,
                values_yaml_doc,
                absence,
                encoding.polarity,
            );
            emissions.push(gate, ancestor_segments, planned, then_value);
        }
        return;
    }
    let alternatives = gate.terminal_omission_alternatives(ancestor_segments, guard_row, encoding);
    for alternative in &alternatives {
        let planned = gate.planned_conjunction(
            alternative,
            ancestor_segments,
            values_yaml_doc,
            absence,
            encoding.polarity,
        );
        if let Some(then_schema) = gate.lint_safe_planned_then(
            ancestor_segments,
            alternative,
            &planned,
            Value::Bool(false),
            encoding,
        ) {
            emissions.push(gate, ancestor_segments, planned, then_schema);
        }
    }
    let planned = gate.planned_conjunction(
        guard_row,
        ancestor_segments,
        values_yaml_doc,
        absence,
        encoding.polarity,
    );
    if let Some(then_schema) = gate.lint_safe_planned_then(
        ancestor_segments,
        guard_row,
        &planned,
        Value::Bool(false),
        encoding,
    ) {
        emissions.push(gate, ancestor_segments, planned, then_schema);
    }
}

/// Appends `if condition then false` at `ancestor_segments` as the lint gate
/// keeps it; `condition` encodes `guards` with `encoding`.
fn append_unless_withdrawn(
    root_schema: &mut SchemaDocument,
    gate: &mut UncoalescedRootGate<'_>,
    ancestor_segments: &[String],
    guards: &[ConditionalGuard],
    condition: SchemaNode,
    encoding: GuardEncoding<'_>,
) {
    if !gate.supports_scope_guards(guards)
        || !gate.requirements_encode_fully(guards, ancestor_segments, encoding)
    {
        let _abstention = tracing::debug_span!(
            target: "helm_schema_gen::counters", "terminal_scope_encoding_abstention",
        )
        .entered();
        return;
    }
    if let Some(then_schema) = gate.lint_safe_then(
        ancestor_segments,
        &GuardConjunction::from(guards.to_vec()),
        &condition,
        Value::Bool(false),
        encoding,
    ) {
        gate.append_conditional(
            root_schema,
            ancestor_segments,
            condition,
            SchemaNode::foreign(then_schema),
        );
    }
}

#[cfg(test)]
#[path = "../tests/terminal_scope_encoding.rs"]
mod tests;
