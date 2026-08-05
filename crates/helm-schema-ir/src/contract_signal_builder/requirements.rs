use super::{
    BTreeMap, BTreeSet, ConditionalGuard, ContractFailImplication, ContractPathAccumulator,
    ContractRequirementTarget, FailValueRequirement, Guard, GuardDnf, GuardValue,
    MemberAccessConditions, Predicate, TruthCondition, has_selection_chain_marker_stamp,
    lowerable_range_outer_guards, member_local_truthy_selector, path_accumulator,
    path_contains_wildcard, predicate_is_truthy_disjunction_over, predicate_skips_falsy_source,
    predicate_to_guard, record_range_input_capture, remove_redundant_approximate_conditions,
    terminal_clause_guard,
};

/// Lower one `fail` conjunction into a path requirement: rendering aborts
/// whenever the conjunction holds, so valid inputs must falsify the failing
/// TEST wherever the OUTER guards hold. Conjunctions whose test cannot be
/// negated structurally are skipped (truthy-fallback predicates approximate
/// undecodable conditions and must never be negated).
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn record_fail_conjunction(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    capture: &crate::eval_effect::FailCapture,
    range_modes: &crate::range_modes::RangeModes,
) {
    if let crate::eval_effect::CaptureKind::RangeKeyStrings {
        paths: range_key_string_paths,
    } = &capture.kind
    {
        record_range_key_string_requirements(paths, capture, range_key_string_paths, range_modes);
        return;
    }
    if let crate::eval_effect::CaptureKind::CollectionItems {
        paths: collection_paths,
        schema_type,
        pattern,
    } = &capture.kind
    {
        record_collection_item_requirements(
            paths,
            capture,
            collection_paths,
            schema_type,
            pattern.as_deref(),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::IndexAccess { path, index } = &capture.kind {
        record_index_access_requirement(paths, capture, path, *index);
        return;
    }
    if let crate::eval_effect::CaptureKind::SplitIndexAccess {
        paths: source_paths,
        separator,
        index,
        total_text_preimage,
    } = &capture.kind
    {
        record_split_index_access_requirement(
            paths,
            capture,
            source_paths,
            separator,
            *index,
            *total_text_preimage,
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::ValueType {
        path,
        schema_type,
        null_aborts,
    } = &capture.kind
    {
        // The path-wide contract set intentionally loses execution scope.
        // Only this capture's conjunction can distinguish a raw consumer
        // from one skipped by its own guard or an unlowerable output selector.
        if schema_type == "string"
            && !capture
                .conjunction
                .iter()
                .any(|predicate| predicate_skips_falsy_source(predicate, path))
            && !matches!(
                capture.conjunction.as_slice(),
                [Predicate::Approximate {
                    role: super::ApproximationRole::OutputSelection,
                    sound_subset: None,
                    ..
                }]
            )
        {
            path_accumulator(paths, path)
                .facts
                .facts
                .has_non_self_guarded_string_contract = true;
        }
        record_value_requirement_capture(
            paths,
            capture,
            path,
            if *null_aborts {
                FailValueRequirement::SchemaTypeEvenNull(schema_type.clone())
            } else {
                FailValueRequirement::SchemaType(schema_type.clone())
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::RangeInput {
        path,
        destructured,
        json_decoded,
    } = &capture.kind
    {
        record_range_input_capture(paths, capture, path, *destructured, *json_decoded);
        return;
    }
    if let crate::eval_effect::CaptureKind::RangeSelection {
        path,
        chain,
        allow_integer,
    } = &capture.kind
    {
        // A `with` selection stamps one positive marker per candidate beside
        // the chain's disjunction. Helper transfer may spell those markers
        // as `Truthy` instead of `With`. Remove exactly one marker per path
        // only when the complete stamp is present; the capture's appended
        // selection tail remains, as do genuine enclosing conditions.
        let mut capture = capture.clone();
        let has_chain_disjunction = capture
            .conjunction
            .iter()
            .any(|predicate| predicate_is_truthy_disjunction_over(predicate, chain));
        let has_all_markers = chain.iter().all(|candidate| {
            capture.conjunction.iter().any(|predicate| {
                matches!(
                    predicate,
                    Predicate::Guard(Guard::Truthy { path } | Guard::With { path })
                        if path == candidate
                )
            })
        });
        if has_chain_disjunction && has_all_markers {
            let mut remaining_markers = chain.iter().cloned().collect::<BTreeSet<_>>();
            capture.conjunction.retain(|predicate| {
                if predicate_is_truthy_disjunction_over(predicate, chain) {
                    return false;
                }
                let Predicate::Guard(
                    Guard::Truthy { path: marker } | Guard::With { path: marker },
                ) = predicate
                else {
                    return true;
                };
                !remaining_markers.remove(marker)
            });
        }
        record_value_requirement_capture(
            paths,
            &capture,
            path,
            FailValueRequirement::Iterable {
                allow_integer: *allow_integer,
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::DigSubject { path } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::SchemaTypeEvenNull("object".to_string()),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::RequiredPresence { path } = &capture.kind {
        // The presence claim lands on the PARENT as a required member so
        // the arm fires when the subject itself is absent. A TOP-LEVEL
        // subject has no parent slot to carry that member, so it takes the
        // document-level absence clause instead — the same vehicle the
        // navigated-host and nil-strict-operand claims use for their own
        // top-level paths (kube-prometheus-stack's `customRules`).
        let mut segments = helm_schema_core::split_value_path(path);
        let Some(member) = segments.pop() else {
            return;
        };
        if segments.is_empty() {
            record_absence_abort_clause(terminal_clauses, capture, path);
            return;
        }
        let parent = segments.join(".");
        record_value_requirement_capture(
            paths,
            capture,
            &parent,
            FailValueRequirement::HasMemberEvenDefaulted(member),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::AbsenceAborts { path } = &capture.kind {
        // A ranged MEMBER FIELD's absence has no document-level spelling —
        // the clause would have to name one visited member — but the member's
        // own slot states it exactly, as a required member of every visited
        // member (the minio chart's `tpl .accessKey $`). The member itself
        // (a bare `A.*`) keeps no claim: a range only visits members that
        // exist.
        if path_contains_wildcard(path) {
            let mut segments = helm_schema_core::split_value_path(path);
            let Some(member) = segments.pop() else {
                return;
            };
            if member == "*" || segments.is_empty() {
                return;
            }
            // A gate on the operand's OWN truthiness excludes absence
            // outright, so the claim would only restate the gate (grafana
            // reads `tpl .prefix $` inside `if .prefix`). This is the
            // member-quantified case of the same self-mention rule the
            // document-level clause applies.
            if capture.conjunction.iter().any(|predicate| {
                matches!(
                    predicate,
                    Predicate::Guard(Guard::Truthy { path: guard } | Guard::With { path: guard })
                        if guard == path
                )
            }) {
                return;
            }
            record_value_requirement_capture(
                paths,
                capture,
                &segments.join("."),
                FailValueRequirement::HasMemberEvenDefaulted(member),
            );
            return;
        }
        record_absence_abort_clause(terminal_clauses, capture, path);
        return;
    }
    if let crate::eval_effect::CaptureKind::ComparableKind { path, schema_type } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::ComparableKind(schema_type.clone()),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::ValuePattern {
        path,
        pattern,
        templated,
    } = &capture.kind
    {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::MatchesPattern {
                pattern: pattern.clone(),
                templated: *templated,
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::QuotedSerialization {
        path,
        style,
        templated,
    } = &capture.kind
    {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::QuotedSerializationSafe {
                style: *style,
                templated: *templated,
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::PrintfStringOperand { path } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::PrintfStringOperand,
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::PlainSlotText {
        path,
        token_initial,
        templated,
    } = &capture.kind
    {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::PlainScalarSafe {
                token_initial: *token_initial,
                templated: *templated,
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::RangeKeyPlainSlot {
        paths: collection_paths,
    } = &capture.kind
    {
        record_range_key_plain_slot_requirements(paths, capture, collection_paths, range_modes);
        return;
    }
    if let crate::eval_effect::CaptureKind::MemberAccess { handled_kinds } = &capture.kind {
        record_member_access_capture(paths, capture, handled_kinds, range_modes);
        return;
    }
    // An approximate enclosing condition abstains unless it admits a sound
    // positive strengthening (it can only ever be an OUTER guard — the
    // requirement extraction below never negates one), and a `$local` name
    // leaking into predicate paths means the condition lowering lost the
    // real subject: both make lowering unsound for the whole capture.
    let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
    if conjunction.iter().any(|predicate| {
        predicate.contains_approximation() && fail_outer_guard(predicate).is_none()
    }) {
        return;
    }
    if conjunction
        .iter()
        .flat_map(Predicate::value_paths)
        .any(|path| path.starts_with('$'))
    {
        return;
    }
    if record_range_key_prefix_requirement(paths, &capture.kind, &conjunction) {
        return;
    }
    if record_range_key_matches_requirement(paths, &capture.kind, &conjunction) {
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
    let conjunction: Vec<Predicate> = conjunction
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
    let execution_range_paths = conjunction.iter().filter_map(|predicate| match predicate {
        Predicate::Guard(Guard::Range { path }) => Some(path.as_str()),
        _ => None,
    });
    let test_candidate_paths = conjunction
        .iter()
        .filter(|predicate| predicate_is_negatable_test(predicate))
        .flat_map(Predicate::value_paths)
        .collect::<BTreeSet<_>>();
    // Member scope and execution are separate facts. A direct range usually
    // carries both as `Range(path)`, but a derived iterable can retain
    // values-backed members while a literal overlay or concat controls when
    // the body executes. `capture.ranged` names the former; conjunction
    // predicates name only the latter.
    let ranged = execution_range_paths
        .filter(|path| {
            range_modes.mode(path).member_identity || capture.ranged.mode(path).member_identity
        })
        .chain(
            capture
                .ranged
                .iter()
                .filter_map(|(path, mode)| mode.member_identity.then_some(path)),
        )
        .filter(|path| {
            let member = format!("{path}.*");
            test_candidate_paths.iter().any(|candidate| {
                candidate == &member
                    || helm_schema_core::values_path_is_descendant(candidate, &member)
            })
        })
        .max_by_key(|path| helm_schema_core::split_value_path(path).len());
    let member_scope = ranged.map(|path| format!("{path}.*"));

    let mut outer_guards = Vec::new();
    let mut member_tests: Vec<&Predicate> = Vec::new();
    let mut requirements = Vec::new();
    let mut test_paths: BTreeSet<String> = BTreeSet::new();
    for predicate in conjunction {
        if let Predicate::Guard(Guard::Range { path }) = predicate {
            if ranged == Some(path.as_str()) {
                continue;
            }
            // An iteration conjunct outside the member test: the body
            // executed, so a DIRECTLY ranged collection is Helm-truthy
            // (a truthy non-collection aborts the range and never reaches
            // a render-valid document). Indirect ranges lose that
            // implication and abstain.
            if capture.ranged.mode(path).input_identity {
                outer_guards.push(ConditionalGuard::Truthy { path: path.clone() });
                continue;
            }
            return;
        }
        let paths_of = predicate.value_paths();
        // A conjunct is part of the failing TEST when it scopes to the
        // ranged member (or, without a range, to a single path) AND its
        // negation states an enforceable requirement; everything else is
        // an outer condition of the arm.
        if let Some(scope) = &member_scope {
            if !paths_of.is_empty()
                && paths_of
                    .iter()
                    .all(|path| path == scope || path.starts_with(&format!("{scope}.")))
            {
                member_tests.push(predicate);
                continue;
            }
        } else if paths_of.len() == 1 && predicate_is_negatable_test(predicate) {
            let path = paths_of.iter().next().cloned().unwrap_or_default();
            if let Some(required) =
                requirements_from_negation(predicate, &path).filter(|required| !required.is_empty())
            {
                requirements.extend(required);
                test_paths.insert(path);
                continue;
            }
        }
        let Some(guard) = fail_outer_guard(predicate) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer_guards.push(guard);
    }
    // Member tests negate against the MEMBER scope first — `required`-style
    // conjuncts name fields relative to the member — and fall back to the
    // single field path they all share (`clusters.*.name` spliced into a
    // quoted flow item), where the implication targets the members' field.
    // A member test that fits neither convention poisons the capture: the
    // requirement would be missing a dimension of the real condition.
    let mut member_field: Option<Vec<String>> = None;
    if let Some(scope) = &member_scope
        && !member_tests.is_empty()
    {
        let requirements_at = |at: &str| -> Option<Vec<Vec<FailValueRequirement>>> {
            member_tests
                .iter()
                .map(|predicate| {
                    requirements_from_negation(predicate, at)
                        .filter(|required| !required.is_empty())
                })
                .collect::<Option<Vec<_>>>()
        };
        // The fail fires only when EVERY member test holds, so validity is
        // the DISJUNCTION of their negations: one satisfied negation keeps
        // the member. A single test lowers flat; several become one AnyOf
        // (traefik's legacy-hostPath-or-typed local plugins).
        let combine = |mut alternatives: Vec<Vec<FailValueRequirement>>| {
            if alternatives.len() == 1 {
                alternatives.remove(0)
            } else {
                alternatives.sort();
                alternatives.dedup();
                vec![FailValueRequirement::AnyOf(alternatives)]
            }
        };
        if let Some(required) = requirements_at(scope) {
            requirements.extend(combine(required));
            test_paths.insert(scope.clone());
        } else {
            let field_path = {
                let mut paths: BTreeSet<String> = member_tests
                    .iter()
                    .flat_map(|predicate| predicate.value_paths())
                    .collect();
                match paths.pop_first() {
                    Some(path) if paths.is_empty() && !path[scope.len()..].contains('*') => {
                        Some(path)
                    }
                    _ => None,
                }
            };
            let Some(field_path) = field_path.filter(|path| path != scope) else {
                return;
            };
            let Some(required) = requirements_at(&field_path) else {
                return;
            };
            // The `MembersAt` target demands the field of EVERY member —
            // correct for unconditional per-member reads, wrong when the
            // failing test was scoped by the field's own truthiness: an
            // absent field is Helm-falsy and escapes the fail, so demanding
            // presence falsely rejects (minio's per-statement
            // `if $statement.conditions` len gate). Such captures abstain.
            if required
                .iter()
                .flatten()
                .any(|requirement| matches!(requirement, FailValueRequirement::HelmFalsy))
            {
                return;
            }
            member_field = Some(helm_schema_core::split_value_path(
                &field_path[scope.len() + 1..],
            ));
            requirements.extend(combine(required));
            test_paths.insert(field_path);
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
        if ranged.is_none() && conjunction.is_empty() {
            if !terminal_clauses.iter().any(Vec::is_empty) {
                terminal_clauses.push(Vec::new());
            }
        } else if ranged.is_none()
            && !conjunction
                .iter()
                .any(|predicate| matches!(predicate, Predicate::Guard(Guard::Range { .. })))
        {
            let clause = conjunction
                .iter()
                .map(terminal_clause_guard)
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
    let target = if let Some(path) = ranged {
        path.to_string()
    } else {
        let Some(path) = test_paths.into_iter().next() else {
            return;
        };
        path
    };
    requirements.sort();
    requirements.dedup();
    // A test whose requirements contradict (a type-dispatch arm's own
    // partition conjunct joins the test on the same path) can never fire;
    // its arm would encode as a tautology, so it is dropped as noise.
    let contradictory = requirements.iter().any(|requirement| {
        matches!(
            requirement,
            FailValueRequirement::SchemaType(schema_type)
                if requirements
                    .contains(&FailValueRequirement::NotSchemaType(schema_type.clone()))
        )
    });
    if contradictory {
        return;
    }
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ranged.map_or(ContractRequirementTarget::Value, |path| {
            let mode = range_modes.mode(path);
            let allow_integer = !mode.destructured && !mode.json_decoded;
            match member_field {
                Some(target_path) => ContractRequirementTarget::MembersAt {
                    target_path,
                    allow_integer,
                },
                None => ContractRequirementTarget::Members { allow_integer },
            }
        }),
        requirements,
    };
    let acc = path_accumulator(paths, &target);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

pub(super) fn record_range_key_prefix_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    kind: &crate::eval_effect::CaptureKind,
    conjunction: &[Predicate],
) -> bool {
    if !matches!(kind, crate::eval_effect::CaptureKind::Fail) {
        return false;
    }
    let prefixes = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::RangeKeyPrefix { path, prefix }) => {
                Some((path.as_str(), prefix.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(collection_path, prefix)] = prefixes.as_slice() else {
        return !prefixes.is_empty();
    };
    let member_scope = format!("{collection_path}.*");
    let has_matching_range = conjunction.iter().any(|predicate| {
        matches!(predicate, Predicate::Guard(Guard::Range { path }) if path == collection_path)
    });
    if !has_matching_range {
        return true;
    }

    let mut outer_guards = Vec::new();
    let mut requirements = Vec::new();
    for predicate in conjunction {
        match predicate {
            Predicate::Guard(Guard::RangeKeyPrefix {
                path,
                prefix: candidate,
            }) if path == collection_path && candidate == prefix => {}
            Predicate::Guard(Guard::Range { path }) if path == collection_path => {}
            _ if {
                let predicate_paths = predicate.value_paths();
                !predicate_paths.is_empty()
                    && predicate_paths.iter().all(|path| {
                        path == &member_scope
                            || helm_schema_core::values_path_is_descendant(path, &member_scope)
                    })
            } =>
            {
                let Some(mut required) = requirements_from_negation(predicate, &member_scope)
                else {
                    return true;
                };
                requirements.append(&mut required);
            }
            _ => {
                let Some(guard) = fail_outer_guard(predicate) else {
                    return true;
                };
                if guard
                    .value_paths()
                    .iter()
                    .any(|path| path_contains_wildcard(path))
                {
                    return true;
                }
                outer_guards.push(guard);
            }
        }
    }
    if requirements.is_empty() {
        return true;
    }
    outer_guards.sort();
    outer_guards.dedup();
    requirements.sort();
    requirements.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::MembersMatchingPrefix {
            prefix: (*prefix).to_string(),
        },
        requirements,
    };
    let acc = path_accumulator(paths, collection_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
    true
}

/// Lower a fail conjunction keyed on a range-KEY regex: rendering aborts
/// for any key matching (or, negated, failing) the pattern, so the
/// collection's key domain excludes it (traefik fails on uppercase
/// `ingressRoute` keys). Bounded to a single key-pattern conjunct with no
/// other member-scoped tests; anything richer abstains.
pub(super) fn record_range_key_matches_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    kind: &crate::eval_effect::CaptureKind,
    conjunction: &[Predicate],
) -> bool {
    fn key_match(predicate: &Predicate) -> Option<(bool, &str, &str)> {
        match predicate {
            Predicate::Guard(Guard::RangeKeyMatches { path, pattern }) => {
                Some((false, path.as_str(), pattern.as_str()))
            }
            Predicate::Not(inner) => match inner.as_ref() {
                Predicate::Guard(Guard::RangeKeyMatches { path, pattern }) => {
                    Some((true, path.as_str(), pattern.as_str()))
                }
                _ => None,
            },
            _ => None,
        }
    }

    if !matches!(kind, crate::eval_effect::CaptureKind::Fail) {
        return false;
    }
    let matches: Vec<(bool, &str, &str)> = conjunction.iter().filter_map(key_match).collect();
    let [(negated, collection_path, pattern)] = matches.as_slice() else {
        return !matches.is_empty();
    };
    let has_matching_range = conjunction.iter().any(|predicate| {
        matches!(predicate, Predicate::Guard(Guard::Range { path }) if path == collection_path)
    });
    if !has_matching_range {
        return true;
    }
    let member_scope = format!("{collection_path}.*");
    let mut outer_guards = Vec::new();
    for predicate in conjunction {
        if key_match(predicate).is_some() {
            continue;
        }
        match predicate {
            Predicate::Guard(Guard::Range { path }) if path == collection_path => {}
            _ if predicate.value_paths().iter().any(|path| {
                path == &member_scope
                    || helm_schema_core::values_path_is_descendant(path, &member_scope)
            }) =>
            {
                return true;
            }
            _ => {
                let Some(guard) = fail_outer_guard(predicate) else {
                    return true;
                };
                if guard
                    .value_paths()
                    .iter()
                    .any(|path| path_contains_wildcard(path))
                {
                    return true;
                }
                outer_guards.push(guard);
            }
        }
    }
    outer_guards.sort();
    outer_guards.dedup();
    let requirement = if *negated {
        FailValueRequirement::MatchesPattern {
            pattern: (*pattern).to_string(),
            templated: false,
        }
    } else {
        FailValueRequirement::NotMatchesPattern {
            pattern: (*pattern).to_string(),
        }
    };
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::Keys,
        requirements: vec![requirement],
    };
    let acc = path_accumulator(paths, collection_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
    true
}

pub(super) fn capture_outer_guards(
    capture: &crate::eval_effect::FailCapture,
) -> Option<Vec<ConditionalGuard>> {
    let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
    // A key-equality conjunct subsumes its companion iteration conjunct:
    // the has-key lowering already implies the range reaches that member.
    let key_equals_ranges: BTreeSet<&str> = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::RangeKeyEquals { path, .. }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let mut guards = conjunction
        .iter()
        .filter(|predicate| {
            !matches!(
                predicate,
                Predicate::Guard(Guard::Range { path }) if key_equals_ranges.contains(path.as_str())
            )
        })
        .map(|predicate| match predicate {
            // An iteration conjunct: the body executed, so a DIRECTLY
            // ranged collection is Helm-truthy (a truthy non-collection
            // aborts the range and never renders). Indirect ranges lose
            // that implication and abstain.
            Predicate::Guard(Guard::Range { path }) => capture
                .ranged
                .mode(path)
                .input_identity
                .then(|| ConditionalGuard::Truthy { path: path.clone() }),
            predicate => fail_outer_guard(predicate),
        })
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
    Some(guards)
}

/// `ConditionalGuard` for one OUTER conjunct of a FAIL implication.
///
/// Fail polarity is positive-only: the emitted guard may hold LESS often
/// than the real condition — the arm then rejects fewer inputs — but never
/// more. That admits bounded strengthenings an exact row guard cannot use:
/// a positive approximate conjunct lowers through its recognized sound
/// subset, and a NEGATED conjunction lowers as the negation of its
/// decodable conjuncts (dropping conjuncts weakens a conjunction, so
/// negating the remainder fires less often than negating all of it —
/// cilium's `else if` log-level arm behind a `has … (splitList …)` chain).
pub(super) fn fail_outer_guard(predicate: &Predicate) -> Option<ConditionalGuard> {
    if !predicate.contains_approximation() {
        return predicate_to_guard(predicate, None);
    }
    match predicate {
        Predicate::Approximate {
            sound_subset: Some(sound_subset),
            ..
        } => predicate_to_guard(sound_subset, None),
        Predicate::Not(inner) => {
            let Predicate::And(items) = inner.as_ref() else {
                return None;
            };
            let mut decodable: Vec<ConditionalGuard> = items
                .iter()
                .filter(|item| !item.contains_approximation())
                .filter_map(|item| predicate_to_guard(item, None))
                .collect();
            decodable.sort();
            decodable.dedup();
            let inner = match decodable.as_slice() {
                [] => return None,
                [guard] => guard.clone(),
                _ => ConditionalGuard::AllOf(decodable),
            };
            Some(ConditionalGuard::Not(Box::new(inner)))
        }
        // A disjunction lowers arm-by-arm: each strengthened arm implies
        // its real arm, so their disjunction implies the real disjunction
        // (jenkins' `or (lt $replicas 0) (gt $replicas 1)` domain check).
        // An undecodable arm DROPS instead of vetoing the whole guard —
        // the remaining arms are a subset of the disjunction, so the fail
        // arm fires less often, never more (airflow's `or .Values.labels
        // <merged workers labels>` mustMerge gate, whose merged disjunct
        // has no flat-guard spelling).
        Predicate::Or(items) => {
            let mut guards = items
                .iter()
                .filter_map(fail_outer_guard)
                .collect::<Vec<_>>();
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AnyOf(guards)),
            }
        }
        // A conjunction lowers all-or-nothing: each conjunct's
        // strengthening strengthens the whole, but DROPPING a conjunct
        // would weaken it (fire more often), so an undecodable conjunct
        // vetoes the guard (cilium's `and (ge (int .Values.cluster.id)
        // 128) (le (int .Values.cluster.id) 255)` ENI window arms).
        Predicate::And(items) => {
            let mut guards = items
                .iter()
                .map(fail_outer_guard)
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        _ => None,
    }
}

/// The escape alternative a per-member type dispatch leaves open: members the
/// tested branch does not claim satisfy the negated test instead. Only the
/// exact `typeIs` partition qualifies — its complement is a plain type
/// alternative, which every other member condition (truthiness, equality,
/// member reads) cannot spell without losing soundness.
pub(super) fn member_dispatch_escape(
    predicate: &Predicate,
    member_path: &str,
) -> Option<FailValueRequirement> {
    let (negated, guard) = match predicate {
        Predicate::Guard(guard) => (false, guard),
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Guard(guard) => (true, guard),
            _ => return None,
        },
        _ => return None,
    };
    let (schema_type, excluded) = match guard {
        Guard::TypeIs { path, schema_type } if path == member_path => (schema_type, false),
        Guard::NotTypeIs { path, schema_type } if path == member_path => (schema_type, true),
        _ => return None,
    };
    if negated == excluded {
        Some(FailValueRequirement::NotSchemaType(schema_type.clone()))
    } else {
        Some(FailValueRequirement::SchemaType(schema_type.clone()))
    }
}

/// Whether the two requirements partition every value, which makes their
/// alternation accept everything.
pub(super) fn complements_requirement(
    escape: &FailValueRequirement,
    requirement: &FailValueRequirement,
) -> bool {
    match (escape, requirement) {
        (
            FailValueRequirement::NotSchemaType(excluded),
            FailValueRequirement::SchemaType(required),
        )
        | (
            FailValueRequirement::SchemaType(required),
            FailValueRequirement::NotSchemaType(excluded),
        ) => excluded == required,
        _ => false,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn record_value_requirement_capture(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
    mut requirement: FailValueRequirement,
) {
    if path.trim().is_empty() {
        return;
    }
    let path_segments = helm_schema_core::split_value_path(path);
    if let Some(first_wildcard) = path_segments.iter().position(|segment| segment == "*") {
        let (collection_segments, wildcard_tail) = path_segments.split_at(first_wildcard);
        let wildcard_count = wildcard_tail
            .iter()
            .take_while(|segment| segment.as_str() == "*")
            .count();
        let suffix = wildcard_tail.get(wildcard_count..).unwrap_or_default();
        if wildcard_count >= 2 && !suffix.iter().any(|segment| segment == "*") {
            let collection_path = helm_schema_core::join_value_path(collection_segments);
            if collection_path.is_empty() {
                return;
            }
            let ranged_collections = (0..wildcard_count)
                .map(|depth| {
                    let mut segments = collection_segments.to_vec();
                    segments.extend(std::iter::repeat_n("*".to_string(), depth));
                    helm_schema_core::join_value_path(segments)
                })
                .collect::<BTreeSet<_>>();
            if ranged_collections.iter().any(|path| {
                let mode = capture.ranged.mode(path);
                !mode.member_identity || (!mode.destructured && !mode.json_decoded)
            }) {
                return;
            }

            let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
            let mut outer_guards = Vec::new();
            for predicate in &conjunction {
                if matches!(
                    predicate,
                    Predicate::Guard(Guard::Range { path })
                        if ranged_collections.contains(path)
                ) {
                    continue;
                }
                let Some(guard) = fail_outer_guard(predicate) else {
                    return;
                };
                if guard
                    .value_paths()
                    .iter()
                    .any(|path| path_contains_wildcard(path))
                {
                    return;
                }
                outer_guards.push(guard);
            }
            outer_guards.sort();
            outer_guards.dedup();

            let mut target_path =
                std::iter::repeat_n("*".to_string(), wildcard_count - 1).collect::<Vec<_>>();
            target_path.extend(suffix.iter().cloned());
            let implication = ContractFailImplication {
                outer_guards,
                target: ContractRequirementTarget::MembersAt {
                    target_path,
                    allow_integer: false,
                },
                requirements: vec![requirement],
            };
            let acc = path_accumulator(paths, &collection_path);
            acc.referenced = true;
            if !acc.fail_implications.contains(&implication) {
                acc.fail_implications.push(implication);
            }
            return;
        }
    }
    // `A.*.field` names one field of EVERY ranged member of `A`: the
    // requirement lowers per member at that relative path (prometheus's
    // `tpl $remoteWrite.url` over `server.remoteWrite.*.url`).
    let member_field_split = path.split_once(".*.").filter(|(collection, suffix)| {
        !collection.contains('*') && !suffix.is_empty() && !suffix.contains('*')
    });
    if let Some((collection_path, member_suffix)) = member_field_split {
        let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
        let key_equals_ranges: BTreeSet<&str> = conjunction
            .iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::RangeKeyEquals { path, .. }) => Some(path.as_str()),
                _ => None,
            })
            .collect();
        let mut outer_guards = Vec::new();
        let mut self_truthy_selected = false;
        let mut member_selector = None;
        for predicate in &conjunction {
            match predicate {
                Predicate::Guard(Guard::Range { path })
                    if path == collection_path || key_equals_ranges.contains(path.as_str()) => {}
                // The consumer's own truthiness selection over the ranged
                // member cannot become an outer guard (its path is
                // per-member); it scopes the requirement to truthy member
                // values instead (`.password | default ""` behind
                // `if $password` reaching `sha256sum`).
                Predicate::Guard(Guard::Truthy { path: guard_path }) if guard_path == path => {
                    self_truthy_selected = true;
                }
                _ => {
                    if let Some(relative) =
                        member_local_truthy_selector(collection_path, predicate, Some(path))
                    {
                        if member_selector.replace(relative).is_some() {
                            return;
                        }
                        continue;
                    }
                    // Fail polarity admits strengthened guards: the arm then
                    // enforces its requirement less often, never more (the
                    // document-level `if (include "redis.createConfigmap" .)`
                    // wrapping the ACL users range).
                    let Some(guard) = fail_outer_guard(predicate) else {
                        return;
                    };
                    if guard
                        .value_paths()
                        .iter()
                        .any(|path| path_contains_wildcard(path))
                    {
                        return;
                    }
                    outer_guards.push(guard);
                }
            }
        }
        let requirement = if self_truthy_selected {
            match requirement {
                FailValueRequirement::SchemaType(schema_type) => {
                    FailValueRequirement::TruthyImpliesSchemaType(schema_type)
                }
                // Only the plain type requirement has a truthy-scoped form;
                // a selection over any other requirement abstains as before.
                _ => return,
            }
        } else {
            requirement
        };
        outer_guards.sort();
        outer_guards.dedup();
        let allow_integer = {
            let mode = capture.ranged.mode(collection_path);
            mode.member_identity && !mode.destructured && !mode.json_decoded
        };
        let target_path = helm_schema_core::split_value_path(member_suffix);
        let implication = ContractFailImplication {
            outer_guards,
            target: match member_selector {
                Some(guard_path) => ContractRequirementTarget::MembersAtWhereTruthy {
                    guard_path,
                    target_path,
                    allow_integer,
                },
                None => ContractRequirementTarget::MembersAt {
                    target_path,
                    allow_integer,
                },
            },
            requirements: vec![requirement],
        };
        let acc = path_accumulator(paths, collection_path);
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
        return;
    }
    let (target_path, target, outer_guards) = if let Some(collection_path) = path.strip_suffix(".*")
    {
        let mut outer_guards = Vec::new();
        let mut prefix = None;
        let mut member_selector = None;
        let mut dispatch_escapes = Vec::new();
        for predicate in &capture.conjunction {
            match predicate {
                Predicate::Guard(Guard::Range { path }) if path == collection_path => {}
                Predicate::Guard(Guard::RangeKeyPrefix {
                    path,
                    prefix: candidate,
                }) if path == collection_path => {
                    if prefix.replace(candidate.clone()).is_some() {
                        return;
                    }
                }
                _ => {
                    // A conjunct testing the MEMBER's own kind is the chart's
                    // per-member type dispatch, not an outer guard: its path is
                    // per-member, so the requirement binds only the members the
                    // dispatch routes here and the others escape through the
                    // negated test. Traefik and Sealed Secrets render each
                    // `extraObjects`/`extraDeploy` member either as `tpl` text
                    // (a string) or as a serialized document (a mapping), and
                    // dropping the else arm's placement left the item domain
                    // open to scalars Helm cannot decode.
                    if let Some(escape) = member_dispatch_escape(predicate, path) {
                        dispatch_escapes.push(vec![escape]);
                        continue;
                    }
                    if let Some(relative) =
                        member_local_truthy_selector(collection_path, predicate, None)
                    {
                        if member_selector.replace(relative).is_some() {
                            return;
                        }
                        continue;
                    }
                    let Some(guard) = predicate_to_guard(predicate, None) else {
                        return;
                    };
                    if guard
                        .value_paths()
                        .iter()
                        .any(|path| path_contains_wildcard(path))
                    {
                        return;
                    }
                    outer_guards.push(guard);
                }
            }
        }
        if !dispatch_escapes.is_empty() {
            // The tested branch's own requirement IS the dispatch condition
            // whenever it types the member the test already selected (the
            // string arm's `tpl` operand), so the alternation would accept
            // every value: record nothing rather than a vacuous arm.
            if dispatch_escapes
                .iter()
                .flatten()
                .any(|escape| complements_requirement(escape, &requirement))
            {
                return;
            }
            let mut alternatives = vec![vec![requirement]];
            alternatives.append(&mut dispatch_escapes);
            requirement = FailValueRequirement::AnyOf(alternatives);
        }
        outer_guards.sort();
        outer_guards.dedup();
        let allow_integer = {
            let mode = capture.ranged.mode(collection_path);
            mode.member_identity && !mode.destructured && !mode.json_decoded
        };
        let target = match (prefix, member_selector) {
            (Some(prefix), _) => ContractRequirementTarget::MembersMatchingPrefix { prefix },
            // The requirement binds the member itself, so it needs no
            // relative path — only the selector deciding which members it
            // reaches.
            (None, Some(guard_path)) => ContractRequirementTarget::MembersAtWhereTruthy {
                guard_path,
                target_path: Vec::new(),
                allow_integer,
            },
            (None, None) => ContractRequirementTarget::Members { allow_integer },
        };
        (collection_path, target, outer_guards)
    } else {
        if path_contains_wildcard(path) {
            return;
        }
        let Some(outer_guards) = capture_outer_guards(capture) else {
            return;
        };
        (path, ContractRequirementTarget::Value, outer_guards)
    };
    let implication = ContractFailImplication {
        outer_guards,
        target,
        requirements: vec![requirement],
    };
    let acc = path_accumulator(paths, target_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

pub(super) fn record_collection_item_requirements(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    collection_paths: &BTreeSet<String>,
    schema_type: &str,
    pattern: Option<&str>,
) {
    let Some(outer_guards) = capture_outer_guards(capture) else {
        return;
    };
    for path in collection_paths {
        if path_contains_wildcard(path) {
            continue;
        }
        let mut requirements = vec![FailValueRequirement::SchemaType(schema_type.to_string())];
        if let Some(pattern) = pattern {
            requirements.push(FailValueRequirement::MatchesPattern {
                pattern: pattern.to_string(),
                templated: false,
            });
        }
        let implication = ContractFailImplication {
            outer_guards: outer_guards.clone(),
            target: ContractRequirementTarget::Members {
                allow_integer: false,
            },
            requirements,
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

/// Lower a nil-strict string consumer's presence claim: rendering aborts
/// wherever the capture's guards hold and the operand is absent, which is a
/// document-level terminal clause. The clause form reaches a TOP-LEVEL
/// operand (a parent member requirement has no slot for one) and carries the
/// `Absent` guard's ownership semantics.
pub(super) fn record_absence_abort_clause(
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
) {
    let segments = helm_schema_core::split_value_path(path);
    // A wildcard member has no absence to claim: the range only visits
    // members that exist.
    if segments.iter().any(|segment| segment == "*") {
        return;
    }
    // Helm injects the parent's `global` into every subchart's values root,
    // so `global` is a faithful spelling only at the document root or
    // directly under a dependency root. A DEEPER `global` segment means the
    // read resolved through a re-rooted context whose spelling this claim
    // cannot trust — and a presence claim on a key the chart never declares
    // rejects its own defaults (k8s-infra's `otelAgent.global.cloud`).
    if segments
        .iter()
        .skip(2)
        .any(|segment| segment == "global" || segment == "Values")
    {
        return;
    }
    let mut clause = Vec::new();
    for predicate in &capture.conjunction {
        let Some(guard) = terminal_clause_guard(predicate) else {
            // An enclosing condition this encoding cannot spell would make
            // the clause fire outside the states the consumer runs in.
            return;
        };
        // Any enclosing guard that talks about the OPERAND ITSELF may
        // already exclude absence, and the clause cannot tell: a
        // self-truthiness gate (`with .Values.x`, `if .Values.x`, `hasKey`)
        // excludes it outright, while a DISJUNCTIVE selection that mentions
        // the operand in one alternative excludes it only together with the
        // sibling conjuncts that kill the other alternatives
        // (cluster-autoscaler's `or $isAzure (and $isAws
        // $awsCredentialsProvided) $isCivo` around the aws `b64enc` arm).
        // Both cases abstain.
        if guard.value_paths().contains(path) {
            return;
        }
        clause.push(guard);
    }
    clause.push(ConditionalGuard::Absent {
        path: path.to_string(),
    });
    clause.sort();
    clause.dedup();
    if !terminal_clauses.contains(&clause) {
        terminal_clauses.push(clause);
    }
}

pub(super) fn record_index_access_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
    index: usize,
) {
    if path.trim().is_empty() || path_contains_wildcard(path) {
        return;
    }
    let Some(outer_guards) = capture_outer_guards(capture) else {
        return;
    };
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::Value,
        requirements: vec![FailValueRequirement::IndexableAt(index)],
    };
    let acc = path_accumulator(paths, path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

pub(super) fn record_split_index_access_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    source_paths: &BTreeSet<String>,
    separator: &str,
    index: usize,
    allow_non_string: bool,
) {
    if index == 0 || separator.is_empty() {
        return;
    }
    let outer_guards = capture_outer_guards(capture);
    for path in source_paths {
        if path.trim().is_empty() {
            continue;
        }
        if path_contains_wildcard(path) {
            record_member_relative_split_requirement(
                paths,
                capture,
                path,
                separator,
                index,
                allow_non_string,
            );
            continue;
        }
        let Some(outer_guards) = outer_guards.clone() else {
            continue;
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Value,
            requirements: vec![FailValueRequirement::SplitSegmentsAtLeast {
                separator: separator.to_string(),
                segments: index + 1,
                allow_non_string,
            }],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

pub(super) fn record_member_relative_split_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    source_path: &str,
    separator: &str,
    index: usize,
    allow_non_string: bool,
) {
    let segments = helm_schema_core::split_value_path(source_path);
    let Some(member_index) = segments.iter().rposition(|segment| segment == "*") else {
        return;
    };
    if member_index == 0 || member_index + 1 >= segments.len() {
        return;
    }
    let Some(collection_segments) = segments.get(..member_index) else {
        return;
    };
    let Some(member_segments) = segments.get(..=member_index) else {
        return;
    };
    let Some(target_path) = segments.get(member_index + 1..) else {
        return;
    };
    let collection_path = helm_schema_core::join_value_path(collection_segments.to_vec());
    let member_scope = helm_schema_core::join_value_path(member_segments.to_vec());
    let target_path = target_path.to_vec();
    let mut member_guards = Vec::new();
    let mut outer_guards = Vec::new();

    for predicate in &capture.conjunction {
        if matches!(predicate, Predicate::Guard(Guard::Range { path })
            if path == &collection_path
                || helm_schema_core::values_path_is_descendant(&member_scope, path))
        {
            continue;
        }
        if let Predicate::Guard(Guard::Eq { path, value }) = predicate
            && let Some(relative) =
                helm_schema_core::split_value_path(path).strip_prefix(member_segments)
            && !relative.is_empty()
            && !relative.iter().any(|segment| segment == "*")
        {
            member_guards.push((relative.to_vec(), value.clone()));
            continue;
        }
        let Some(guard) = predicate_to_guard(predicate, None) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer_guards.push(guard);
    }
    let [(guard_path, value)] = member_guards.as_slice() else {
        return;
    };
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::MembersWhereEquals {
            guard_path: guard_path.clone(),
            value: value.clone(),
            target_path,
        },
        requirements: vec![FailValueRequirement::SplitSegmentsAtLeast {
            separator: separator.to_string(),
            segments: index + 1,
            allow_non_string,
        }],
    };
    let acc = path_accumulator(paths, &collection_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

pub(super) fn record_range_key_string_requirements(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    range_key_string_paths: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
) {
    if capture.contains_approximation() {
        return;
    }
    for path in range_key_string_paths {
        if path_contains_wildcard(path)
            || (!range_modes.mode(path).member_identity
                && !capture.ranged.mode(path).member_identity)
            || has_selection_chain_marker_stamp(&capture.conjunction)
        {
            continue;
        }
        let Some(outer_guards) = lowerable_range_outer_guards(path, &capture.conjunction) else {
            continue;
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Keys,
            requirements: vec![FailValueRequirement::SchemaType("string".to_string())],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

/// Lower an unquoted-slot claim on a ranged collection's KEYS. The keys are
/// what renders, so the requirement rides `propertyNames` — the same lane the
/// strict-consumer key contract uses, and with the same direct-iteration
/// precondition (only a direct range has member key identities).
pub(super) fn record_range_key_plain_slot_requirements(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    collection_paths: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
) {
    if capture.contains_approximation() {
        return;
    }
    for path in collection_paths {
        if path_contains_wildcard(path)
            || (!range_modes.mode(path).member_identity
                && !capture.ranged.mode(path).member_identity)
            || has_selection_chain_marker_stamp(&capture.conjunction)
        {
            continue;
        }
        let Some(outer_guards) = lowerable_range_outer_guards(path, &capture.conjunction) else {
            continue;
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Keys,
            requirements: vec![FailValueRequirement::PlainScalarSafe {
                token_initial: true,
                templated: false,
            }],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

/// Whether a conjunct is a structurally negatable failing test. Positive
/// truthiness is excluded: the condition lowering falls back to truthy
/// approximations for conditions it cannot decode, and negating an
/// approximation would manufacture requirements the chart never stated.
pub(super) fn predicate_is_negatable_test(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Not(inner) => !matches!(inner.as_ref(), Predicate::Guard(Guard::Range { .. })),
        Predicate::Guard(Guard::TypeIs { .. } | Guard::Absent { .. } | Guard::Eq { .. }) => true,
        // A not-equals over a ranged MEMBER's field negates to the exact
        // equality (nats' jsonpatch `ne $patch.op "add"` chain, whose
        // conjunction of inequalities negates to the op enum). Absolute
        // paths stay out: a scalar-target `ne` rides the terminal-clause
        // lane, like `Eq` below.
        Predicate::Guard(Guard::NotEq { path, .. } | Guard::Truthy { path }) => path.contains(".*"),
        // A truthiness test over a ranged MEMBER's field is an exact
        // member decode: the fallback truthy stand-ins for undecodable
        // conditions ride absolute paths, never wildcard member scopes.
        Predicate::Or(items) => items.iter().all(predicate_is_negatable_test),
        _ => false,
    }
}

/// Requirements implied by the NEGATION of a failing test: the negation
/// must hold for the value at `scope` (a member scope `p.*` or the path
/// itself).
pub(super) fn requirements_from_negation(
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
        // A truthiness test over a member's FIELD negates to "the field,
        // when present, is Helm-falsy" (oauth2-proxy's legacy extraPaths
        // gate fires on `.backend.serviceName`); the member-scope
        // restriction keeps absolute-path truthy stand-ins out. The
        // member's OWN truthiness (`if $config` around a ranged terminal)
        // negates to Helm-falsiness of the member value itself.
        Predicate::Guard(Guard::Truthy { path }) if scope.contains(".*") => {
            if path == scope {
                return Some(vec![FailValueRequirement::HelmFalsy]);
            }
            let field = path.strip_prefix(&format!("{scope}."))?;
            (!field.contains('*')).then(|| {
                vec![FailValueRequirement::FieldHelmFalsy {
                    path: helm_schema_core::split_value_path(field),
                }]
            })
        }
        // ¬(field ≠ literal) is the exact equality, with presence riding
        // along exactly like the positive `eq` decode (Go's `ne` reads a
        // missing field as nil, which differs from every literal, so the
        // failing inequality HELD there — nats' jsonpatch `op` chain
        // negates to the enum of valid operations this way).
        Predicate::Guard(Guard::NotEq { path, value }) if scope.contains(".*") => {
            let field = path.strip_prefix(&format!("{scope}."))?;
            (!field.contains('*')).then(|| {
                vec![FailValueRequirement::FieldEquals {
                    path: helm_schema_core::split_value_path(field),
                    value: value.clone(),
                }]
            })
        }
        // A concrete scalar equality over a tested MEMBER negates to a
        // not-equals requirement (cilium's forbidden `extraEnv` names). A
        // scalar VALUE target keeps the equality as its selection guard —
        // the terminal-clause lane already encodes that exactly, and
        // reading it as the test would invert selection and test in
        // multi-conjunct captures (loki's default htpasswd program). Null,
        // empty-string, and float arms stay dropped — they ride `required`
        // emptiness tests whose absence semantics the tolerant-leaf
        // encoding already covers — and dropping an arm only weakens the
        // conjunction of negations, which is the safe direction.
        Predicate::Guard(Guard::Eq { path, value }) => match value {
            GuardValue::String(text)
                if path == scope && scope.contains(".*") && !text.is_empty() =>
            {
                Some(vec![FailValueRequirement::NotEquals(value.clone())])
            }
            GuardValue::Int(_) | GuardValue::Bool(_) if path == scope && scope.contains(".*") => {
                Some(vec![FailValueRequirement::NotEquals(value.clone())])
            }
            // An equality on a member FIELD negates to "the field, when
            // present, differs" — Helm's `eq` compares a missing or null
            // field against any literal without aborting, so absence
            // escapes and no presence requirement rides along (traefik's
            // non-HTTPS gateway listeners escape the certificateRefs
            // terminal through this arm). Empty-string arms stay dropped:
            // they ride `required` emptiness tests whose absence semantics
            // the tolerant-leaf encoding already covers.
            GuardValue::String(text) if scope.contains(".*") && path != scope => {
                if text.is_empty() {
                    return Some(Vec::new());
                }
                let field = path.strip_prefix(&format!("{scope}."))?;
                (!field.contains('*')).then(|| {
                    vec![FailValueRequirement::FieldNotEquals {
                        path: helm_schema_core::split_value_path(field),
                        value: value.clone(),
                    }]
                })
            }
            GuardValue::Int(_) | GuardValue::Bool(_) if scope.contains(".*") && path != scope => {
                let field = path.strip_prefix(&format!("{scope}."))?;
                (!field.contains('*')).then(|| {
                    vec![FailValueRequirement::FieldNotEquals {
                        path: helm_schema_core::split_value_path(field),
                        value: value.clone(),
                    }]
                })
            }
            _ => Some(Vec::new()),
        },
        _ => None,
    }
}

/// Requirements implied by a predicate HOLDING for the value at `scope`.
pub(super) fn requirements_from_holding(
    predicate: &Predicate,
    scope: &str,
) -> Option<Vec<FailValueRequirement>> {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, schema_type }) if path == scope => {
            Some(vec![FailValueRequirement::SchemaType(schema_type.clone())])
        }
        // `regexMatch` type-asserts a string subject, so the negated fail
        // test (`if not (regexMatch …) fail`) requires a matching string.
        Predicate::Guard(Guard::MatchesPattern {
            path,
            pattern,
            templated,
        }) if path == scope => Some(vec![FailValueRequirement::MatchesPattern {
            pattern: pattern.clone(),
            templated: *templated,
        }]),
        // The tested value's own PRESENCE holding (the `hasKey` conjunct of
        // the fail path, `¬Absent(scope)` in the conjunction): an arm
        // encoded at the value's position is vacuous when the value is
        // absent, so presence needs no spelled requirement.
        Predicate::Guard(Guard::Absent { path }) if path == scope => Some(Vec::new()),
        // The tested MEMBER's own truthiness (`and $v (kindIs "string"
        // $v)`): a falsy member — including the empty string — takes the
        // failing arm, so validity requires Helm-truthiness alongside any
        // type requirement (sealed-secrets' privateKeyAnnotations members).
        // A ranged member always EXISTS when tested, so the requirement is
        // exact per member; a scalar VALUE target instead lowers through
        // the terminal-clause lane, whose guard encoding carries the
        // absence semantics a properties-anchored arm cannot.
        Predicate::Guard(Guard::Truthy { path }) if path == scope => {
            if scope.contains(".*") {
                Some(vec![FailValueRequirement::HelmTruthy])
            } else {
                Some(Vec::new())
            }
        }
        Predicate::Guard(Guard::Truthy { path }) => {
            let member = path.strip_prefix(&format!("{scope}."))?;
            // At a ranged member scope the exact form is "present and
            // Helm-truthy", including nested fields (traefik's
            // `http.tls.enabled` beside an http3 gate); the non-member
            // lane keeps its established presence-only decode.
            if scope.contains(".*") {
                return (!member.contains('*')).then(|| {
                    vec![FailValueRequirement::FieldHelmTruthy {
                        path: helm_schema_core::split_value_path(member),
                    }]
                });
            }
            (!member.contains('.'))
                .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
        }
        // An equality on a member FIELD holding: the field is present and
        // equals the literal — Go's `eq` aborts on a nil operand, so
        // presence rides along (traefik's `eq $plugin.type "hostPath"`
        // dispatch arms negate their else-`fail` this way).
        Predicate::Guard(Guard::Eq { path, value }) if scope.contains(".*") => {
            let field = path.strip_prefix(&format!("{scope}."))?;
            (!field.contains('*')).then(|| {
                vec![FailValueRequirement::FieldEquals {
                    path: helm_schema_core::split_value_path(field),
                    value: value.clone(),
                }]
            })
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

/// Records one member-access capture (`[outer…, ¬object(P)]`).
///
/// Exact execution predicates remain predicates until every access for the
/// path has been unioned and normalized. An approximate condition may retain
/// its sound subset as a non-owning arm: that arm can reject only states
/// where navigation certainly executes, while its incompleteness cannot
/// change the host's base ownership.
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn record_member_access_capture(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    handled_kinds: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
) {
    let incomplete = capture.contains_approximation();
    let mut target = None;
    for predicate in &capture.conjunction {
        if let Predicate::Not(inner) = predicate
            && let Predicate::Guard(Guard::TypeIs { path, schema_type }) = inner.as_ref()
            && schema_type == "object"
        {
            target = Some(path.clone());
        }
    }
    let Some(target) = target else {
        return;
    };
    if let Some(parent) = target.strip_suffix(".*")
        && !path_contains_wildcard(parent)
    {
        if incomplete {
            return;
        }
        if !capture.ranged.mode(parent).member_identity {
            return;
        }
        let mut outer_guards = Vec::new();
        for predicate in &capture.conjunction {
            if matches!(
                predicate,
                Predicate::Guard(Guard::Range { path }) if path == parent
            ) || matches!(
                predicate,
                Predicate::Not(inner)
                    if matches!(
                        inner.as_ref(),
                        Predicate::Guard(Guard::TypeIs { path, schema_type })
                            if path == &target && schema_type == "object"
                    )
            ) {
                continue;
            }
            let Some(guard) = predicate_to_guard(predicate, None) else {
                return;
            };
            if guard
                .value_paths()
                .iter()
                .any(|path| path_contains_wildcard(path))
            {
                return;
            }
            outer_guards.push(guard);
        }
        outer_guards.sort();
        outer_guards.dedup();
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Members {
                allow_integer: {
                    let mode = range_modes.mode(parent);
                    let capture_mode = capture.ranged.mode(parent);
                    !mode.destructured
                        && !capture_mode.destructured
                        && !mode.json_decoded
                        && !capture_mode.json_decoded
                },
            },
            requirements: vec![FailValueRequirement::SchemaType("object".to_string())],
        };
        let acc = path_accumulator(paths, parent);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
        return;
    }
    if path_contains_wildcard(&target) {
        return;
    }
    let mut outer = Vec::new();
    let mut condition_lowerable = true;
    for predicate in &capture.conjunction {
        match predicate {
            Predicate::Not(inner)
                if matches!(
                    inner.as_ref(),
                    Predicate::Guard(Guard::TypeIs { path, schema_type })
                        if path == &target && schema_type == "object"
                ) =>
            {
                continue;
            }
            // A `with` gate enters only when its path is truthy: the same
            // condition the guard encoding can spell.
            Predicate::Guard(Guard::With { path }) if !path_contains_wildcard(path) => {
                outer.push(Predicate::truthy_path(path.clone()));
                continue;
            }
            _ => {}
        }
        let predicate = if predicate.contains_approximation() {
            let subset = TruthCondition::from_predicate(predicate.clone()).when_true();
            if subset == Predicate::False {
                condition_lowerable = false;
                break;
            }
            subset
        } else {
            predicate.clone()
        };
        let Some(guard) = predicate_to_guard(&predicate, None) else {
            condition_lowerable = false;
            break;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            condition_lowerable = false;
            break;
        }
        outer.push(predicate);
    }
    let access_conditions = &mut path_accumulator(paths, &target).member_access_conditions;
    if condition_lowerable {
        access_conditions.record(
            handled_kinds.iter().cloned().collect(),
            GuardDnf::from_conjunction(outer),
            !incomplete,
        );
    } else {
        access_conditions.mark_incomplete();
    }
}

/// Project normalized member-access predicates into schema-lowerable guards.
type MemberAccessGuardSets = BTreeMap<Vec<String>, BTreeSet<Vec<ConditionalGuard>>>;

pub(super) fn lower_member_access_condition(
    condition: &GuardDnf,
) -> Option<BTreeSet<Vec<ConditionalGuard>>> {
    let mut guard_sets = BTreeSet::new();
    for conjunction in condition.disjuncts() {
        let mut guards = Vec::new();
        for predicate in conjunction {
            let guard = predicate_to_guard(predicate, None)?;
            if guard
                .value_paths()
                .iter()
                .any(|path| path_contains_wildcard(path))
            {
                return None;
            }
            guards.push(guard);
        }
        guards.sort();
        guards.dedup();
        guard_sets.insert(guards);
    }
    Some(factor_guard_sets(guard_sets))
}

/// Spell an arm set as the outer guards of one implication: no guards at all
/// when some access is unconditional, that access's own conjuncts when there
/// is exactly one arm, and an any-of otherwise.
pub(super) fn fold_member_access_arms(
    guard_sets: BTreeSet<Vec<ConditionalGuard>>,
) -> Vec<ConditionalGuard> {
    let mut outer_guards = Vec::new();
    if guard_sets.contains(&Vec::new()) {
        return outer_guards;
    }
    let mut arms: Vec<ConditionalGuard> = guard_sets
        .into_iter()
        .map(|mut set| {
            if set.len() == 1 {
                set.remove(0)
            } else {
                ConditionalGuard::AllOf(set)
            }
        })
        .collect();
    if arms.len() == 1 {
        match arms.remove(0) {
            ConditionalGuard::AllOf(set) => outer_guards.extend(set),
            guard => outer_guards.push(guard),
        }
    } else {
        outer_guards.push(ConditionalGuard::AnyOf(arms));
    }
    outer_guards.sort();
    outer_guards.dedup();
    outer_guards
}

/// Exact Boolean factoring over a disjunction of guard conjunctions,
/// deliberately bounded to the dependency-activation shape: two
/// conjunctions differing ONLY in `Truthy(p)` versus `Absent(p)` of one
/// path fold to `X ∧ (Truthy(p) ∨ Absent(p))` — Helm's "condition path
/// set-truthy or missing" activation state. Applied to fixpoint so a
/// nested activation product stays factored instead of repeating the same
/// access condition for every dependency clone.
pub(super) fn factor_guard_sets(
    sets: BTreeSet<Vec<ConditionalGuard>>,
) -> BTreeSet<Vec<ConditionalGuard>> {
    let mut sets: Vec<Vec<ConditionalGuard>> = sets.into_iter().collect();
    loop {
        let mut merged = None;
        'search: for left in 0..sets.len() {
            for right in left + 1..sets.len() {
                let Some(left_set) = sets.get(left) else {
                    continue;
                };
                let Some(right_set) = sets.get(right) else {
                    continue;
                };
                if let Some(folded) = fold_activation_pair(left_set, right_set) {
                    merged = Some((left, right, folded));
                    break 'search;
                }
            }
        }
        let Some((left, right, folded)) = merged else {
            break;
        };
        sets.remove(right);
        if let Some(left_set) = sets.get_mut(left) {
            *left_set = folded;
        }
    }
    sets.into_iter().collect()
}

pub(super) fn fold_activation_pair(
    left: &[ConditionalGuard],
    right: &[ConditionalGuard],
) -> Option<Vec<ConditionalGuard>> {
    let only_left: Vec<&ConditionalGuard> =
        left.iter().filter(|guard| !right.contains(guard)).collect();
    let only_right: Vec<&ConditionalGuard> =
        right.iter().filter(|guard| !left.contains(guard)).collect();
    let ([left_guard], [right_guard]) = (only_left.as_slice(), only_right.as_slice()) else {
        return None;
    };
    let activation_pair = |a: &ConditionalGuard, b: &ConditionalGuard| match (a, b) {
        (
            ConditionalGuard::Truthy { path: truthy_path },
            ConditionalGuard::Absent { path: absent_path },
        ) => (truthy_path == absent_path).then(|| {
            let mut alternatives = vec![a.clone(), b.clone()];
            alternatives.sort();
            ConditionalGuard::AnyOf(alternatives)
        }),
        _ => None,
    };
    let folded_guard = activation_pair(left_guard, right_guard)
        .or_else(|| activation_pair(right_guard, left_guard))?;
    let mut folded: Vec<ConditionalGuard> = left
        .iter()
        .filter(|guard| guard != left_guard)
        .cloned()
        .collect();
    folded.push(folded_guard);
    folded.sort();
    folded.dedup();
    Some(folded)
}

/// Whether the guard can only hold while `path` is present and non-null,
/// so a navigation scoped by it never runs on a nil receiver. `Absent`
/// deliberately counts null as absent, matching the guard encoding.
pub(super) fn guard_implies_present(guard: &ConditionalGuard, path: &str) -> bool {
    match guard {
        ConditionalGuard::Truthy { path: guarded } | ConditionalGuard::With { path: guarded } => {
            guarded == path
        }
        ConditionalGuard::TypeIs {
            path: guarded,
            schema_type,
        } => guarded == path && schema_type != "null",
        ConditionalGuard::HasKey { path: host, key } => {
            // The key is an OPAQUE property name (it may contain dots), so
            // it must be appended as one escaped segment, not concatenated.
            helm_schema_core::append_value_path(host, key) == path
        }
        ConditionalGuard::Not(inner) => {
            matches!(inner.as_ref(), ConditionalGuard::Absent { path: guarded } if guarded == path)
        }
        ConditionalGuard::AllOf(set) => set.iter().any(|guard| guard_implies_present(guard, path)),
        ConditionalGuard::AnyOf(set) => set.iter().all(|guard| guard_implies_present(guard, path)),
        _ => false,
    }
}

pub(super) fn record_member_access_implications(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
) {
    // Helm rebuilds a MISSING or null dependency values root from the
    // subchart's own defaults (`coalesceDeps` creates the table, then the
    // subchart coalesces into it), so a root itself never reaches a consumer
    // as nil and its clause would be a contradiction. A deletion INSIDE a
    // present root does stick and does abort; that claim rides the `Absent`
    // encoding, which anchors every dependency-owned absence on the root
    // being present as a table.
    let dependency_roots: BTreeSet<String> = paths
        .iter()
        .filter(|(_, acc)| acc.facts.facts.accepted_dependency_values_root_fragment)
        .map(|(path, _)| path.clone())
        .collect();
    let pending: Vec<(String, MemberAccessConditions)> = paths
        .iter()
        .filter(|(path, acc)| {
            !acc.member_access_conditions.is_empty() && !path_contains_wildcard(path)
        })
        .map(|(path, acc)| (path.clone(), acc.member_access_conditions.clone()))
        .collect();
    for (path, conditions) in pending {
        let mut exact_guard_sets = MemberAccessGuardSets::new();
        for (kinds, condition) in &conditions.exact_by_handled_kinds {
            if let Some(guard_sets) = lower_member_access_condition(condition) {
                exact_guard_sets.insert(kinds.clone(), guard_sets);
            }
        }
        let mut partial_guard_sets = MemberAccessGuardSets::new();
        for (kinds, condition) in &conditions.partial_by_handled_kinds {
            if let Some(guard_sets) = lower_member_access_condition(condition) {
                partial_guard_sets.insert(kinds.clone(), guard_sets);
            }
        }

        let exact_domain_is_complete = !conditions.saw_incomplete_access
            && exact_guard_sets.len() == conditions.exact_by_handled_kinds.len();
        for (guard_sets_by_kind, complete_domain) in [
            (&exact_guard_sets, exact_domain_is_complete),
            (&partial_guard_sets, false),
        ] {
            for (handled_kinds, guard_sets) in guard_sets_by_kind {
                let outer_guards = fold_member_access_arms(guard_sets.clone());
                let implication = ContractFailImplication {
                    outer_guards,
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::MemberHost {
                        handled_kinds: handled_kinds.clone(),
                        complete_domain,
                    }],
                };
                let acc = path_accumulator(paths, &path);
                if !acc.fail_implications.contains(&implication) {
                    acc.fail_implications.push(implication);
                }
            }
        }

        if dependency_roots.contains(&path) {
            continue;
        }
        let mut combined_condition = GuardDnf::never();
        for condition in conditions
            .exact_by_handled_kinds
            .into_values()
            .chain(conditions.partial_by_handled_kinds.into_values())
        {
            combined_condition.union_absorbing(condition);
        }
        let absent_abort_sets = lower_member_access_condition(&combined_condition)
            .unwrap_or_default()
            .into_iter()
            // A set scoping the read by the host's own presence contributes
            // no absent-abort state: the nil-safe grouped form
            // (`(.Values.x).member`) and `with` chains render at an absent
            // or null-deleted receiver.
            .filter(|guards| {
                !guards
                    .iter()
                    .any(|guard| guard_implies_present(guard, &path))
            })
            .collect::<BTreeSet<_>>();
        if absent_abort_sets.is_empty() {
            continue;
        }
        // Navigation ABORTS on a nil receiver, so a host read outside its
        // own presence gate must exist. The claim lands as a terminal
        // clause rather than a `required` member on the parent: `Absent`
        // spells the ownership semantics a properties-anchored arm cannot,
        // the clause anchors above every union lane, and it reaches
        // TOP-LEVEL hosts, which have no parent slot to carry a member
        // requirement.
        let mut clause = fold_member_access_arms(absent_abort_sets);
        clause.push(ConditionalGuard::Absent { path });
        clause.sort();
        clause.dedup();
        if !terminal_clauses.contains(&clause) {
            terminal_clauses.push(clause);
        }
    }
}
