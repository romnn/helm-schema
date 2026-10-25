use super::eval::Executor;
use super::{Datum, DocumentExecution, Field, ObjectData, Remainder, State, Value};
use crate::abstract_value::AbstractValue;
use helm_schema_core::ValuesPath;

#[tracing::instrument(skip_all)]
pub(crate) fn execute_document(
    source: &str,
    source_path: Option<&str>,
    db: &crate::analysis_db::IrAnalysisDb,
) -> DocumentExecution {
    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Open);
    if let Some(id) = state.map_target(&root) {
        state.heap.write(
            &id,
            "Values",
            Field::Present(Value::input(ValuesPath::from_segments(std::iter::empty::<
                String,
            >()))),
        );
        let files = state.heap.allocate(
            ObjectData::Opaque {
                reachable: super::AliasSet::RenderGraph,
            },
            super::Multiplicity::Singleton,
        );
        state.heap.write(
            &id,
            "Files",
            Field::Present(Value::leaf(Datum::Unknown(super::UnknownValue {
                types: super::TypeCoverage::Unresolved,
                aliases: super::AliasSet::Known(std::collections::BTreeSet::from([files])),
            }))),
        );
        for (key, value) in db.static_root_fields() {
            let value = initial_value(value, &mut state);
            state.heap.write(&id, key, Field::Present(value));
        }
    }
    if let Some(frame) = state.frames.first_mut() {
        frame.bindings.insert("$".to_owned(), root.clone());
        frame.dot = Some(root);
    }
    DocumentExecution::new(Executor { db }.body(source, source_path, state))
}
fn initial_value(value: &AbstractValue, state: &mut State) -> Value {
    match value {
        AbstractValue::Dict(fields) => {
            let value = state.heap.map(Remainder::Open);
            if let Some(id) = state.map_target(&value) {
                for (key, value) in fields {
                    let value = initial_value(value, state);
                    state.heap.write(&id, key, Field::Present(value));
                }
            }
            value
        }
        AbstractValue::StringSet(strings) if strings.len() == 1 => strings
            .first()
            .map_or_else(Value::unknown, |value| Value::string(value)),
        AbstractValue::ValuesPath(path) => Value::input(path.clone()),
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::JsonDecodedPath(_)
        | AbstractValue::RangeKey(_)
        | AbstractValue::KeysList(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::List(_)
        | AbstractValue::Overlay {
            entries: _,
            fallback: _,
        }
        | AbstractValue::Choice(_)
        | AbstractValue::FirstTruthy(_)
        | AbstractValue::MergedLayers(_)
        | AbstractValue::SplitList {
            source_paths: _,
            separator: _,
            total_text_preimage: _,
        }
        | AbstractValue::SplitSegment {
            source_paths: _,
            separator: _,
            last: _,
            total_text_preimage: _,
        }
        | AbstractValue::Widened(_) => Value::unknown(),
    }
}
