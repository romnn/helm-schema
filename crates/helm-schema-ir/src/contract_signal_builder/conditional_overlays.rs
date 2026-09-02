use super::{
    BTreeMap, BTreeSet, ConditionalGuard, ContractPathAccumulator, ContractRequirementImplication,
    ContractRequirementTarget, ContractUse, FailValueRequirement, Guard, Predicate,
    ProviderSchemaUse, ValueKind, ValuesPath, path_accumulator,
};

/// Paths tested under a HARD negation of the predicate: every
/// `Predicate::Not` subtree except plain presence (`¬Absent`), whose
/// positive reading keeps it out of the dormancy class.
pub(super) fn hard_negation_paths(predicate: &Predicate, out: &mut BTreeSet<String>) {
    match predicate.kind() {
        helm_schema_core::PredicateKind::Not(inner) => {
            if !matches!(
                inner.kind(),
                helm_schema_core::PredicateKind::Guard(Guard::Absent { .. })
            ) {
                out.extend(inner.value_paths().into_iter().map(|path| path.encode()));
            }
        }
        helm_schema_core::PredicateKind::And(items)
        | helm_schema_core::PredicateKind::Or(items) => {
            for item in items {
                hard_negation_paths(item, out);
            }
        }
        _ => {}
    }
}

pub(super) fn lowerable_conditional_guard_set(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<Vec<ConditionalGuard>> {
    // A key-equality conjunct subsumes its companion iteration conjunct:
    // the has-key lowering already implies the range reaches that member
    // (prometheus's serverFiles dispatch around the remoteWrite rows).
    let source_path = contract_use.source_expr.clone();
    let source_expr = source_path.encode();
    let key_equals_ranges: BTreeSet<&helm_schema_core::ValuesPath> = predicates
        .iter()
        .filter_map(|predicate| match predicate.kind() {
            helm_schema_core::PredicateKind::Guard(Guard::RangeKeyEquals { path, .. }) => {
                Some(path)
            }
            _ => None,
        })
        .collect();
    let mut guards = Vec::new();
    for predicate in predicates {
        // The row's own iteration (`range .Values.x` around a render of
        // `.Values.x` itself) is how the row fires, not a foreign
        // condition; the overlay keys on the residual conjuncts. A range
        // over a DIFFERENT path stays unlowerable unless a key-equality
        // pins the exact member the iteration must contain.
        if matches!(
            predicate.kind(),
            helm_schema_core::PredicateKind::Guard(Guard::Range { path })
                if path == &source_path
                    || range_guard_is_iteration_ancestor(&source_expr, path)
                    || key_equals_ranges.contains(path)
        ) {
            continue;
        }
        extend_lowerable_predicate(predicate, &source_expr, &mut guards)?;
    }
    ConditionalGuard::canonicalize_conjunction(&mut guards);
    Some(guards)
}

/// Collapse per-layer spellings of one merged read out of an arm gate.
/// A conjunction of `Truthy(layer.suffix)` guards sharing a suffix across
/// two or more merge layers is the historic all-paths approximation of
/// the merged member's truthiness: the merged value is truthy when ANY
/// layer supplies it, so the conjunctive form under-fires on live
/// renders. The group collapses to that disjunction, with a wildcard
/// (per-set) layer contributing the non-emptiness of the collection it
/// iterates — the strongest spelling the document root has for "some
/// member could supply this".
pub(super) fn collapse_layered_truthy_gates<'a>(
    guards: Vec<ConditionalGuard>,
    layers: impl IntoIterator<Item = &'a helm_schema_core::ValuesPath>,
) -> Vec<ConditionalGuard> {
    // Layers arrive member-projected (each ends with the row's shared
    // member suffix); the merge ROOTS are the layers with the longest
    // common dot-suffix stripped.
    let split: Vec<Vec<helm_schema_core::Segment>> = layers
        .into_iter()
        .map(|layer| layer.segments().cloned().collect())
        .collect();
    let Some(first) = split.first() else {
        return guards;
    };
    let mut common = 0;
    'suffix: while common < first.len() {
        let candidate = first
            .len()
            .checked_sub(1 + common)
            .and_then(|index| first.get(index));
        let Some(candidate) = candidate else {
            break;
        };
        for segments in &split {
            let segment = segments
                .len()
                .checked_sub(1 + common)
                .filter(|&index| index > 0)
                .and_then(|index| segments.get(index));
            if segment != Some(candidate) {
                break 'suffix;
            }
        }
        common += 1;
    }
    let roots: Vec<helm_schema_core::ValuesPath> = split
        .iter()
        .map(|segments| {
            let keep = segments.len().saturating_sub(common);
            helm_schema_core::ValuesPath::from_segments(
                segments.get(..keep).unwrap_or_default().iter().cloned(),
            )
        })
        .collect();
    let concrete_layers: Vec<&helm_schema_core::ValuesPath> = roots
        .iter()
        .filter(|layer| !values_path_contains_wildcard(layer))
        .collect();
    // A wildcard layer has no document-root spelling of its own, but a
    // per-set member can only supply the merged value when the set
    // collection HAS members, so the layer contributes that collection's
    // non-emptiness. It is implied by every state the real layer holds in,
    // which is what an arm gate needs.
    //
    // The collection is everything before the layer's first `*`. A layer
    // spelled entirely concretely contributes no wildcard collection.
    let wildcard_collections: Vec<helm_schema_core::ValuesPath> = roots
        .iter()
        .filter_map(|layer| {
            let segments = layer.segments().collect::<Vec<_>>();
            let wildcard = segments
                .iter()
                .position(|segment| segment.is_each_member())?;
            let prefix = segments.get(..wildcard)?;
            (!prefix.is_empty())
                .then(|| helm_schema_core::ValuesPath::from_segments(prefix.iter().copied()))
        })
        .collect();
    let mut suffix_members: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (index, guard) in guards.iter().enumerate() {
        let ConditionalGuard::Truthy { path } = guard else {
            continue;
        };
        for layer in &concrete_layers {
            if let Some(suffix) = layered_guard_suffix(path, layer) {
                suffix_members.entry(suffix).or_default().insert(index);
            }
        }
    }
    let mut dropped = BTreeSet::new();
    let mut replacements = Vec::new();
    for members in suffix_members.into_values() {
        if members.len() < 2 || members.iter().any(|index| dropped.contains(index)) {
            continue;
        }
        dropped.extend(members.iter().copied());
        let mut arms: Vec<ConditionalGuard> = members
            .iter()
            .filter_map(|&index| guards.get(index).cloned())
            .collect();
        arms.extend(
            wildcard_collections
                .iter()
                .map(|path| ConditionalGuard::Truthy { path: path.clone() }),
        );
        arms.sort();
        arms.dedup();
        replacements.push(ConditionalGuard::AnyOf(arms));
    }
    if dropped.is_empty() {
        return guards;
    }
    let mut out: Vec<ConditionalGuard> = guards
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !dropped.contains(index))
        .map(|(_, guard)| guard)
        .collect();
    out.extend(replacements);
    out.sort();
    out.dedup();
    out
}

fn layered_guard_suffix(
    path: &helm_schema_core::ValuesPath,
    layer: &helm_schema_core::ValuesPath,
) -> Option<String> {
    if !path.is_descendant_of(layer) {
        return None;
    }
    Some(
        helm_schema_core::ValuesPath::from_segments(path.segments().skip(layer.segments().len()))
            .encode(),
    )
}

/// The maximal lowerable SUBSET of the row conditions, at whole-conjunct
/// granularity. Only gate consumers that tolerate dropped conjuncts may
/// use it: each kept guard is an exact decode of one conjunct, so the
/// subset holds in every state the full conjunction holds — a gated arm
/// never goes silent on a live render — while a dropped conjunct leaves
/// the arm firing in some states the render never reaches (widen-only
/// for gates, unsound for fail-requirement conditions).
pub(super) fn lowerable_conditional_guard_subset(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Vec<ConditionalGuard> {
    let source_path = contract_use.source_expr.clone();
    let source_expr = source_path.encode();
    let key_equals_ranges: BTreeSet<&helm_schema_core::ValuesPath> = predicates
        .iter()
        .filter_map(|predicate| match predicate.kind() {
            helm_schema_core::PredicateKind::Guard(Guard::RangeKeyEquals { path, .. }) => {
                Some(path)
            }
            _ => None,
        })
        .collect();
    let mut guards = Vec::new();
    for predicate in predicates {
        // The row's own iteration is how the row fires, not a foreign
        // condition — the same skip as the full-set decode.
        if matches!(
            predicate.kind(),
            helm_schema_core::PredicateKind::Guard(Guard::Range { path })
                if path == &source_path
                    || range_guard_is_iteration_ancestor(&source_expr, path)
                    || key_equals_ranges.contains(path)
        ) {
            continue;
        }
        let mut lowered = Vec::new();
        if extend_lowerable_predicate(predicate, &source_expr, &mut lowered).is_some() {
            guards.extend(lowered);
        }
    }
    ConditionalGuard::canonicalize_conjunction(&mut guards);
    guards
}

pub(super) fn provider_schema_use(
    contract_use: &ContractUse,
    self_range_guarded: bool,
    source_null_tolerant: bool,
) -> Option<ProviderSchemaUse> {
    // A nil-omitting stringification (Sprig `quote`/`squote` skip nil
    // operands) of a RANGED member leaf still forces an explicit null
    // into the slot on a missing source: its use survives — with the
    // Serialized kind, so slot typing keeps abstaining — solely to carry
    // provider requiredness to the ranged-member presence synthesis
    // (traefik's `mountPath: {{ $plugin.mountPath | quote }}`).
    let nil_omitting_ranged_leaf = contract_use.kind == ValueKind::Serialized
        && contract_use.nil_omitting
        && values_path_contains_wildcard(&contract_use.source_expr);
    if contract_use.source_expr.segments().next().is_none()
        || (matches!(
            contract_use.kind,
            ValueKind::PartialScalar | ValueKind::Serialized
        ) && !nil_omitting_ranged_leaf)
        || (contract_use.stringified
            && !matches!(
                contract_use.kind,
                ValueKind::Scalar | ValueKind::YamlSerialized | ValueKind::TemplatedYamlSerialized
            )
            && !nil_omitting_ranged_leaf)
        || contract_use.path.0.is_empty()
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        stringified: contract_use.stringified,
        resource,
        template_supplied_member_keys: contract_use.template_supplied_member_keys.clone(),
        split_segment: contract_use.split_segment.clone(),
        merge_layers: contract_use.merge_layers.clone(),
        range_key: contract_use.range_key,
        nil_omitting: contract_use.nil_omitting,
        omitted_members: contract_use
            .omitted_members
            .iter()
            .map(|(key, retain_guards)| {
                // A retain guard that cannot lower keeps the subtraction
                // but drops the re-add arm: the member's typing then
                // abstains, which only accepts more.
                let guards = retain_guards
                    .iter()
                    .map(|guard| guard_to_conditional_guard(guard, None))
                    .collect::<Option<Vec<_>>>()
                    .unwrap_or_default();
                (key.clone(), guards)
            })
            .collect(),
        is_self_range_collection: self_range_guarded
            && contract_use
                .path
                .0
                .last()
                .is_none_or(|segment| !segment.ends_with("[*]")),
        source_null_tolerant,
        outer_guards: Vec::new(),
    })
}

pub(super) fn predicate_to_guard(
    predicate: &Predicate,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    match predicate.kind() {
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. } => None,
        helm_schema_core::PredicateKind::Guard(guard) => {
            guard_to_conditional_guard(guard, target_value_path)
        }
        // Predicates inside a negation are load-bearing even when they test
        // the target itself. Dropping a target conjunct from `not (a &&
        // target)` widens the branch into states where the render never
        // occurs (for example a `default` fallback shadowed by its primary).
        // A negated range-key equality has no document-level encoding: the
        // else-arm runs for every OTHER member even when the named key is
        // also present, so inverting the has-key lowering would be unsound.
        helm_schema_core::PredicateKind::Not(inner) => {
            if matches!(
                inner.kind(),
                helm_schema_core::PredicateKind::Guard(Guard::RangeKeyEquals { .. })
            ) {
                return None;
            }
            Some(ConditionalGuard::Not(Box::new(predicate_to_guard(
                inner, None,
            )?)))
        }
        helm_schema_core::PredicateKind::And(predicates) => {
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            ConditionalGuard::canonicalize_conjunction(&mut guards);
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        helm_schema_core::PredicateKind::Or(predicates) => {
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
                .any(|path| path_contains_wildcard(&path.encode()))
            {
                return None;
            }
            ConditionalGuard::canonicalize_conjunction(&mut guards);
            (target_value_path.is_some() || !guards.is_empty())
                .then_some(ConditionalGuard::AnyOf(guards))
        }
    }
}

pub(super) fn extend_lowerable_predicate(
    predicate: &Predicate,
    target_value_path: &str,
    out: &mut Vec<ConditionalGuard>,
) -> Option<()> {
    let target = helm_schema_core::ValuesPath::parse(target_value_path);
    match predicate.kind() {
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. }
        | helm_schema_core::PredicateKind::Guard(Guard::Range { .. }) => return None,
        helm_schema_core::PredicateKind::Guard(Guard::With { path }) if path == &target => {}
        helm_schema_core::PredicateKind::Guard(Guard::With { .. }) => {
            out.push(predicate_to_guard(predicate, None)?);
        }
        helm_schema_core::PredicateKind::And(predicates) => {
            for predicate in predicates {
                extend_lowerable_predicate(predicate, target_value_path, out)?;
            }
        }
        helm_schema_core::PredicateKind::Guard(Guard::Default { path }) if path == &target => {}
        // The row's own truthiness is nullability evidence (captured as
        // source null-tolerance), not a conditional shape over *other*
        // paths; like the self-default and self-negation arms it must not
        // poison the foreign overlay keys. Root-to-leaf guard stacks put it
        // on every `with .Values.x`-wrapped render since the fragment
        // interpreter landed.
        helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == &target => {}
        // Self-negation carries the branch's own-arm exclusion, not a
        // conditional shape over *other* paths; the overlay keys stay on the
        // foreign conditions.
        helm_schema_core::PredicateKind::Not(inner)
            if matches!(
                inner.kind(),
                helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == &target
            ) => {}
        // A type test on the row's own path (also negated or a disjunction
        // of such tests) partitions its domain (a type-switch arm). The
        // partition is load-bearing: the arm's sink typing holds only for
        // its tested types, and an executing complement arm's requirements
        // hold exactly for the untested ones — so it stays ON the overlay
        // key rather than leaking the arm's shape over the whole domain.
        // On a `.*` member row the partition keys the MEMBER overlay: the
        // wildcard guard path is encodable at the member slot, exactly like
        // its negated complement (signoz's per-member object-versus-scalar
        // EnvVar dispatch).
        _ if predicate_is_self_type_partition(predicate, target_value_path) => {
            let target = if path_contains_wildcard(target_value_path) {
                None
            } else {
                Some(target_value_path)
            };
            out.push(predicate_to_guard(predicate, target)?);
        }
        _ => {
            out.push(predicate_to_guard(predicate, Some(target_value_path))?);
        }
    }
    Some(())
}

/// A terminal-clause conjunct may lower through an approximate predicate's
/// recognized SOUND SUBSET: the clause then rejects a subset of the real
/// failing states (firing less often is safe in this positive position).
/// The subset must never lower through a negation — `predicate_to_guard`'s
/// `Not` arm keeps returning `None` for approximate inners.
pub(super) fn terminal_clause_guard(predicate: &Predicate) -> Option<ConditionalGuard> {
    // A range body runs exactly for a non-empty collection, and truthiness
    // is that test everywhere the distinction can matter to a TERMINATING
    // clause: the remaining truthy values (a string, a number, a bool) are
    // the ones `range` refuses to iterate, so the render terminates at the
    // range itself. Both readings of a truthy subject therefore terminate,
    // which is what this clause claims.
    if let helm_schema_core::PredicateKind::Guard(Guard::Range { path }) = predicate.kind() {
        return Some(ConditionalGuard::Truthy { path: path.clone() });
    }
    if let helm_schema_core::PredicateKind::Approximate {
        sound_subset: Some(sound_subset),
        ..
    } = predicate.kind()
    {
        return predicate_to_guard(sound_subset, None);
    }
    // A disjunction of strengthened arms implies the real disjunction, so
    // it stays inside the clause's positive position (jenkins' two-sided
    // `$replicas` domain check).
    if let helm_schema_core::PredicateKind::Or(items) = predicate.kind()
        && predicate.contains_approximation()
    {
        let mut guards = items
            .iter()
            .map(terminal_clause_guard)
            .collect::<Option<Vec<_>>>()?;
        ConditionalGuard::canonicalize_conjunction(&mut guards);
        return match guards.as_slice() {
            [] => None,
            [guard] => Some(guard.clone()),
            _ => Some(ConditionalGuard::AnyOf(guards)),
        };
    }
    // A conjunction strengthens all-or-nothing: each conjunct's sound
    // strengthening strengthens the whole, while dropping one would widen
    // it (cilium's `and (ge …) (le …)` cluster-id window arms inside the
    // ENI check's disjunction).
    if let helm_schema_core::PredicateKind::And(items) = predicate.kind()
        && predicate.contains_approximation()
    {
        let mut guards = items
            .iter()
            .map(terminal_clause_guard)
            .collect::<Option<Vec<_>>>()?;
        ConditionalGuard::canonicalize_conjunction(&mut guards);
        return match guards.as_slice() {
            [] => None,
            [guard] => Some(guard.clone()),
            _ => Some(ConditionalGuard::AllOf(guards)),
        };
    }
    predicate_to_guard(predicate, None)
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn guard_to_conditional_guard(
    guard: &Guard,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    let path = |path: &helm_schema_core::ValuesPath| match target_value_path {
        Some(target_value_path) => lowerable_guard_path(path, target_value_path),
        None => Some(path.clone()),
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
        Guard::HasKey {
            path: value_path,
            key,
        } => Some(ConditionalGuard::HasKey {
            path: path(value_path)?,
            key: key.clone(),
        }),
        Guard::NotHasKey {
            path: value_path,
            key,
        } => Some(ConditionalGuard::Not(Box::new(ConditionalGuard::HasKey {
            path: path(value_path)?,
            key: key.clone(),
        }))),
        Guard::ContainsMemberEquals {
            path: value_path,
            member,
            value,
        } => Some(ConditionalGuard::ContainsMemberEquals {
            path: path(value_path)?,
            member: member.clone(),
            value: value.clone(),
        }),
        Guard::ContainsTruthyMember {
            path: value_path,
            member,
        } => Some(ConditionalGuard::ContainsTruthyMember {
            path: path(value_path)?,
            member: member.clone(),
        }),
        Guard::ContainsEquals {
            path: value_path,
            value,
        } => Some(ConditionalGuard::ContainsEquals {
            path: path(value_path)?,
            value: value.clone(),
        }),
        Guard::MatchesPattern {
            path: value_path,
            pattern,
            templated: false,
        } => Some(ConditionalGuard::MatchesPattern {
            path: path(value_path)?,
            pattern: pattern.clone(),
        }),
        Guard::NotMatchesPattern {
            path: value_path,
            pattern,
        } => {
            let value_path = path(value_path)?;
            Some(ConditionalGuard::AllOf(vec![
                ConditionalGuard::TypeIs {
                    path: value_path.clone(),
                    schema_type: "string".to_string(),
                },
                ConditionalGuard::Not(Box::new(ConditionalGuard::MatchesPattern {
                    path: value_path,
                    pattern: pattern.clone(),
                })),
            ]))
        }
        Guard::MatchesPattern { .. }
        | Guard::RangeKeyPrefix { .. }
        | Guard::RangeKeyMatches { .. }
        | Guard::Range { .. }
        | Guard::With { .. }
        | Guard::Default { .. } => None,
        Guard::AtMostOneMember { path: value_path } => Some(ConditionalGuard::AtMostOneMember {
            path: path(value_path)?,
        }),
        Guard::MinMembers {
            path: value_path,
            bound,
        } => {
            // A member-count gate on the TARGET itself is load-bearing:
            // the render fires only for maps that large, so the arm must
            // keep the bound (external-secrets' `gt (keys . | len) 1`).
            let path = if target_value_path
                .is_some_and(|target| value_path == &helm_schema_core::ValuesPath::parse(target))
            {
                (!values_path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::MinMembers {
                path,
                bound: *bound,
            })
        }
        Guard::TypeIs {
            path: value_path,
            schema_type,
        } => {
            // A type test on the TARGET itself is load-bearing dispatch
            // structure (the `else` of `if typeIs "string" x` scopes an
            // object overlay to non-strings); only truthiness self-guards
            // are the row's own firing condition and stay stripped.
            let path = if target_value_path
                .is_some_and(|target| value_path == &helm_schema_core::ValuesPath::parse(target))
            {
                (!values_path_contains_wildcard(value_path)).then(|| value_path.clone())?
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
            let path = if target_value_path
                .is_some_and(|target| value_path == &helm_schema_core::ValuesPath::parse(target))
            {
                (!values_path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::Not(Box::new(ConditionalGuard::TypeIs {
                path,
                schema_type: schema_type.clone(),
            })))
        }
        Guard::IntGt {
            path: value_path,
            bound,
        } => Some(ConditionalGuard::IntGt {
            path: path(value_path)?,
            bound: *bound,
        }),
        Guard::IntLt {
            path: value_path,
            bound,
        } => Some(ConditionalGuard::IntLt {
            path: path(value_path)?,
            bound: *bound,
        }),
        // The POSITIVE key-equality selects exactly one member, so at the
        // document level it holds iff the collection HAS that key (// prometheus's `eq $key "prometheus.yml"` serverFiles arm). The
        // negated form runs for every OTHER member and must not lower —
        // `predicate_to_guard`'s Not arm rejects it.
        Guard::RangeKeyEquals {
            path: value_path,
            key,
        } => {
            if key.is_empty() {
                return None;
            }
            Some(ConditionalGuard::HasKey {
                path: path(value_path)?,
                key: key.clone(),
            })
        }
        // The De Morgan flatten emits falsiness and disjunction guards
        // (`¬(a ∨ b)` → two `Not`s; `¬(a ∧ b)` → an `AnyOf`); each has a
        // direct conditional-guard encoding, so a mode-dispatch condition
        // (vault's `ne .mode "external"`) keys arms instead of dropping
        // the whole access or row.
        Guard::Not { path: value_path } => {
            Some(ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                path: path(value_path)?,
            })))
        }
        Guard::Or { paths } => Some(ConditionalGuard::AnyOf(
            paths
                .iter()
                .map(|value_path| {
                    Some(ConditionalGuard::Truthy {
                        path: path(value_path)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        Guard::AnyOf { alternatives } => Some(ConditionalGuard::AnyOf(
            alternatives
                .iter()
                .map(|alternative| {
                    let mut guards = alternative
                        .iter()
                        .map(|guard| guard_to_conditional_guard(guard, target_value_path))
                        .collect::<Option<Vec<_>>>()?;
                    ConditionalGuard::canonicalize_conjunction(&mut guards);
                    match guards.as_slice() {
                        [] => None,
                        [guard] => Some(guard.clone()),
                        _ => Some(ConditionalGuard::AllOf(guards)),
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        )),
    }
}

/// A nested range over each member of `parent` (`p.*` ranged): members
/// must be rangeable wherever the outer conditions hold.
pub(super) fn record_member_range_requirement(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    parent: &ValuesPath,
    predicates: &[Predicate],
    outer_allows_integer: bool,
    inner_allows_integer: bool,
) {
    let mut member_path = parent.clone();
    member_path.push_each_member();
    let mut outer_guards = Vec::new();
    for predicate in predicates {
        if matches!(
            predicate.kind(),
            helm_schema_core::PredicateKind::Guard(Guard::Range { path })
                if path == parent || path == &member_path
        ) {
            continue;
        }
        let Some(guard) = predicate_to_guard(predicate, None) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(&path.encode()))
        {
            return;
        }
        outer_guards.push(guard);
    }
    let implication = ContractRequirementImplication::new(
        outer_guards,
        ContractRequirementTarget::Members {
            allow_integer: outer_allows_integer,
        },
        vec![FailValueRequirement::Iterable {
            allow_integer: inner_allows_integer,
        }],
    );
    let acc = path_accumulator(paths, parent);
    acc.referenced = true;
    if !acc.requirement_implications.contains(&implication) {
        acc.requirement_implications.push(implication);
    }
}

/// Whether a conjunct tests the TYPE of `source_expr`, positively or under
/// negation.
pub(super) fn predicate_tests_source_type(predicate: &Predicate, source_expr: &str) -> bool {
    let source_path = helm_schema_core::ValuesPath::parse(source_expr);
    match predicate.kind() {
        helm_schema_core::PredicateKind::Guard(Guard::TypeIs { path, .. }) => path == &source_path,
        helm_schema_core::PredicateKind::Not(inner) => {
            predicate_tests_source_type(inner, source_expr)
        }
        helm_schema_core::PredicateKind::And(items)
        | helm_schema_core::PredicateKind::Or(items) => items
            .iter()
            .any(|item| predicate_tests_source_type(item, source_expr)),
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. }
        | helm_schema_core::PredicateKind::Guard(_) => false,
    }
}

/// Whether every leaf of `predicate` is a type test on `target_value_path`
/// itself: such a predicate partitions the row's own domain instead of
/// conditioning it on other paths.
pub(super) fn predicate_is_self_type_partition(
    predicate: &Predicate,
    target_value_path: &str,
) -> bool {
    let target = helm_schema_core::ValuesPath::parse(target_value_path);
    match predicate.kind() {
        helm_schema_core::PredicateKind::Guard(Guard::TypeIs { path, .. }) => path == &target,
        helm_schema_core::PredicateKind::Not(inner) => {
            predicate_is_self_type_partition(inner, target_value_path)
        }
        helm_schema_core::PredicateKind::And(items)
        | helm_schema_core::PredicateKind::Or(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| predicate_is_self_type_partition(item, target_value_path))
        }
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. }
        | helm_schema_core::PredicateKind::Guard(_) => false,
    }
}

pub(super) fn lowerable_guard_path(
    path: &helm_schema_core::ValuesPath,
    target_value_path: &str,
) -> Option<helm_schema_core::ValuesPath> {
    let target = helm_schema_core::ValuesPath::parse(target_value_path);
    if path == &target {
        return None;
    }
    if !values_path_contains_wildcard(path) {
        return Some(path.clone());
    }

    let path_segments = path.segments().collect::<Vec<_>>();
    let target = helm_schema_core::ValuesPath::parse(target_value_path);
    let target_segments = target.segments().collect::<Vec<_>>();
    let last_wildcard = path_segments
        .iter()
        .rposition(|segment| segment.is_each_member())?;
    let member_prefix = path_segments.get(..=last_wildcard)?;
    // A wildcard guard is exact when every wildcard it contains identifies
    // the same ranged member as the target. Conditional lowering then
    // anchors at that shared member, so the remaining guard and target
    // suffixes are ordinary relative paths.
    target_segments
        .get(..=last_wildcard)
        .is_some_and(|segments| segments == member_prefix)
        .then(|| path.clone())
}

pub(super) fn path_contains_wildcard(path: &str) -> bool {
    helm_schema_core::ValuesPath::parse(path)
        .segments()
        .any(helm_schema_core::Segment::is_each_member)
}

fn values_path_contains_wildcard(path: &helm_schema_core::ValuesPath) -> bool {
    path.segments()
        .any(helm_schema_core::Segment::is_each_member)
}

/// A truthiness gate over one field of `collection`'s ranged members, which
/// selects WHICH members reach the consumer. It can never become an outer
/// guard — the document level has no way to name "this member" — but the
/// member's own slot can carry it beside the requirement. `constrained` is
/// the path the requirement itself binds, whose own truthiness scopes the
/// requirement rather than selecting members.
pub(super) fn member_local_truthy_selector(
    collection: &str,
    predicate: &Predicate,
    constrained: Option<&str>,
) -> Option<Vec<String>> {
    let helm_schema_core::PredicateKind::Guard(Guard::Truthy { path } | Guard::With { path }) =
        predicate.kind()
    else {
        return None;
    };
    if constrained
        .is_some_and(|constrained| path == &helm_schema_core::ValuesPath::parse(constrained))
    {
        return None;
    }
    // The suffix is the member-relative field when `path` addresses one
    // field of `collection`'s ranged members
    // (`users.*.existingSecret` under `users`). A deeper wildcard abstains:
    // it names members of that field, not the field itself.
    let collection = helm_schema_core::ValuesPath::parse(collection);
    let path_segments = path.segments().collect::<Vec<_>>();
    let collection_segments = collection.segments().collect::<Vec<_>>();
    let suffix = path_segments.strip_prefix(collection_segments.as_slice())?;
    let (member, suffix) = suffix.split_first()?;
    if !member.is_each_member()
        || suffix.is_empty()
        || suffix.iter().any(|segment| segment.is_each_member())
    {
        return None;
    }
    suffix
        .iter()
        .map(|segment| segment.literal().map(str::to_string))
        .collect()
}

pub(super) fn ranged_member_parent(path: &str) -> Option<&str> {
    path.strip_suffix(".*")
        .or_else(|| path.split_once(".*.").map(|(parent, _)| parent))
}

pub(super) fn range_guard_is_iteration_ancestor(
    source_path: &str,
    guard_path: &helm_schema_core::ValuesPath,
) -> bool {
    let source = helm_schema_core::ValuesPath::parse(source_path);
    let source_segments = source.segments().collect::<Vec<_>>();
    let guard_segments = guard_path.segments().collect::<Vec<_>>();
    source_segments.len() > guard_segments.len()
        && source_segments
            .iter()
            .copied()
            .zip(guard_segments.iter().copied())
            .all(|(source, guard)| source == guard)
        && source_segments
            .get(guard_segments.len())
            .is_some_and(|segment| segment.is_each_member())
}

pub(super) fn predicate_is_structural_ancestor_guard(
    predicate: &Predicate,
    source_path: &str,
) -> bool {
    let helm_schema_core::PredicateKind::Guard(Guard::Truthy { path } | Guard::With { path }) =
        predicate.kind()
    else {
        return false;
    };
    let source = helm_schema_core::ValuesPath::parse(source_path);
    let source_segments = source.segments().collect::<Vec<_>>();
    let guard_segments = path.segments().collect::<Vec<_>>();
    source_segments.len() > guard_segments.len()
        && source_segments
            .iter()
            .copied()
            .zip(guard_segments.iter().copied())
            .all(|(source, guard)| source == guard)
}

/// All strict ancestors of the referenced paths, the subset whose
/// descendant continues through a `*` item segment (a ranged collection's
/// element rows, as opposed to a literal member read), and the subset
/// whose `*` descendant continues INTO element structure (`p.*.field`) —
/// a bare `p.*` value row proves no LIST shape, since `range` iterates
/// maps too.
pub(super) fn collect_paths_with_descendants(
    paths: &BTreeSet<ValuesPath>,
) -> (
    BTreeSet<ValuesPath>,
    BTreeSet<ValuesPath>,
    BTreeSet<ValuesPath>,
) {
    let mut ancestors = BTreeSet::new();
    let mut item_ancestors = BTreeSet::new();
    let mut structured_item_ancestors = BTreeSet::new();
    for path in paths {
        let segments = path.segments().collect::<Vec<_>>();
        for prefix_len in 1..segments.len() {
            let Some(prefix) = segments.get(..prefix_len) else {
                continue;
            };
            let Some(segment) = segments.get(prefix_len) else {
                continue;
            };
            let ancestor = ValuesPath::from_segments(prefix.iter().copied());
            if segment.is_each_member() {
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
