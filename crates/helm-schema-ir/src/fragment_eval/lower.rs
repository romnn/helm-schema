//! Fragment parts projected from frozen scalar content.

use std::collections::BTreeSet;

use crate::ValueKind;
use crate::scalar_value::{ExecutedScalarView, ScalarRenderPart};

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, Splice, SpliceMeta, StringPart,
};

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
            ScalarRenderPart::Identity {
                path,
                stringified,
                lexical_escapes,
            } => StringPart::Splice(Splice {
                values_path: path.clone(),
                kind,
                meta: SpliceMeta {
                    input_identity: true,
                    stringified: *stringified,
                    lexical_escapes: lexical_escapes.clone(),
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
    payload: &crate::value::output::SerializedPayload<'_>,
) -> Guarded<AbstractFragment> {
    let crate::value::output::SerializedPayload {
        condition,
        path,
        kind,
        spelling: _,
        strip_marker,
        flow_node: _,
        omitted_members,
    } = payload;
    Guarded {
        arms: vec![(
            condition.clone(),
            AbstractFragment::Splice(Splice {
                values_path: (*path).clone(),
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
    payload: &crate::value::output::SerializedPayload<'_>,
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
            path: payload.path.clone(),
            schema_type: "array".to_owned(),
        }),
        memo,
    );
    result
}
