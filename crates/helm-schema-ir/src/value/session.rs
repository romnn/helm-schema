use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::eval::{Executor, normal};
use super::namespace::{Namespace, TemplateLookup, TemplateResolution};
use super::{
    AbortReason, Datum, DocumentExecution, Exit, Field, Flow, Leaf, MapType, ObjectData, Origin,
    Remainder, State, TextBuffer, Value, ValueNode,
};
use crate::analysis_db::{
    DefinitionInventory, IrAnalysisDb, ParsedProgram, ParsedProgramId, ParsedTemplateSource,
    helm_template_order,
};
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
    fn for_body(&self, state: &State) -> Self {
        Self {
            program: state.heap.arena.program(),
            call: self.call,
            template: self.template.clone(),
        }
    }
}

pub(super) struct FileExecution {
    pub(super) exits: Vec<Exit>,
    pub(super) output: super::FileOutput,
    programs: BTreeMap<ParsedProgramId, ExecutedProgram>,
}
struct ExecutedProgram {
    program: Rc<ParsedProgram>,
    occurrence: FileOccurrence,
}

#[cfg(test)]
#[tracing::instrument(skip_all)]
pub(crate) fn execute_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> DocumentExecution {
    let mut document = DocumentExecution::new(Vec::new(), super::FileOutput::Manifest);
    execute_files(
        vec![db.parsed_source(source, source_path, source_path.unwrap_or("gotpl"))],
        db,
        |_, execution| document = DocumentExecution::new(execution.exits, execution.output),
    );
    document
}

pub(crate) fn project_documents(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    mut seal: impl FnMut(usize, Option<&ParsedProgram>, &DocumentExecution),
) {
    execute_files(sources, db, |index, execution| {
        let FileExecution {
            exits,
            output,
            programs,
        } = execution;
        let mut document = DocumentExecution::new(exits, output);
        if programs.is_empty() {
            seal(index, None, &document);
            return;
        }
        for program in programs.into_values() {
            document.root_programs = BTreeSet::from([program.occurrence.program]);
            seal(index, Some(&program.program), &document);
        }
    });
}

#[cfg(test)]
pub(super) fn prepared_sources(
    sources: &[crate::symbolic::SymbolicTemplate<'_>],
    db: &IrAnalysisDb,
) -> Vec<Rc<ParsedTemplateSource>> {
    sources
        .iter()
        .map(|source| db.parsed_source(source.source, Some(source.source_path), source.source_path))
        .collect()
}

pub(super) fn execute_files(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    mut seal: impl FnMut(usize, FileExecution),
) {
    if sources.is_empty() {
        return;
    }
    let state = initial_state(db);
    let root = state.read_root().cloned().unwrap_or_else(Value::unknown);
    let mut seen = BTreeSet::new();
    let roster = db
        .template_sources()
        .iter()
        .chain(&sources)
        .filter(|source| seen.insert(source.root.id()))
        .cloned()
        .collect();
    let build = Namespace::from_sources(roster, DefinitionInventory::Unknown, &state.heap.arena);
    let mut states = Vec::new();
    let mut startup = Vec::new();
    for exit in build.install(state) {
        match exit.flow {
            Flow::Normal(_) => states.push(exit.state),
            flow => startup.push(Exit {
                state: exit.state,
                flow,
            }),
        }
    }
    let mut files: Vec<_> = sources.into_iter().enumerate().collect();
    files.sort_by(|(_, left), (_, right)| helm_template_order(&left.name, &right.name));
    for (index, source) in files {
        let mut execution = execute_file(&mut states, &source, &root, db);
        execution.exits.splice(0..0, std::mem::take(&mut startup));
        seal(index, execution);
    }
}

#[tracing::instrument(skip_all, level = "debug", fields(path = %source.name, incoming_states = states.len()))]
fn execute_file(
    states: &mut Vec<State>,
    source: &ParsedTemplateSource,
    root: &Value,
    db: &IrAnalysisDb,
) -> FileExecution {
    let mut programs = BTreeMap::new();
    let output = if source.name.ends_with("NOTES.txt") {
        super::FileOutput::Text
    } else {
        super::FileOutput::Manifest
    };
    let Some(first) = states.first() else {
        return FileExecution {
            exits: Vec::new(),
            output,
            programs,
        };
    };
    let occurrence = FileOccurrence::new(first);
    let mut exits = super::ExitBuffer::default();
    for mut state in std::mem::take(states) {
        begin_file(&mut state, root, source.root.source_path(), &occurrence);
        let namespace = Rc::clone(&state.namespace);
        for lookup in namespace.resolve(&source.name, state) {
            exits.extend(execute_lookup(lookup, &occurrence, &mut programs, db));
        }
    }
    let exits = exits.finish();
    *states = exits
        .iter()
        .filter_map(|exit| match exit.flow {
            Flow::Normal(_) => {
                let mut state = exit.state.clone();
                clear_file(&mut state);
                Some(state)
            }
            Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
        })
        .collect();
    FileExecution {
        exits,
        output,
        programs,
    }
}

fn execute_lookup(
    TemplateLookup {
        mut state,
        resolution,
    }: TemplateLookup,
    occurrence: &FileOccurrence,
    programs: &mut BTreeMap<ParsedProgramId, ExecutedProgram>,
    db: &IrAnalysisDb,
) -> Vec<Exit> {
    match resolution {
        TemplateResolution::Found(program) => {
            let body = programs
                .entry(program.id())
                .or_insert_with(|| ExecutedProgram {
                    program: Rc::clone(&program),
                    occurrence: occurrence.for_body(&state),
                });
            Executor { db }.parsed_file_body(
                program.source(),
                program.source_path(),
                program.syntax(),
                state,
                &body.occurrence,
            )
        }
        TemplateResolution::Missing => vec![Exit {
            state,
            flow: Flow::Abort(AbortReason::MissingTemplate),
        }],
        TemplateResolution::Unknown => {
            state.mark_invocation_incomplete();
            super::operations::invalidate(&mut state);
            state.output.emit(state.heap.freeze(&Value::unknown()));
            normal(state, Value::unknown())
        }
    }
}

pub(super) fn finish_file(mut exits: Vec<super::Exit>) -> Vec<super::Exit> {
    for exit in &mut exits {
        if !matches!(exit.flow, Flow::Normal(_)) {
            continue;
        }
        let state = &mut exit.state;
        let memo = Rc::clone(&state.heap.arena.predicate_memo);
        super::output_layout::certify_plain_scalar_placements(state, &memo);
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
    clear_file(state);
    state.enter_file_call(root.clone(), occurrence);
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

pub(super) fn clear_file(state: &mut State) {
    state.frames.clear();
    state.program = 0;
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
}

fn initial_state(db: &IrAnalysisDb) -> State {
    let mut state = State::with_heap(
        Value::nil(),
        super::Heap::with_predicate_memo(Rc::clone(db.predicate_memo())),
    );
    let root = state.heap.map(Remainder::Closed);
    let Some(id) = state.map_target(&root) else {
        return state;
    };
    let path = ValuesPath::default();
    let input = Value::input(path.clone());
    if let Some(target) = state.map_target(&input) {
        let values = Value {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Map {
                    view: MapType::Values,
                    target: Some(target),
                },
                origin: Origin {
                    identity: Some(path),
                    influences: BTreeSet::default(),
                },
            })),
        };
        state.heap.write(&id, "Values", Field::Present(values));
    }
    for key in ["Chart", "Release", "Capabilities", "Subcharts", "Template"] {
        let value = Value::unknown();
        state.heap.write(&id, key, Field::Present(value));
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
            aliases: super::AliasSet::Known(BTreeSet::from([files])),
        }))),
    );
    for (path, text) in db.static_root_strings() {
        let mut parent = root.clone();
        let Some((last, parents)) = path.split_last() else {
            continue;
        };
        for key in parents {
            let next = state.get_field(&parent, key);
            if state.map_target(&next).is_some() {
                parent = next;
            } else {
                let next = state.heap.map(Remainder::Open);
                if let Some(target) = state.map_target(&parent) {
                    state.heap.write(&target, key, Field::Present(next.clone()));
                }
                parent = next;
            }
        }
        if let Some(target) = state.map_target(&parent) {
            state
                .heap
                .write(&target, last, Field::Present(Value::string(text)));
        }
    }
    if let Some(frame) = state.frames.first_mut() {
        frame.bindings.insert("$".to_owned(), root.clone());
        frame.dot = Some(root);
    }
    state
}
