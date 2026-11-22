use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_json_schema_walk::{
    GENERATED_DEFINITIONS, ReferenceSiblings, parse_definition_reference, visit_subschemas,
};
use serde_json::Value;

use crate::schema_tree::SchemaDocument;

#[cfg(test)]
#[path = "../tests/condition_junctions.rs"]
mod tests;

#[tracing::instrument(skip_all)]
pub(super) fn compact(document: &SchemaDocument, definitions: &mut BTreeMap<String, Value>) {
    {
        let _span = tracing::info_span!("prune_condition_definitions").entered();
        crate::provider_definitions::prune_unreachable_provider_definitions(document, definitions);
    }
    let uses = junction_uses(document, definitions);
    flatten_condition_junctions(definitions, &uses);
}

/// Splices private, single-use junctions into the enclosing junction of the same kind.
#[tracing::instrument(skip_all)]
fn flatten_condition_junctions(
    definitions: &mut BTreeMap<String, Value>,
    uses: &HashMap<String, JunctionUses>,
) {
    let mut inline = BTreeSet::new();
    for (name, body) in definitions.iter() {
        if uses
            .get(name)
            .is_some_and(|usage| usage.descendant_addressed)
        {
            continue;
        }
        let Some((keyword, items)) = pure_junction(body) else {
            continue;
        };
        for item in items {
            if let Some(name) = condition_junction_reference(item)
                && uses
                    .get(&name)
                    .is_some_and(|usage| usage.whole == 1 && !usage.descendant_addressed)
                && definitions
                    .get(&name)
                    .and_then(pure_junction)
                    .is_some_and(|(child_keyword, _)| child_keyword == keyword)
            {
                inline.insert(name);
            }
        }
    }
    if inline.is_empty() {
        return;
    }

    let mut nested = definitions
        .extract_if(.., |name, _| inline.contains(name))
        .collect::<HashMap<_, _>>();
    for (name, body) in definitions.iter_mut() {
        if uses
            .get(name)
            .is_some_and(|usage| usage.descendant_addressed)
        {
            continue;
        }
        let Some((keyword, _)) = pure_junction(body) else {
            continue;
        };
        let Some(items) = body.get_mut(keyword).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut pending = std::mem::take(items);
        pending.reverse();
        while let Some(item) = pending.pop() {
            if let Some(name) = condition_junction_reference(&item)
                && let Some(mut child) = nested.remove(&name)
                && let Some(children) = child.get_mut(keyword).and_then(Value::as_array_mut)
            {
                pending.extend(std::mem::take(children).into_iter().rev());
            } else {
                items.push(item);
            }
        }
    }
    debug_assert!(nested.is_empty());
}

fn pure_junction(value: &Value) -> Option<(&'static str, &[Value])> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    for keyword in ["allOf", "anyOf"] {
        if let Some(items) = object.get(keyword).and_then(Value::as_array) {
            return Some((keyword, items));
        }
    }
    None
}

fn condition_junction_reference(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let reference = parse_definition_reference(object.get("$ref")?.as_str()?)?;
    (reference.keyword == GENERATED_DEFINITIONS && reference.suffix.is_empty())
        .then_some(reference.name)
}

#[derive(Default)]
struct JunctionUses {
    whole: usize,
    descendant_addressed: bool,
}

#[tracing::instrument(skip_all)]
fn junction_uses(
    document: &SchemaDocument,
    definitions: &BTreeMap<String, Value>,
) -> HashMap<String, JunctionUses> {
    let mut uses = definitions
        .iter()
        .filter(|(_, body)| pure_junction(body).is_some())
        .map(|(name, _)| (name.clone(), JunctionUses::default()))
        .collect::<HashMap<_, _>>();
    if !uses.is_empty() {
        document.visit_embedded_values(&mut |value| count_junction_uses(value, &mut uses));
        for body in definitions.values() {
            count_junction_uses(body, &mut uses);
        }
    }
    uses
}

fn count_junction_uses(value: &Value, uses: &mut HashMap<String, JunctionUses>) {
    if let Some(reference) = value.get("$ref").and_then(Value::as_str)
        && let Some(reference) = parse_definition_reference(reference)
        && reference.keyword == GENERATED_DEFINITIONS
        && let Some(usage) = uses.get_mut(&reference.name)
    {
        if reference.suffix.is_empty() {
            usage.whole += 1;
        } else {
            // An addressed junction keeps its body as well as its definition name.
            usage.descendant_addressed = true;
        }
    }
    visit_subschemas(value, ReferenceSiblings::Visit, &mut |child| {
        count_junction_uses(child, uses);
    });
}
