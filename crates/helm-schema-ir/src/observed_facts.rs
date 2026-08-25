use std::collections::{BTreeMap, BTreeSet};

use crate::eval_effect::FailCapture;

pub(crate) type TypeHints = BTreeMap<String, BTreeSet<String>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ActivatedValuesDefaultSource {
    pub(crate) guards: Vec<crate::Guard>,
    pub(crate) source: crate::ValuesDefaultSource,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ActivatedValuesRootOverlay {
    pub(crate) guards: Vec<crate::Guard>,
    pub(crate) target_path: String,
    pub(crate) source_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ValuesRootOverlay {
    pub(crate) target_path: String,
    pub(crate) source_path: String,
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
    pub(crate) type_hints: BTreeMap<HintGrade, TypeHints>,
    pub(crate) shape_erased_paths: BTreeSet<String>,
    pub(crate) range_modes: crate::range_modes::RangeModes,
    pub(crate) values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    pub(crate) activated_values_default_sources: BTreeSet<ActivatedValuesDefaultSource>,
    pub(crate) values_root_overlays: BTreeSet<ValuesRootOverlay>,
    pub(crate) activated_values_root_overlays: BTreeSet<ActivatedValuesRootOverlay>,
    pub(crate) values_root_helper_includes: BTreeSet<String>,
    pub(crate) captures: BTreeSet<FailCapture>,
}

impl ObservedFacts {
    pub(crate) fn insert_type_hint(&mut self, grade: HintGrade, path: String, schema_type: &str) {
        crate::helper_meta::insert_type_hint(
            self.type_hints.entry(grade).or_default(),
            path,
            schema_type,
        );
    }

    pub(crate) fn extend_type_hints(
        &mut self,
        grade: HintGrade,
        path: &str,
        hints: &BTreeSet<String>,
    ) {
        self.type_hints
            .entry(grade)
            .or_default()
            .entry(path.to_owned())
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

    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        for paths in self.type_hints.values_mut() {
            let mut mapped = TypeHints::new();
            for (path, hints) in std::mem::take(paths) {
                mapped.entry(map(&path)).or_default().extend(hints);
            }
            *paths = mapped;
        }
        self.shape_erased_paths = std::mem::take(&mut self.shape_erased_paths)
            .into_iter()
            .map(|path| map(&path))
            .collect();
        self.range_modes.map_value_paths(map);
        self.values_default_sources = std::mem::take(&mut self.values_default_sources)
            .into_iter()
            .map(|source| crate::ValuesDefaultSource {
                target_path: map(&source.target_path),
                source_path: map(&source.source_path),
            })
            .collect();
        self.activated_values_default_sources =
            std::mem::take(&mut self.activated_values_default_sources)
                .into_iter()
                .map(|fact| ActivatedValuesDefaultSource {
                    guards: fact
                        .guards
                        .into_iter()
                        .map(|guard| guard.map_value_paths(map))
                        .collect(),
                    source: crate::ValuesDefaultSource {
                        target_path: map(&fact.source.target_path),
                        source_path: map(&fact.source.source_path),
                    },
                })
                .collect();
        self.values_root_overlays = std::mem::take(&mut self.values_root_overlays)
            .into_iter()
            .map(|fact| ValuesRootOverlay {
                target_path: map(&fact.target_path),
                source_path: map(&fact.source_path),
            })
            .collect();
        self.activated_values_root_overlays =
            std::mem::take(&mut self.activated_values_root_overlays)
                .into_iter()
                .map(|fact| ActivatedValuesRootOverlay {
                    guards: fact
                        .guards
                        .into_iter()
                        .map(|guard| guard.map_value_paths(map))
                        .collect(),
                    target_path: map(&fact.target_path),
                    source_path: map(&fact.source_path),
                })
                .collect();
        self.captures = std::mem::take(&mut self.captures)
            .into_iter()
            .map(|mut capture| {
                capture.conjunction = capture
                    .conjunction
                    .into_iter()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect();
                capture.ranged.map_value_paths(map);
                capture.kind.map_value_paths(map);
                capture
            })
            .collect();
    }

    pub(crate) fn absorb(&mut self, other: &Self) {
        let Self {
            type_hints,
            shape_erased_paths,
            range_modes,
            values_default_sources,
            activated_values_default_sources,
            values_root_overlays,
            activated_values_root_overlays,
            values_root_helper_includes,
            captures,
        } = other;
        for (grade, paths) in type_hints {
            for (path, hints) in paths {
                self.extend_type_hints(*grade, path, hints);
            }
        }
        self.shape_erased_paths
            .extend(shape_erased_paths.iter().cloned());
        self.range_modes.merge(range_modes);
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
