use std::collections::{BTreeMap, BTreeSet};

pub(crate) type TypeHints = BTreeMap<String, BTreeSet<String>>;

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
}

impl ObservedFacts {
    pub(crate) fn insert_type_hint(
        &mut self,
        legacy: &mut TypeHints,
        grade: HintGrade,
        path: String,
        schema_type: &str,
    ) {
        crate::helper_meta::insert_type_hint(legacy, path.clone(), schema_type);
        crate::helper_meta::insert_type_hint(
            self.type_hints.entry(grade).or_default(),
            path,
            schema_type,
        );
    }

    pub(crate) fn extend_type_hints(
        &mut self,
        legacy: &mut TypeHints,
        grade: HintGrade,
        path: &str,
        hints: &BTreeSet<String>,
    ) {
        legacy
            .entry(path.to_owned())
            .or_default()
            .extend(hints.iter().cloned());
        self.type_hints
            .entry(grade)
            .or_default()
            .entry(path.to_owned())
            .or_default()
            .extend(hints.iter().cloned());
    }

    pub(crate) fn absorb(&mut self, other: &Self) {
        let Self { type_hints } = other;
        for (grade, paths) in type_hints {
            for (path, hints) in paths {
                self.type_hints
                    .entry(*grade)
                    .or_default()
                    .entry(path.clone())
                    .or_default()
                    .extend(hints.iter().cloned());
            }
        }
    }
}
