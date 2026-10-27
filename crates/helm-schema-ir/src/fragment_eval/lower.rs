//! Fragment parts projected from frozen scalar content.

use std::collections::BTreeSet;

use crate::ValueKind;
use crate::scalar_value::{ScalarRenderPart, ScalarValueDispatch};
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, Splice, SpliceMeta, StringPart,
};

pub(crate) fn lower_scalar_dispatch_arms(
    dispatch: &ScalarValueDispatch,
    kind: ValueKind,
) -> Option<Vec<(PathCondition, Vec<StringPart>)>> {
    let dispatch = dispatch.content()?;
    let mut arms = Vec::with_capacity(dispatch.arms().count());
    for (condition, value) in dispatch.arms() {
        if *condition == Predicate::False {
            continue;
        }
        let rendered = value.rendered_parts()?;
        let parts = lower_rendered_parts(&rendered, kind);
        arms.push((condition.clone(), parts));
    }
    Some(arms)
}

pub(crate) fn lower_scalar_dispatch(
    dispatch: &ScalarValueDispatch,
    kind: ValueKind,
) -> Option<Guarded<AbstractFragment>> {
    let dispatch = dispatch.content()?;
    let mut out = Guarded::empty();
    for (condition, value) in dispatch.arms() {
        if *condition == Predicate::False {
            continue;
        }
        let rendered = value.rendered_parts()?;
        out.arms.push((
            condition.clone(),
            AbstractFragment::Scalar(AbstractString {
                parts: lower_rendered_parts(&rendered, kind),
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
