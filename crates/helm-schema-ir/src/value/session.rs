use super::eval::Executor;
use super::{
    Datum, DocumentExecution, Field, Flow, Leaf, MapType, ObjectData, Origin, Remainder, State,
    TextBuffer, Value, ValueNode,
};
use crate::abstract_value::AbstractValue;
use helm_schema_core::ValuesPath;

pub(super) struct FileOccurrence {
    pub(super) program: u64,
    pub(super) call: u64,
    template: super::ObjectId,
}

impl FileOccurrence {
    pub(super) fn new(state: &State) -> Self {
        Self {
            program: state.heap.arena.program(),
            call: state.heap.arena.call(),
            template: state.heap.arena.object(),
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn execute_document(
    source: &str,
    source_path: Option<&str>,
    db: &crate::analysis_db::IrAnalysisDb,
) -> DocumentExecution {
    let mut state = initial_state(db);
    let occurrence = FileOccurrence::new(&state);
    let root = state.read_root().cloned().unwrap_or_else(Value::unknown);
    begin_file(&mut state, &root, source_path, &occurrence);
    let syntax = crate::analysis_db::ParsedProgramSyntax::parse(source);
    DocumentExecution::new(finish_file(
        Executor { db }.parsed_file_body(source, source_path, syntax.as_ref(), state, &occurrence),
        db.predicate_memo(),
    ))
}

#[tracing::instrument(skip_all)]
pub(crate) fn execute_documents(
    sources: &[crate::symbolic::SymbolicTemplate<'_>],
    db: &crate::analysis_db::IrAnalysisDb,
) -> Vec<DocumentExecution> {
    let state = initial_state(db);
    let root = state.read_root().cloned().unwrap_or_else(Value::unknown);
    let mut states = vec![state];
    let mut documents: Vec<_> = sources
        .iter()
        .map(|_| DocumentExecution::new(Vec::new()))
        .collect();
    let mut order: Vec<_> = (0..sources.len()).collect();
    order.sort_by(|left, right| {
        let Some(left) = sources.get(*left) else {
            return std::cmp::Ordering::Equal;
        };
        let Some(right) = sources.get(*right) else {
            return std::cmp::Ordering::Equal;
        };
        right
            .source_path
            .bytes()
            .filter(|byte| *byte == b'/')
            .count()
            .cmp(
                &left
                    .source_path
                    .bytes()
                    .filter(|byte| *byte == b'/')
                    .count(),
            )
            .then_with(|| right.source_path.cmp(left.source_path))
    });
    for index in order {
        let Some(source) = sources.get(index) else {
            continue;
        };
        let Some(first) = states.first() else {
            continue;
        };
        let occurrence = FileOccurrence::new(first);
        let syntax = crate::analysis_db::ParsedProgramSyntax::parse(source.source);
        let file_span = tracing::debug_span!(
            "execute_file",
            path = source.source_path,
            incoming_states = states.len(),
            active_edges = states.iter().map(|state| state.active.len()).max(),
            heap_objects = states
                .iter()
                .map(|state| state
                    .heap
                    .version
                    .pages
                    .values()
                    .map(|page| page.len())
                    .sum::<usize>())
                .max()
        );
        let _entered = file_span.enter();
        let mut exits = super::ExitBuffer::default();
        for mut state in states {
            begin_file(&mut state, &root, Some(source.source_path), &occurrence);
            exits.extend(Executor { db }.parsed_file_body(
                source.source,
                Some(source.source_path),
                syntax.as_ref(),
                state,
                &occurrence,
            ));
        }
        let exits = finish_file(
            super::control::reconverge(exits.finish()),
            db.predicate_memo(),
        );
        if let Some(document) = documents.get_mut(index) {
            *document = DocumentExecution::new(exits.clone());
        }
        states = exits
            .into_iter()
            .filter_map(|exit| match exit.flow {
                Flow::Normal(_) => Some(exit.state),
                Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
            })
            .collect();
    }
    documents
}

fn finish_file(
    mut exits: Vec<super::Exit>,
    memo: &helm_schema_core::PredicateMemo,
) -> Vec<super::Exit> {
    for exit in &mut exits {
        if !matches!(exit.flow, Flow::Normal(_)) {
            continue;
        }
        let state = &mut exit.state;
        super::output_layout::certify_plain_scalar_placements(state, memo);
        let original = state.output.exact_text();
        let stripped = state.output.stripped_value();
        // Helm removes the marker after concatenation, including across emission boundaries.
        if original
            .as_ref()
            .is_none_or(|text| text.contains("<no value>"))
        {
            demote_stripped_sites(state, original.as_deref());
        }
        state.output = TextBuffer::default();
        state.output.emit(state.heap.freeze(&stripped));
    }
    exits
}

fn demote_stripped_sites(state: &mut State, original: Option<&str>) {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Site {
        Text(usize),
        Value(usize),
    }
    struct Piece {
        ordinal: usize,
        site: Site,
        text: String,
    }
    let mut pieces = Vec::new();
    for (index, site) in state.text_sites.iter().enumerate() {
        if site.program == state.program {
            pieces.push(Piece {
                ordinal: site.ordinal,
                site: Site::Text(index),
                text: site.bytes.clone(),
            });
        }
    }
    let mut complete = true;
    for (index, site) in state.sites.iter().enumerate() {
        if site.program == state.program && site.rendered {
            if let Some(text) = site.value.exact_text() {
                pieces.push(Piece {
                    ordinal: site.ordinal,
                    site: Site::Value(index),
                    text,
                });
            } else {
                complete = false;
            }
        }
    }
    pieces.sort_by_key(|piece| piece.ordinal);
    let joined: String = pieces.iter().map(|piece| piece.text.as_str()).collect();
    let mut affected = std::collections::BTreeSet::new();
    if let Some(original) = original.filter(|original| complete && *original == joined) {
        let removed: Vec<_> = original
            .match_indices("<no value>")
            .map(|(start, text)| start..start + text.len())
            .collect();
        let mut offset = 0;
        for piece in &pieces {
            let end = offset + piece.text.len();
            if removed
                .iter()
                .any(|removed| offset < removed.end && removed.start < end)
            {
                affected.insert(piece.site);
            }
            offset = end;
        }
    } else {
        // Unknown neighbors can complete only a marker prefix or suffix of a known piece.
        for piece in &pieces {
            if may_be_stripped(&piece.text) {
                affected.insert(piece.site);
            }
        }
        for (index, site) in state.sites.iter().enumerate() {
            if site.program == state.program && site.rendered && site.value.exact_text().is_none() {
                affected.insert(Site::Value(index));
            }
        }
    }
    for (index, site) in state.text_sites.iter_mut().enumerate() {
        if site.program == state.program && affected.contains(&Site::Text(index)) {
            site.rendered_unchanged = false;
        }
    }
    for (index, site) in state.sites.iter_mut().enumerate() {
        if site.program == state.program && site.rendered && affected.contains(&Site::Value(index))
        {
            site.rendered_unchanged = false;
        }
    }
}

fn may_be_stripped(text: &str) -> bool {
    const REMOVED: &str = "<no value>";
    text.contains(REMOVED)
        || (!text.is_empty() && REMOVED.contains(text))
        || (1..REMOVED.len()).any(|length| {
            text.ends_with(&REMOVED[..length])
                || text.starts_with(&REMOVED[REMOVED.len() - length..])
        })
}

pub(super) fn begin_file(
    state: &mut State,
    root: &Value,
    source_path: Option<&str>,
    occurrence: &FileOccurrence,
) {
    state.frames.clear();
    state.enter_file_call(root.clone(), occurrence);
    state.output = TextBuffer::default();
    state.output_widened = false;
    state.observation_count = super::output::BranchCounts::default();
    state.invocation_count = super::output::BranchCounts::default();
    state.sites.clear();
    state.text_sites.clear();
    state.reads.clear();
    state.invocations.clear();
    state.invocation_records_complete = true;
    state.calls.clear();
    state.loops.clear();
    let Some(source_path) = source_path else {
        return;
    };
    let base_path = source_path
        .rsplit_once("/templates/")
        .map_or("templates".to_owned(), |(chart, _)| {
            format!("{chart}/templates")
        });
    let template_id = state.heap.allocate_at(
        Some(&occurrence.template),
        ObjectData::Map {
            fields: std::collections::BTreeMap::from([
                (
                    "Name".to_owned(),
                    Field::Present(Value::string(source_path)),
                ),
                (
                    "BasePath".to_owned(),
                    Field::Present(Value::string(&base_path)),
                ),
            ]),
            remainder: Remainder::Closed,
        },
        super::Multiplicity::Singleton,
    );
    if let Some(id) = state.map_target(root) {
        let template = Value::leaf(Datum::Map {
            view: MapType::Values,
            target: Some(template_id),
        });
        state.heap.write(&id, "Template", Field::Present(template));
    }
}

fn initial_state(db: &crate::analysis_db::IrAnalysisDb) -> State {
    let mut state = State::with_heap(
        Value::nil(),
        super::Heap::with_predicate_memo(std::rc::Rc::clone(db.predicate_memo())),
    );
    let root = state.heap.map(Remainder::Open);
    if let Some(id) = state.map_target(&root) {
        let path = ValuesPath::default();
        let input = Value::input(path.clone());
        if let Some(target) = state.map_target(&input) {
            let values = Value {
                node: std::rc::Rc::new(ValueNode::Leaf(Leaf {
                    datum: Datum::Map {
                        view: MapType::Values,
                        target: Some(target),
                    },
                    origin: Origin {
                        identity: Some(path),
                        influences: std::collections::BTreeSet::default(),
                    },
                })),
            };
            state.heap.write(&id, "Values", Field::Present(values));
        }
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
    state
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
