use std::collections::BTreeMap;
use std::rc::Rc;

use crate::analysis_db::{
    DefinitionError, DefinitionInventory, ParsedDefinition, ParsedProgram, ParsedTemplateSource,
    helm_template_order,
};

use super::{AbortReason, Arena, Exit, Flow, Normal, State, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct NamespaceId {
    ordinal: u64,
}

pub(super) struct Namespace {
    id: NamespaceId,
    definitions: BTreeMap<String, Vec<Definition>>,
    inventory: DefinitionInventory,
    unknown_sources: helm_schema_core::Predicate,
}

struct Definition {
    program: Rc<ParsedProgram>,
    enabled: helm_schema_core::Predicate,
    empty: bool,
}

impl Clone for Definition {
    fn clone(&self) -> Self {
        Self {
            program: Rc::clone(&self.program),
            enabled: self.enabled.clone(),
            empty: self.empty,
        }
    }
}

impl std::fmt::Debug for Namespace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Namespace")
            .field("id", &self.id)
            .field("definition_names", &self.definitions.len())
            .field("inventory", &self.inventory)
            .field("unknown_sources", &self.unknown_sources)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(super) struct NamespaceFailure {
    pub(super) source_path: String,
    pub(super) reason: DefinitionError,
    enabled: helm_schema_core::Predicate,
}

#[derive(Clone, Debug)]
pub(super) struct NamespaceBuild {
    pub(super) namespace: Rc<Namespace>,
    pub(super) failures: Vec<NamespaceFailure>,
}

pub(super) enum TemplateResolution {
    Found(Rc<ParsedProgram>),
    Missing,
    Unknown,
}

pub(super) struct TemplateLookup {
    pub(super) state: State,
    pub(super) resolution: TemplateResolution,
}

impl Arena {
    fn namespace_id(&self) -> NamespaceId {
        let ordinal = self.next_namespace.get();
        self.next_namespace.set(ordinal + 1);
        NamespaceId { ordinal }
    }
}

impl Namespace {
    pub(super) fn same_source_names(&self, source: &str) -> Vec<&str> {
        if self.unknown_sources != helm_schema_core::Predicate::False {
            return Vec::new();
        }
        self.definitions
            .keys()
            .filter_map(|name| {
                self.program_source(name)
                    .filter(|body| *body == source)
                    .map(|_| name.as_str())
            })
            .collect()
    }

    pub(super) fn program_source(&self, name: &str) -> Option<&str> {
        if self.unknown_sources != helm_schema_core::Predicate::False {
            return None;
        }
        let definitions = self.definitions.get(name)?;
        let definition = definitions
            .iter()
            .rev()
            .find(|definition| !definition.empty)
            .or_else(|| definitions.first())?;
        (definition.enabled == helm_schema_core::Predicate::True)
            .then(|| definition.program.source())
    }

    pub(super) fn id(&self) -> NamespaceId {
        self.id
    }

    pub(super) fn unknown(arena: &Arena) -> Rc<Self> {
        Self::from_sources(Vec::new(), DefinitionInventory::Unknown, arena).namespace
    }

    /// An incomplete roster preserves supplied bindings but cannot prove other names absent.
    pub(super) fn from_sources(
        sources: Vec<Rc<ParsedTemplateSource>>,
        inventory: DefinitionInventory,
        arena: &Arena,
    ) -> NamespaceBuild {
        Self::from_chart_sources(sources, inventory, &[], arena)
    }

    pub(super) fn from_chart_sources(
        sources: Vec<Rc<ParsedTemplateSource>>,
        inventory: DefinitionInventory,
        charts: &[crate::SymbolicChart],
        arena: &Arena,
    ) -> NamespaceBuild {
        // A fresh source at the same logical path replaces the old file's entire declaration roster.
        let mut by_name = BTreeMap::new();
        for source in sources {
            by_name.insert(source.name.clone(), source);
        }
        let mut sources: Vec<_> = by_name.into_values().collect();
        sources.sort_by(|left, right| helm_template_order(&left.name, &right.name));
        let mut namespace = Self {
            id: arena.namespace_id(),
            definitions: BTreeMap::new(),
            inventory,
            unknown_sources: helm_schema_core::Predicate::False,
        };
        let mut failures = Vec::new();
        for source in sources {
            let enabled = charts
                .iter()
                .filter(|chart| source.name.starts_with(&format!("{}/", chart.namespace)))
                .max_by_key(|chart| chart.namespace.len())
                .map_or(helm_schema_core::Predicate::True, |chart| {
                    chart.enabled.clone()
                });
            namespace.add_source(&source, &enabled, &mut failures);
        }
        NamespaceBuild {
            namespace: Rc::new(namespace),
            failures,
        }
    }

    pub(super) fn overlay(&self, source: &ParsedTemplateSource, arena: &Arena) -> NamespaceBuild {
        let mut namespace = Self {
            id: arena.namespace_id(),
            definitions: self.definitions.clone(),
            inventory: self.inventory,
            unknown_sources: self.unknown_sources.clone(),
        };
        let mut failures = Vec::new();
        namespace.add_source(source, &helm_schema_core::Predicate::True, &mut failures);
        NamespaceBuild {
            namespace: Rc::new(namespace),
            failures,
        }
    }

    fn add_source(
        &mut self,
        source: &ParsedTemplateSource,
        enabled: &helm_schema_core::Predicate,
        failures: &mut Vec<NamespaceFailure>,
    ) {
        let ParsedTemplateSource {
            name,
            root,
            root_syntactically_empty,
            declarations,
            inventory,
            error,
        } = source;
        if *inventory == DefinitionInventory::Unknown {
            tracing::debug!(target: "helm_schema_ir::counters", source = name, "unknown namespace source inventory");
            self.unknown_sources = helm_schema_core::Predicate::Or(vec![
                self.unknown_sources.clone(),
                enabled.clone(),
            ])
            .normalize_boolean();
        }
        if let Some(reason) = error {
            failures.push(NamespaceFailure {
                source_path: root.source_path().unwrap_or(name).to_string(),
                reason: reason.clone(),
                enabled: enabled.clone(),
            });
        }
        let root = ParsedDefinition {
            name: name.clone(),
            program: Rc::clone(root),
            syntactically_empty: *root_syntactically_empty,
        };
        for ParsedDefinition {
            name,
            program,
            syntactically_empty,
        } in declarations.iter().cloned().chain(std::iter::once(root))
        {
            self.definitions.entry(name).or_default().push(Definition {
                program,
                enabled: enabled.clone(),
                empty: syntactically_empty,
            });
        }
    }

    pub(super) fn resolve(&self, name: &str, state: State) -> Vec<TemplateLookup> {
        let mut lookups = Vec::new();
        let mut pending = vec![state];
        if self.unknown_sources != helm_schema_core::Predicate::False {
            let mut known = Vec::new();
            for state in pending {
                let test = state.heap.arena.decision(
                    self.unknown_sources.clone(),
                    self.unknown_sources.negated(),
                    super::Coverage::Exact,
                );
                let unknown = state.fork(Rc::clone(&test), true);
                if !unknown.path_is_infeasible() {
                    lookups.push(TemplateLookup {
                        state: unknown,
                        resolution: TemplateResolution::Unknown,
                    });
                }
                let state = state.fork(test, false);
                if !state.path_is_infeasible() {
                    known.push(state);
                }
            }
            pending = known;
        }
        if let Some(definitions) = self.definitions.get(name) {
            for definition in definitions
                .iter()
                .rev()
                .filter(|definition| !definition.empty)
                .chain(definitions.iter().filter(|definition| definition.empty))
            {
                if definition.enabled == helm_schema_core::Predicate::False {
                    continue;
                }
                let mut absent = Vec::new();
                for state in pending {
                    if definition.enabled == helm_schema_core::Predicate::True {
                        lookups.push(TemplateLookup {
                            state,
                            resolution: TemplateResolution::Found(Rc::clone(&definition.program)),
                        });
                        continue;
                    }
                    let test = state.heap.arena.decision(
                        definition.enabled.clone(),
                        definition.enabled.negated(),
                        super::Coverage::Exact,
                    );
                    match state
                        .heap
                        .arena
                        .path_guard(&state.active)
                        .implies_test(&test)
                    {
                        Some(true) => {
                            lookups.push(TemplateLookup {
                                state,
                                resolution: TemplateResolution::Found(Rc::clone(
                                    &definition.program,
                                )),
                            });
                            continue;
                        }
                        Some(false) => {
                            absent.push(state);
                            continue;
                        }
                        None => {}
                    }
                    let present = state.fork(Rc::clone(&test), true);
                    if !present.path_is_infeasible() {
                        lookups.push(TemplateLookup {
                            state: present,
                            resolution: TemplateResolution::Found(Rc::clone(&definition.program)),
                        });
                    }
                    let state = state.fork(test, false);
                    if !state.path_is_infeasible() {
                        absent.push(state);
                    }
                }
                pending = absent;
                if pending.is_empty() {
                    break;
                }
            }
        }
        lookups.extend(pending.into_iter().map(|state| TemplateLookup {
            state,
            resolution: match self.inventory {
                DefinitionInventory::Complete => TemplateResolution::Missing,
                DefinitionInventory::Unknown => TemplateResolution::Unknown,
            },
        }));
        lookups
    }
}

impl NamespaceBuild {
    /// Installs one immutable namespace and rejects source-local parse errors before rendering.
    pub(super) fn install(self, mut state: State) -> Vec<Exit> {
        let NamespaceBuild {
            namespace,
            failures,
        } = self;
        state.namespace = namespace;
        let mut exits = Vec::new();
        let mut pending = Some(state);
        for failure in failures {
            let Some(state) = pending.take() else {
                break;
            };
            let test = state.heap.arena.decision(
                failure.enabled.clone(),
                failure.enabled.negated(),
                super::Coverage::Exact,
            );
            let abort = state.fork(Rc::clone(&test), true);
            if !abort.path_is_infeasible() {
                tracing::debug!(source = failure.source_path, error = %failure.reason, "template namespace parse abort");
                exits.push(Exit {
                    state: abort,
                    flow: Flow::Abort(AbortReason::TemplateParse),
                });
            }
            let state = state.fork(test, false);
            if !state.path_is_infeasible() {
                pending = Some(state);
            }
        }
        if let Some(state) = pending {
            exits.push(Exit {
                state,
                flow: Flow::Normal(Normal {
                    value: Value::nil(),
                }),
            });
        }
        exits
    }
}

#[cfg(test)]
#[path = "../tests/runtime_namespace.rs"]
mod tests;
