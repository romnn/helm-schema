//! Canonical sharing of existing private definitions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasher as _, RandomState};

use helm_schema_json_schema_walk::{
    DefinitionsKeyword, ReferenceSiblings, local_reference_fragment, parse_definition_reference,
    visit_subschemas, visit_subschemas_mut,
};
use serde_json::Value;

use crate::{DefinitionOrigin, has_reference_scope, protected_reference_targets, value_address};

const DOCUMENT: usize = 0;

/// Compacts private draft-07 decisions and shares exact bodies after canonicalizing references.
#[tracing::instrument(skip_all, fields(definitions = tracing::field::Empty, removed = tracing::field::Empty))]
pub(crate) fn compact(schema: &mut Value, anonymous: &BTreeMap<String, Vec<DefinitionOrigin>>) {
    if !anonymous.values().any(Vec::is_empty) {
        return;
    }
    let Some(mut graph) = DefinitionGraph::new(schema, anonymous) else {
        return;
    };
    // Draft-07 has no unevaluated-keyword dependence on condition annotations.
    let boolean_decisions = matches!(
        schema.get("$schema").and_then(Value::as_str),
        Some(
            "http://json-schema.org/draft-07/schema#" | "https://json-schema.org/draft-07/schema#"
        )
    );
    let order = graph.finish_order();
    graph.protect_cycles(&order);
    tracing::Span::current().record("definitions", graph.vertices.len() - 1);

    let hasher = RandomState::new();
    let mut interned = HashMap::<(DefinitionsKeyword, u64), Vec<usize>>::new();
    let mut representatives = (0..graph.vertices.len()).collect::<Vec<_>>();
    let mut removed = Vec::new();
    for id in order {
        let Some(definition) = graph.shareable(id) else {
            continue;
        };
        let Some(slot) = definition.body_mut(schema) else {
            continue;
        };
        let mut body = std::mem::take(slot);
        if boolean_decisions {
            compact_boolean_decision(&mut body);
        }
        graph.rewrite_references(&mut body, &representatives, false);
        let alias = pure_reference(&body)
            .and_then(|reference| graph.references.get(reference).copied())
            .filter(|target| graph.shareable(*target).is_some());
        let equivalent = alias.or_else(|| {
            let fingerprint = hasher.hash_one(&body);
            let bucket = interned
                .entry((definition.container, fingerprint))
                .or_default();
            let equivalent = bucket.iter().copied().find(|&candidate| {
                graph
                    .definition(candidate)
                    .and_then(|definition| definition.body(schema))
                    == Some(&body)
            });
            if equivalent.is_none() {
                bucket.push(id);
            }
            equivalent
        });
        if let Some(equivalent) = equivalent
            && let Some(representative) = representatives.get_mut(id)
        {
            *representative = equivalent;
            removed.push(id);
        } else if let Some(slot) = definition.body_mut(schema) {
            *slot = body;
        }
    }
    tracing::Span::current().record("removed", removed.len());
    for id in removed {
        if let Some(definition) = graph.definition(id) {
            definition.remove(schema);
        }
    }
    graph.rewrite_references(schema, &representatives, true);
}

fn compact_boolean_decision(body: &mut Value) {
    let Some(object) = body.as_object().filter(|object| object.len() == 3) else {
        return;
    };
    let (Some(test), Some(yes), Some(no)) =
        (object.get("if"), object.get("then"), object.get("else"))
    else {
        return;
    };
    if ![test, yes, no]
        .into_iter()
        .all(|schema| schema.is_object() || schema.is_boolean())
    {
        return;
    }
    let compact = match (yes.as_bool(), no.as_bool()) {
        (Some(true), Some(false)) => test.clone(),
        (Some(false), Some(true)) => serde_json::json!({"not": test}),
        (Some(false), None) => serde_json::json!({"allOf": [{"not": test}, no]}),
        (None, Some(false)) => serde_json::json!({"allOf": [test, yes]}),
        // An `anyOf` would expose annotations from a successful but unselected arm.
        (Some(true) | None, None) | (None, Some(true)) | (Some(_), Some(_)) => return,
    };
    *body = compact;
}

struct Definition {
    container: DefinitionsKeyword,
    name: String,
    reference: Option<String>,
}

impl Definition {
    fn body<'schema>(&self, schema: &'schema Value) -> Option<&'schema Value> {
        schema.get(self.container.as_str())?.get(&self.name)
    }

    fn body_mut<'schema>(&self, schema: &'schema mut Value) -> Option<&'schema mut Value> {
        schema.get_mut(self.container.as_str())?.get_mut(&self.name)
    }

    fn remove(&self, schema: &mut Value) {
        if let Some(definitions) = schema
            .get_mut(self.container.as_str())
            .and_then(Value::as_object_mut)
        {
            definitions.remove(&self.name);
        }
    }
}

#[derive(Default)]
struct Vertex {
    definition: Option<Definition>,
    protected: bool,
    edges: Vec<usize>,
    incoming: Vec<usize>,
}

struct DefinitionGraph {
    vertices: Vec<Vertex>,
    references: HashMap<String, usize>,
}

impl DefinitionGraph {
    fn new(schema: &Value, anonymous: &BTreeMap<String, Vec<DefinitionOrigin>>) -> Option<Self> {
        let protected = protected_reference_targets(schema)?;
        let mut graph = Self {
            vertices: vec![Vertex::default()],
            references: HashMap::new(),
        };
        let mut names = HashMap::<DefinitionsKeyword, HashMap<&str, usize>>::new();
        let mut locations = HashMap::new();
        for container in DefinitionsKeyword::ALL {
            let Some(definitions) = schema.get(container.as_str()).and_then(Value::as_object)
            else {
                continue;
            };
            let mut definitions = definitions.iter().collect::<Vec<_>>();
            definitions.sort_unstable_by_key(|(name, _)| *name);
            let namespace = names.entry(container).or_default();
            for (name, body) in definitions {
                let id = graph.vertices.len();
                let address = value_address(body);
                locations.insert(address, id);
                namespace.insert(name.as_str(), id);
                let ambiguous = DefinitionsKeyword::ALL.into_iter().any(|other| {
                    other != container
                        && schema
                            .get(other.as_str())
                            .and_then(|map| map.get(name))
                            .is_some()
                });
                graph.vertices.push(Vertex {
                    definition: Some(Definition {
                        container,
                        name: name.clone(),
                        reference: None,
                    }),
                    protected: ambiguous
                        || protected.contains(&address)
                        || !anonymous.get(name).is_some_and(Vec::is_empty)
                        || !(body.is_object() || body.is_boolean()),
                    ..Vertex::default()
                });
            }
        }
        graph.collect_references(schema, Source::document(), &names, &locations)?;
        if !graph.vertices.iter().any(|vertex| {
            !vertex.protected
                && vertex
                    .definition
                    .as_ref()
                    .is_some_and(|definition| definition.reference.is_some())
        }) {
            return None;
        }

        for vertex in &mut graph.vertices {
            vertex.edges.sort_unstable();
            vertex.edges.dedup();
        }
        Some(graph)
    }

    fn definition(&self, id: usize) -> Option<&Definition> {
        self.vertices.get(id)?.definition.as_ref()
    }

    fn shareable(&self, id: usize) -> Option<&Definition> {
        let vertex = self.vertices.get(id)?;
        if vertex.protected {
            return None;
        }
        vertex
            .definition
            .as_ref()
            .filter(|definition| definition.reference.is_some())
    }

    fn collect_references(
        &mut self,
        schema: &Value,
        source: Source,
        names: &HashMap<DefinitionsKeyword, HashMap<&str, usize>>,
        locations: &HashMap<usize, usize>,
    ) -> Option<()> {
        let Source {
            id,
            at_root,
            scoped,
        } = source;
        // The dialect and its $ref sibling rules determine which resource owns a fragment.
        if !at_root && (schema.get("$id").is_some() || schema.get("id").is_some()) {
            return None;
        }
        let scoped = scoped || !at_root && has_reference_scope(schema);
        if scoped {
            self.vertices.get_mut(id)?.protected = true;
        }
        if schema.get("$dynamicRef").is_some() || schema.get("$recursiveRef").is_some() {
            return None;
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let fragment = local_reference_fragment(reference)?;
            let target = if let Some(target) = definition_target(reference, names) {
                let Target { id, suffix } = target;
                let vertex = self.vertices.get_mut(id)?;
                if suffix || scoped || pure_reference(schema).is_none() {
                    vertex.protected = true;
                } else {
                    self.references.entry(reference.to_string()).or_insert(id);
                    let definition = vertex.definition.as_mut()?;
                    if definition
                        .reference
                        .as_deref()
                        .is_none_or(|old| reference < old)
                    {
                        definition.reference = Some(reference.to_string());
                    }
                }
                id
            } else if fragment.is_empty() || fragment.starts_with('/') {
                DOCUMENT
            } else {
                return None;
            };
            self.vertices.get_mut(id)?.edges.push(target);
            self.vertices.get_mut(target)?.incoming.push(id);
        }
        let mut complete = Some(());
        visit_subschemas(schema, ReferenceSiblings::Visit, &mut |child| {
            let id = if at_root {
                locations.get(&value_address(child)).copied().unwrap_or(id)
            } else {
                id
            };
            let source = Source {
                id,
                at_root: false,
                scoped,
            };
            complete =
                complete.and_then(|()| self.collect_references(child, source, names, locations));
        });
        complete
    }

    fn finish_order(&self) -> Vec<usize> {
        let mut seen = HashSet::with_capacity(self.vertices.len());
        let mut order = Vec::with_capacity(self.vertices.len());
        let mut pending = Vec::new();
        for (root, vertex) in self.vertices.iter().enumerate() {
            if !seen.insert(root) {
                continue;
            }
            pending.push((root, vertex.edges.iter()));
            while let Some((id, mut edges)) = pending.pop() {
                if let Some(&child) = edges.find(|&&child| seen.insert(child))
                    && let Some(vertex) = self.vertices.get(child)
                {
                    pending.push((id, edges));
                    pending.push((child, vertex.edges.iter()));
                } else {
                    order.push(id);
                }
            }
        }
        order
    }

    fn protect_cycles(&mut self, order: &[usize]) {
        let mut seen = HashSet::with_capacity(self.vertices.len());
        let mut component = Vec::new();
        let mut pending = Vec::new();
        for &root in order.iter().rev() {
            if !seen.insert(root) {
                continue;
            }
            pending.push(root);
            while let Some(id) = pending.pop() {
                component.push(id);
                if let Some(vertex) = self.vertices.get(id) {
                    pending.extend(
                        vertex
                            .incoming
                            .iter()
                            .copied()
                            .filter(|&parent| seen.insert(parent)),
                    );
                }
            }
            let recursive = component.len() > 1
                || self
                    .vertices
                    .get(root)
                    .is_some_and(|vertex| vertex.edges.contains(&root));
            if recursive {
                for &id in &component {
                    if let Some(vertex) = self.vertices.get_mut(id) {
                        vertex.protected = true;
                    }
                }
            }
            component.clear();
        }
    }

    fn rewrite_references(&self, schema: &mut Value, representatives: &[usize], at_root: bool) {
        if !at_root && has_reference_scope(schema) {
            return;
        }
        if let Some(object) = schema.as_object_mut()
            && object.len() == 1
            && let Some(Value::String(reference)) = object.get_mut("$ref")
            && let Some(&target) = self.references.get(reference.as_str())
            && let Some(&representative) = representatives.get(target)
            && let Some(definition) = self.shareable(representative)
            && let Some(canonical) = &definition.reference
            && canonical != reference
        {
            reference.clone_from(canonical);
        }
        visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
            self.rewrite_references(child, representatives, false);
        });
    }
}

#[derive(Clone, Copy)]
struct Source {
    id: usize,
    at_root: bool,
    scoped: bool,
}

impl Source {
    const fn document() -> Self {
        Self {
            id: DOCUMENT,
            at_root: true,
            scoped: false,
        }
    }
}

struct Target {
    id: usize,
    suffix: bool,
}

fn definition_target(
    reference: &str,
    names: &HashMap<DefinitionsKeyword, HashMap<&str, usize>>,
) -> Option<Target> {
    let parsed = parse_definition_reference(reference)?;
    let id = *names.get(&parsed.keyword)?.get(parsed.name.as_str())?;
    Some(Target {
        id,
        suffix: !parsed.suffix.is_empty(),
    })
}

fn pure_reference(schema: &Value) -> Option<&str> {
    let object = schema.as_object()?;
    (object.len() == 1).then_some(())?;
    object.get("$ref")?.as_str()
}

#[cfg(test)]
#[path = "tests/definition_graph.rs"]
mod tests;
