use helm_schema_core::Predicate;

use super::scalar_value::ScalarContent;
use super::{ScalarValue, ScalarValueDispatch};

/// Runtime types carried by the scalar leaf and function signatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GoType {
    Nil,
    Bool,
    String,
    Int,
    Int64,
    Uint64,
    Float64,
    Map,
    Slice,
    Unknown,
}

impl GoType {
    pub(crate) fn joined(self, other: Self) -> Self {
        if self == other { self } else { Self::Unknown }
    }
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
    /// Refines the type on retained leaves without changing content or arm coverage.
    ///
    /// Unmodeled branches remain unresolved instead of acquiring a parallel type-only partition.
    pub(crate) fn refine_type(&mut self, go_type: GoType) {
        for (_, value) in &mut self.arms {
            value.go_type = go_type;
        }
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

#[derive(Clone, Copy)]
pub(crate) enum LegacyTypeView<'a> {
    Schema(&'static str),
    Identity(&'a helm_schema_core::ValuesPath),
    JsonDecoded(&'a helm_schema_core::ValuesPath),
    Alternatives(&'a std::collections::BTreeSet<crate::abstract_value::AbstractValue>),
    FirstTruthy(&'a [crate::abstract_value::AbstractValue]),
    Unknown,
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

impl crate::abstract_value::AbstractValue {
    pub(crate) fn legacy_type(&self) -> LegacyTypeView<'_> {
        use LegacyTypeView::{Alternatives, FirstTruthy, Identity, JsonDecoded, Schema, Unknown};
        match self {
            Self::ValuesPath(path) => Identity(path),
            Self::OutputPath(path, meta) if meta.is_input_identity() => Identity(path),
            Self::JsonDecodedPath(path) => JsonDecoded(path),
            Self::OutputPath(path, meta) if meta.json_decoded => JsonDecoded(path),
            Self::Dict(_) | Self::Overlay { .. } | Self::MergedLayers(_) | Self::RootContext => {
                Schema("object")
            }
            Self::List(_) | Self::KeysList(_) | Self::SplitList { .. } => Schema("array"),
            Self::StringSet(_) | Self::SplitSegment { .. } => Schema("string"),
            Self::DerivedBoolean(_) => Schema("boolean"),
            Self::Choice(choices) => Alternatives(choices),
            Self::FirstTruthy(candidates) => FirstTruthy(candidates),
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::OutputPath(_, _)
            | Self::Widened(_) => Unknown,
        }
    }
}
