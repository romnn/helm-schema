use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_json_schema_walk::{
    DefinitionsKeyword, ReferenceSiblings, local_reference_fragment, parse_definition_reference,
    visit_subschemas, visit_subschemas_mut,
};
use serde_json::Value;

use crate::{has_reference_scope, protected_reference_targets, value_address};

/// Output-layout budget in JSON containers for a single inlined body at its use site.
/// A body that would exceed this budget keeps its reference and validation meaning.
/// See ADR 0072, decision 10, for the scope of local bounds.
const INLINE_DEPTH_BUDGET: usize = 64;

// The document root and the definitions map enclose each root definition body.
const DEFINITION_ENCLOSING_DEPTH: usize = 2;

/// Moves a definition body to its sole, sibling-free reference in the same resource.
#[tracing::instrument(skip_all, fields(depth_limited = tracing::field::Empty))]
pub(crate) fn inline_single_use_definitions(schema: &mut Value) {
    let Some(graph) = ReferenceGraph::new(schema) else {
        return;
    };
    let order = graph.finish_order();
    let recursive = graph.recursive_definitions(&order);
    let moves = graph.plan_moves(schema, &order, &recursive);
    let mut bodies = (0..graph.edges.len()).map(|_| None).collect::<Vec<_>>();
    for (keyword, names) in &graph.definitions {
        let Some(definitions) = schema
            .get_mut(keyword.as_str())
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        for (name, id) in names {
            if moves.contains(id)
                && let Some(body) = bodies.get_mut(*id)
            {
                *body = definitions.remove(name);
            }
        }
    }

    // Moving the single use preserves the use count of every referenced body.
    // Process dependencies first so the move collapses chains in one pass.
    for id in order {
        if let Some(mut body) = bodies.get_mut(id).and_then(Option::take) {
            inline_references(&mut body, &graph, &mut bodies);
            if let Some(slot) = bodies.get_mut(id) {
                *slot = Some(body);
            }
        }
    }
    inline_references(schema, &graph, &mut bodies);

    if let Some(object) = schema.as_object_mut() {
        for keyword in graph.definitions.keys() {
            if object
                .get(keyword.as_str())
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty)
            {
                object.remove(keyword.as_str());
            }
        }
    }
}

struct DefinitionPlan<'a> {
    body: &'a Value,
    height: Option<usize>,
    inline: bool,
}

fn planned_height<'schema>(
    mut value: &'schema Value,
    enclosing_depth: usize,
    reference_sites: &HashMap<usize, usize>,
    plans: &mut [Option<DefinitionPlan<'schema>>],
) -> usize {
    while let Some(&id) = reference_sites.get(&value_address(value))
        && let Some(Some(plan)) = plans.get_mut(id)
        && let Some(height) = plan.height
        && enclosing_depth + height <= INLINE_DEPTH_BUDGET
    {
        plan.inline = true;
        if enclosing_depth >= DEFINITION_ENCLOSING_DEPTH {
            return height;
        }
        // Revisit a moved body's skipped children only when their use sites get shallower.
        value = plan.body;
    }
    let child_depth = enclosing_depth + 1;
    match value {
        Value::Object(members) => {
            1 + members
                .values()
                .map(|child| planned_height(child, child_depth, reference_sites, plans))
                .max()
                .unwrap_or(0)
        }
        Value::Array(items) => {
            1 + items
                .iter()
                .map(|child| planned_height(child, child_depth, reference_sites, plans))
                .max()
                .unwrap_or(0)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 0,
    }
}

fn inline_references(schema: &mut Value, graph: &ReferenceGraph, bodies: &mut [Option<Value>]) {
    if let Some(reference) = pure_reference(schema)
        && let Some(id) = graph.definition_id(reference)
        && let Some(body) = bodies.get_mut(id).and_then(Option::take)
    {
        *schema = body;
        return;
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
        inline_references(child, graph, bodies);
    });
}

fn pure_reference(schema: &Value) -> Option<&str> {
    let object = schema.as_object()?;
    if object.len() == 1 {
        object.get("$ref")?.as_str()
    } else {
        None
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Uses {
    #[default]
    None,
    Once,
    Pinned,
}

struct ReferenceGraph {
    definitions: BTreeMap<DefinitionsKeyword, BTreeMap<String, usize>>,
    reference_sites: HashMap<usize, usize>,
    edges: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    uses: Vec<Uses>,
}

impl ReferenceGraph {
    const DOCUMENT: usize = 0;

    fn new(schema: &Value) -> Option<Self> {
        let mut definitions = BTreeMap::<_, BTreeMap<_, _>>::new();
        let mut locations = HashMap::new();
        for keyword in DefinitionsKeyword::ALL {
            let Some(bodies) = schema.get(keyword.as_str()).and_then(Value::as_object) else {
                continue;
            };
            for (name, body) in bodies {
                let id = locations.len() + 1;
                locations.insert(value_address(body), id);
                definitions
                    .entry(keyword)
                    .or_default()
                    .insert(name.clone(), id);
            }
        }
        if definitions.is_empty() || protected_reference_targets(schema).is_none() {
            return None;
        }

        let count = locations.len() + 1;
        let mut graph = Self {
            definitions,
            reference_sites: HashMap::new(),
            edges: vec![Vec::new(); count],
            incoming: vec![Vec::new(); count],
            uses: vec![Uses::None; count],
        };
        graph.record_references(schema, Self::DOCUMENT, &locations, true)?;
        Some(graph)
    }

    fn plan_moves(
        &self,
        schema: &Value,
        order: &[usize],
        recursive: &HashSet<usize>,
    ) -> HashSet<usize> {
        let mut plans = (0..self.edges.len()).map(|_| None).collect::<Vec<_>>();
        for (keyword, names) in &self.definitions {
            for (name, &id) in names {
                if let Some(body) = schema.get(keyword.as_str()).and_then(|map| map.get(name))
                    && let Some(slot) = plans.get_mut(id)
                {
                    *slot = Some(DefinitionPlan {
                        body,
                        height: None,
                        inline: false,
                    });
                }
            }
        }

        // Plan dependencies first so a parent's height includes its child moves.
        for &id in order {
            if let Some(Some(DefinitionPlan { body, .. })) = plans.get(id) {
                let height = planned_height(
                    body,
                    DEFINITION_ENCLOSING_DEPTH,
                    &self.reference_sites,
                    &mut plans,
                );
                if self.uses.get(id) == Some(&Uses::Once)
                    && !recursive.contains(&id)
                    && let Some(Some(plan)) = plans.get_mut(id)
                {
                    plan.height = Some(height);
                }
            }
        }

        if let Some(root) = schema.as_object() {
            for (key, value) in root {
                if !self
                    .definitions
                    .keys()
                    .any(|keyword| keyword.as_str() == key)
                {
                    planned_height(value, 1, &self.reference_sites, &mut plans);
                }
            }
        }
        let mut moves = HashSet::new();
        let mut depth_limited = 0;
        for (id, plan) in plans.into_iter().enumerate() {
            if let Some(plan) = plan {
                if plan.inline {
                    moves.insert(id);
                } else if plan.height.is_some() {
                    depth_limited += 1;
                }
            }
        }
        tracing::Span::current().record("depth_limited", depth_limited);
        moves
    }

    fn definition_id(&self, reference: &str) -> Option<usize> {
        let parsed = parse_definition_reference(reference)?;
        if !parsed.suffix.is_empty() {
            return None;
        }
        self.definitions
            .get(&parsed.keyword)?
            .get(&parsed.name)
            .copied()
    }

    fn record_references(
        &mut self,
        schema: &Value,
        source: usize,
        locations: &HashMap<usize, usize>,
        root: bool,
    ) -> Option<()> {
        if (!root && has_reference_scope(schema))
            || schema.get("$dynamicRef").is_some()
            || schema.get("$recursiveRef").is_some()
        {
            return None;
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let target = if let Some(parsed) = parse_definition_reference(reference) {
                let id = *self.definitions.get(&parsed.keyword)?.get(&parsed.name)?;
                let uses = self.uses.get_mut(id)?;
                *uses = if *uses == Uses::None
                    && parsed.suffix.is_empty()
                    && pure_reference(schema).is_some()
                {
                    self.reference_sites.insert(value_address(schema), id);
                    Uses::Once
                } else {
                    Uses::Pinned
                };
                id
            } else {
                let fragment = local_reference_fragment(reference)?;
                if !fragment.is_empty() && !fragment.starts_with('/') {
                    return None;
                }
                // A pointer outside definitions can return to any definition
                // reached through the document's own schema positions.
                Self::DOCUMENT
            };
            self.edges.get_mut(source)?.push(target);
            self.incoming.get_mut(target)?.push(source);
        }
        let mut complete = Some(());
        visit_subschemas(schema, ReferenceSiblings::Visit, &mut |child| {
            let source = if root {
                locations
                    .get(&value_address(child))
                    .copied()
                    .unwrap_or(source)
            } else {
                source
            };
            complete =
                complete.and_then(|()| self.record_references(child, source, locations, false));
        });
        complete
    }

    /// Depth-first postorder on the reference graph.
    fn finish_order(&self) -> Vec<usize> {
        let mut seen = HashSet::with_capacity(self.edges.len());
        let mut order = Vec::with_capacity(self.edges.len());
        let mut stack = Vec::new();
        for (root, children) in self.edges.iter().enumerate() {
            if !seen.insert(root) {
                continue;
            }
            stack.push((root, children.iter()));
            while let Some((node, mut children)) = stack.pop() {
                if let Some(&child) = children.find(|&&child| seen.insert(child))
                    && let Some(edges) = self.edges.get(child)
                {
                    stack.push((node, children));
                    stack.push((child, edges.iter()));
                } else {
                    order.push(node);
                }
            }
        }
        order
    }

    /// Collect strongly connected components via the reverse graph in reverse postorder.
    fn recursive_definitions(&self, order: &[usize]) -> HashSet<usize> {
        let mut seen = HashSet::with_capacity(self.edges.len());
        let mut recursive = HashSet::new();
        let mut stack = Vec::new();
        let mut component = Vec::new();
        for &root in order.iter().rev() {
            if !seen.insert(root) {
                continue;
            }
            stack.push(root);
            while let Some(node) = stack.pop() {
                component.push(node);
                for &parent in self.incoming.get(node).into_iter().flatten() {
                    if seen.insert(parent) {
                        stack.push(parent);
                    }
                }
            }
            if component.len() > 1
                || self
                    .edges
                    .get(root)
                    .is_some_and(|edges| edges.contains(&root))
            {
                recursive.extend(component.iter().copied());
            }
            component.clear();
        }
        recursive
    }
}
