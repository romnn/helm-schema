//! Stable definition names and the one simultaneous definition rename.
//!
//! Readable output names a generated definition after the SHA-256 digest of
//! its content.
//! The digest looks through references to other generated definitions, so a
//! subtree hashes the same inline or extracted.
//!
//! Every other definition (named helpers, provider definitions, caller-owned
//! names) is an identity boundary: a reference to it hashes as its name.
//! Editing a helper's body therefore does not rename the definitions that
//! reference it.
//!
//! A name is stable while the definition's content, the names of the
//! boundaries it references, the order of its `allOf`/`anyOf` arms, and the
//! set of occupied names are unchanged.
//! Two exceptions follow from that.
//! Arm order follows the private handles of provider definitions, so an
//! unrelated provider candidate can shift handles, reorder arms, and rename
//! the definitions enclosing them.
//! Equal digests receive `-2`, `-3`, … suffixes in the lexical order of their
//! private ids, which an unrelated insertion can change.
//! Collision disambiguation can also lengthen a name when another digest
//! starts with the same digits.
//!
//! Shipped output may instead use short frequency-ranked names, a bijective
//! rename of the same graph applied by [`rename_definitions`].

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_json_schema_walk::{
    ReferenceSiblings, SchemaTraversalContext, escape_json_pointer_segment,
    schema_child_context_for_keyword, visit_subschemas, visit_subschemas_mut,
};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::{
    DEFINITION_REF_PREFIX, DEFINITIONS_KEY, base62, has_reference_scope, is_nested_resource,
    protected_reference_targets,
};

/// Keywords whose identifiers or dynamic resolution the shipping rename does
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

/// Hex digits of the content digest in a readable name, before any collision
/// extension.
const CONTENT_NAME_DIGITS: usize = 12;

/// Prefix of readable names for definitions the minifier extracts.
const GENERATED_NAME_PREFIX: &str = "h";

/// Lowercase hex SHA-256 digest of a schema's content.
///
/// Object keys are hashed in sorted order; array order, descriptions, and
/// every other keyword payload are part of the content.
#[must_use]
pub fn content_digest(schema: &Value) -> String {
    let definitions = Map::new();
    let generated = BTreeSet::new();
    let mut hasher = ContentHasher::new(&definitions, &generated);
    hex(&hasher.schema_digest(schema))
}

/// Names each id `prefix` plus the first twelve hex digits of its digest.
///
/// Only ids whose twelve-digit prefixes collide get longer names: each
/// extends to the shortest prefix that separates it from every other digest.
/// Names in `taken` are never produced, and equal digests stay distinct.
#[must_use]
pub fn content_names(
    prefix: &str,
    digests: &BTreeMap<String, String>,
    taken: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut sorted = digests.values().map(String::as_str).collect::<Vec<_>>();
    sorted.sort_unstable();
    let mut used = BTreeSet::new();
    let mut names = BTreeMap::new();
    for (id, digest) in digests {
        // Sorted order puts the digests sharing the longest prefix with this
        // one right next to it; an equal digest sits right after it.
        let position = sorted.partition_point(|other| *other < digest.as_str());
        let mut shared = 0;
        if let Some(previous) = position.checked_sub(1).and_then(|index| sorted.get(index)) {
            shared = common_prefix_len(previous, digest);
        }
        if let Some(next) = sorted.get(position + 1) {
            shared = shared.max(common_prefix_len(next, digest));
        }
        let mut length = CONTENT_NAME_DIGITS.max(shared + 1).min(digest.len());
        let mut name = format!("{prefix}{}", digest.get(..length).unwrap_or(digest));
        while (taken.contains(&name) || used.contains(&name)) && length < digest.len() {
            length += 1;
            name = format!("{prefix}{}", digest.get(..length).unwrap_or(digest));
        }
        let mut ordinal = 2;
        while taken.contains(&name) || used.contains(&name) {
            name = format!("{prefix}{digest}-{ordinal}");
            ordinal += 1;
        }
        used.insert(name.clone());
        names.insert(id.clone(), name);
    }
    names
}

/// Renames private definition handles to their final names.
///
/// A caller that extracts definitions under private handles hands their final
/// names in here, after every extraction decision.
/// Entries whose handle is no longer a root definition, or whose name is
/// already taken, are skipped.
pub fn name_private_definitions(schema: &mut Value, names: &BTreeMap<String, String>) {
    let Some(definitions) = schema.get(DEFINITIONS_KEY).and_then(Value::as_object) else {
        return;
    };
    let mut renames = BTreeMap::new();
    for (handle, name) in names {
        if definitions.contains_key(handle) && !definitions.contains_key(name) {
            renames.insert(handle.clone(), name.clone());
        }
    }
    rename_definitions(schema, &renames);
}

/// Readable names for the generated root definitions of `schema`.
pub(crate) fn readable_definition_names(
    schema: &Value,
    generated: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let Some(definitions) = schema.get(DEFINITIONS_KEY).and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    let mut hasher = ContentHasher::new(definitions, generated);
    let mut digests = BTreeMap::new();
    for name in generated {
        digests.insert(name.clone(), hex(&hasher.definition_digest(name)));
    }
    let taken = definitions
        .keys()
        .filter(|name| !generated.contains(*name))
        .cloned()
        .collect();
    content_names(GENERATED_NAME_PREFIX, &digests, &taken)
}

/// Short shipping names for every root definition of `schema`.
///
/// The most referenced definition receives the shortest base-62 name.
/// The map is empty, so nothing is renamed, unless every reference is a plain
/// resolvable `#/…` pointer: a non-local, unresolved, anchor or
/// percent-encoded reference, a dynamic or recursive reference, or a nested
/// identifier or anchor keeps the document's names.
#[must_use]
pub fn shipping_definition_names(schema: &Value) -> BTreeMap<String, String> {
    let Some(definitions) = schema.get(DEFINITIONS_KEY).and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    if protected_reference_targets(schema).is_none() || uses_unmodelled_references(schema, true) {
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

/// Whether a schema uses a keyword the shipping rename does not model; the
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

type Digest = [u8; 32];

/// Content digests of one document's root definitions.
struct ContentHasher<'a> {
    definitions: &'a Map<String, Value>,
    generated: &'a BTreeSet<String>,
    digests: BTreeMap<String, Digest>,
    /// Generated definitions whose digest is being computed.
    active: BTreeSet<String>,
}

impl<'a> ContentHasher<'a> {
    fn new(definitions: &'a Map<String, Value>, generated: &'a BTreeSet<String>) -> Self {
        Self {
            definitions,
            generated,
            digests: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }

    fn definition_digest(&mut self, name: &str) -> Digest {
        if let Some(digest) = self.digests.get(name) {
            return *digest;
        }
        let Some(body) = self.definitions.get(name) else {
            return literal_digest(name);
        };
        // A generated body references only strictly smaller extracted
        // subtrees, so generated definitions form a DAG.
        // This guard is an internal-invariant fallback: should that
        // invariant break, it stops unbounded recursion by hashing the edge
        // as its literal name, and names then depend on traversal context.
        if !self.active.insert(name.to_string()) {
            return literal_digest(name);
        }
        let mut hasher = Sha256::new();
        self.update(&mut hasher, body, SchemaTraversalContext::Schema);
        let digest: Digest = hasher.finalize().into();
        self.active.remove(name);
        self.digests.insert(name.to_string(), digest);
        digest
    }

    fn schema_digest(&mut self, schema: &Value) -> Digest {
        if let Some(name) = self.generated_reference(schema) {
            return self.definition_digest(name);
        }
        let mut hasher = Sha256::new();
        self.update(&mut hasher, schema, SchemaTraversalContext::Schema);
        hasher.finalize().into()
    }

    /// The generated definition `schema` stands for, when it is exactly a
    /// reference to one.
    fn generated_reference<'s>(&self, schema: &'s Value) -> Option<&'s str> {
        let object = schema.as_object()?;
        if object.len() != 1 {
            return None;
        }
        let name = object
            .get("$ref")?
            .as_str()?
            .strip_prefix(DEFINITION_REF_PREFIX)?;
        self.generated.contains(name).then_some(name)
    }

    fn update(&mut self, hasher: &mut Sha256, value: &Value, context: SchemaTraversalContext) {
        match value {
            Value::Null => hasher.update(b"z"),
            Value::Bool(true) => hasher.update(b"t"),
            Value::Bool(false) => hasher.update(b"f"),
            Value::Number(number) => {
                hasher.update(b"n");
                update_text(hasher, &number.to_string());
            }
            Value::String(text) => {
                hasher.update(b"s");
                update_text(hasher, text);
            }
            Value::Array(items) => {
                hasher.update(b"a");
                update_len(hasher, items.len());
                let item_context = match context {
                    SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                        SchemaTraversalContext::Schema
                    }
                    SchemaTraversalContext::SchemaMapValues => {
                        SchemaTraversalContext::SchemaMapValues
                    }
                    SchemaTraversalContext::Ref | SchemaTraversalContext::Data => {
                        SchemaTraversalContext::Data
                    }
                };
                for item in items {
                    self.update_child(hasher, item, item_context);
                }
            }
            Value::Object(object) => {
                hasher.update(b"o");
                update_len(hasher, object.len());
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by_key(|(key, _)| *key);
                for (key, child) in entries {
                    update_text(hasher, key);
                    let child_context = match context {
                        SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                            schema_child_context_for_keyword(key)
                        }
                        SchemaTraversalContext::SchemaMapValues => SchemaTraversalContext::Schema,
                        SchemaTraversalContext::Ref | SchemaTraversalContext::Data => {
                            SchemaTraversalContext::Data
                        }
                    };
                    self.update_child(hasher, child, child_context);
                }
            }
        }
    }

    /// Hashes a schema-position object as its own digest, so an inline
    /// subtree and a reference to its extracted definition hash the same.
    fn update_child(
        &mut self,
        hasher: &mut Sha256,
        value: &Value,
        context: SchemaTraversalContext,
    ) {
        if context != SchemaTraversalContext::Schema || !value.is_object() {
            self.update(hasher, value, context);
            return;
        }
        // Scoped subtrees are never extracted and resolve their own
        // references, so they hash literally.
        let digest = if has_reference_scope(value) {
            let mut scoped = Sha256::new();
            self.update(&mut scoped, value, SchemaTraversalContext::Data);
            scoped.finalize().into()
        } else {
            self.schema_digest(value)
        };
        hasher.update(b"#");
        hasher.update(digest);
    }
}

fn literal_digest(name: &str) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"literal");
    update_text(&mut hasher, name);
    hasher.finalize().into()
}

fn update_len(hasher: &mut Sha256, len: usize) {
    hasher.update(u64::try_from(len).unwrap_or(u64::MAX).to_be_bytes());
}

fn update_text(hasher: &mut Sha256, text: &str) {
    update_len(hasher, text.len());
    hasher.update(text.as_bytes());
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    left.bytes()
        .zip(right.bytes())
        .take_while(|(left, right)| left == right)
        .count()
}

fn hex(digest: &Digest) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        for nibble in [byte >> 4, byte & 0x0f] {
            if let Some(digit) = DIGITS.get(usize::from(nibble)) {
                text.push(char::from(*digit));
            }
        }
    }
    text
}
