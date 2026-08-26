use std::collections::{BTreeMap, BTreeSet};

use crate::contract::FinalizedContract;
use crate::contract_normalization::{
    canonicalize_contract_uses, drop_default_guard_subsumed_duplicates,
    drop_self_truthy_subsumed_duplicates, normalize_contract_uses,
};
use crate::observed_facts::{
    ActivatedValuesDefaultSource, ActivatedValuesRootOverlay, HintGrade, ObservedFacts,
};
use crate::{ContractUse, Guard, ValueKind, YamlPath};

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
    dependency_uses: Vec<ContractUse>,
    observed_facts: ObservedFacts,
    values_program_wrappers: BTreeSet<helm_schema_core::ValuesProgramWrapper>,
    /// Values paths whose nodes must NOT gain a wrapper alternative: a
    /// strict string consumer reads them BEFORE the engine's values-root
    /// rewrite, so a wrapper map there aborts rendering (nats'
    /// `nameOverride` through `fullname | trunc`).
    values_program_wrapper_exclusions: BTreeSet<String>,
    dependency_values_root_fragments: BTreeSet<String>,
}

impl ContractIr {
    /// Build a contract graph from already-structured contract claims.
    ///
    /// This is the contract-layer constructor for tests and expert callers
    /// that already have semantic claims. Schema signals are still derived
    /// through [`ContractIr::finalize`], so semantic finalization stays on
    /// the contract graph rather than a serialized document.
    #[must_use]
    pub fn from_contract_uses(uses: Vec<ContractUse>) -> Self {
        Self {
            uses,
            ..Self::default()
        }
    }

    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.uses.push(contract_use);
    }

    pub(crate) fn push_dependency_use(&mut self, contract_use: ContractUse) {
        self.dependency_uses.push(contract_use);
    }

    /// Add a pathless scalar claim for a value path.
    ///
    /// Pathless claims make a value path visible to downstream schema
    /// generation without asserting any rendered Kubernetes field shape.
    pub fn push_pathless_scalar(&mut self, source_expr: impl Into<String>) {
        self.push(ContractUse::new(
            source_expr.into(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
    }

    /// Records a pathless fragment accepted at a dependency values root.
    pub fn push_pathless_dependency_fragment(&mut self, source_expr: impl Into<String>) {
        self.dependency_values_root_fragments
            .insert(source_expr.into());
    }

    /// Move all claims from another contract graph into this graph.
    pub fn append(&mut self, mut other: Self) {
        self.uses.append(&mut other.uses);
        self.dependency_uses.append(&mut other.dependency_uses);
        self.dependency_values_root_fragments
            .append(&mut other.dependency_values_root_fragments);
        self.observed_facts.absorb(&other.observed_facts);
        self.values_program_wrappers
            .append(&mut other.values_program_wrappers);
        self.values_program_wrapper_exclusions
            .append(&mut other.values_program_wrapper_exclusions);
    }

    /// Record that rendering FAILS whenever `condition` holds — an
    /// unconditionally reached `include` whose helper only an inactive
    /// optional dependency defines aborts with "no template". The predicate
    /// lowers through the standard terminal-clause machinery.
    pub fn add_terminal_fail_condition(&mut self, condition: helm_schema_core::Predicate) {
        let capture = crate::eval_effect::FailCapture {
            conjunction: vec![condition],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::Fail,
        };
        self.observed_facts.captures.insert(capture);
    }

    /// Append guards to every claim in the graph without rewriting any paths.
    ///
    /// This is used for chart-structural activation predicates that apply to
    /// an already-scoped batch of claims, such as dependency `condition:` /
    /// `tags:` liveness from `Chart.yaml`.
    pub fn append_guards_to_all_uses(&mut self, guards: &[Guard]) {
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            contract_use.condition = contract_use
                .condition
                .conjoined_with_guards(guards.iter().cloned());
        }
        // Fail captures are claims too: a `fail` inside a dependency gated
        // off by `condition:` / `tags:` cannot abort rendering, so its
        // conjunction must carry the activation predicate like every row.
        self.observed_facts.captures = std::mem::take(&mut self.observed_facts.captures)
            .into_iter()
            .map(|mut capture| {
                capture.conjunction.splice(
                    0..0,
                    guards
                        .iter()
                        .cloned()
                        .map(helm_schema_core::Predicate::from),
                );
                capture
            })
            .collect();
        if !guards.is_empty() {
            let values_default_sources =
                std::mem::take(&mut self.observed_facts.values_default_sources)
                    .into_iter()
                    .filter(|source| {
                        self.uses.iter().chain(&self.dependency_uses).any(|use_| {
                            !use_.path.0.is_empty()
                                && use_.source_expr != source.target_path
                                && use_.source_expr != source.source_path
                                && !helm_schema_core::values_path_is_descendant(
                                    &use_.source_expr,
                                    &source.source_path,
                                )
                        })
                    })
                    .collect::<Vec<_>>();
            let append_guards = |mut existing: Vec<Guard>| {
                existing.extend(guards.iter().cloned());
                existing.sort();
                existing.dedup();
                existing
            };
            self.observed_facts.activated_values_default_sources =
                std::mem::take(&mut self.observed_facts.activated_values_default_sources)
                    .into_iter()
                    .map(|fact| ActivatedValuesDefaultSource {
                        guards: append_guards(fact.guards),
                        source: fact.source,
                    })
                    .chain(values_default_sources.into_iter().map(|source| {
                        ActivatedValuesDefaultSource {
                            guards: append_guards(Vec::new()),
                            source,
                        }
                    }))
                    .collect();
            self.observed_facts.activated_values_root_overlays =
                std::mem::take(&mut self.observed_facts.activated_values_root_overlays)
                    .into_iter()
                    .map(|fact| ActivatedValuesRootOverlay {
                        guards: append_guards(fact.guards),
                        target_path: fact.target_path,
                        source_path: fact.source_path,
                    })
                    .chain(
                        std::mem::take(&mut self.observed_facts.values_root_overlays)
                            .into_iter()
                            .map(|fact| ActivatedValuesRootOverlay {
                                guards: append_guards(Vec::new()),
                                target_path: fact.target_path,
                                source_path: fact.source_path,
                            }),
                    )
                    .collect();
        }
    }

    /// Mark rendered claims as textual output rather than structured YAML
    /// placements. Runtime operand contracts and terminal effects remain
    /// unchanged.
    pub fn mark_rendered_output_textual(&mut self) {
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            contract_use.kind = ValueKind::Serialized;
        }
    }

    /// Rewrite all referenced values paths while preserving rendered YAML paths.
    ///
    /// This is used at chart boundaries where a dependency's `.Values.foo`
    /// contract becomes `.Values.subchart.foo`, while rendered manifest paths
    /// such as `metadata.name` stay unchanged.
    pub fn map_value_paths<F>(&mut self, mut map: F)
    where
        F: FnMut(&str) -> String,
    {
        let Self {
            uses,
            dependency_uses,
            observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            dependency_values_root_fragments,
        } = self;
        for contract_use in uses.iter_mut().chain(dependency_uses) {
            contract_use.map_value_paths(&mut map);
        }
        *dependency_values_root_fragments = std::mem::take(dependency_values_root_fragments)
            .into_iter()
            .map(|path| map(&path))
            .collect();
        observed_facts.map_value_paths(&mut map);
        *values_program_wrappers = std::mem::take(values_program_wrappers)
            .into_iter()
            .map(|wrapper| helm_schema_core::ValuesProgramWrapper {
                scope_path: map(&wrapper.scope_path),
                key: wrapper.key,
                spread: wrapper.spread,
            })
            .collect();
        *values_program_wrapper_exclusions = std::mem::take(values_program_wrapper_exclusions)
            .into_iter()
            .map(|path| map(&path))
            .collect();
    }

    /// Projects dependency `global.*` contracts through Helm's parent-first
    /// coalesce while preserving the dependency-local fallback.
    pub fn project_dependency_global_contracts(&mut self, prefix: &[String]) {
        if prefix.is_empty() {
            return;
        }
        let global_sources = dependency_global_sources(prefix);
        project_global_uses(&mut self.uses, &global_sources);
        project_global_uses(&mut self.dependency_uses, &global_sources);
        project_global_fail_captures(&mut self.observed_facts.captures, &global_sources);
        project_global_range_modes(&mut self.observed_facts.range_modes, &global_sources);
    }

    /// Add declared input-type hints for values paths without projecting them
    /// as inspection rows.
    pub fn add_type_hint(&mut self, path: impl Into<String>, schema_type: impl Into<String>) {
        let path = path.into();
        let schema_type = schema_type.into();
        if path.trim().is_empty() || schema_type.trim().is_empty() {
            return;
        }
        self.observed_facts
            .insert_type_hint(HintGrade::DECLARED, path, &schema_type);
    }

    pub(crate) fn absorb_observed_facts(&mut self, facts: &ObservedFacts) {
        self.observed_facts.absorb(facts);
    }

    pub(crate) fn extend_values_program_wrappers(
        &mut self,
        wrappers: impl IntoIterator<Item = helm_schema_core::ValuesProgramWrapper>,
    ) {
        self.values_program_wrappers.extend(wrappers);
    }

    pub(crate) fn extend_values_program_wrapper_exclusions(
        &mut self,
        paths: impl IntoIterator<Item = String>,
    ) {
        self.values_program_wrapper_exclusions.extend(paths);
    }

    /// Drop evidence recorded AT a program-wrapper sentinel key: within a
    /// wrapper-engine chart a `$tplYaml`-keyed member is the engine's own
    /// dispatch convention — probed by the recursive walker, replaced
    /// before ordinary consumers read the tree — never an ordinary chart
    /// value, so reads and fail predicates over such paths must not mint
    /// values properties. The wrapper alternatives model those nodes.
    pub(crate) fn scrub_program_wrapper_sentinel_evidence(&mut self) {
        let keys: std::collections::BTreeSet<String> = self
            .values_program_wrappers
            .iter()
            .map(|wrapper| wrapper.key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        let touches = |path: &str| {
            helm_schema_core::split_value_path(path)
                .iter()
                .any(|segment| keys.contains(segment))
        };
        self.uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.dependency_uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.observed_facts.captures.retain(|capture| {
            let mut paths: Vec<String> = capture
                .conjunction
                .iter()
                .flat_map(helm_schema_core::Predicate::value_paths)
                .collect();
            let mut kind = capture.kind.clone();
            kind.map_value_paths(&mut |path: &str| {
                paths.push(path.to_string());
                path.to_string()
            });
            !paths.iter().any(|path| touches(path))
        });
    }

    /// Finalize the contract once and derive downstream artifacts from that
    /// one normalized contract representation.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn finalize(mut self) -> FinalizedContract {
        self.scrub_program_wrapper_sentinel_evidence();
        let Self {
            mut uses,
            mut dependency_uses,
            observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            dependency_values_root_fragments,
        } = self;
        for source_expr in &dependency_values_root_fragments {
            dependency_uses.push(ContractUse::new(
                source_expr.clone(),
                YamlPath(Vec::new()),
                ValueKind::Fragment,
                Vec::new(),
                None,
            ));
        }
        normalize_contract_uses(&mut uses);
        drop_self_truthy_subsumed_duplicates(&mut dependency_uses);
        canonicalize_contract_uses(&mut dependency_uses);
        uses.append(&mut dependency_uses);
        drop_default_guard_subsumed_duplicates(&mut uses);
        drop_self_truthy_subsumed_duplicates(&mut uses);
        canonicalize_contract_uses(&mut uses);
        let fail_conditions = observed_facts.captures.iter().cloned().collect::<Vec<_>>();
        lower_string_requirement_merge_sources(&mut uses, &fail_conditions);
        canonicalize_contract_uses(&mut uses);
        FinalizedContract::new(
            uses,
            &observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            &dependency_values_root_fragments,
        )
    }
}

fn string_requirements_by_ancestor(
    fail_conditions: &[crate::eval_effect::FailCapture],
) -> BTreeMap<String, BTreeSet<(String, Vec<helm_schema_core::Predicate>)>> {
    let mut requirements: BTreeMap<String, BTreeSet<(String, Vec<helm_schema_core::Predicate>)>> =
        BTreeMap::new();
    for capture in fail_conditions {
        let crate::eval_effect::CaptureKind::StringRequirement {
            path,
            route,
            selection,
        } = &capture.kind
        else {
            continue;
        };
        if *route == crate::eval_effect::StringRequirementRoute::Serialized {
            continue;
        }
        let mut predicates = capture.conjunction.clone();
        predicates.extend(selection.iter().cloned());
        predicates.sort();
        predicates.dedup();
        let segments = helm_schema_core::split_value_path(path);
        for end in 1..segments.len() {
            requirements
                .entry(helm_schema_core::join_value_path(
                    segments.get(..end).unwrap_or_default().iter().cloned(),
                ))
                .or_default()
                .insert((path.clone(), predicates.clone()));
        }
    }
    requirements
}

fn lower_string_requirement_merge_sources(
    uses: &mut [ContractUse],
    fail_conditions: &[crate::eval_effect::FailCapture],
) {
    let requirements_by_ancestor = string_requirements_by_ancestor(fail_conditions);

    let mut overlap_cache = BTreeMap::new();

    for contract_use in uses.iter_mut() {
        let Some(merge) = &contract_use.merge_layers else {
            continue;
        };
        let Some(requirements) = requirements_by_ancestor.get(&contract_use.source_expr) else {
            continue;
        };
        let source_segments = helm_schema_core::split_value_path(&contract_use.source_expr);
        let specific_layers = merge
            .layers
            .iter()
            .map(|layer| helm_schema_core::split_value_path(layer))
            .filter(|layer| {
                layer.len() > source_segments.len() && layer.starts_with(source_segments.as_slice())
            })
            .collect::<Vec<_>>();
        let [first, second, rest @ ..] = specific_layers.as_slice() else {
            continue;
        };
        let mut common_suffix_len = first
            .iter()
            .rev()
            .zip(second.iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        for layer in rest {
            common_suffix_len = common_suffix_len.min(
                first
                    .iter()
                    .rev()
                    .zip(layer.iter().rev())
                    .take_while(|(left, right)| left == right)
                    .count(),
            );
        }
        let suffix = first.get(first.len().saturating_sub(common_suffix_len)..);
        let Some(suffix) = suffix.filter(|suffix| {
            !suffix.is_empty() && suffix.iter().all(|segment| segment.as_str() != "*")
        }) else {
            continue;
        };
        let Some(requirements) =
            merge_suffix_string_requirements(&contract_use.source_expr, requirements, suffix)
        else {
            continue;
        };
        let cache_key = (
            contract_use.source_expr.clone(),
            suffix.to_vec(),
            contract_use.condition.clone(),
        );
        let overlaps_requirement = overlap_cache.get(&cache_key).copied().unwrap_or_else(|| {
            let overlaps = contract_use.condition.disjuncts().iter().any(|row| {
                requirements.iter().any(|requirement| {
                    !helm_schema_core::GuardDnf::from_conjunction(
                        row.iter().cloned().chain(requirement.iter().cloned()),
                    )
                    .is_never()
                })
            });
            overlap_cache.insert(cache_key, overlaps);
            overlaps
        });
        if !overlaps_requirement {
            continue;
        }
        let source = contract_use.source_expr.clone();
        let projected = helm_schema_core::join_value_path(
            source_segments
                .iter()
                .cloned()
                .chain(suffix.iter().cloned()),
        );
        contract_use.map_value_paths(&mut |path| {
            if path == source {
                projected.clone()
            } else {
                path.to_string()
            }
        });
    }
}

fn merge_suffix_string_requirements(
    source: &str,
    requirements: &BTreeSet<(String, Vec<helm_schema_core::Predicate>)>,
    suffix: &[String],
) -> Option<BTreeSet<Vec<helm_schema_core::Predicate>>> {
    let source_segments = helm_schema_core::split_value_path(source);
    let mut paths = BTreeSet::new();
    let mut conjunctions = BTreeSet::new();
    for (path, predicates) in requirements {
        let segments = helm_schema_core::split_value_path(path);
        if segments.len() <= source_segments.len()
            || !segments.starts_with(source_segments.as_slice())
            || !segments.ends_with(suffix)
        {
            continue;
        }
        paths.insert(segments);
        conjunctions.insert(predicates.clone());
    }
    (paths.len() >= 2).then_some(conjunctions)
}

fn dependency_global_sources(prefix: &[String]) -> Vec<String> {
    (0..=prefix.len())
        .map(|prefix_len| {
            helm_schema_core::join_value_path(
                prefix
                    .get(..prefix_len)
                    .unwrap_or_default()
                    .iter()
                    .cloned()
                    .chain(std::iter::once("global".to_string())),
            )
        })
        .collect()
}

fn project_global_uses(uses: &mut Vec<ContractUse>, global_sources: &[String]) {
    let Some(dependency_global) = global_sources.last() else {
        return;
    };
    let dependency_global_segments = helm_schema_core::split_value_path(dependency_global);
    let mut projected = Vec::with_capacity(uses.len());
    for dependency_use in std::mem::take(uses) {
        let source_segments = helm_schema_core::split_value_path(&dependency_use.source_expr);
        let Some(relative) = source_segments.strip_prefix(dependency_global_segments.as_slice())
        else {
            projected.push(dependency_use);
            continue;
        };
        let Some((selection_relative, key)) = global_selection_path(relative) else {
            projected.push(dependency_use);
            continue;
        };

        for (source_index, global_source) in global_sources.iter().enumerate() {
            let selection_source = helm_schema_core::join_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(selection_relative.iter().cloned()),
            );
            let mut selected_use = dependency_use.clone();
            selected_use.map_value_paths(&mut |path| {
                replace_value_path_prefix(path, dependency_global, global_source)
            });
            selected_use.condition =
                selected_use
                    .condition
                    .conjoined_with_guards(global_source_selection_guards(
                        global_sources,
                        selection_relative,
                        source_index,
                        &selection_source,
                        key,
                    ));
            projected.push(selected_use);
        }
    }
    *uses = projected;
}

fn global_selection_path(relative: &[String]) -> Option<(&[String], &str)> {
    let selection_len = relative
        .iter()
        .position(|segment| segment == "*")
        .unwrap_or(relative.len());
    let selection = relative.get(..selection_len)?;
    Some((selection, selection.last()?.as_str()))
}

fn global_source_selection_guards(
    global_sources: &[String],
    relative: &[String],
    source_index: usize,
    source: &str,
    key: &str,
) -> Vec<Guard> {
    let mut guards = Vec::new();
    for higher_priority in global_sources.iter().take(source_index) {
        let higher_source = helm_schema_core::join_value_path(
            helm_schema_core::split_value_path(higher_priority)
                .into_iter()
                .chain(relative.iter().cloned()),
        );
        let mut container_segments = helm_schema_core::split_value_path(&higher_source);
        container_segments.pop();
        guards.push(Guard::AnyOf {
            alternatives: vec![
                vec![Guard::NotHasKey {
                    path: helm_schema_core::join_value_path(container_segments),
                    key: key.to_string(),
                }],
                vec![Guard::Eq {
                    path: higher_source,
                    value: helm_schema_core::GuardValue::Null,
                }],
            ],
        });
    }
    if source_index + 1 < global_sources.len() {
        let mut container_segments = helm_schema_core::split_value_path(source);
        container_segments.pop();
        guards.push(Guard::HasKey {
            path: helm_schema_core::join_value_path(container_segments),
            key: key.to_string(),
        });
        guards.push(Guard::NotEq {
            path: source.to_string(),
            value: helm_schema_core::GuardValue::Null,
        });
    }
    guards
}

fn replace_value_path_prefix(path: &str, from: &str, to: &str) -> String {
    let path_segments = helm_schema_core::split_value_path(path);
    let from_segments = helm_schema_core::split_value_path(from);
    let Some(relative) = path_segments.strip_prefix(from_segments.as_slice()) else {
        return path.to_string();
    };
    helm_schema_core::join_value_path(
        helm_schema_core::split_value_path(to)
            .into_iter()
            .chain(relative.iter().cloned()),
    )
}

fn project_global_fail_captures(
    captures: &mut BTreeSet<crate::eval_effect::FailCapture>,
    global_sources: &[String],
) {
    let Some(dependency_global) = global_sources.last() else {
        return;
    };
    let dependency_global_segments = helm_schema_core::split_value_path(dependency_global);
    let mut projected = BTreeSet::new();
    for dependency_capture in std::mem::take(captures) {
        let Some(source_path) = dependency_capture.kind.sole_value_path() else {
            projected.insert(dependency_capture);
            continue;
        };
        let source_segments = helm_schema_core::split_value_path(source_path);
        let Some(relative) = source_segments.strip_prefix(dependency_global_segments.as_slice())
        else {
            projected.insert(dependency_capture);
            continue;
        };
        if relative.is_empty() {
            if matches!(
                dependency_capture.kind,
                crate::eval_effect::CaptureKind::RangeInput { .. }
            ) {
                for global_source in global_sources {
                    let mut selected_capture = dependency_capture.clone();
                    selected_capture.conjunction = selected_capture
                        .conjunction
                        .into_iter()
                        .map(|predicate| {
                            predicate.map_value_paths(&mut |path| {
                                replace_value_path_prefix(path, dependency_global, global_source)
                            })
                        })
                        .collect();
                    selected_capture.kind.map_value_paths(&mut |path| {
                        replace_value_path_prefix(path, dependency_global, global_source)
                    });
                    projected.insert(selected_capture);
                }
            } else {
                projected.insert(dependency_capture);
            }
            continue;
        }
        let Some((selection_relative, key)) = global_selection_path(relative) else {
            projected.insert(dependency_capture);
            continue;
        };

        for (source_index, global_source) in global_sources.iter().enumerate() {
            let selection_source = helm_schema_core::join_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(selection_relative.iter().cloned()),
            );
            let mut selected_capture = dependency_capture.clone();
            selected_capture.conjunction = selected_capture
                .conjunction
                .into_iter()
                .map(|predicate| {
                    predicate.map_value_paths(&mut |path| {
                        replace_value_path_prefix(path, dependency_global, global_source)
                    })
                })
                .collect();
            selected_capture.kind.map_value_paths(&mut |path| {
                replace_value_path_prefix(path, dependency_global, global_source)
            });
            selected_capture.conjunction.extend(
                global_source_selection_guards(
                    global_sources,
                    selection_relative,
                    source_index,
                    &selection_source,
                    key,
                )
                .into_iter()
                .map(helm_schema_core::Predicate::from),
            );
            projected.insert(selected_capture);
        }
    }
    *captures = projected;
}

fn project_global_range_modes(
    range_modes: &mut crate::range_modes::RangeModes,
    global_sources: &[String],
) {
    let Some(dependency_global) = global_sources.last() else {
        return;
    };
    let dependency_path = helm_schema_core::ValuesPath::parse(dependency_global);
    let dependency_segments = dependency_path
        .segments()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let projected_paths = range_modes
        .iter()
        .filter_map(|(path, mode)| {
            let segments = path.segments().map(str::to_string).collect::<Vec<_>>();
            let relative = segments.strip_prefix(dependency_segments.as_slice())?;
            Some((path.encode(), relative.to_vec(), mode))
        })
        .collect::<Vec<_>>();

    for (path, relative, mode) in projected_paths {
        range_modes.remove(&path);
        if relative.is_empty() {
            for global_source in global_sources {
                range_modes.merge_mode(global_source, mode);
            }
            continue;
        }
        if global_selection_path(&relative).is_none() {
            continue;
        }
        for global_source in global_sources {
            let projected = helm_schema_core::join_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(relative.iter().cloned()),
            );
            range_modes.merge_mode(&projected, mode);
        }
    }
}
