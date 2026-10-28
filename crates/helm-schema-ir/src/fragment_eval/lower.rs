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
