//! Fragment parts projected from frozen scalar content.

use std::collections::BTreeSet;

use crate::ValueKind;
use crate::scalar_value::ScalarContent;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, Splice, SpliceMeta, StringPart,
};

/// One exact contribution to helper-rendered scalar text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarRenderPart {
    Text(String),
    Identity {
        path: helm_schema_core::ValuesPath,
        /// The text is Go's total scalar rendering of the raw value. When
        /// false, an earlier string consumer has already restricted the
        /// runtime input to strings.
        stringified: bool,
    },
}

impl ScalarContent {
    fn rendered_parts(&self) -> Option<Vec<ScalarRenderPart>> {
        let parts = match self {
            Self::StringDomain {
                pattern: _,
                min_bytes: _,
                max_bytes: _,
            } => return None,
            Self::Literal(value) => {
                let text = match value {
                    helm_schema_core::GuardValue::String(text)
                    | helm_schema_core::GuardValue::Float(text) => text.clone(),
                    helm_schema_core::GuardValue::Bool(value) => value.to_string(),
                    helm_schema_core::GuardValue::Int(value) => value.to_string(),
                    helm_schema_core::GuardValue::Null => return None,
                };
                vec![ScalarRenderPart::Text(text)]
            }
            Self::Identity(path) => vec![ScalarRenderPart::Identity {
                path: path.clone(),
                stringified: true,
            }],
        };
        Some(parts)
    }
}

/// Rendered scalar content read from one executed emission.
pub(crate) struct ExecutedScalarView {
    projection: crate::value::ScalarValueDispatch,
}

impl crate::value::ExecutedSite {
    pub(crate) fn scalar_view(&self) -> Option<ExecutedScalarView> {
        self.rendered_scalar()
            .map(|projection| ExecutedScalarView { projection })
    }
}

impl ExecutedScalarView {
    /// Maps known renderable arms without asserting exhaustive coverage.
    /// Returns `None` when a retained content term cannot be rendered.
    pub(crate) fn map_rendered_arms<T>(
        &self,
        mut map: impl FnMut(&Predicate, &[ScalarRenderPart]) -> T,
    ) -> Option<Vec<T>> {
        let Self { projection } = self;
        let content = projection.content()?;
        let mut arms = Vec::with_capacity(content.arms().count());
        for (condition, value) in content.arms() {
            if *condition == Predicate::False {
                continue;
            }
            arms.push(map(condition, &value.rendered_parts()?));
        }
        Some(arms)
    }
}

pub(crate) fn lower_executed_scalar_arms(
    scalar: &ExecutedScalarView,
    kind: ValueKind,
) -> Option<Vec<(PathCondition, Vec<StringPart>)>> {
    scalar.map_rendered_arms(|condition, rendered| {
        (condition.clone(), lower_rendered_parts(rendered, kind))
    })
}

pub(crate) fn lower_executed_scalar(
    scalar: &ExecutedScalarView,
    kind: ValueKind,
) -> Option<Guarded<AbstractFragment>> {
    let arms = lower_executed_scalar_arms(scalar, kind)?;
    let mut out = Guarded::empty();
    for (condition, parts) in arms {
        out.arms.push((
            condition,
            AbstractFragment::Scalar(AbstractString {
                parts,
                suppressed: false,
            }),
        ));
    }
    Some(out)
}

fn lower_rendered_parts(rendered: &[ScalarRenderPart], kind: ValueKind) -> Vec<StringPart> {
    let mut parts = rendered
        .iter()
        .map(|part| match part {
            ScalarRenderPart::Text(text) => StringPart::Text(BTreeSet::from([text.clone()])),
            ScalarRenderPart::Identity { path, stringified } => StringPart::Splice(Splice {
                values_path: path.clone(),
                kind,
                meta: SpliceMeta {
                    input_identity: true,
                    stringified: *stringified,
                    ..SpliceMeta::default()
                },
            }),
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        parts.push(StringPart::Text(BTreeSet::from([String::new()])));
    }
    parts
}

pub(crate) fn lower_serialized_payload(
    payload: &crate::value::use_policy::RelationshipUse<'_>,
) -> Guarded<AbstractFragment> {
    let crate::value::use_policy::RelationshipUse {
        condition,
        source,
        output_path: _,
        kind,
        stringified: _,
        nil_omitting: _,
        digest: _,
        capture: _,
        strip_marker,
        flow_node: _,
        text_preimage: _,
        omitted_members,
    } = payload;
    Guarded {
        arms: vec![(
            condition.clone(),
            AbstractFragment::Splice(Splice {
                values_path: source.as_ref().clone(),
                kind: *kind,
                meta: SpliceMeta {
                    input_identity: omitted_members.is_empty(),
                    strip_marker: *strip_marker,
                    omitted_members: omitted_members
                        .iter()
                        .cloned()
                        .map(|key| (key, Vec::new()))
                        .collect(),
                    ..SpliceMeta::default()
                },
            }),
        )],
    }
}

pub(crate) fn lower_serialized_items(
    payload: &crate::value::use_policy::RelationshipUse<'_>,
    memo: &helm_schema_core::PredicateMemo,
) -> Guarded<AbstractFragment> {
    let mut result = lower_serialized_payload(payload);
    for (_, fragment) in &mut result.arms {
        if let AbstractFragment::Splice(splice) = fragment {
            splice.values_path = helm_schema_core::ValuesPath::from_segments(
                splice
                    .values_path
                    .segments()
                    .cloned()
                    .chain(std::iter::once(helm_schema_core::Segment::EachMember)),
            );
        }
    }
    result.guard_all(
        &helm_schema_core::Predicate::from(helm_schema_core::Guard::TypeIs {
            path: payload.source.as_ref().clone(),
            schema_type: "array".to_owned(),
        }),
        memo,
    );
    result
}
