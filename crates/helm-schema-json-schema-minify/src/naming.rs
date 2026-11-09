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
//! or else the smallest stable source path among the positions that
//! reference it, or else, below `allOf`/`anyOf` arms whose index is not
//! stable, the values paths it tests and constrains (`when/…/then/…`,
//! `constrains/…`).
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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;
use std::rc::Rc;

use ahash::{AHashMap, AHashSet};

use helm_schema_json_schema_walk::{
    DefinitionReference, ReferenceSiblings, SchemaTraversalContext, canonical_json_string,
    definition_reference, local_reference_fragment, parse_definition_reference,
    schema_child_context_for_keyword, unescape_json_pointer_segment, visit_subschemas,
    visit_subschemas_mut,
};
use serde_json::{Map, Value};

use crate::{DEFINITIONS_KEY, base62, is_nested_resource};

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
    /// Whether any definition name changed.
    pub shortened: bool,
}

/// Names the anonymous root definitions of `schema` by `policy`.
///
/// `anonymous` lists private handles with the recorded origins of their
/// content, which may be none.
/// Handles that are not root definitions are ignored.
#[tracing::instrument(skip_all, fields(anonymous = anonymous.len()))]
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
/// resolvable `#/…` pointer: a non-local, unresolved or anchor reference, a
/// dynamic or recursive reference, or a nested
/// identifier or anchor keeps the document's names.
#[must_use]
pub fn shorten_definition_names(schema: &Value) -> ShortenedSchema {
    let short_names = short_definition_names(schema);
    let mut shortened = schema.clone();
    rename_definitions(&mut shortened, &short_names);
    let mut readable_names = BTreeMap::new();
    let mut names_changed = false;
    for (readable, short) in short_names {
        names_changed |= readable != short;
        readable_names.insert(short, readable);
    }
    ShortenedSchema {
        schema: shortened,
        readable_names,
        shortened: names_changed,
    }
}

/// Replaces every short definition key in a root definition reference
/// `#/$defs/<key>` in `text` with its readable name, spelled as in a `$ref`
/// (see [`definition_reference`]).
///
/// Error messages from Helm and JSON Schema validators locate schema
/// positions as pointers, such as `file:///values.schema.json#/$defs/2b`.
/// The key is the complete reference token after `#/$defs/`, which ends at
/// `/`, a quote, whitespace, a control byte or the end of `text`; it is
/// translated only when the whole token is a key of `readable_names`, so key
/// `1` leaves `#/$defs/1+x` or `#/$defs/1~0x` alone. A pointer below a
/// definition, such as `#/$defs/1/properties/a`, keeps its suffix. Every
/// other byte, including invalid UTF-8, is kept as is.
#[must_use]
pub fn expand_short_definition_names(
    text: &[u8],
    readable_names: &BTreeMap<String, String>,
) -> Vec<u8> {
    let marker = format!("#/{DEFINITIONS_KEY}/");
    let marker = marker.as_bytes();
    let mut expanded = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest
        .windows(marker.len())
        .position(|window| window == marker)
    {
        let (before, after) = rest.split_at(start + marker.len());
        expanded.extend_from_slice(before);
        let key_len = after
            .iter()
            .position(|&byte| {
                b"/\"'`".contains(&byte) || byte.is_ascii_whitespace() || byte.is_ascii_control()
            })
            .unwrap_or(after.len());
        let (key, remainder) = after.split_at(key_len);
        let readable = std::str::from_utf8(key)
            .ok()
            .and_then(|key| readable_names.get(key));
        match readable {
            Some(readable) => {
                expanded.extend_from_slice(definition_pointer_token(readable).as_bytes());
            }
            None => expanded.extend_from_slice(key),
        }
        rest = remainder;
    }
    expanded.extend_from_slice(rest);
    expanded
}

/// `name` as the token after `#/$defs/` in its `$ref`.
fn definition_pointer_token(name: &str) -> String {
    let reference = definition_reference(name);
    reference
        .get(definition_reference("").len()..)
        .unwrap_or_default()
        .to_string()
}

/// Renames root definitions and every reference to them at once.
///
/// Only `$ref` values at schema positions are rewritten, never data payloads
/// such as `default` or `examples`, and never references inside a nested
/// resource, which resolve against that resource instead of the root.
/// A reference keeps its pointer suffix below the renamed definition.
#[tracing::instrument(skip_all, fields(renames = renames.len()))]
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

/// Source names: a recorded origin; else the smallest stable source path of
/// the positions referencing the definition; else its meaning.
///
/// The source path of a position is its container's name plus the steps
/// below it, where the container is the document (`values`), a named or
/// origin-named definition, or an anonymous definition named by position.
/// A path is stable when it crosses no array index: an `allOf`/`anyOf` arm
/// index shifts whenever an unrelated arm is inserted.
/// A definition without a stable path is named by what it constrains (see
/// [`meaning_name`]); equal meanings are told apart by `@2`, `@3`, … in the
/// order of their content, so an unrelated fragment never renames them.
/// A definition is named once every container referencing it is, so each
/// minimum is exact; a reference cycle among unnamed definitions is broken
/// at its first handle.
#[tracing::instrument(skip_all)]
fn source_names(
    schema: &Value,
    definitions: &Map<String, Value>,
    anonymous: &BTreeMap<String, &[DefinitionOrigin]>,
) -> BTreeMap<String, String> {
    let mut taken = TakenNames::default();
    let mut names = BTreeMap::new();
    for name in definitions.keys() {
        if !anonymous.contains_key(name) {
            taken.insert(name.clone());
            names.insert(name.clone(), name.clone());
        }
    }
    for (handle, origins) in anonymous {
        if let Some(name) = origins.iter().map(origin_name).min() {
            names.insert(handle.clone(), taken.unique(name));
        }
    }
    let index = DefinitionIndex::new(definitions, anonymous);
    let mut program = MeaningProgram::new(&index);

    let references = references_by_target(schema, definitions);

    // Unnamed definitions wait for the unnamed containers that reference them.
    let mut waiting = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for handle in anonymous.keys() {
        if names.contains_key(handle) {
            continue;
        }
        let mut containers = BTreeSet::new();
        for (container, _) in references.get(handle.as_str()).into_iter().flatten() {
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
    // Meaning names, without their ordinals; they never prefix other names.
    let mut meanings = BTreeMap::<String, Vec<&str>>::new();
    let mut by_meaning = BTreeMap::new();
    while let Some(handle) = ready.pop_first().or_else(|| waiting.keys().next().copied()) {
        waiting.remove(handle);
        let sites = references
            .get(handle)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let (stable, ancestor) = position_names(sites, &names, &by_meaning);
        if let Some(name) = stable {
            names.insert(handle.to_string(), taken.unique(name));
        } else if let Some(body) = definitions.get(handle) {
            let (meaning, ancestor) =
                if let Some(meaning) = meaning_name(handle, body, &mut program) {
                    (meaning.clone(), meaning)
                } else {
                    // Without constrained paths, the leaf is named by what it
                    // is, below the nearest stable path above it.
                    let ancestor = ancestor.unwrap_or_else(|| ROOT_DOCUMENT.to_string());
                    let leaf = leaf_name(body, &index, 0);
                    (cut_name(&format!("{ancestor}/{leaf}")), ancestor)
                };
            names.insert(handle.to_string(), meaning.clone());
            meanings.entry(meaning).or_default().push(handle);
            by_meaning.insert(handle, ancestor);
        }
        for dependent in dependents.get(handle).into_iter().flatten() {
            if let Some(containers) = waiting.get_mut(dependent) {
                containers.remove(handle);
                if containers.is_empty() {
                    ready.insert(dependent);
                }
            }
        }
    }
    for (meaning, mut handles) in meanings {
        if handles.len() > 1 {
            handles.sort_by_cached_key(|handle| {
                let mut body = definitions.get(*handle).cloned().unwrap_or_default();
                erase_anonymous_references(&mut body, &index);
                (canonical_json_string(&body), *handle)
            });
        }
        for handle in handles {
            names.insert(handle.to_string(), taken.unique(meaning.clone()));
        }
    }
    // Unreferenced definitions have no source path and keep their handles.
    for handle in anonymous.keys() {
        if !names.contains_key(handle) {
            names.insert(handle.clone(), taken.unique(handle.clone()));
        }
    }
    names
}

/// Every plain reference to a root definition, from the document or a
/// definition body, grouped by the definition it references.
#[tracing::instrument(skip_all)]
fn references_by_target<'a>(
    schema: &'a Value,
    definitions: &'a Map<String, Value>,
) -> BTreeMap<&'a str, Vec<(Option<&'a str>, Site<'a>)>> {
    let mut references = BTreeMap::<&str, Vec<(Option<&str>, Site<'_>)>>::new();
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
            None,
            &mut sites,
        );
        for site in sites {
            references
                .entry(site.target)
                .or_default()
                .push((container, site));
        }
    }
    references
}

/// The smallest stable source path among `sites` whose containers are named
/// by position, and the smallest stable prefix of any of them: the nearest
/// named ancestor.
///
/// `by_meaning` maps each container named by meaning to the ancestor its
/// own fragments take: its meaning, or the ancestor it was named after.
fn position_names(
    sites: &[(Option<&str>, Site<'_>)],
    names: &BTreeMap<String, String>,
    by_meaning: &BTreeMap<&str, String>,
) -> (Option<String>, Option<String>) {
    let mut stable: Option<String> = None;
    let mut ancestor: Option<String> = None;
    for (container, site) in sites {
        let (prefix, is_document) = match container {
            None => (ROOT_DOCUMENT, true),
            Some(container) => match names.get(*container) {
                Some(prefix) => (prefix.as_str(), false),
                None => continue,
            },
        };
        if let Some(container) = container
            && let Some(ancestor_of_container) = by_meaning.get(container)
        {
            // Below a fragment named by meaning, its name is the ancestor.
            if ancestor
                .as_ref()
                .is_none_or(|least| ancestor_of_container < least)
            {
                ancestor = Some(ancestor_of_container.clone());
            }
            continue;
        }
        if site.first_index.is_none() {
            let candidate = join_path(prefix, &site.steps, is_document);
            if stable.as_ref().is_none_or(|least| candidate < *least) {
                stable = Some(candidate);
            }
        }
        let stable_steps = site
            .steps
            .get(..site.first_index.unwrap_or(site.steps.len()))
            .unwrap_or_default();
        let candidate = if stable_steps.is_empty() {
            prefix.to_string()
        } else {
            join_path(prefix, stable_steps, is_document)
        };
        if ancestor.as_ref().is_none_or(|least| candidate < *least) {
            ancestor = Some(candidate);
        }
    }
    (stable, ancestor)
}

/// Replaces references to anonymous definitions, whose handles are not
/// stable, with one placeholder.
fn erase_anonymous_references(schema: &mut Value, index: &DefinitionIndex<'_>) {
    if let Some(Value::String(reference)) = schema.get_mut("$ref")
        && let Some(parsed) = parse_definition_reference(reference)
        && index.anonymous.contains(parsed.name.as_str())
    {
        *reference = definition_reference("");
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
        erase_anonymous_references(child, index);
    });
}

/// Longest rendered path list in a meaning name before the rest is counted.
const MEANING_LIST_CHARS: usize = 80;

/// Characters of a stated value kept in a meaning name.
const MEANING_VALUE_CHARS: usize = 20;

/// Longest meaning name; a longer one is cut and marked `...`.
const MEANING_NAME_CHARS: usize = 200;

/// Schema nodes one meaning walk may visit.
const MEANING_WALK_BUDGET: usize = 4_000;

/// The name of a fragment after the values paths it constrains, read from
/// its own schema: `when/<paths its if tests>/then/<paths its then
/// constrains>` (and `/else/…`) for a conditional, `constrains/<paths>`
/// otherwise; `None` when it constrains no path.
///
/// A path carries the operator its schema states: `=v` for `const` or
/// `enum`, `!=v` for a negated one, `:name` for a reference to a named
/// definition, `?` for a key that is only required.
/// References to anonymous definitions are followed.
fn meaning_name(
    handle: &str,
    body: &Value,
    program: &mut MeaningProgram<'_, '_>,
) -> Option<String> {
    let root = program.definition(handle);
    let mut walk = MeaningWalk::new(program);
    if let Some(condition) = body.get("if") {
        let [if_node, then_node, else_node] = root
            .and_then(|root| walk.program.nodes.get(root))
            .map(|node| node.branches)
            .unwrap_or_default();
        let mut name = format!("when/{}", walk.path_list(condition, if_node));
        for (branch, node) in [("then", then_node), ("else", else_node)] {
            if let Some(schema) = body.get(branch) {
                let paths = walk.path_list(schema, node);
                name = format!("{name}/{branch}/{paths}");
            }
        }
        return Some(cut_name(&name));
    }
    let paths = walk.paths(root);
    (!paths.is_empty()).then(|| cut_name(&format!("constrains/{}", paths.render())))
}

/// Words of a description kept in a leaf name.
const LEAF_DESCRIPTION_WORDS: usize = 4;

/// Characters of a pattern kept in a leaf name.
const LEAF_PATTERN_CHARS: usize = 20;

/// References a leaf name follows into anonymous definitions.
const LEAF_REFERENCE_DEPTH: usize = 4;

/// What a leaf schema is, read from its own keywords: its `type` (and
/// `:format`, `=value`) with `;<first description words>` or
/// `;pattern-<pattern start>` (`~` would need escaping in every `$ref`); `not/<inner>` or `anyOf/<member>+<member>`
/// for a combinator; `:name` for a reference to a named definition; the
/// sorted keyword list for anything else.
fn leaf_name(schema: &Value, index: &DefinitionIndex<'_>, depth: usize) -> String {
    let Value::Object(object) = schema else {
        return schema.to_string();
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some(parsed) = parse_definition_reference(reference)
        && parsed.suffix.is_empty()
        && let Some(&(_, body)) = index.bodies.get(parsed.name.as_str())
    {
        if index.anonymous.contains(parsed.name.as_str()) && depth < LEAF_REFERENCE_DEPTH {
            return leaf_name(body, index, depth + 1);
        }
        return format!(":{}", encode_document(&parsed.name));
    }
    if let Some(kind) = object.get("type") {
        let mut name = match kind {
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(","),
            _ => kind.as_str().unwrap_or_default().to_string(),
        };
        if let Some(format) = object.get("format").and_then(Value::as_str) {
            name = format!("{name}:{}", encode_key(format));
        }
        if let Some(value) = object.get("const") {
            name = format!("{name}={}", render_value(value));
        } else if let Some(Value::Array(values)) = object.get("enum") {
            name = format!("{name}={}", render_values(values));
        }
        if let Some(description) = object.get("description").and_then(Value::as_str) {
            let words = slug(description.split_whitespace().take(LEAF_DESCRIPTION_WORDS));
            if !words.is_empty() {
                name = format!("{name};{words}");
            }
        } else if let Some(pattern) = object.get("pattern").and_then(Value::as_str) {
            let start = pattern.chars().take(LEAF_PATTERN_CHARS).collect::<String>();
            name = format!("{name};pattern-{}", slug(std::iter::once(start.as_str())));
        }
        return name;
    }
    if let Some(inner) = object.get("not") {
        return format!("not/{}", leaf_name(inner, index, depth));
    }
    for keyword in ["anyOf", "allOf", "oneOf"] {
        if let Some(Value::Array(members)) = object.get(keyword) {
            let members = members
                .iter()
                .map(|member| leaf_name(member, index, depth))
                .collect::<BTreeSet<_>>();
            return format!(
                "{keyword}/{}",
                render_path_list(members.iter().map(String::as_str))
            );
        }
    }
    if let Some(value) = object.get("const") {
        return format!("={}", render_value(value));
    }
    if let Some(Value::Array(values)) = object.get("enum") {
        return format!("={}", render_values(values));
    }
    let mut keywords = object.keys().map(|key| encode_key(key)).collect::<Vec<_>>();
    keywords.sort();
    if keywords.is_empty() {
        "any".to_string()
    } else {
        format!("keywords/{}", keywords.join("+"))
    }
}

/// Lowercase ASCII letters and digits of `words`, each run joined by `-`.
fn slug<'w>(words: impl Iterator<Item = &'w str>) -> String {
    let mut slug = String::new();
    for word in words {
        for character in word.chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character.to_ascii_lowercase());
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// `name`, cut to [`MEANING_NAME_CHARS`] characters.
fn cut_name(name: &str) -> String {
    if name.chars().count() <= MEANING_NAME_CHARS {
        return name.to_string();
    }
    let cut = name
        .chars()
        .take(MEANING_NAME_CHARS - 3)
        .collect::<String>();
    format!("{cut}...")
}

/// Root definitions by name, and the anonymous ones among them, for the
/// lookups every naming walk repeats.
struct DefinitionIndex<'a> {
    bodies: AHashMap<&'a str, (&'a str, &'a Value)>,
    anonymous: AHashSet<&'a str>,
    /// The root definition each distinct `$ref` text names, if any.
    references: RefCell<AHashMap<&'a str, Option<(&'a str, &'a Value)>>>,
}

impl<'a> DefinitionIndex<'a> {
    fn new(
        definitions: &'a Map<String, Value>,
        anonymous: &'a BTreeMap<String, &[DefinitionOrigin]>,
    ) -> Self {
        Self {
            bodies: definitions
                .iter()
                .map(|(name, body)| (name.as_str(), (name.as_str(), body)))
                .collect(),
            anonymous: anonymous.keys().map(String::as_str).collect(),
            references: RefCell::new(AHashMap::new()),
        }
    }

    /// The root definition `reference` names without a pointer below it.
    fn resolve(&self, reference: &'a str) -> Option<(&'a str, &'a Value)> {
        if let Some(target) = self.references.borrow().get(reference) {
            return *target;
        }
        let target = parse_definition_reference(reference)
            .filter(|parsed| parsed.suffix.is_empty())
            .and_then(|parsed| self.bodies.get(parsed.name.as_str()).copied());
        self.references.borrow_mut().insert(reference, target);
        target
    }
}

/// The keywords a meaning walk reads, found in one pass over a schema.
#[derive(Default)]
struct MeaningKeywords<'a> {
    reference: Option<&'a Value>,
    constant: Option<&'a Value>,
    enumeration: Option<&'a Value>,
    not: Option<&'a Value>,
    properties: Option<&'a Value>,
    required: Option<&'a Value>,
    all_of: Option<&'a Value>,
    any_of: Option<&'a Value>,
    one_of: Option<&'a Value>,
    if_schema: Option<&'a Value>,
    then_schema: Option<&'a Value>,
    else_schema: Option<&'a Value>,
    items: Option<&'a Value>,
    additional_properties: Option<&'a Value>,
    pattern_properties: Option<&'a Value>,
}

impl<'a> MeaningKeywords<'a> {
    fn of(object: &'a Map<String, Value>) -> Self {
        let mut keywords = Self::default();
        for (key, value) in object {
            let slot = match key.as_str() {
                "$ref" => &mut keywords.reference,
                "const" => &mut keywords.constant,
                "enum" => &mut keywords.enumeration,
                "not" => &mut keywords.not,
                "properties" => &mut keywords.properties,
                "required" => &mut keywords.required,
                "allOf" => &mut keywords.all_of,
                "anyOf" => &mut keywords.any_of,
                "oneOf" => &mut keywords.one_of,
                "if" => &mut keywords.if_schema,
                "then" => &mut keywords.then_schema,
                "else" => &mut keywords.else_schema,
                "items" => &mut keywords.items,
                "additionalProperties" => &mut keywords.additional_properties,
                "patternProperties" => &mut keywords.pattern_properties,
                _ => continue,
            };
            *slot = Some(value);
        }
        keywords
    }
}

/// A compiled schema object, by its position in [`MeaningProgram::nodes`].
type NodeId = usize;

/// An anonymous definition, by its position in [`MeaningProgram::roots`].
type DefinitionId = usize;

/// A values path, by its position in [`PathTable::texts`].
type PathId = usize;

/// A path segment or stated operator, by its position in
/// [`PathTable::words`].
type WordId = usize;

/// The empty path: the root of the fragment a walk starts from.
const ROOT_PATH: PathId = 0;

/// What a schema's `$ref` contributes to a meaning walk.
#[derive(Debug, Clone, Copy)]
enum MeaningReference {
    /// No reference to a root definition.
    None,
    /// A named definition, stated as `:name`.
    Named(WordId),
    /// An anonymous definition, whose body is walked in place.
    Anonymous(DefinitionId),
}

/// One schema object as the meaning walk reads it, its children compiled in
/// the order the walk visits them; non-object children, which the walk
/// skips, are left out.
#[derive(Debug)]
struct MeaningNode {
    reference: MeaningReference,
    /// Operators stated at the node's path: `const`, `enum`, then a negated
    /// `const` or `enum`.
    stated: Box<[WordId]>,
    /// A negation that states no value, walked at the node's path.
    not: Option<NodeId>,
    /// Properties in key order, with the object schema of each.
    properties: Box<[(WordId, Option<NodeId>)]>,
    required: Box<[WordId]>,
    /// `allOf`, `anyOf` and `oneOf` arms, then `if`, `then` and `else`.
    arms: Box<[NodeId]>,
    /// The `if`, `then` and `else` schemas on their own.
    branches: [Option<NodeId>; 3],
    items: Box<[NodeId]>,
    /// `additionalProperties`, then every `patternProperties` schema.
    any_key: Box<[NodeId]>,
}

/// One step from a values path to a longer one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PathStep {
    /// `.key`, or `key` at the root.
    Key(WordId),
    /// An operator such as `=v`, `!=v` or `:name`.
    Stated(WordId),
    /// `@items`.
    Items,
    /// `@*`.
    AnyKey,
    /// `?`, a key that is only required.
    Required,
}

/// Every values path the walks of one document spell, each text once.
///
/// A path is keyed by its parent and the step below it; two keys that spell
/// the same text share one id, so a set of ids is a set of texts.
#[derive(Debug)]
struct PathTable {
    texts: Vec<Rc<str>>,
    by_text: AHashMap<Rc<str>, PathId>,
    steps: AHashMap<(PathId, PathStep), PathId>,
    words: Vec<String>,
    word_ids: AHashMap<String, WordId>,
}

impl Default for PathTable {
    fn default() -> Self {
        let root: Rc<str> = Rc::from("");
        Self {
            texts: vec![Rc::clone(&root)],
            by_text: AHashMap::from_iter([(root, ROOT_PATH)]),
            steps: AHashMap::new(),
            words: Vec::new(),
            word_ids: AHashMap::new(),
        }
    }
}

impl PathTable {
    fn word(&mut self, word: String) -> WordId {
        if let Some(&id) = self.word_ids.get(&word) {
            return id;
        }
        let id = self.words.len();
        self.words.push(word.clone());
        self.word_ids.insert(word, id);
        id
    }

    fn text(&self, path: PathId) -> &str {
        self.texts.get(path).map_or("", |text| text)
    }

    fn child(&mut self, parent: PathId, step: PathStep) -> PathId {
        if let Some(&path) = self.steps.get(&(parent, step)) {
            return path;
        }
        let parent_text = self.text(parent);
        let word = |word: WordId| self.words.get(word).map_or("", String::as_str);
        let text = match step {
            PathStep::Key(key) if parent_text.is_empty() => word(key).to_string(),
            PathStep::Key(key) => format!("{parent_text}.{}", word(key)),
            PathStep::Stated(operator) => format!("{parent_text}{}", word(operator)),
            PathStep::Items => format!("{parent_text}@items"),
            PathStep::AnyKey => format!("{parent_text}@*"),
            PathStep::Required => format!("{parent_text}?"),
        };
        let path = if let Some(&path) = self.by_text.get(text.as_str()) {
            path
        } else {
            let path = self.texts.len();
            let text: Rc<str> = Rc::from(text);
            self.texts.push(Rc::clone(&text));
            self.by_text.insert(text, path);
            path
        };
        self.steps.insert((parent, step), path);
        path
    }
}

/// An anonymous definition's body until a walk first enters it, then its
/// compiled root.
#[derive(Debug, Clone, Copy)]
enum DefinitionRoot<'a> {
    Body(&'a Value),
    Compiled(Option<NodeId>),
}

/// The anonymous definitions of one document compiled for the meaning walk,
/// each once, and the values paths its walks spell.
///
/// Every entry is a function of its key alone: a compiled definition of the
/// definition's body, a path of its parent and step. Walks from different
/// definitions share them, so no walk re-reads a schema keyword or re-spells
/// a path another walk already did.
struct MeaningProgram<'i, 'a> {
    index: &'i DefinitionIndex<'a>,
    definition_ids: AHashMap<&'a str, DefinitionId>,
    roots: Vec<DefinitionRoot<'a>>,
    nodes: Vec<Rc<MeaningNode>>,
    paths: PathTable,
    /// The walk that last visited each definition.
    visited_by: Vec<usize>,
    /// The path set that last held each path.
    held_by: Vec<usize>,
    walks: usize,
    path_sets: usize,
}

impl<'i, 'a> MeaningProgram<'i, 'a> {
    fn new(index: &'i DefinitionIndex<'a>) -> Self {
        Self {
            index,
            definition_ids: AHashMap::new(),
            roots: Vec::new(),
            nodes: Vec::new(),
            paths: PathTable::default(),
            visited_by: Vec::new(),
            held_by: Vec::new(),
            walks: 0,
            path_sets: 0,
        }
    }

    /// The compiled root of the anonymous definition `handle`.
    fn definition(&mut self, handle: &str) -> Option<NodeId> {
        let &(handle, body) = self.index.bodies.get(handle)?;
        let definition = self.definition_id(handle, body);
        self.root(definition)
    }

    fn definition_id(&mut self, handle: &'a str, body: &'a Value) -> DefinitionId {
        if let Some(&definition) = self.definition_ids.get(handle) {
            return definition;
        }
        let definition = self.roots.len();
        self.roots.push(DefinitionRoot::Body(body));
        self.visited_by.push(0);
        self.definition_ids.insert(handle, definition);
        definition
    }

    fn root(&mut self, definition: DefinitionId) -> Option<NodeId> {
        let body = match *self.roots.get(definition)? {
            DefinitionRoot::Body(body) => body,
            DefinitionRoot::Compiled(root) => return root,
        };
        let root = self.compile(body);
        if let Some(slot) = self.roots.get_mut(definition) {
            *slot = DefinitionRoot::Compiled(root);
        }
        root
    }

    fn compile(&mut self, schema: &'a Value) -> Option<NodeId> {
        let Value::Object(object) = schema else {
            return None;
        };
        let keywords = MeaningKeywords::of(object);
        let reference = match keywords.reference {
            Some(Value::String(reference)) => match self.index.resolve(reference) {
                Some((target, body)) if self.index.anonymous.contains(target) => {
                    MeaningReference::Anonymous(self.definition_id(target, body))
                }
                Some((target, _)) => MeaningReference::Named(
                    self.paths.word(format!(":{}", encode_document(target))),
                ),
                None => MeaningReference::None,
            },
            _ => MeaningReference::None,
        };
        let mut stated = Vec::new();
        if let Some(value) = keywords.constant {
            stated.push(self.paths.word(format!("={}", render_value(value))));
        }
        if let Some(Value::Array(values)) = keywords.enumeration {
            stated.push(self.paths.word(format!("={}", render_values(values))));
        }
        let mut not = None;
        if let Some(negated) = keywords.not {
            if let Some(value) = negated.get("const") {
                stated.push(self.paths.word(format!("!={}", render_value(value))));
            } else if let Some(Value::Array(values)) = negated.get("enum") {
                stated.push(self.paths.word(format!("!={}", render_values(values))));
            } else {
                not = self.compile(negated);
            }
        }
        let mut properties = Vec::new();
        if let Some(Value::Object(schemas)) = keywords.properties {
            let mut schemas = schemas.iter().collect::<Vec<_>>();
            schemas.sort_unstable_by_key(|(key, _)| *key);
            for (key, child) in schemas {
                let key = self.paths.word(encode_key(key));
                properties.push((key, self.compile(child)));
            }
        }
        let mut required = Vec::new();
        if let Some(Value::Array(keys)) = keywords.required {
            for key in keys.iter().filter_map(Value::as_str) {
                required.push(self.paths.word(encode_key(key)));
            }
        }
        let mut arms = Vec::new();
        for list in [keywords.all_of, keywords.any_of, keywords.one_of] {
            if let Some(Value::Array(list)) = list {
                arms.extend(list.iter().filter_map(|arm| self.compile(arm)));
            }
        }
        let branches = [
            keywords.if_schema,
            keywords.then_schema,
            keywords.else_schema,
        ]
        .map(|branch| branch.and_then(|branch| self.compile(branch)));
        arms.extend(branches.iter().flatten());
        let items = match keywords.items {
            Some(Value::Array(items)) => {
                items.iter().filter_map(|item| self.compile(item)).collect()
            }
            Some(items) => self.compile(items).into_iter().collect(),
            None => Vec::new(),
        };
        let mut any_key = Vec::new();
        if let Some(additional) = keywords.additional_properties {
            any_key.extend(self.compile(additional));
        }
        if let Some(Value::Object(patterns)) = keywords.pattern_properties {
            any_key.extend(
                patterns
                    .values()
                    .filter_map(|pattern| self.compile(pattern)),
            );
        }
        let node = self.nodes.len();
        self.nodes.push(Rc::new(MeaningNode {
            reference,
            stated: stated.into(),
            not,
            properties: properties.into(),
            required: required.into(),
            arms: arms.into(),
            branches,
            items: items.into(),
            any_key: any_key.into(),
        }));
        Some(node)
    }
}

/// The values paths one walk reached, ordered by text.
struct PathSet {
    texts: BTreeSet<Rc<str>>,
    /// This set's mark in [`MeaningProgram::held_by`].
    mark: usize,
}

impl PathSet {
    fn len(&self) -> usize {
        self.texts.len()
    }

    fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    /// Whether a held path starts with `prefix`.
    fn has_prefix(&self, prefix: &str) -> bool {
        self.texts
            .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
            .next()
            .is_some_and(|text| text.starts_with(prefix))
    }

    fn render(&self) -> String {
        render_path_list(self.texts.iter().map(|text| &**text))
    }
}

/// One definition's meaning walk: up to [`MEANING_WALK_BUDGET`] schema
/// objects in depth-first order, each anonymous definition entered once.
struct MeaningWalk<'p, 'i, 'a> {
    program: &'p mut MeaningProgram<'i, 'a>,
    /// This walk's mark in [`MeaningProgram::visited_by`].
    mark: usize,
    budget: usize,
}

impl<'p, 'i, 'a> MeaningWalk<'p, 'i, 'a> {
    fn new(program: &'p mut MeaningProgram<'i, 'a>) -> Self {
        program.walks += 1;
        let mark = program.walks;
        Self {
            program,
            mark,
            budget: MEANING_WALK_BUDGET,
        }
    }

    fn path_list(&mut self, schema: &Value, node: Option<NodeId>) -> String {
        if *schema == Value::Bool(false) {
            return "fail".to_string();
        }
        let paths = self.paths(node);
        if paths.is_empty() {
            "any".to_string()
        } else {
            paths.render()
        }
    }

    fn paths(&mut self, node: Option<NodeId>) -> PathSet {
        self.program.path_sets += 1;
        let mut paths = PathSet {
            texts: BTreeSet::new(),
            mark: self.program.path_sets,
        };
        if let Some(node) = node {
            self.walk(node, ROOT_PATH, &mut paths);
        }
        paths
    }

    /// Whether `definition` is entered for the first time in this walk.
    fn enter(&mut self, definition: DefinitionId) -> bool {
        match self.program.visited_by.get_mut(definition) {
            Some(visited_by) if *visited_by != self.mark => {
                *visited_by = self.mark;
                true
            }
            Some(_) | None => false,
        }
    }

    fn insert(&mut self, paths: &mut PathSet, path: PathId) {
        if self.program.held_by.len() <= path {
            self.program.held_by.resize(path + 1, 0);
        }
        if let Some(held_by) = self.program.held_by.get_mut(path)
            && *held_by != paths.mark
        {
            *held_by = paths.mark;
            if let Some(text) = self.program.paths.texts.get(path) {
                paths.texts.insert(Rc::clone(text));
            }
        }
    }

    /// Records `path` with a stated operator; the fragment root is no values
    /// path, so what it states there is left to [`leaf_name`].
    fn insert_stated(&mut self, paths: &mut PathSet, path: PathId, operator: WordId) {
        if path != ROOT_PATH {
            let stated = self.program.paths.child(path, PathStep::Stated(operator));
            self.insert(paths, stated);
        }
    }

    fn walk(&mut self, node: NodeId, path: PathId, paths: &mut PathSet) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        let Some(node) = self.program.nodes.get(node).map(Rc::clone) else {
            return;
        };
        match node.reference {
            MeaningReference::None => {}
            MeaningReference::Named(operator) => self.insert_stated(paths, path, operator),
            MeaningReference::Anonymous(definition) => {
                if self.enter(definition)
                    && let Some(root) = self.program.root(definition)
                {
                    self.walk(root, path, paths);
                }
            }
        }
        for &operator in &node.stated {
            self.insert_stated(paths, path, operator);
        }
        if let Some(negated) = node.not {
            self.walk(negated, path, paths);
        }
        for &(key, child) in &node.properties {
            let child_path = self.program.paths.child(path, PathStep::Key(key));
            let before = paths.len();
            if let Some(child) = child {
                self.walk(child, child_path, paths);
            }
            if paths.len() == before {
                self.insert(paths, child_path);
            }
        }
        for &key in &node.required {
            let child_path = self.program.paths.child(path, PathStep::Key(key));
            if !paths.has_prefix(self.program.paths.text(child_path)) {
                let required = self.program.paths.child(child_path, PathStep::Required);
                self.insert(paths, required);
            }
        }
        for &arm in &node.arms {
            self.walk(arm, path, paths);
        }
        if !node.items.is_empty() {
            let items_path = self.program.paths.child(path, PathStep::Items);
            for &item in &node.items {
                self.walk(item, items_path, paths);
            }
        }
        if !node.any_key.is_empty() {
            let any_key_path = self.program.paths.child(path, PathStep::AnyKey);
            for &schema in &node.any_key {
                self.walk(schema, any_key_path, paths);
            }
        }
    }
}

/// Sorted paths joined by `+`, the tail past [`MEANING_LIST_CHARS`] counted
/// as `+Nmore`.
fn render_path_list<'p>(paths: impl ExactSizeIterator<Item = &'p str>) -> String {
    let total = paths.len();
    let mut rendered = String::new();
    let mut listed = 0;
    for path in paths {
        if listed > 0 && rendered.len() + 1 + path.len() > MEANING_LIST_CHARS {
            break;
        }
        if listed > 0 {
            rendered.push('+');
        }
        rendered.push_str(path);
        listed += 1;
    }
    if listed < total {
        rendered = format!("{rendered}+{}more", total - listed);
    }
    rendered
}

/// A stated value, its text cut to [`MEANING_VALUE_CHARS`] characters.
fn render_value(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Null | Value::Bool(_) | Value::Number(_) => return value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    };
    if text.chars().count() <= MEANING_VALUE_CHARS {
        return encode_key(&text);
    }
    let cut = text.chars().take(MEANING_VALUE_CHARS).collect::<String>();
    encode_key(&format!("{cut}..."))
}

/// Up to three values joined by `,`, the rest counted.
fn render_values(values: &[Value]) -> String {
    let mut rendered = values
        .iter()
        .take(3)
        .map(render_value)
        .collect::<Vec<_>>()
        .join(",");
    if values.len() > 3 {
        rendered = format!("{rendered},+{}more", values.len() - 3);
    }
    rendered
}

/// Destination names: the path of the first reference in a canonical
/// traversal that visits the document before the definitions, and each
/// definition once, in the order it is first reached.
fn destination_names(
    schema: &Value,
    definitions: &Map<String, Value>,
    anonymous: &BTreeMap<String, &[DefinitionOrigin]>,
) -> BTreeMap<String, String> {
    let mut taken = TakenNames::default();
    for name in definitions.keys() {
        if !anonymous.contains_key(name) {
            taken.insert(name.clone());
        }
    }
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
                taken.unique(root.to_string())
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
                None,
                &mut sites,
            );
            for Site { steps, target, .. } in sites {
                if !reached.insert(target) {
                    continue;
                }
                let name = if anonymous.contains_key(target) {
                    let mut path = join_path(&prefix, &steps, container.is_none());
                    if path.is_empty() {
                        path.push_str("@root");
                    }
                    taken.unique(path)
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

/// A plain reference to a root definition, as reached from its container.
struct Site<'a> {
    /// The rendered path from the container to the reference.
    steps: String,
    /// Length of `steps` before its first array index, when it has one.
    ///
    /// An index names a position in `allOf`/`anyOf`/…, which shifts when an
    /// unrelated arm is inserted, so a path through one is not a stable name.
    first_index: Option<usize>,
    /// The referenced definition.
    target: &'a str,
}

/// Collects every plain reference to a root definition below `schema`, in
/// canonical order: object keys sorted, array items in order.
///
/// The root `$defs` of the document is skipped when `is_document_root`.
fn collect_references<'a>(
    schema: &Value,
    definitions: &'a Map<String, Value>,
    is_document_root: bool,
    steps: &mut String,
    first_index: Option<usize>,
    sites: &mut Vec<Site<'a>>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some(parsed) = parse_definition_reference(reference)
        && parsed.suffix.is_empty()
        && let Some((target, _)) = definitions.get_key_value(&parsed.name)
    {
        sites.push(Site {
            steps: steps.clone(),
            first_index,
            target: target.as_str(),
        });
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
                    let first_index = first_index.or(Some(length));
                    collect_child(item, definitions, steps, first_index, sites);
                    steps.truncate(length);
                }
            }
            (SchemaTraversalContext::Schema, _) => {
                push_step(steps, key, None);
                collect_child(value, definitions, steps, first_index, sites);
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
                    collect_child(entry, definitions, steps, first_index, sites);
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
    first_index: Option<usize>,
    sites: &mut Vec<Site<'a>>,
) {
    if !is_nested_resource(schema) {
        collect_references(schema, definitions, false, steps, first_index, sites);
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
        .map(unescape_json_pointer_segment)
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
    let mut encoded = String::new();
    push_encoded_key(&mut encoded, key);
    encoded
}

/// Appends `key` as [`encode_key`] spells it.
fn push_encoded_key(out: &mut String, key: &str) {
    if !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        out.push_str(key);
        return;
    }
    out.push('\'');
    for character in key.chars() {
        if character.is_ascii_alphanumeric() || "-._~!$&*+,;=:@/?".contains(character) {
            out.push(character);
        } else {
            push_escaped(out, character);
        }
    }
    out.push('\'');
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

/// The names given so far.
///
/// Names are only ever added, so every ordinal below a name's next ordinal
/// stays taken, and each name's ordinals are probed once.
#[derive(Default)]
struct TakenNames {
    names: AHashSet<String>,
    next_ordinal: AHashMap<String, usize>,
}

impl TakenNames {
    fn insert(&mut self, name: String) {
        self.names.insert(name);
    }

    /// `name`, or `name@2`, `name@3`, … when it is taken; records the result.
    fn unique(&mut self, name: String) -> String {
        if self.names.insert(name.clone()) {
            return name;
        }
        let ordinal = self.next_ordinal.entry(name.clone()).or_insert(2);
        loop {
            let candidate = format!("{name}@{ordinal}");
            *ordinal += 1;
            if self.names.insert(candidate.clone()) {
                return candidate;
            }
        }
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
        && let Some(parsed) = parse_definition_reference(reference)
        && let Some(renamed) = renames.get(&parsed.name)
    {
        *reference = DefinitionReference {
            name: renamed.clone(),
            suffix: parsed.suffix,
        }
        .to_reference();
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

/// Counts references per root definition; `false` when one is not a
/// same-document JSON Pointer.
fn count_definition_references(schema: &Value, counts: &mut BTreeMap<String, usize>) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let Some(fragment) = local_reference_fragment(reference) else {
            return false;
        };
        if !(fragment.is_empty() || fragment.starts_with('/')) {
            return false;
        }
        if let Some(parsed) = parse_definition_reference(reference)
            && let Some(count) = counts.get_mut(&parsed.name)
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
