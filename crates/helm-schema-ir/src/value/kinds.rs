use helm_schema_core::{Predicate, PredicateMemo};

use super::scalar_value::{ScalarContent, TruthCondition};
use super::{ScalarValue, ScalarValueDispatch};

/// Runtime types distinguished by R2 parameter checks; conflicting types widen to unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GoType {
    Nil,
    Bool,
    String,
    Int,
    Int64,
    Uint8,
    Uint64,
    Float64,
    Complex128,
    Map,
    Slice,
    Unknown,
}

impl GoType {
    pub(crate) fn joined(self, other: Self) -> Self {
        if self == other { self } else { Self::Unknown }
    }
}

pub(crate) struct KindArm<'a> {
    pub(crate) condition: &'a Predicate,
    pub(crate) go_type: GoType,
    pub(crate) content: Option<&'a ScalarContent>,
}

impl ScalarValue {
    pub(super) fn untyped(content: ScalarContent) -> Self {
        Self {
            go_type: GoType::Unknown,
            content: Some(content),
        }
    }

    pub(super) fn type_only(go_type: GoType) -> Self {
        Self {
            go_type,
            content: None,
        }
    }
}

impl ScalarValueDispatch {
    pub(crate) fn kinds(&self) -> impl Iterator<Item = KindArm<'_>> {
        self.arms.iter().map(|(condition, value)| {
            let ScalarValue { go_type, content } = value;
            KindArm {
                condition,
                go_type: *go_type,
                content: content.as_ref(),
            }
        })
    }

    /// Refines existing leaves without changing content, truth, selection, effects or content bounds.
    ///
    /// Unresolved coverage receives disjoint type-only arms with a separate bound.
    /// A legacy empty projection remains empty rather than acquiring a new fallback meaning.
    pub(crate) fn refine_type(&mut self, go_type: GoType, memo: &PredicateMemo) {
        if self.arms.is_empty() {
            return;
        }
        for (_, value) in &mut self.arms {
            value.go_type = go_type;
        }
        if self.complete {
            return;
        }
        let covered = TruthCondition::any_with_memo(
            self.arms.iter().map(|(condition, _)| {
                TruthCondition::from_predicate_with_memo(condition.clone(), memo)
            }),
            memo,
        );
        if let Some(covered) = covered.predicate() {
            let missing = covered.clone().negated();
            if missing != Predicate::False {
                self.arms.push((missing, ScalarValue::type_only(go_type)));
            }
        }
        self.bound_type_arms();
    }

    pub(crate) fn typed(go_type: GoType) -> Self {
        Self {
            arms: vec![(Predicate::True, ScalarValue::type_only(go_type))],
            complete: false,
        }
    }

    pub(super) fn widen_types(&mut self) {
        for (_, value) in &mut self.arms {
            value.go_type = GoType::Unknown;
        }
    }

    pub(super) fn bound_type_arms(&mut self) {
        const MAX_TYPE_ARMS: usize = 128;
        if self
            .arms
            .iter()
            .filter(|(_, value)| value.content.is_none())
            .count()
            > MAX_TYPE_ARMS
        {
            self.arms.retain(|(_, value)| value.content.is_some());
            self.widen_types();
            self.complete = false;
            if self.arms.is_empty() {
                self.arms
                    .push((Predicate::True, ScalarValue::type_only(GoType::Unknown)));
            }
        }
    }
}

impl ScalarValueDispatch {
    pub(crate) fn content_eq(&self, other: &Self) -> bool {
        match (self.content(), other.content()) {
            (Some(left), Some(right)) => {
                left.complete == right.complete && left.arms().eq(right.arms())
            }
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }

    pub(crate) fn join_equal_content<'a>(
        &self,
        others: impl IntoIterator<Item = &'a Self>,
    ) -> Option<Self> {
        let mut joined = self.clone();
        for other in others {
            if !self.content_eq(other) {
                return None;
            }
            for (condition, leaf) in &mut joined.arms {
                let incoming = other.arms.iter().find(|(incoming_condition, incoming)| {
                    incoming_condition == condition && incoming.content == leaf.content
                });
                leaf.go_type = incoming.map_or(GoType::Unknown, |(_, incoming)| {
                    leaf.go_type.joined(incoming.go_type)
                });
            }
        }
        Some(joined)
    }

    pub(super) fn content_len(&self) -> usize {
        self.arms
            .iter()
            .filter(|(_, value)| value.content.is_some())
            .count()
    }
}

pub(crate) enum LegacyTypeView<'a> {
    Schema(&'static str),
    Identity(&'a helm_schema_core::ValuesPath),
}

impl ScalarContent {
    pub(crate) fn legacy_type(&self) -> LegacyTypeView<'_> {
        use helm_schema_core::GuardValue;
        LegacyTypeView::Schema(match self {
            Self::Literal(GuardValue::String(_))
            | Self::Rendered(_)
            | Self::PrintfStringIdentity(_) => "string",
            Self::Literal(GuardValue::Bool(_)) => "boolean",
            Self::Literal(GuardValue::Int(_)) | Self::SplitLength { .. } | Self::Length(_) => {
                "integer"
            }
            Self::Literal(GuardValue::Float(_)) => "number",
            Self::Literal(GuardValue::Null) => "null",
            Self::Identity(path) => return LegacyTypeView::Identity(path),
        })
    }
}
