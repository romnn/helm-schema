use super::{
    BTreeMap, BTreeSet, ConditionalGuard, ConditionalPathOverlay, EmissionClass, EmissionReport,
    GuardValue, LoweredConjunct, NestedGuardScope, PathSchemaResolver, ResolvedPathSchema,
    ResourceSchemaOracle, SchemaDocument, SchemaNode, Value, YamlValue, build_condition_clauses,
    common_prefix_len, evaluate_guard_set_on_values, guard_encodes_fully,
};
use helm_schema_core::ValuesPath;

use crate::values_yaml::yaml_value_at_values_path;

pub(super) fn resolve_overlay_target_schema(
    target_value_path: &ValuesPath,
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> ResolvedPathSchema {
    let evidence = overlay.evidence.as_path_evidence();
    PathSchemaResolver::resolve_single_path_evidence(target_value_path, &evidence, provider)
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
pub(super) fn implication_guards_supported(
    guards: &[ConditionalGuard],
    target_value_path: &ValuesPath,
    resolved_by_path: &BTreeMap<&ValuesPath, &ResolvedPathSchema>,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
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
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => true,
            ConditionalGuard::Not(inner) => implication_guards_supported(
                std::slice::from_ref(inner),
                target_value_path,
                resolved_by_path,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                implication_guards_supported(guards, target_value_path, resolved_by_path)
            }
        })
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
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
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

#[tracing::instrument(skip_all)]
/// Lower terminating validator formulas: for each clause, no valid values
/// document satisfies ALL its guards, so the document gets
/// `if <guards> then false` at the guards' shared ancestor. Clauses with
/// any unencodable guard are skipped whole — a partially encoded `if`
/// would reject documents the validator never terminates.
pub(crate) fn append_terminal_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[Vec<ConditionalGuard>],
    values_default_sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
    guarded_values_default_sources: &BTreeSet<helm_schema_core::GuardedValuesDefaultSource>,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
) {
    append_values_default_source_absence_clauses(
        root_schema,
        clauses,
        values_default_sources,
        guarded_values_default_sources,
        documents,
        dependency_roots,
    );
    // A deleted dependency values root is not the document minus a key:
    // helm recreates the table from the SUBCHART's own values, so a clause
    // whose guards all hold against that refill terminates every document
    // missing the root — the half a clause anchored inside the root cannot
    // reach. One clause per root states it.
    let mut deleted_roots = BTreeSet::new();
    for guards in clauses {
        let (_, absence) = documents.condition_context(guards, dependency_roots);
        if let Some(root) =
            crate::condition_encoding::deleted_dependency_root_terminates(guards, absence)
        {
            deleted_roots.insert(root);
        }
    }
    for root in deleted_roots {
        let Some(condition) = crate::condition_encoding::dependency_root_gone_condition(root)
        else {
            continue;
        };
        root_schema.append_conditional(&[], condition, SchemaNode::foreign(Value::Bool(false)));
    }
    for guards in clauses {
        let (values_yaml_doc, absence) = documents.condition_context(guards, dependency_roots);
        let shared_ancestor = shared_guard_ancestor_segments(guards);
        let all_vacuous = guards.iter().all(guard_holds_vacuously);
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
        if !guards
            .iter()
            .all(|guard| guard_encodes_fully(guard, &ancestor_segments, values_yaml_doc, absence))
        {
            continue;
        }
        let condition = SchemaNode::all_of(build_condition_clauses(
            guards,
            &ancestor_segments,
            values_yaml_doc,
            absence,
            crate::condition_encoding::ConditionPolarity::Narrow,
        ));
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(Value::Bool(false)),
        );
        // `deeper_stage` is the effective subtree when the document supplies
        // no ancestor. A definitively false original formula makes the
        // missing-ancestor companion unreachable; uncertainty stays open.
        if split_vacuous_ancestor
            && evaluate_guard_set_on_values(guards, absence.deeper_stage) != Some(false)
        {
            let path = ValuesPath::from_segments(
                ancestor_segments
                    .iter()
                    .map(|segment| helm_schema_core::Segment::from_encoded_component(segment)),
            );
            let absent = ConditionalGuard::Absent { path };
            let root = Vec::new();
            let mut missing_ancestor_guards = guards.clone();
            missing_ancestor_guards.push(absent);
            if missing_ancestor_guards
                .iter()
                .all(|guard| guard_encodes_fully(guard, &root, values_yaml_doc, absence))
            {
                let condition = SchemaNode::all_of(build_condition_clauses(
                    &missing_ancestor_guards,
                    &root,
                    values_yaml_doc,
                    absence,
                    crate::condition_encoding::ConditionPolarity::Narrow,
                ));
                root_schema.append_conditional(
                    &root,
                    condition,
                    SchemaNode::foreign(Value::Bool(false)),
                );
            }
        }
    }
}

fn append_values_default_source_absence_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[Vec<ConditionalGuard>],
    values_default_sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
    guarded_values_default_sources: &BTreeSet<helm_schema_core::GuardedValuesDefaultSource>,
    documents: &crate::emission_plan::RootValuesDocuments,
    dependency_roots: &BTreeSet<Vec<String>>,
) {
    for guards in clauses {
        let [ConditionalGuard::Absent { path }] = guards.as_slice() else {
            continue;
        };
        let Some(source_path) = unique_values_default_source_path(path, values_default_sources)
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
        root_schema.append_conditional(
            &[],
            SchemaNode::all_of(vec![target_absent, source_absent]),
            SchemaNode::foreign(Value::Bool(false)),
        );
    }
    for guards in clauses {
        let guard_predicate = helm_schema_core::Predicate::all(
            guards.iter().map(ConditionalGuard::predicate).collect(),
        );
        for fact in guarded_values_default_sources {
            let activation = helm_schema_core::Predicate::all(
                fact.outer_guards
                    .iter()
                    .map(ConditionalGuard::predicate)
                    .collect(),
            );
            if !guard_predicate.exactly_implies(&activation) {
                continue;
            }
            let Some(ConditionalGuard::Absent { path }) = guards.iter().find(|guard| {
                matches!(guard, ConditionalGuard::Absent { path } if source_path_for_effective(path, &fact.source).is_some())
            }) else {
                continue;
            };
            let Some(source_path) = source_path_for_effective(path, &fact.source) else {
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
            let remaining_guards = guards
                .iter()
                .filter(|guard| *guard != &ConditionalGuard::Absent { path: path.clone() })
                .cloned()
                .collect::<Vec<_>>();
            let (values_yaml_doc, absence) = documents.condition_context(guards, dependency_roots);
            let mut conditions = build_condition_clauses(
                &remaining_guards,
                &[],
                values_yaml_doc,
                absence,
                crate::condition_encoding::ConditionPolarity::Narrow,
            );
            conditions.push(target_absent);
            conditions.push(source_absent);
            root_schema.append_conditional(
                &[],
                SchemaNode::all_of(conditions),
                SchemaNode::foreign(Value::Bool(false)),
            );
        }
    }
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
        | ConditionalGuard::IntGt { .. }
        | ConditionalGuard::IntLt { .. }
        | ConditionalGuard::HasKey { .. }
        | ConditionalGuard::ContainsMemberEquals { .. }
        | ConditionalGuard::ContainsTruthyMember { .. }
        | ConditionalGuard::ContainsEquals { .. }
        | ConditionalGuard::MinMembers { .. } => false,
        ConditionalGuard::Eq { value, .. } => matches!(value, GuardValue::Null),
        ConditionalGuard::NotEq { .. }
        | ConditionalGuard::Absent { .. }
        | ConditionalGuard::AtMostOneMember { .. }
        | ConditionalGuard::Not(_) => true,
        ConditionalGuard::AllOf(inner) => inner.iter().all(guard_holds_vacuously),
        ConditionalGuard::AnyOf(inner) => inner.iter().any(guard_holds_vacuously),
    }
}

/// The longest common prefix of the PARENTS of every path the guards
/// reference. Presence tests (`required`/`Absent` encodings) need the
/// tested segment to stay relative, so a single-path clause anchors at the
/// path's parent rather than the path itself.
fn shared_guard_ancestor_segments(guards: &[ConditionalGuard]) -> Vec<String> {
    let mut shared: Option<Vec<String>> = None;
    for guard in guards {
        for guard_path in guard.value_paths() {
            let mut segments = guard_path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
            segments.pop();
            shared = Some(match shared {
                None => segments,
                Some(prefix) => {
                    let len = common_prefix_len(&prefix, &segments);
                    prefix.get(..len).unwrap_or_default().to_vec()
                }
            });
        }
    }
    shared.unwrap_or_default()
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
        if conditional.carrier.relax_untyped_host
            && !crate::schema_model::is_empty_schema(&conditional.schema)
        {
            let mut segments = conditional.carrier.ancestor_segments.clone();
            segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
            preparation.relaxed_host_paths.insert(segments);
        }
    }
    preparation
}

#[tracing::instrument(skip_all)]
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
) {
    let mut condition_cache = crate::condition_encoding::ConditionFragmentCache::new();
    // Conditionals sharing one guard set and scope conjoin into one if/then:
    // `allOf [{if G then A}, {if G then B}]` is `{if G then A ∧ B}`, and the
    // repeated `if` blocks dominate emitted size on charts with many guarded
    // blocks. Distinct targets merge disjointly; a leaf collision falls back
    // to its own conditional.
    let mut grouped: BTreeMap<(Vec<String>, Vec<ConditionalGuard>), Vec<LoweredConjunct>> =
        BTreeMap::new();
    for conditional in conditionals {
        // Schema-less conditionals carry base ownership established by a
        // transform or by a separate implication that already emits the
        // complete runtime domain; they have no schema arm to append.
        if crate::schema_model::is_empty_schema(&conditional.schema) {
            if matches!(conditional.class, EmissionClass::Mandatory) {
                report.mandatory_outcomes.redundant += 1;
            }
            continue;
        }
        grouped
            .entry((
                conditional.carrier.ancestor_segments.clone(),
                conditional.outer_guards().to_vec(),
            ))
            .or_default()
            .push(conditional);
    }
    struct ContentGroup {
        fragment: Value,
        guard_sets: Vec<Vec<ConditionalGuard>>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut by_content: BTreeMap<(Vec<String>, String), ContentGroup> = BTreeMap::new();
    for ((ancestor_segments, guards), group) in grouped {
        let (values_yaml_doc, absence) = documents.condition_context(&guards, dependency_roots);
        let mut merged: Option<(Value, usize, usize)> = None;
        let mut separate = Vec::new();
        for conditional in group {
            let mandatory = usize::from(matches!(conditional.class, EmissionClass::Mandatory));
            let Some(fragment) = build_scoped_target_fragment(
                &conditional,
                values_yaml_doc,
                absence,
                &mut condition_cache,
            ) else {
                report.mandatory_outcomes.fallback += mandatory;
                continue;
            };
            match &mut merged {
                None => merged = Some((fragment, 1, mandatory)),
                Some((target, facts, mandatory_facts)) => {
                    if merge_disjoint_property_fragment(target, fragment.clone()) {
                        *facts += 1;
                        *mandatory_facts += mandatory;
                    } else {
                        separate.push((fragment, 1, mandatory));
                    }
                }
            }
        }
        for (fragment, facts, mandatory_facts) in merged.into_iter().chain(separate) {
            // Conditionals with identical content under one scope disjoin
            // their guards: `if G1 then X` and `if G2 then X` is
            // `if anyOf [G1, G2] then X`, and X (often a repeated provider
            // schema) is the dominant emitted size.
            let content = by_content
                .entry((
                    ancestor_segments.clone(),
                    helm_schema_json_schema_walk::canonical_json_string(&fragment),
                ))
                .or_insert_with(|| ContentGroup {
                    fragment,
                    guard_sets: Vec::new(),
                    facts: 0,
                    mandatory_facts: 0,
                });
            content.guard_sets.push(guards.clone());
            content.facts += facts;
            content.mandatory_facts += mandatory_facts;
        }
    }
    // Arms sharing one scope and one encoded condition conjoin their
    // contents: `if C then A` beside `if C then B` is `if C then A ∧ B`,
    // and the repeated condition trees dominate emitted size on charts
    // whose lanes share a few big gates (temporal's per-service config).
    // Coalesced arms keep the FIRST occurrence's position so unaffected
    // documents keep their emission order; trivially-true fragments have
    // no if-block to save and land as their own conjuncts unchanged.
    struct PendingEmission {
        ancestor_segments: Vec<String>,
        condition: SchemaNode,
        contents: Vec<SchemaNode>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut emissions = Vec::<PendingEmission>::new();
    let mut emission_index: BTreeMap<(Vec<String>, String), usize> = BTreeMap::new();
    for ((ancestor_segments, _), group) in by_content {
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded requirement implication).
        if group.guard_sets.iter().any(Vec::is_empty) {
            emissions.push(PendingEmission {
                ancestor_segments,
                condition: SchemaNode::empty(),
                contents: vec![SchemaNode::foreign(group.fragment)],
                facts: group.facts,
                mandatory_facts: group.mandatory_facts,
            });
            continue;
        }
        let mut conditions: Vec<SchemaNode> =
            helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(group.guard_sets)
                .into_iter()
                .map(|guards| {
                    let (values_yaml_doc, absence) =
                        documents.condition_context(&guards, dependency_roots);
                    SchemaNode::all_of(crate::condition_encoding::build_condition_clauses_cached(
                        &guards,
                        &ancestor_segments,
                        values_yaml_doc,
                        absence,
                        &mut condition_cache,
                    ))
                })
                .collect();
        let condition = if conditions.len() == 1 {
            conditions.remove(0)
        } else {
            SchemaNode::any_of(conditions)
        };
        let condition_value = condition.clone().into_value();
        let key = (
            ancestor_segments.clone(),
            helm_schema_json_schema_walk::canonical_json_string(&condition_value),
        );
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
        root_schema.append_conditional(&emission.ancestor_segments, emission.condition, content);
    }
}

fn build_scoped_target_fragment(
    conditional: &LoweredConjunct,
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
    condition_cache: &mut crate::condition_encoding::ConditionFragmentCache,
) -> Option<Value> {
    let mut target_segments = conditional.carrier.ancestor_segments.clone();
    target_segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
    let mut current_anchor = target_segments;
    let mut content = SchemaNode::foreign(conditional.schema.clone());

    for scope in conditional.nested_guard_scopes().iter().rev() {
        let relative = current_anchor.strip_prefix(scope.ancestor_segments.as_slice())?;
        let then_schema = build_target_fragment(relative, content);
        if !scope.guards.iter().all(|guard| {
            guard_encodes_fully(guard, &scope.ancestor_segments, values_yaml_doc, absence)
        }) {
            return None;
        }
        let condition =
            SchemaNode::all_of(crate::condition_encoding::build_condition_clauses_cached(
                &scope.guards,
                &scope.ancestor_segments,
                values_yaml_doc,
                absence,
                condition_cache,
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

/// Merge `incoming` into `target` when both are plain `properties` object
/// fragments whose leaves do not collide; returns false (leaving `target`
/// unchanged) when they do.
fn merge_disjoint_property_fragment(target: &mut Value, incoming: Value) -> bool {
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
        return false;
    }
    merge(target, incoming);
    true
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
