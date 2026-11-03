use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ValuesPath;

mod captures;

pub(crate) use captures::{CaptureKind, FailCapture, StringRequirementRoute};

pub(crate) type TypeHints = BTreeMap<ValuesPath, BTreeSet<String>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObservedResource {
    pub(crate) condition: helm_schema_core::Predicate,
    pub(crate) resource: helm_schema_core::ResourceRef,
}

impl ObservedResource {
    fn map_value_paths(&mut self, map: &mut impl FnMut(ValuesPath) -> ValuesPath, shared: bool) {
        let Self {
            condition,
            resource,
        } = self;
        *condition = if shared {
            condition.clone().map_value_paths_shared(map)
        } else {
            condition.clone().map_value_paths(map)
        };
        let helm_schema_core::ResourceRef {
            api_version: _,
            kind: _,
            kind_candidates: _,
            api_version_candidates: _,
            api_version_branches: _,
            kind_branches,
            kind_selector,
        } = resource;
        for helm_schema_core::KindBranch { predicate, kind: _ } in kind_branches {
            *predicate = if shared {
                predicate.clone().map_value_paths_shared(map)
            } else {
                predicate.clone().map_value_paths(map)
            };
        }
        if let Some(path) = kind_selector {
            *path = map(path.clone());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ActivatedValuesDefaultSource {
    pub(crate) guards: Vec<crate::Guard>,
    pub(crate) source: crate::ValuesDefaultSource,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ActivatedValuesRootOverlay {
    pub(crate) guards: Vec<crate::Guard>,
    pub(crate) target_path: ValuesPath,
    pub(crate) source_path: ValuesPath,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ValuesRootOverlay {
    pub(crate) target_path: ValuesPath,
    pub(crate) source_path: ValuesPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HintScope {
    Unconditional,
    Guarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HintIntent {
    Declared,
    Fallback,
    Tested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HintGrade {
    pub(crate) scope: HintScope,
    pub(crate) intent: HintIntent,
}

impl HintGrade {
    pub(crate) const DECLARED: Self = Self::new(HintScope::Unconditional, HintIntent::Declared);
    pub(crate) const GUARDED_DECLARED: Self = Self::new(HintScope::Guarded, HintIntent::Declared);
    pub(crate) const FALLBACK: Self = Self::new(HintScope::Unconditional, HintIntent::Fallback);
    pub(crate) const GUARDED_FALLBACK: Self = Self::new(HintScope::Guarded, HintIntent::Fallback);
    pub(crate) const TESTED: Self = Self::new(HintScope::Unconditional, HintIntent::Tested);

    const fn new(scope: HintScope, intent: HintIntent) -> Self {
        Self { scope, intent }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ObservedFacts {
    pub(crate) resources: BTreeSet<ObservedResource>,
    pub(crate) type_hints: BTreeMap<HintGrade, TypeHints>,
    pub(crate) shape_erased_paths: BTreeSet<ValuesPath>,
    pub(crate) range_modes: crate::range_modes::RangeModes,
    pub(crate) opaque_values_root_paths: BTreeSet<ValuesPath>,
    pub(crate) values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    pub(crate) activated_values_default_sources: BTreeSet<ActivatedValuesDefaultSource>,
    pub(crate) values_root_overlays: BTreeSet<ValuesRootOverlay>,
    pub(crate) activated_values_root_overlays: BTreeSet<ActivatedValuesRootOverlay>,
    pub(crate) values_root_helper_includes: BTreeSet<String>,
    pub(crate) captures: BTreeSet<FailCapture>,
    pub(crate) consumptions: Vec<crate::value::FrozenConsumption>,
}

impl ObservedFacts {
    pub(crate) fn insert_type_hint(
        &mut self,
        grade: HintGrade,
        path: ValuesPath,
        schema_type: &str,
    ) {
        if path.segments().next().is_none() {
            return;
        }
        self.type_hints
            .entry(grade)
            .or_default()
            .entry(path)
            .or_default()
            .insert(schema_type.to_string());
    }

    pub(crate) fn extend_type_hints(
        &mut self,
        grade: HintGrade,
        path: &ValuesPath,
        hints: &BTreeSet<String>,
    ) {
        self.type_hints
            .entry(grade)
            .or_default()
            .entry(path.clone())
            .or_default()
            .extend(hints.iter().cloned());
    }

    pub(crate) fn promote_tested_type_hints(&mut self) {
        let tested = self
            .type_hints
            .remove(&HintGrade::TESTED)
            .unwrap_or_default();
        for (path, hints) in tested {
            self.extend_type_hints(HintGrade::GUARDED_DECLARED, &path, &hints);
        }
    }

    pub(crate) fn execution_only(mut self) -> Self {
        self.type_hints.clear();
        self
    }

    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F, shared: bool)
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        let Self {
            type_hints,
            resources,
            shape_erased_paths,
            range_modes,
            opaque_values_root_paths,
            values_default_sources,
            activated_values_default_sources,
            values_root_overlays,
            activated_values_root_overlays,
            values_root_helper_includes: _,
            captures,
            consumptions,
        } = self;
        crate::value::map_consumption_paths(consumptions, map, shared);
        *resources = std::mem::take(resources)
            .into_iter()
            .map(|mut resource| {
                resource.map_value_paths(map, shared);
                resource
            })
            .collect();
        for paths in type_hints.values_mut() {
            let mut mapped = TypeHints::new();
            for (path, hints) in std::mem::take(paths) {
                mapped.entry(map(path)).or_default().extend(hints);
            }
            *paths = mapped;
        }
        *shape_erased_paths = std::mem::take(shape_erased_paths)
            .into_iter()
            .map(&mut *map)
            .collect();
        range_modes.map_value_paths(map);
        *opaque_values_root_paths = std::mem::take(opaque_values_root_paths)
            .into_iter()
            .map(&mut *map)
            .collect();
        *values_default_sources = std::mem::take(values_default_sources)
            .into_iter()
            .map(|source| crate::ValuesDefaultSource {
                target_path: map(source.target_path),
                source_path: map(source.source_path),
            })
            .collect();
        *activated_values_default_sources = std::mem::take(activated_values_default_sources)
            .into_iter()
            .map(|fact| ActivatedValuesDefaultSource {
                guards: fact
                    .guards
                    .into_iter()
                    .map(|guard| guard.map_value_paths(map))
                    .collect(),
                source: crate::ValuesDefaultSource {
                    target_path: map(fact.source.target_path),
                    source_path: map(fact.source.source_path),
                },
            })
            .collect();
        *values_root_overlays = std::mem::take(values_root_overlays)
            .into_iter()
            .map(|fact| ValuesRootOverlay {
                target_path: map(fact.target_path),
                source_path: map(fact.source_path),
            })
            .collect();
        *activated_values_root_overlays = std::mem::take(activated_values_root_overlays)
            .into_iter()
            .map(|fact| ActivatedValuesRootOverlay {
                guards: fact
                    .guards
                    .into_iter()
                    .map(|guard| guard.map_value_paths(map))
                    .collect(),
                target_path: map(fact.target_path),
                source_path: map(fact.source_path),
            })
            .collect();
        *captures = std::mem::take(captures)
            .into_iter()
            .map(|mut capture| {
                capture.conjunction = capture
                    .conjunction
                    .into_iter()
                    .map(|predicate| {
                        if shared {
                            predicate.map_value_paths_shared(map)
                        } else {
                            predicate.map_value_paths(map)
                        }
                    })
                    .collect();
                capture.ranged.map_value_paths(map);
                capture.kind.map_paths(map, shared);
                capture
            })
            .collect();
    }

    pub(crate) fn absorb(&mut self, other: &Self) {
        let Self {
            type_hints,
            resources,
            shape_erased_paths,
            range_modes,
            opaque_values_root_paths,
            values_default_sources,
            activated_values_default_sources,
            values_root_overlays,
            activated_values_root_overlays,
            values_root_helper_includes,
            captures,
            consumptions,
        } = other;
        self.consumptions.extend(consumptions.iter().cloned());
        self.resources.extend(resources.iter().cloned());
        for (grade, paths) in type_hints {
            for (path, hints) in paths {
                self.extend_type_hints(*grade, path, hints);
            }
        }
        self.shape_erased_paths
            .extend(shape_erased_paths.iter().cloned());
        self.range_modes.merge(range_modes);
        self.opaque_values_root_paths
            .extend(opaque_values_root_paths.iter().cloned());
        self.values_default_sources
            .extend(values_default_sources.iter().cloned());
        self.activated_values_default_sources
            .extend(activated_values_default_sources.iter().cloned());
        self.values_root_overlays
            .extend(values_root_overlays.iter().cloned());
        self.activated_values_root_overlays
            .extend(activated_values_root_overlays.iter().cloned());
        self.values_root_helper_includes
            .extend(values_root_helper_includes.iter().cloned());
        self.captures.extend(captures.iter().cloned());
    }
}
