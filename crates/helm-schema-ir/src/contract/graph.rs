use std::collections::BTreeSet;

use crate::contract::FinalizedContract;
use crate::contract_normalization::normalize_contract_uses;
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
    values_program_wrapper_exclusions: BTreeSet<helm_schema_core::ValuesPath>,
    dependency_values_roots: BTreeSet<DependencyValuesRoot>,
}

/// A dependency chart instance's values root, with the chart-tree facts that
/// decide when Helm v4.2.3 type-asserts it (`type mismatch on <key>`).
///
/// Helm asserts every dependency root twice: once over every loaded chart
/// before disabled charts are pruned, where a nested root is keyed by its
/// chart NAME and the parent's declared default deletes a user null first;
/// and once while rendering, only for active charts, after each parent's
/// defaults have absorbed its dependencies' tables. A pruned root therefore
/// keeps a user null (and, when unasserted before pruning, a scalar) in the
/// final document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyValuesRoot {
    /// The instance's values path.
    pub path: helm_schema_core::ValuesPath,
    /// Guard alternatives under which Helm enables this instance and every
    /// ancestor; empty when it always is.
    pub active: Vec<Vec<Guard>>,
    /// The root is asserted before pruning: it is a direct dependency of the
    /// analyzed chart, or every nested edge on its path is keyed by the
    /// chart's own name.
    pub asserted_before_pruning: bool,
    /// The parent chart's own `values.yaml` declares the key.
    pub declared_by_parent: bool,
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
    pub fn push_pathless_scalar(&mut self, path: helm_schema_core::ValuesPath) {
        self.push(ContractUse::new(
            path,
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
    }

    /// Records a dependency chart instance's values root.
    pub fn push_dependency_values_root(&mut self, root: DependencyValuesRoot) {
        self.dependency_values_roots.insert(root);
    }

    /// Move all claims from another contract graph into this graph.
    pub fn append(&mut self, mut other: Self) {
        self.uses.append(&mut other.uses);
        self.dependency_uses.append(&mut other.dependency_uses);
        self.dependency_values_roots
            .append(&mut other.dependency_values_roots);
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
        let capture = crate::observed_facts::FailCapture {
            conjunction: vec![condition].into(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::observed_facts::CaptureKind::Fail,
        };
        self.observed_facts.captures.insert(capture);
    }

    /// Append guards to every claim in the graph without rewriting any paths.
    ///
    /// This is used for chart-structural activation predicates that apply to
    /// an already-scoped batch of claims, such as dependency `condition:` /
    /// `tags:` liveness from `Chart.yaml`.
    pub fn append_guards_to_all_uses(&mut self, guards: &[Guard]) {
        self.observed_facts.resources = std::mem::take(&mut self.observed_facts.resources)
            .into_iter()
            .map(|mut observation| {
                observation.condition = helm_schema_core::Predicate::all(
                    std::iter::once(observation.condition)
                        .chain(
                            guards
                                .iter()
                                .cloned()
                                .map(helm_schema_core::Predicate::from),
                        )
                        .collect(),
                );
                observation
            })
            .collect();
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
                capture.conjunction.prepend(
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
                                && !use_.source_expr.is_descendant_of(&source.source_path)
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
            contract_use.plain_scalar_placement = helm_schema_core::PlainScalarPlacement::Unknown;
            contract_use.kind = match contract_use.kind {
                ValueKind::Scalar
                | ValueKind::PartialScalar
                | ValueKind::Fragment
                | ValueKind::Serialized
                | ValueKind::YamlSerialized
                | ValueKind::TemplatedYamlSerialized
                | ValueKind::GoFormattedMap
                | ValueKind::GoFormattedSequence => ValueKind::Serialized,
                ValueKind::WidenedDependency => ValueKind::WidenedDependency,
                ValueKind::WidenedRegion => ValueKind::WidenedRegion,
            };
        }
    }

    /// Rewrite all referenced values paths while preserving rendered YAML paths.
    ///
    /// This is used at chart boundaries where a dependency's `.Values.foo`
    /// contract becomes `.Values.subchart.foo`, while rendered manifest paths
    /// such as `metadata.name` stay unchanged.
    pub fn map_value_paths<F>(&mut self, mut map: F)
    where
        F: FnMut(helm_schema_core::ValuesPath) -> helm_schema_core::ValuesPath,
    {
        self.map_paths(&mut map, false);
    }

    /// Rewrites paths with a deterministic mapping, visiting shared predicate nodes once.
    pub fn map_value_paths_shared<F>(&mut self, mut map: F)
    where
        F: FnMut(helm_schema_core::ValuesPath) -> helm_schema_core::ValuesPath,
    {
        self.map_paths(&mut map, true);
    }

    fn map_paths<F>(&mut self, map: &mut F, shared: bool)
    where
        F: FnMut(helm_schema_core::ValuesPath) -> helm_schema_core::ValuesPath,
    {
        let Self {
            uses,
            dependency_uses,
            observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            dependency_values_roots,
        } = self;
        for contract_use in uses.iter_mut().chain(dependency_uses) {
            if shared {
                contract_use.map_value_paths_shared(&mut *map);
            } else {
                contract_use.map_value_paths(&mut *map);
            }
        }
        *dependency_values_roots = std::mem::take(dependency_values_roots)
            .into_iter()
            .map(|root| DependencyValuesRoot {
                path: map(root.path),
                active: root
                    .active
                    .into_iter()
                    .map(|guards| {
                        guards
                            .into_iter()
                            .map(|guard| guard.map_value_paths(&mut *map))
                            .collect()
                    })
                    .collect(),
                ..root
            })
            .collect();
        observed_facts.map_value_paths(&mut *map, shared);
        *values_program_wrappers = std::mem::take(values_program_wrappers)
            .into_iter()
            .map(|wrapper| helm_schema_core::ValuesProgramWrapper {
                scope_path: map(wrapper.scope_path),
                key: wrapper.key,
                spread: wrapper.spread,
            })
            .collect();
        *values_program_wrapper_exclusions = std::mem::take(values_program_wrapper_exclusions)
            .into_iter()
            .map(&mut *map)
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
        self.observed_facts.insert_type_hint(
            HintGrade::DECLARED,
            helm_schema_core::ValuesPath::parse(&path),
            &schema_type,
        );
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
        paths: impl IntoIterator<Item = helm_schema_core::ValuesPath>,
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
        let touches = |path: &helm_schema_core::ValuesPath| {
            path.segments()
                .filter_map(helm_schema_core::Segment::literal)
                .any(|segment| keys.contains(segment))
        };
        self.uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.dependency_uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.observed_facts.captures.retain(|capture| {
            let mut paths: Vec<helm_schema_core::ValuesPath> = capture
                .conjunction
                .iter()
                .flat_map(helm_schema_core::Predicate::value_paths)
                .collect();
            let mut kind = capture.kind.clone();
            kind.map_value_paths(&mut |path| {
                paths.push(path.clone());
                path
            });
            !paths.iter().any(touches)
        });
    }

    /// Finalize the contract once and derive downstream artifacts from that
    /// one normalized contract representation.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn finalize(mut self) -> FinalizedContract {
        self.scrub_program_wrapper_sentinel_evidence();
        let Self {
            uses,
            mut dependency_uses,
            observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            dependency_values_roots,
        } = self;
        for root in &dependency_values_roots {
            dependency_uses.push(ContractUse::new(
                root.path.clone(),
                YamlPath(Vec::new()),
                ValueKind::Fragment,
                Vec::new(),
                None,
            ));
        }
        let fail_conditions = observed_facts.captures.iter().cloned().collect::<Vec<_>>();
        let uses = normalize_contract_uses(uses, dependency_uses, &fail_conditions);
        let predicate_memo = helm_schema_core::PredicateMemo::new();
        FinalizedContract::new(
            uses,
            &observed_facts,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            &dependency_values_roots,
            &predicate_memo,
        )
    }
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
        let source_segments = dependency_use
            .source_expr
            .segments()
            .map(helm_schema_core::Segment::encode_component)
            .collect::<Vec<_>>();
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
            let selection_source = helm_schema_core::join_encoded_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(selection_relative.iter().cloned()),
            );
            let mut selected_use = dependency_use.clone();
            selected_use.map_value_paths_shared(&mut |path| {
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
        let higher_source = helm_schema_core::join_encoded_value_path(
            helm_schema_core::split_value_path(higher_priority)
                .into_iter()
                .chain(relative.iter().cloned()),
        );
        let mut container_segments = helm_schema_core::split_value_path(&higher_source);
        container_segments.pop();
        guards.push(Guard::AnyOf {
            alternatives: vec![
                vec![Guard::NotHasKey {
                    path: helm_schema_core::ValuesPath::parse(
                        &helm_schema_core::join_encoded_value_path(container_segments),
                    ),
                    key: key.to_string(),
                }],
                vec![Guard::Eq {
                    path: helm_schema_core::ValuesPath::parse(&higher_source),
                    value: helm_schema_core::GuardValue::Null,
                }],
            ],
        });
    }
    if source_index + 1 < global_sources.len() {
        let mut container_segments = helm_schema_core::split_value_path(source);
        container_segments.pop();
        guards.push(Guard::HasKey {
            path: helm_schema_core::ValuesPath::parse(&helm_schema_core::join_encoded_value_path(
                container_segments,
            )),
            key: key.to_string(),
        });
        guards.push(Guard::NotEq {
            path: helm_schema_core::ValuesPath::parse(source),
            value: helm_schema_core::GuardValue::Null,
        });
    }
    guards
}

fn replace_value_path_prefix(
    path: helm_schema_core::ValuesPath,
    from: &str,
    to: &str,
) -> helm_schema_core::ValuesPath {
    let from = helm_schema_core::ValuesPath::parse(from);
    if path != from && !path.is_descendant_of(&from) {
        return path;
    }
    let relative = path
        .segments()
        .skip(from.segments().len())
        .cloned()
        .collect::<Vec<_>>();
    let target = helm_schema_core::ValuesPath::parse(to);
    helm_schema_core::ValuesPath::from_segments(target.segments().chain(&relative))
}

fn project_global_fail_captures(
    captures: &mut BTreeSet<crate::observed_facts::FailCapture>,
    global_sources: &[String],
) {
    let Some(dependency_global) = global_sources.last() else {
        return;
    };
    let dependency_global_path = helm_schema_core::ValuesPath::parse(dependency_global);
    let mut projected = BTreeSet::new();
    for dependency_capture in std::mem::take(captures) {
        let Some(source_path) = dependency_capture.kind.sole_value_path() else {
            if let Some(capture) =
                project_pathless_global_failure(dependency_capture, global_sources)
            {
                projected.insert(capture);
            }
            continue;
        };
        if source_path != &dependency_global_path
            && !source_path.is_descendant_of(&dependency_global_path)
        {
            projected.insert(dependency_capture);
            continue;
        }
        let relative = source_path
            .segments()
            .skip(dependency_global_path.segments().len())
            .map(helm_schema_core::Segment::encode_component)
            .collect::<Vec<_>>();
        if relative.is_empty() {
            if matches!(
                dependency_capture.kind,
                crate::observed_facts::CaptureKind::RangeInput { .. }
            ) {
                for global_source in global_sources {
                    let mut selected_capture = dependency_capture.clone();
                    selected_capture.conjunction = selected_capture
                        .conjunction
                        .into_iter()
                        .map(|predicate| {
                            predicate.map_value_paths_shared(&mut |path| {
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
        let Some((selection_relative, key)) = global_selection_path(&relative) else {
            projected.insert(dependency_capture);
            continue;
        };

        for (source_index, global_source) in global_sources.iter().enumerate() {
            let selection_source = helm_schema_core::join_encoded_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(selection_relative.iter().cloned()),
            );
            let mut selected_capture = dependency_capture.clone();
            selected_capture.conjunction = selected_capture
                .conjunction
                .into_iter()
                .map(|predicate| {
                    predicate.map_value_paths_shared(&mut |path| {
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

fn project_pathless_global_failure(
    capture: crate::observed_facts::FailCapture,
    global_sources: &[String],
) -> Option<crate::observed_facts::FailCapture> {
    let crate::observed_facts::FailCapture {
        conjunction,
        ranged,
        kind,
    } = capture;
    let mut budget = 4096;
    let domain = global_failure_non_map_domain(&conjunction, global_sources, &mut budget)?;
    let mut conjunction = conjunction
        .iter()
        .map(|predicate| {
            project_global_failure_predicate(predicate, global_sources, &mut budget, false)
        })
        .collect::<Option<helm_schema_core::Conjunction>>()?;
    conjunction.extend(domain);
    Some(crate::observed_facts::FailCapture {
        conjunction,
        ranged,
        kind,
    })
}

// Helm recursively merges global tables and can retain a scalar beneath a parent table.
// Scalar source selection is exact only while every candidate member stays outside that case.
fn global_failure_non_map_domain(
    conjunction: &helm_schema_core::Conjunction,
    global_sources: &[String],
    budget: &mut usize,
) -> Option<BTreeSet<helm_schema_core::Predicate>> {
    use helm_schema_core::{Predicate, PredicateKind, Segment, ValuesPath};

    let dependency_path = ValuesPath::parse(global_sources.last()?);
    let mut paths = BTreeSet::new();
    let mut predicates = conjunction.iter().collect::<Vec<_>>();
    while let Some(predicate) = predicates.pop() {
        *budget = budget.checked_sub(1)?;
        match predicate.kind() {
            PredicateKind::True | PredicateKind::False => {}
            PredicateKind::Not(inner) => predicates.push(inner),
            PredicateKind::And(parts) | PredicateKind::Or(parts) => predicates.extend(parts),
            PredicateKind::Approximate {
                marker: _,
                paths: mentioned,
                role: _,
                sound_subset,
            } => {
                paths.extend(mentioned.iter().cloned());
                predicates.extend(sound_subset);
            }
            PredicateKind::Guard(guard) => {
                let mut guards = vec![guard];
                while let Some(guard) = guards.pop() {
                    *budget = budget.checked_sub(1)?;
                    match guard {
                        Guard::AnyOf { alternatives } => {
                            guards.extend(alternatives.iter().flatten());
                        }
                        Guard::HasKey { path, key } | Guard::NotHasKey { path, key } => {
                            paths.insert(ValuesPath::from_segments(
                                path.segments()
                                    .cloned()
                                    .chain([Segment::from(key.as_str())]),
                            ));
                        }
                        _ => paths.extend(guard.value_paths()),
                    }
                }
            }
        }
    }
    let mut domain = BTreeSet::new();
    for path in paths {
        if !path.is_descendant_of(&dependency_path) {
            continue;
        }
        let relative = path
            .segments()
            .skip(dependency_path.segments().len())
            .map(Segment::encode_component)
            .collect::<Vec<_>>();
        let Some((selection_relative, _)) = global_selection_path(&relative) else {
            continue;
        };
        for global_source in global_sources {
            // An intermediate table also changes recursive coalescing before the tested leaf.
            for member_len in 1..=selection_relative.len() {
                *budget = budget.checked_sub(1)?;
                let member = helm_schema_core::join_encoded_value_path(
                    helm_schema_core::split_value_path(global_source)
                        .into_iter()
                        .chain(selection_relative.iter().take(member_len).cloned()),
                );
                domain.insert(Predicate::Not(Box::new(Predicate::from(Guard::TypeIs {
                    path: ValuesPath::parse(&member),
                    schema_type: "object".to_owned(),
                }))));
            }
        }
    }
    Some(domain)
}

// A collapsed abort can test several global members with independent parent selections.
// Pull back each atom before Boolean composition so negation preserves that partition.
fn project_global_failure_predicate(
    predicate: &helm_schema_core::Predicate,
    global_sources: &[String],
    budget: &mut usize,
    require_exact: bool,
) -> Option<helm_schema_core::Predicate> {
    use helm_schema_core::{Predicate, PredicateKind};

    *budget = budget.checked_sub(1)?;
    match predicate.kind() {
        PredicateKind::True | PredicateKind::False => Some(predicate.clone()),
        PredicateKind::Guard(guard) => {
            project_global_failure_guard(guard, global_sources, budget, require_exact)
        }
        PredicateKind::Not(inner) => Some(Predicate::Not(Box::new(
            project_global_failure_predicate(inner, global_sources, budget, true)?,
        ))),
        PredicateKind::And(parts) => Some(Predicate::And(
            parts
                .iter()
                .map(|part| {
                    project_global_failure_predicate(part, global_sources, budget, require_exact)
                })
                .collect::<Option<_>>()?,
        )),
        PredicateKind::Or(parts) => global_failure_disjunction(
            parts.iter().map(|part| {
                project_global_failure_predicate(part, global_sources, budget, require_exact)
            }),
            require_exact,
        ),
        PredicateKind::Approximate {
            marker,
            paths,
            role,
            sound_subset,
        } if !require_exact => Some(Predicate::Approximate(
            marker.to_owned(),
            paths.clone(),
            role,
            match sound_subset {
                Some(subset) => Some(Box::new(project_global_failure_predicate(
                    subset,
                    global_sources,
                    budget,
                    false,
                )?)),
                None => None,
            },
        )),
        PredicateKind::Approximate {
            marker: _,
            paths: _,
            role: _,
            sound_subset: _,
        } => None,
    }
}

// An unknown positive failure alternative can be omitted; negation needs every alternative.
fn global_failure_disjunction(
    alternatives: impl Iterator<Item = Option<helm_schema_core::Predicate>>,
    require_exact: bool,
) -> Option<helm_schema_core::Predicate> {
    let mut projected = Vec::new();
    for alternative in alternatives {
        match alternative {
            Some(predicate) => projected.push(predicate),
            None if require_exact => return None,
            None => {}
        }
    }
    Some(helm_schema_core::Predicate::Or(projected))
}

fn project_global_failure_guard(
    guard: &Guard,
    global_sources: &[String],
    budget: &mut usize,
    require_exact: bool,
) -> Option<helm_schema_core::Predicate> {
    use helm_schema_core::{Predicate, Segment, ValuesPath};

    match guard {
        Guard::AnyOf { alternatives } => {
            return global_failure_disjunction(
                alternatives.iter().map(|alternative| {
                    Some(Predicate::And(
                        alternative
                            .iter()
                            .map(|guard| {
                                project_global_failure_guard(
                                    guard,
                                    global_sources,
                                    budget,
                                    require_exact,
                                )
                            })
                            .collect::<Option<_>>()?,
                    ))
                }),
                require_exact,
            );
        }
        Guard::Or { paths } => {
            return global_failure_disjunction(
                paths.iter().map(|path| {
                    project_global_failure_guard(
                        &Guard::Truthy { path: path.clone() },
                        global_sources,
                        budget,
                        require_exact,
                    )
                }),
                require_exact,
            );
        }
        _ => {}
    }
    let dependency_global = global_sources.last()?;
    let dependency_path = ValuesPath::parse(dependency_global);
    let paths = match guard {
        Guard::HasKey { path, key } | Guard::NotHasKey { path, key } => {
            vec![ValuesPath::from_segments(
                path.segments()
                    .cloned()
                    .chain([Segment::from(key.as_str())]),
            )]
        }
        _ => guard.value_paths(),
    };
    let paths = paths.into_iter().collect::<BTreeSet<_>>();
    let mut paths = paths.iter();
    let Some(path) = paths.next() else {
        return Some(Predicate::from(guard.clone()));
    };
    if paths.next().is_some() {
        return None;
    }
    if path == &dependency_path {
        return None;
    }
    if !path.is_descendant_of(&dependency_path) {
        return Some(Predicate::from(guard.clone()));
    }
    let relative = path
        .segments()
        .skip(dependency_path.segments().len())
        .map(Segment::encode_component)
        .collect::<Vec<_>>();
    let (selection_relative, key) = global_selection_path(&relative)?;
    let mut alternatives = Vec::new();
    for (source_index, global_source) in global_sources.iter().enumerate() {
        *budget = budget.checked_sub(1)?;
        let selection_source = helm_schema_core::join_encoded_value_path(
            helm_schema_core::split_value_path(global_source)
                .into_iter()
                .chain(selection_relative.iter().cloned()),
        );
        let mut conjunction = global_source_selection_guards(
            global_sources,
            selection_relative,
            source_index,
            &selection_source,
            key,
        )
        .into_iter()
        .map(Predicate::from)
        .collect::<Vec<_>>();
        conjunction.push(Predicate::from(guard.clone().map_value_paths(
            &mut |path| replace_value_path_prefix(path, dependency_global, global_source),
        )));
        alternatives.push(Predicate::And(conjunction));
    }
    Some(Predicate::Or(alternatives))
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
        .map(helm_schema_core::Segment::encode_component)
        .collect::<Vec<_>>();
    let projected_paths = range_modes
        .iter()
        .filter_map(|(path, mode)| {
            let segments = path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
            let relative = segments.strip_prefix(dependency_segments.as_slice())?;
            Some((path.clone(), relative.to_vec(), mode))
        })
        .collect::<Vec<_>>();

    for (path, relative, mode) in projected_paths {
        range_modes.remove(&path);
        if relative.is_empty() {
            for global_source in global_sources {
                range_modes.merge_mode(&helm_schema_core::ValuesPath::parse(global_source), mode);
            }
            continue;
        }
        if global_selection_path(&relative).is_none() {
            continue;
        }
        for global_source in global_sources {
            let projected = helm_schema_core::join_encoded_value_path(
                helm_schema_core::split_value_path(global_source)
                    .into_iter()
                    .chain(relative.iter().cloned()),
            );
            range_modes.merge_mode(&helm_schema_core::ValuesPath::parse(&projected), mode);
        }
    }
}
