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
    definitions: BTreeMap<String, Rc<ParsedProgram>>,
    inventory: DefinitionInventory,
    unknown_sources: bool,
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
        if self.unknown_sources {
            return Vec::new();
        }
        self.definitions
            .iter()
            .filter_map(|(name, program)| (program.source() == source).then_some(name.as_str()))
            .collect()
    }

    pub(super) fn program_source(&self, name: &str) -> Option<&str> {
        (!self.unknown_sources)
            .then(|| self.definitions.get(name))
            .flatten()
            .map(|program| program.source())
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
            unknown_sources: false,
        };
        let mut failures = Vec::new();
        for source in sources {
            namespace.add_source(&source, &mut failures);
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
            unknown_sources: self.unknown_sources,
        };
        let mut failures = Vec::new();
        namespace.add_source(source, &mut failures);
        NamespaceBuild {
            namespace: Rc::new(namespace),
            failures,
        }
    }

    fn add_source(&mut self, source: &ParsedTemplateSource, failures: &mut Vec<NamespaceFailure>) {
        let ParsedTemplateSource {
            name,
            root,
            root_syntactically_empty,
            declarations,
            inventory,
            error,
        } = source;
        self.unknown_sources |= *inventory == DefinitionInventory::Unknown;
        if let Some(reason) = error {
            failures.push(NamespaceFailure {
                source_path: root.source_path().unwrap_or(name).to_string(),
                reason: reason.clone(),
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
            // Across separate parses even an earlier whitespace body survives an empty replacement.
            if !syntactically_empty || !self.definitions.contains_key(&name) {
                self.definitions.insert(name, program);
            }
        }
    }

    pub(super) fn resolve(&self, name: &str, state: State) -> Vec<TemplateLookup> {
        let resolution = if self.unknown_sources {
            TemplateResolution::Unknown
        } else if let Some(program) = self.definitions.get(name) {
            TemplateResolution::Found(Rc::clone(program))
        } else {
            match self.inventory {
                DefinitionInventory::Complete => TemplateResolution::Missing,
                DefinitionInventory::Unknown => TemplateResolution::Unknown,
            }
        };
        vec![TemplateLookup { state, resolution }]
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
        let flow = if let Some(failure) = failures.first() {
            tracing::debug!(source = failure.source_path, error = %failure.reason, "template namespace parse abort");
            Flow::Abort(AbortReason::TemplateParse)
        } else {
            Flow::Normal(Normal {
                value: Value::nil(),
            })
        };
        vec![Exit { state, flow }]
    }
}

#[cfg(test)]
#[path = "../tests/runtime_namespace.rs"]
mod tests;
