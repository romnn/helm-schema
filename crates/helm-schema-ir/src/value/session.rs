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

pub(crate) fn project_documents(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    mut seal: impl FnMut(usize, Option<&ParsedProgram>, &DocumentExecution),
) {
    project_chart_documents(sources, db, &[], |index, program, execution| {
        if let Some(index) = index {
            seal(index, program, execution);
        }
    });
}

/// Executes every file of a render graph in one heap; `None` seals the setup of a graph that
/// has no executable files.
pub(crate) fn project_chart_documents(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    charts: &[crate::SymbolicChart],
    mut seal: impl FnMut(Option<usize>, Option<&ParsedProgram>, &DocumentExecution),
) {
    execute_chart_files(sources, db, charts, |index, execution| {
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

pub(super) fn execute_files(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    mut seal: impl FnMut(usize, FileExecution),
) {
    execute_chart_files(sources, db, &[], |index, execution| {
        if let Some(index) = index {
            seal(index, execution);
        }
    });
}

/// Helm's render (`engine.go:280-312`): one template set and one schedule for every chart
/// instance, descending slash count then reverse lexical order, each file executing against its
/// instance's root under that instance's activation.
fn execute_chart_files(
    sources: Vec<Rc<ParsedTemplateSource>>,
    db: &IrAnalysisDb,
    charts: &[crate::SymbolicChart],
    mut seal: impl FnMut(Option<usize>, FileExecution),
) {
    if sources.is_empty() && charts.is_empty() {
        return;
    }
    let mut state = initial_state(db, !charts.is_empty());
    if let Some(source) = sources.first() {
        state
            .heap
            .arena
            .requirement_scope
            .replace(source.name.clone());
    }
    let file_catalogs = super::hosts::file_catalogs(db, &mut state);
    let root = state.read_root().cloned().unwrap_or_else(Value::unknown);
    let roots = render_roots(&mut state, &root, charts, &file_catalogs);
    let mut seen = BTreeSet::new();
    let roster = db
        .template_sources()
        .iter()
        .chain(&sources)
        .filter(|source| seen.insert(source.root.id()))
        .cloned()
        .collect();
    // A render graph supplies every chart's templates, so an unknown name is Helm's abort.
    let inventory = if charts.is_empty() {
        DefinitionInventory::Unknown
    } else {
        DefinitionInventory::Complete
    };
    let build = Namespace::from_chart_sources(roster, inventory, charts, &state.heap.arena);
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
    if sources.is_empty() {
        startup.extend(states.into_iter().map(|state| Exit {
            state,
            flow: Flow::Normal(super::Normal {
                value: Value::nil(),
            }),
        }));
        seal(
            None,
            FileExecution {
                exits: startup,
                output: super::FileOutput::Manifest,
                programs: BTreeMap::new(),
            },
        );
        return;
    }
    let mut files: Vec<_> = sources.into_iter().enumerate().collect();
    files.sort_by(|(_, left), (_, right)| helm_template_order(&left.name, &right.name));
    let mut assumptions = InstanceAssumptions::new(files.iter().map(|(_, source)| {
        owning_chart(charts, &source.name).map(|chart| chart.namespace.as_str())
    }));
    let mut executing = None;
    let count = files.len();
    for (position, (index, source)) in files.iter().enumerate() {
        let chart = owning_chart(charts, &source.name);
        let context = chart
            .and_then(|chart| roots.get(&chart.namespace))
            .unwrap_or(&root);
        // Each chart instance's run of files allocates within its own bound, as when every
        // chart was a render of its own.
        let instance = chart.map(|chart| chart.namespace.as_str());
        if executing != Some(instance) {
            executing = Some(instance);
            leave_instance(&mut states);
            if let Some(state) = states.first() {
                begin_instance(state);
            }
        }
        let before = assumptions_made(&states);
        let enabled = chart.map_or(&helm_schema_core::Predicate::True, |chart| &chart.enabled);
        let mut execution = if *enabled == helm_schema_core::Predicate::True {
            execute_file(&mut states, source, context, db, &file_catalogs)
        } else {
            execute_activated_file(&mut states, enabled, source, context, db, &file_catalogs)
        };
        execution.exits.splice(0..0, std::mem::take(&mut startup));
        seal(Some(*index), execution);
        if assumptions_made(&states) != before {
            assumptions.assumed(instance, context);
        }
        if position + 1 < count {
            states = assumptions.after_file(position, instance, states);
        }
    }
}

/// How many analysis assumptions the render the continuations share has made.
fn assumptions_made(states: &[State]) -> Option<u64> {
    states
        .first()
        .map(|state| state.heap.arena.assumptions_made.get())
}

/// The chart instances whose files made the analysis assumptions the paths hold. Helm's schedule
/// can return to an instance after another one ran, so an instance's assumptions are discharged
/// only once every assuming instance has run its last file.
struct InstanceAssumptions<'a> {
    last_files: BTreeMap<Option<&'a str>, usize>,
    assuming: BTreeMap<Option<&'a str>, Value>,
    ended: BTreeSet<Option<&'a str>>,
}

impl<'a> InstanceAssumptions<'a> {
    fn new(instances: impl Iterator<Item = Option<&'a str>>) -> Self {
        Self {
            last_files: instances
                .enumerate()
                .map(|(position, instance)| (instance, position))
                .collect(),
            assuming: BTreeMap::new(),
            ended: BTreeSet::new(),
        }
    }

    fn assumed(&mut self, instance: Option<&'a str>, context: &Value) {
        self.assuming.insert(instance, context.clone());
    }

    /// The continuations after the file at `position`: once no later file of an assuming
    /// instance remains, their assumptions are discharged.
    fn after_file(
        &mut self,
        position: usize,
        instance: Option<&'a str>,
        states: Vec<State>,
    ) -> Vec<State> {
        if self.last_files.get(&instance) != Some(&position) {
            return states;
        }
        self.ended.insert(instance);
        if self.assuming.is_empty() || !self.assuming.keys().all(|owner| self.ended.contains(owner))
        {
            return states;
        }
        let contexts = std::mem::take(&mut self.assuming)
            .into_values()
            .collect::<Vec<_>>();
        states
            .into_iter()
            .flat_map(|state| super::operations::discharge_assumptions(&contexts, state))
            .collect()
    }
}

/// Executes a file of an instance that `enabled` activates: a disabled instance's files do not
/// exist in Helm's render, so that continuation skips the file.
fn execute_activated_file(
    states: &mut Vec<State>,
    enabled: &helm_schema_core::Predicate,
    source: &Rc<ParsedTemplateSource>,
    context: &Value,
    db: &IrAnalysisDb,
    file_catalogs: &BTreeMap<String, Value>,
) -> FileExecution {
    let mut skipped = Vec::new();
    let mut active = Vec::new();
    for state in std::mem::take(states) {
        let test =
            state
                .heap
                .arena
                .decision(enabled.clone(), enabled.negated(), super::Coverage::Exact);
        let mut disabled = state.fork(Rc::clone(&test), false);
        if !disabled.path_is_infeasible() {
            // The skipped continuation leaves the file as an executed one does.
            clear_file(&mut disabled);
            skipped.push(disabled);
        }
        let enabled = state.fork(test, true);
        if !enabled.path_is_infeasible() {
            active.push(enabled);
        }
    }
    let execution = execute_file(&mut active, source, context, db, file_catalogs);
    active.extend(skipped);
    *states = join_file_states(active);
    execution
}

/// Leaves the executing instance's run of files. Its survivals are dropped: a later fact claimed
/// on a document that failed one of its checks rejects a document Helm rejects anyway, and each
/// run's checks then refine only its own files, as when every chart was a render of its own.
fn leave_instance(states: &mut [State]) {
    for state in states {
        if let Some(active) = state.active.without(super::path::LinkKind::Survival) {
            state.active = active;
        }
    }
}

/// Starts a chart instance's run of files: its allocation bound counts only its own
/// allocations, and its requirements fold apart from every other instance's.
fn begin_instance(state: &State) {
    let arena = &state.heap.arena;
    arena
        .instance_object_base
        .set(state.heap.version.object_count());
    arena
        .requirement_identity
        .replace(super::consumption::RequirementScope::default());
}

/// The deepest chart instance whose namespace contains the template name.
fn owning_chart<'a>(
    charts: &'a [crate::SymbolicChart],
    name: &str,
) -> Option<&'a crate::SymbolicChart> {
    charts
        .iter()
        .filter(|chart| {
            name.strip_prefix(&chart.namespace)
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|chart| chart.namespace.len())
}

/// Reconverges the enabled and skipped continuations of one file.
fn join_file_states(active: Vec<State>) -> Vec<State> {
    let joined = super::control::reconverge(
        active
            .into_iter()
            .map(|state| Exit {
                state,
                flow: Flow::Normal(super::Normal {
                    value: Value::nil(),
                }),
            })
            .collect(),
    );
    let mut states: Vec<_> = super::control::bound_exits(joined)
        .into_iter()
        .map(|exit| exit.state)
        .collect();
    let mut paths = states
        .iter_mut()
        .map(|state| &mut state.active)
        .collect::<Vec<_>>();
    super::path::settle_survival(&super::path::Path::default(), &mut paths);
    states
}

/// One root map per chart instance (`engine.go:544-590`): the shared Release and Capabilities,
/// the instance's own Chart metadata and Files, and `.Values` bound to the instance's values
/// subtree, which is the parent's `.Values.<key>` object itself.
fn render_roots(
    state: &mut State,
    root: &Value,
    charts: &[crate::SymbolicChart],
    file_catalogs: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut roots = BTreeMap::new();
    let release = state.field(root, "Release");
    let capabilities = state.field(root, "Capabilities");
    for chart in charts {
        let Some(values) = values_root(state, &chart.values_path) else {
            continue;
        };
        let context = state.heap.map(Remainder::Closed);
        let files = file_catalogs
            .get(&chart.namespace)
            .cloned()
            .unwrap_or_else(|| {
                let files = state.heap.map(Remainder::Closed);
                super::hosts::as_files(files, &state.heap.arena)
            });
        let Some(target) = state.map_target(&context) else {
            continue;
        };

        for (key, value) in [
            ("Values", values),
            ("Release", release.clone()),
            ("Capabilities", capabilities.clone()),
            ("Files", files),
            ("Template", Value::unknown()),
        ] {
            state.heap.write(&target, key, Field::Present(value));
        }
        install_strings(state, &context, &chart.static_root_strings);
        let metadata = state.field(&context, "Chart");
        if let Some(target) = state.map_target(&metadata) {
            state.heap.write(
                &target,
                "IsRoot",
                Field::Present(Value::literal(helm_schema_core::GuardValue::Bool(
                    chart.values_path.segments().next().is_none(),
                ))),
            );
        }
        let annotations = state.field(&metadata, "Annotations");
        let files = state.field(&context, "Files");
        for value in [&context, &metadata, &annotations, &files] {
            own_object(state, value, &chart.values_path);
        }
        roots.insert(chart.namespace.clone(), context);
    }
    install_subcharts(state, charts, &roots);
    roots
}

/// The `.Values` map of the instance rooted at `path`, carrying that input identity.
fn values_root(state: &mut State, path: &ValuesPath) -> Option<Value> {
    // Helm coalesces a table for every loaded dependency instance; a non-table there aborts
    // before rendering (`DependencyValuesRoot`).
    let input =
        super::kinds::refined_parameter(&Value::input(path.clone()), super::kinds::GoType::Map);
    let target = state.map_target(&input)?;
    Some(Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: MapType::Values,
                target: Some(target),
            },
            origin: Origin {
                derivation: None,
                identity: Some(path.clone()),
                influences: BTreeSet::default(),
            },
        })),
    })
}

/// Records that a render-context map belongs to the instance bound to `values_path`.
fn own_object(state: &mut State, value: &Value, values_path: &ValuesPath) {
    if let Some(target) = state.map_target(value) {
        state
            .heap
            .arena
            .instance_objects
            .borrow_mut()
            .insert(target, values_path.clone());
    }
}

/// `.Subcharts` holds each enabled direct dependency's root (`engine.go:575`).
fn install_subcharts(
    state: &mut State,
    charts: &[crate::SymbolicChart],
    roots: &BTreeMap<String, Value>,
) {
    for chart in charts {
        let Some(context) = roots.get(&chart.namespace) else {
            continue;
        };
        let children = state.heap.map(Remainder::Closed);
        if let Some(target) = state.map_target(&children) {
            for child in charts
                .iter()
                .filter(|child| child.values_path.parent().as_ref() == Some(&chart.values_path))
            {
                let (Some(child_context), Some(key)) = (
                    roots.get(&child.namespace),
                    child
                        .values_path
                        .segments()
                        .last()
                        .and_then(helm_schema_core::Segment::literal),
                ) else {
                    continue;
                };
                let test = state.heap.arena.decision(
                    child.enabled.clone(),
                    child.enabled.negated(),
                    super::Coverage::Exact,
                );
                state.heap.write(
                    &target,
                    key,
                    Field::select(test, Field::Present(child_context.clone()), Field::Absent),
                );
            }
        }
        own_object(state, &children, &chart.values_path);
        if let Some(target) = state.map_target(context) {
            state
                .heap
                .write(&target, "Subcharts", Field::Present(children));
        }
    }
}

/// Writes exact scalar strings beneath `root`, creating open maps along each path.
fn install_strings(state: &mut State, root: &Value, strings: &BTreeMap<Vec<String>, String>) {
    for (path, text) in strings {
        let Some((last, parents)) = path.split_last() else {
            continue;
        };
        let mut parent = root.clone();
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
}

#[tracing::instrument(skip_all, level = "debug", fields(path = %source.name, incoming_states = states.len()))]
fn execute_file(
    states: &mut Vec<State>,
    source: &ParsedTemplateSource,
    root: &Value,
    db: &IrAnalysisDb,
    file_catalogs: &BTreeMap<String, Value>,
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
        if let Some(target) = state.map_target(root) {
            let files = file_catalogs
                .get(db.chart_instance(&source.name))
                .cloned()
                .unwrap_or_else(|| {
                    let empty = state.heap.map(Remainder::Closed);
                    super::hosts::as_files(empty, &state.heap.arena)
                });
            state.heap.write(&target, "Files", Field::Present(files));
        }
        begin_file(&mut state, root, source.root.source_path(), &occurrence);
        let namespace = Rc::clone(&state.namespace);
        for lookup in namespace.resolve(&source.name, state) {
            exits.extend(execute_lookup(lookup, &occurrence, &mut programs, db));
        }
    }
    let mut exits = exits.finish();
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
    // Later files need no condition that every surviving continuation took.
    let mut paths = states
        .iter_mut()
        .map(|state| &mut state.active)
        .collect::<Vec<_>>();
    super::path::settle_survival(&super::path::Path::default(), &mut paths);
    // Sealing needs completeness and recorded evidence, not execution counters.
    for exit in &mut exits {
        let state = &mut exit.state;
        state.invocation_records_complete = state.invocations_complete();
        state.observation_count = super::output::BranchCounts::default();
        state.invocation_count = super::output::BranchCounts::default();
        state.output.counter = None;
    }
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
        let stripped = state.output.stripped_value();
        state.output = TextBuffer::default();
        state.output.emit(state.heap.freeze(&stripped));
    }
    exits
}

pub(super) fn begin_file(
    state: &mut State,
    root: &Value,
    source_path: Option<&str>,
    occurrence: &FileOccurrence,
) {
    clear_file(state);
    state.enter_file_call(root, occurrence);
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
            fields: Rc::new(super::ObjectFields::from_iter([
                (
                    "Name".to_owned(),
                    Field::Present(Value::string(source_path)),
                ),
                (
                    "BasePath".to_owned(),
                    Field::Present(Value::string(&base_path)),
                ),
            ])),
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
    state.consumptions.clear();
    state.invocations.clear();
    state.invocation_records_complete = true;
    state.calls.clear();
    state.loops.clear();
}

/// The render's initial heap. In a render graph every write to the one shared Release map is
/// executed, so Release starts with Helm's typed initial fields; a lone template set cannot see
/// the writes of the other chart instances of its render, so its Release stays unknown.
fn initial_state(db: &IrAnalysisDb, render_graph: bool) -> State {
    let mut state = State::with_heap(
        Value::nil(),
        super::Heap::with_predicate_memo(Rc::clone(db.predicate_memo())),
    );
    let root = state.heap.map(Remainder::Closed);
    let Some(id) = state.map_target(&root) else {
        return state;
    };
    state
        .heap
        .arena
        .instance_objects
        .borrow_mut()
        .insert(id.clone(), ValuesPath::default());
    if let Some(values) = values_root(&mut state, &ValuesPath::default()) {
        state.heap.write(&id, "Values", Field::Present(values));
    }
    for key in ["Chart", "Subcharts", "Template"] {
        let value = Value::unknown();
        state.heap.write(&id, key, Field::Present(value));
    }
    let release = super::hosts::release(&mut state, render_graph);
    state.heap.write(&id, "Release", Field::Present(release));
    let capabilities = super::hosts::capabilities(&mut state);
    state
        .heap
        .write(&id, "Capabilities", Field::Present(capabilities));
    install_strings(&mut state, &root, db.static_root_strings());
    if let Some(frame) = state.frames.first_mut() {
        let frame = Rc::make_mut(frame);
        frame.bindings.insert("$".to_owned(), root.clone());
        frame.dot = Some(root);
    }
    state
}
