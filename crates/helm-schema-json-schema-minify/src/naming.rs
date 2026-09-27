//! Readable definition names, the short names shipped to Helm, and the one
//! simultaneous definition rename.
//!
//! Only anonymous definitions are named here: those the minifier extracts and
//! the private handles a caller lists.
//! Every other definition keeps its name.
//!
//! A readable name is a schema path.
//! [`DefinitionNames::Source`] names a definition after where its content
//! comes from: the source document and JSON pointer a caller records for it,
//! or else the smallest source path among the positions that reference it.
//! [`DefinitionNames::Destination`] names it after its first reference in a
//! canonical traversal of the document.
//!
//! A path renders property steps as `.name` and every other schema step with
//! an `@` marker, such as `@items`, `@anyOf(1)` or `@patternProperties('(5e)a')`.
//! A name uses only characters a URI fragment carries unencoded, so it is a
//! valid `$ref` target once escaped as a JSON pointer segment.
//! A reference at the root of a definition body adds the step `@ref`.
//! A name that is already taken receives an `@2`, `@3`, … suffix.
//!
//! [`shorten_definition_names`] is a bijective rename of a finished document
//! to short keys; [`expand_short_definition_names`] translates text that
//! mentions them back.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use helm_schema_json_schema_walk::{
    ReferenceSiblings, SchemaTraversalContext, escape_json_pointer_segment,
    schema_child_context_for_keyword, visit_subschemas, visit_subschemas_mut,
};
use serde_json::{Map, Value};

use crate::{DEFINITION_REF_PREFIX, DEFINITIONS_KEY, base62, is_nested_resource};

/// Keywords whose identifiers or dynamic resolution the short rename does
/// not model; a document using any of them keeps its names.
const UNMODELLED_REFERENCE_KEYWORDS: [&str; 7] = [
    "$id",
    "id",
    "$anchor",
    "$dynamicAnchor",
    "$recursiveAnchor",
    "$dynamicRef",
    "$recursiveRef",
];

/// Source document of every position outside the root `$defs`: the values
/// schema itself.
const ROOT_DOCUMENT: &str = "values";

/// How readable output names anonymous definitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DefinitionNames {
    /// The path where the definition's content comes from.
    #[default]
    Source,
    /// The path of the definition's first reference.
    Destination,
}

/// A place a definition's content was taken from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinitionOrigin {
    /// The source family, such as `k8s` or `crd`.
    pub provider: String,
    /// The source document, such as `postgresql.cnpg.io/cluster_v1`.
    pub document: String,
    /// JSON pointer of the content inside `document`.
    pub pointer: String,
}

/// A document with short definition names and the way back to the readable
/// ones.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortenedSchema {
    /// The renamed document.
    pub schema: Value,
    /// The readable name of every short name.
    pub readable_names: BTreeMap<String, String>,
}

/// Names the anonymous root definitions of `schema` by `policy`.
///
/// `anonymous` lists private handles with the recorded origins of their
/// content, which may be none.
/// Handles that are not root definitions are ignored.
pub fn name_definitions(
    schema: &mut Value,
    anonymous: &BTreeMap<String, Vec<DefinitionOrigin>>,
    policy: DefinitionNames,
) {
    let Some(definitions) = schema.get(DEFINITIONS_KEY).and_then(Value::as_object) else {
        return;
    };
    let mut present = BTreeMap::new();
    for (handle, origins) in anonymous {
        if definitions.contains_key(handle) {
            present.insert(handle.clone(), origins.as_slice());
        }
    }
    if present.is_empty() {
        return;
    }
    let names = match policy {
        DefinitionNames::Source => source_names(schema, definitions, &present),
        DefinitionNames::Destination => destination_names(schema, definitions, &present),
    };
    let mut renames = BTreeMap::new();
    for (handle, name) in names {
        if handle != name {
            renames.insert(handle, name);
        }
    }
    rename_definitions(schema, &renames);
}

/// Renames every root definition to a short base-62 key.
///
/// The most referenced definition receives the shortest key.
/// Nothing is renamed, and the map is empty, unless every reference is a plain
/// resolvable `#/…` pointer: a non-local, unresolved, anchor or
/// percent-encoded reference, a dynamic or recursive reference, or a nested
/// identifier or anchor keeps the document's names.
#[must_use]
pub fn shorten_definition_names(schema: &Value) -> ShortenedSchema {
    let short_names = short_definition_names(schema);
    let mut shortened = schema.clone();
    rename_definitions(&mut shortened, &short_names);
    let mut readable_names = BTreeMap::new();
    for (readable, short) in short_names {
        readable_names.insert(short, readable);
    }
    ShortenedSchema {
        schema: shortened,
        readable_names,
    }
}

/// Replaces every short definition key that follows `$defs/` in `text` with
/// its readable name, escaped as a JSON pointer segment.
///
/// Error messages from Helm and JSON Schema validators locate schema
/// positions as pointers, such as `file:///values.schema.json#/$defs/2b`.
#[must_use]
pub fn expand_short_definition_names(
    text: &str,
    readable_names: &BTreeMap<String, String>,
) -> String {
    let marker = format!("{DEFINITIONS_KEY}/");
    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&marker) {
        let (before, after) = rest.split_at(start + marker.len());
        expanded.push_str(before);
        let key_len = after
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        let (key, remainder) = after.split_at(key_len);
        match readable_names.get(key) {
            Some(readable) => expanded.push_str(&escape_json_pointer_segment(readable)),
            None => expanded.push_str(key),
        }
        rest = remainder;
    }
    expanded.push_str(rest);
    expanded
}

/// Renames root definitions and every reference to them at once.
///
/// Only `$ref` values at schema positions are rewritten, never data payloads
/// such as `default` or `examples`, and never references inside a nested
/// resource, which resolve against that resource instead of the root.
/// A reference keeps its pointer suffix below the renamed definition.
pub fn rename_definitions(schema: &mut Value, renames: &BTreeMap<String, String>) {
    if renames.is_empty() {
        return;
    }
    if let Some(Value::Object(definitions)) = schema.get_mut(DEFINITIONS_KEY) {
        let mut rekeyed = Map::new();
        for (name, body) in std::mem::take(definitions) {
            let name = renames.get(&name).cloned().unwrap_or(name);
            rekeyed.insert(name, body);
        }
        *definitions = rekeyed;
    }
    rewrite_definition_references(schema, renames);
}

/// Source names: a recorded origin, else the smallest source path of the
/// positions referencing the definition.
///
/// The source path of a position is its container's source plus the steps
/// below it: `values` for the document root, the name of a named or
/// origin-named definition, or the computed name of another anonymous one.
/// A definition is named once every container referencing it is, so each
/// minimum is exact.
/// A reference cycle among unnamed definitions is broken at the smallest
/// name reachable from outside it.
fn source_names(
    schema: &Value,
    definitions: &Map<String, Value>,
    anonymous: &BTreeMap<String, &[DefinitionOrigin]>,
) -> BTreeMap<String, String> {
    let mut taken = BTreeSet::new();
    let mut names = BTreeMap::new();
    for name in definitions.keys() {
        if !anonymous.contains_key(name) {
            taken.insert(name.clone());
            names.insert(name.clone(), name.clone());
        }
    }
    for (handle, origins) in anonymous {
        if let Some(name) = origins.iter().map(origin_name).min() {
            names.insert(handle.clone(), unique_name(name, &mut taken));
        }
    }

    // Every referencing position, grouped by the definition it references.
    let mut references = BTreeMap::<&str, Vec<(Option<&str>, String)>>::new();
    let mut containers = vec![(None, schema)];
    for (name, body) in definitions {
        containers.push((Some(name.as_str()), body));
    }
    for (container, body) in containers {
        let mut sites = Vec::new();
        collect_references(
            body,
            definitions,
            container.is_none(),
            &mut String::new(),
            &mut sites,
        );
        for (steps, target) in sites {
            references
                .entry(target)
                .or_default()
                .push((container, steps));
        }
    }

    // Unnamed definitions wait for the unnamed containers that reference them.
    let mut waiting = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for handle in anonymous.keys() {
        if names.contains_key(handle) {
            continue;
        }
        let Some(sites) = references.get(handle.as_str()) else {
            continue;
        };
        let mut containers = BTreeSet::new();
        for (container, _) in sites {
            if let Some(container) = container
                && !names.contains_key(*container)
            {
                containers.insert(*container);
            }
        }
        for container in &containers {
            dependents.entry(container).or_default().push(handle);
        }
        waiting.insert(handle, containers);
    }
    let mut ready = waiting
        .iter()
        .filter(|(_, containers)| containers.is_empty())
        .map(|(handle, _)| *handle)
        .collect::<BTreeSet<_>>();
    while !waiting.is_empty() {
        let (handle, name) = if let Some(handle) = ready.pop_first() {
            let Some(name) = smallest_source(&references, &names, handle) else {
                continue;
            };
            (handle, name)
        } else {
            let mut smallest: Option<(String, &str)> = None;
            for handle in waiting.keys() {
                if let Some(name) = smallest_source(&references, &names, handle)
                    && smallest.as_ref().is_none_or(|(least, _)| name < *least)
                {
                    smallest = Some((name, handle));
                }
            }
            let Some((name, handle)) = smallest else {
                break;
            };
            (handle, name)
        };
        waiting.remove(handle);
        names.insert(handle.to_string(), unique_name(name, &mut taken));
        for dependent in dependents.get(handle).into_iter().flatten() {
            if let Some(containers) = waiting.get_mut(dependent) {
                containers.remove(handle);
                if containers.is_empty() {
                    ready.insert(dependent);
                }
            }
        }
    }
    // Unreferenced definitions have no source path and keep their handles.
    for handle in anonymous.keys() {
        if !names.contains_key(handle) {
            names.insert(handle.clone(), unique_name(handle.clone(), &mut taken));
        }
    }
    names
}

/// The smallest source path among the references to `handle` whose
/// containers are named.
fn smallest_source(
    references: &BTreeMap<&str, Vec<(Option<&str>, String)>>,
    names: &BTreeMap<String, String>,
    handle: &str,
) -> Option<String> {
    let mut smallest: Option<String> = None;
    for (container, steps) in references.get(handle).into_iter().flatten() {
        let candidate = match container {
            None => join_path(ROOT_DOCUMENT, steps, true),
            Some(container) => match names.get(*container) {
                Some(prefix) => join_path(prefix, steps, false),
                None => continue,
            },
        };
        if smallest.as_ref().is_none_or(|least| candidate < *least) {
            smallest = Some(candidate);
        }
    }
    smallest
}

/// Destination names: the path of the first reference in a canonical
/// traversal that visits the document before the definitions, and each
/// definition once, in the order it is first reached.
fn destination_names(
    schema: &Value,
    definitions: &Map<String, Value>,
    anonymous: &BTreeMap<String, &[DefinitionOrigin]>,
) -> BTreeMap<String, String> {
    let mut taken = definitions
        .keys()
        .filter(|name| !anonymous.contains_key(*name))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut names = BTreeMap::new();
    let mut reached = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut roots = vec![(None, schema)];
    // Unreached definitions are traversed afterwards, as roots of their own.
    for (name, body) in definitions {
        roots.push((Some(name.as_str()), body));
    }
    for (root, body) in roots {
        if let Some(root) = root {
            if !reached.insert(root) {
                continue;
            }
            let name = if anonymous.contains_key(root) {
                unique_name(root.to_string(), &mut taken)
            } else {
                root.to_string()
            };
            names.insert(root.to_string(), name);
        }
        queue.push_back((root, body));
        while let Some((container, body)) = queue.pop_front() {
            let prefix = container
                .and_then(|container| names.get(container))
                .cloned()
                .unwrap_or_default();
            let mut sites = Vec::new();
            collect_references(
                body,
                definitions,
                container.is_none(),
                &mut String::new(),
                &mut sites,
            );
            for (steps, target) in sites {
                if !reached.insert(target) {
                    continue;
                }
                let name = if anonymous.contains_key(target) {
                    let mut path = join_path(&prefix, &steps, container.is_none());
                    if path.is_empty() {
                        path.push_str("@root");
                    }
                    unique_name(path, &mut taken)
                } else {
                    target.to_string()
                };
                names.insert(target.to_string(), name);
                if let Some(target_body) = definitions.get(target) {
                    queue.push_back((Some(target), target_body));
                }
            }
        }
    }
    names
}

/// Collects `(steps, definition)` for every plain reference to a root
/// definition below `schema`, in canonical order: object keys sorted, array
/// items in order.
///
/// `steps` is the rendered path from `schema` to the reference.
/// The root `$defs` of the document is skipped when `is_document_root`.
fn collect_references<'a>(
    schema: &Value,
    definitions: &'a Map<String, Value>,
    is_document_root: bool,
    steps: &mut String,
    sites: &mut Vec<(String, &'a str)>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some((name, suffix)) = root_definition_reference(reference)
        && suffix.is_empty()
        && let Some((target, _)) = definitions.get_key_value(&name)
    {
        sites.push((steps.clone(), target.as_str()));
    }
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort_unstable();
    for key in keys {
        if is_document_root && key == DEFINITIONS_KEY {
            continue;
        }
        let Some(value) = object.get(key) else {
            continue;
        };
        let length = steps.len();
        match (schema_child_context_for_keyword(key), value) {
            (
                SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray,
                Value::Array(items),
            ) => {
                for (index, item) in items.iter().enumerate() {
                    push_step(steps, key, Some(&index.to_string()));
                    collect_child(item, definitions, steps, sites);
                    steps.truncate(length);
                }
            }
            (SchemaTraversalContext::Schema, _) => {
                push_step(steps, key, None);
                collect_child(value, definitions, steps, sites);
                steps.truncate(length);
            }
            (SchemaTraversalContext::SchemaMapValues, Value::Object(entries)) => {
                let mut entry_keys = entries.keys().collect::<Vec<_>>();
                entry_keys.sort_unstable();
                for entry_key in entry_keys {
                    let Some(entry) = entries.get(entry_key) else {
                        continue;
                    };
                    if key == "properties" {
                        steps.push('.');
                        steps.push_str(&encode_key(entry_key));
                    } else {
                        push_step(steps, key, Some(&encode_key(entry_key)));
                    }
                    collect_child(entry, definitions, steps, sites);
                    steps.truncate(length);
                }
            }
            _ => {}
        }
    }
}

fn collect_child<'a>(
    schema: &Value,
    definitions: &'a Map<String, Value>,
    steps: &mut String,
    sites: &mut Vec<(String, &'a str)>,
) {
    if !is_nested_resource(schema) {
        collect_references(schema, definitions, false, steps, sites);
    }
}

/// `prefix` followed by rendered `steps`; below a source document the first
/// property step drops its dot and the document is separated by `/`.
///
/// A reference at the root of a definition body is the step `@ref` below the
/// definition's name.
fn join_path(prefix: &str, steps: &str, prefix_is_document: bool) -> String {
    if !prefix_is_document {
        if steps.is_empty() {
            return format!("{prefix}@ref");
        }
        return format!("{prefix}{steps}");
    }
    let steps = steps.strip_prefix('.').unwrap_or(steps);
    if steps.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        steps.to_string()
    } else {
        format!("{prefix}/{steps}")
    }
}

/// The source path of an origin.
///
/// A pointer into a root `definitions` or `$defs` entry names a type, which
/// then stands for the document.
fn origin_name(origin: &DefinitionOrigin) -> String {
    let mut segments = origin
        .pointer
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<VecDeque<_>>();
    let mut document = format!(
        "{}/{}",
        encode_document(&origin.provider),
        encode_document(&origin.document)
    );
    if segments.len() >= 2
        && segments
            .front()
            .is_some_and(|keyword| keyword == "definitions" || keyword == DEFINITIONS_KEY)
    {
        segments.pop_front();
        if let Some(type_name) = segments.pop_front() {
            document = format!(
                "{}/{}",
                encode_document(&origin.provider),
                encode_document(&type_name)
            );
        }
    }
    let mut steps = String::new();
    while let Some(keyword) = segments.pop_front() {
        match schema_child_context_for_keyword(&keyword) {
            SchemaTraversalContext::SchemaMapValues => {
                let entry = segments.pop_front().unwrap_or_default();
                if keyword == "properties" {
                    steps.push('.');
                    steps.push_str(&encode_key(&entry));
                } else {
                    push_step(&mut steps, &keyword, Some(&encode_key(&entry)));
                }
            }
            SchemaTraversalContext::SchemaArray => {
                let index = segments.pop_front().unwrap_or_default();
                push_step(&mut steps, &keyword, Some(&encode_key(&index)));
            }
            _ => {
                // A numeric segment is an index: keywords are never numeric.
                match segments.front() {
                    Some(index) if index.parse::<usize>().is_ok() => {
                        push_step(&mut steps, &keyword, Some(index));
                        segments.pop_front();
                    }
                    _ => push_step(&mut steps, &encode_key(&keyword), None),
                }
            }
        }
    }
    join_path(&document, &steps, true)
}

/// A property or map key as a path step: bare when it is a plain identifier,
/// else quoted with every character a URI fragment cannot carry as `(hex)`.
fn encode_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return key.to_string();
    }
    let mut quoted = String::from("'");
    for character in key.chars() {
        if character.is_ascii_alphanumeric() || "-._~!$&*+,;=:@/?".contains(character) {
            quoted.push(character);
        } else {
            push_escaped(&mut quoted, character);
        }
    }
    quoted.push('\'');
    quoted
}

/// Appends the step `@keyword`, or `@keyword(argument)` for an index or a
/// map key.
fn push_step(steps: &mut String, keyword: &str, argument: Option<&str>) {
    steps.push('@');
    steps.push_str(keyword);
    if let Some(argument) = argument {
        steps.push('(');
        steps.push_str(argument);
        steps.push(')');
    }
}

/// Appends a character a URI fragment cannot carry as `(hex)`.
fn push_escaped(out: &mut String, character: char) {
    let hex = format!("{:x}", u32::from(character));
    out.push('(');
    out.push_str(&hex);
    out.push(')');
}

/// A document identity, whose `/`-separated segments and dots stay bare.
fn encode_document(document: &str) -> String {
    let document = document.strip_suffix(".json").unwrap_or(document);
    let mut encoded = String::new();
    for character in document.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/') {
            encoded.push(character);
        } else {
            push_escaped(&mut encoded, character);
        }
    }
    encoded
}

/// `name`, or `name@2`, `name@3`, … when it is taken; records the result.
fn unique_name(name: String, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(name.clone()) {
        return name;
    }
    let mut ordinal = 2;
    loop {
        let candidate = format!("{name}@{ordinal}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        ordinal += 1;
    }
}

/// Short base-62 names for every root definition, most referenced first;
/// empty when a reference cannot be rewritten faithfully.
fn short_definition_names(schema: &Value) -> BTreeMap<String, String> {
    let Some(definitions) = schema.get(DEFINITIONS_KEY).and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    if crate::protected_reference_targets(schema).is_none()
        || uses_unmodelled_references(schema, true)
    {
        return BTreeMap::new();
    }
    let mut counts = definitions
        .keys()
        .map(|name| (name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    if !count_definition_references(schema, &mut counts) {
        return BTreeMap::new();
    }
    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_name, left_count), (right_name, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_name.cmp(right_name))
    });
    let mut names = BTreeMap::new();
    for (index, (name, _)) in ranked.into_iter().enumerate() {
        names.insert(name, base62(index + 1));
    }
    names
}

fn rewrite_definition_references(schema: &mut Value, renames: &BTreeMap<String, String>) {
    if let Some(Value::String(reference)) = schema.get_mut("$ref")
        && let Some((name, suffix)) = root_definition_reference(reference)
        && let Some(renamed) = renames.get(&name)
    {
        *reference = format!(
            "{DEFINITION_REF_PREFIX}{}{suffix}",
            escape_json_pointer_segment(renamed)
        );
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
        if !is_nested_resource(child) {
            rewrite_definition_references(child, renames);
        }
    });
}

/// Whether a schema uses a keyword the short rename does not model; the
/// root may carry its own `$id`.
fn uses_unmodelled_references(schema: &Value, is_root: bool) -> bool {
    for keyword in UNMODELLED_REFERENCE_KEYWORDS {
        if schema.get(keyword).is_some() && !(is_root && matches!(keyword, "$id" | "id")) {
            return true;
        }
    }
    let mut found = false;
    visit_subschemas(schema, ReferenceSiblings::Visit, &mut |child| {
        found = found || uses_unmodelled_references(child, false);
    });
    found
}

/// Counts references per root definition; `false` when one is not a plain
/// `#/…` pointer.
fn count_definition_references(schema: &Value, counts: &mut BTreeMap<String, usize>) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if reference.contains('%') || !(reference == "#" || reference.starts_with("#/")) {
            return false;
        }
        if let Some((name, _)) = root_definition_reference(reference)
            && let Some(count) = counts.get_mut(&name)
        {
            *count += 1;
        }
    }
    let mut faithful = true;
    visit_subschemas(schema, ReferenceSiblings::Visit, &mut |child| {
        if faithful && !is_nested_resource(child) {
            faithful = count_definition_references(child, counts);
        }
    });
    faithful
}

/// Splits `#/$defs/<name><suffix>` into the unescaped name and the suffix.
fn root_definition_reference(reference: &str) -> Option<(String, String)> {
    let rest = reference.strip_prefix(DEFINITION_REF_PREFIX)?;
    let (segment, suffix) = match rest.split_once('/') {
        Some((segment, suffix)) => (segment, format!("/{suffix}")),
        None => (rest, String::new()),
    };
    Some((segment.replace("~1", "/").replace("~0", "~"), suffix))
}
