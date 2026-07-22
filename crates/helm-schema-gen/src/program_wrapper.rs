//! Wrapper alternatives for chart-authored values-program conventions.
//!
//! A chart whose engine rewrites its values tree (nats' `$tplYaml`) accepts
//! a singleton `{KEY: PROGRAM}` map at ANY node: the engine executes the
//! program with `tpl`, reparses the output as YAML, and substitutes the
//! typed result before consumers read the tree. Every value-position schema
//! node therefore gains a wrapper alternative.
//!
//! Program strings are constrained by what the engine does with the result:
//!
//! - A REPLACE sentinel substitutes the decoded result at the node, so a
//!   static program (no template action) whose decoded kind certainly falls
//!   outside the node's accepted kinds is rejected; dynamic programs and
//!   lexically ambiguous static programs stay an explicit open alternative.
//! - A SPREAD sentinel splices the decoded result into the PARENT
//!   collection: a scalar result always aborts (only maps spread onto maps
//!   and slices onto slices; a null result is a no-op removal), a map
//!   result aborts at list-item edges, a slice result aborts at map-member
//!   edges, and the values root rejects the spread wrapper outright.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use helm_schema_core::ValuesProgramWrapper;
use serde_json::{Map, Value, json};

use crate::resolve_policy::{
    PLAIN_SCALAR_BOOL_TOKEN_PATTERN, PLAIN_SCALAR_NULL_TOKEN_PATTERN,
    PLAIN_SCALAR_NUMBER_TOKEN_PATTERN, PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN,
};

/// The schema edge a wrapped node hangs from, which is the wrapper engine's
/// parent collection kind: spread results must be slices at item edges and
/// maps at member edges. Shared `$defs` payloads are reachable from either
/// edge kind, so only edge-independent rules apply there.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WrapperEdge {
    Member,
    Item,
    Unknown,
}

pub(crate) fn apply_program_wrapper_alternatives(
    root: &mut Value,
    wrappers: &BTreeSet<ValuesProgramWrapper>,
    exclusions: &BTreeSet<String>,
) {
    if wrappers.is_empty() {
        return;
    }
    let mut keys_by_scope: BTreeMap<&str, BTreeMap<&str, bool>> = BTreeMap::new();
    for wrapper in wrappers {
        // A key claimed as both replace and spread (two engines sharing a
        // sentinel) degrades to replace: spread adds rejections, and an
        // uncertain classification must not reject what one engine accepts.
        keys_by_scope
            .entry(wrapper.scope_path.as_str())
            .or_default()
            .entry(wrapper.key.as_str())
            .and_modify(|spread| *spread &= wrapper.spread)
            .or_insert(wrapper.spread);
    }
    // A strict pre-rewrite consumer aborts on a wrapper map before the
    // engine substitutes it, so its exact property path skips the wrapper
    // alternative (nats' `nameOverride` through `fullname | trunc`). The
    // exclusion applies along literal property chains only — nodes reached
    // through `additionalProperties`, `items`, or shared `$defs` have no
    // stable path and keep their alternatives.
    let excluded: BTreeSet<Vec<String>> = exclusions
        .iter()
        .map(|path| crate::split_value_path(path))
        .collect();
    for (scope, keys) in keys_by_scope {
        let keys: Vec<(&str, bool)> = keys.into_iter().collect();
        if scope.is_empty() {
            rewrite_document(root, &keys, &excluded);
            wrap_document_root(root, &keys);
            reject_root_spread_wrappers(root, &keys);
        } else if let Some(node) = properties_node_mut(root, scope) {
            let scope_path = crate::split_value_path(scope);
            rewrite_value_edges(node, &keys, Some(&scope_path), &excluded);
            reject_root_spread_wrappers(node, &keys);
        }
    }
}

/// The engine's recursion starts at the values document itself, so the
/// ROOT may be a singleton REPLACE wrapper (`{"$tplYaml": program}`; the
/// spread form is rejected separately). Union the root's own value domain
/// with the wrapper alternative, leaving `$schema`/`$defs` and the
/// conditional `allOf` arms as top-level conjuncts — they constrain the
/// REWRITTEN tree and evaluate vacuously against the raw singleton form.
/// Children were already wrapped by `rewrite_document`, so no recursion.
fn wrap_document_root(root: &mut Value, keys: &[(&str, bool)]) {
    const VALUE_DOMAIN: [&str; 5] = [
        "type",
        "properties",
        "additionalProperties",
        "patternProperties",
        "anyOf",
    ];
    let Some(object) = root.as_object_mut() else {
        return;
    };
    let mut domain = serde_json::Map::new();
    for keyword in VALUE_DOMAIN {
        if let Some(child) = object.remove(keyword) {
            domain.insert(keyword.to_string(), child);
        }
    }
    if domain.is_empty() {
        return;
    }
    let node = Value::Object(domain);
    if schema_accepts_everything(&node) {
        if let Value::Object(domain) = node {
            object.extend(domain);
        }
        return;
    }
    let alternative = wrapper_schema(&node, keys, WrapperEdge::Unknown);
    let ordinary = if accepted_kinds(&node).object {
        json!({ "allOf": [node, { "not": sentinel_singleton_shape(keys) }] })
    } else {
        node
    };
    object.insert(
        "anyOf".to_string(),
        Value::Array(vec![ordinary, alternative]),
    );
}

/// The schema node declared for `scope` under the document's base
/// properties tree, when every segment resolves.
fn properties_node_mut<'a>(root: &'a mut Value, scope: &str) -> Option<&'a mut Value> {
    let mut node = root;
    for segment in crate::split_value_path(scope) {
        node = node.get_mut("properties")?.get_mut(&segment)?;
    }
    Some(node)
}

/// Root-document edges: the base properties tree, conditional arms, and
/// shared definitions. Definitions encoding TESTS rather than accepted
/// values (helm truthiness, quoted-content grammars) stay untouched — a
/// wrapper alternative inside a test would change what the test means.
fn rewrite_document(root: &mut Value, keys: &[(&str, bool)], excluded: &BTreeSet<Vec<String>>) {
    let Some(object) = root.as_object_mut() else {
        return;
    };
    for (keyword, child) in object.iter_mut() {
        match keyword.as_str() {
            "properties" | "patternProperties" => {
                wrap_member_values(child, keys, Some(&[]), excluded);
            }
            "additionalProperties" if child.is_object() => {
                wrap_node(child, keys, WrapperEdge::Member, excluded);
            }
            "allOf" | "anyOf" | "oneOf" => {
                if let Some(arms) = child.as_array_mut() {
                    for arm in arms {
                        rewrite_value_edges(arm, keys, Some(&[]), excluded);
                    }
                }
            }
            "$defs" => {
                if let Some(definitions) = child.as_object_mut() {
                    for (name, definition) in definitions.iter_mut() {
                        if name.starts_with("helm-") {
                            continue;
                        }
                        wrap_node(definition, keys, WrapperEdge::Unknown, excluded);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The engine refuses to spread at the recursion root (`cannot
/// $tplYamlSpread on root object`), so the scope's own document — before
/// any descent — must not be a singleton spread wrapper.
fn reject_root_spread_wrappers(root: &mut Value, keys: &[(&str, bool)]) {
    let spread_keys: Vec<&str> = keys
        .iter()
        .filter(|(_, spread)| *spread)
        .map(|(key, _)| *key)
        .collect();
    if spread_keys.is_empty() {
        return;
    }
    let Some(object) = root.as_object_mut() else {
        return;
    };
    let conjuncts = object
        .entry("allOf")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(conjuncts) = conjuncts.as_array_mut() {
        for key in spread_keys {
            conjuncts.push(json!({
                "not": {
                    "type": "object",
                    "maxProperties": 1,
                    "required": [key],
                }
            }));
        }
    }
}

/// Descend one value node's value-position edges, wrapping each child.
/// Condition-position keywords (`if`, `not`, `propertyNames`) encode tests
/// and are left alone; union alternatives recurse without a wholesale wrap
/// because the node owning the union is wrapped at ITS edge.
fn rewrite_value_edges(
    node: &mut Value,
    keys: &[(&str, bool)],
    path: Option<&[String]>,
    excluded: &BTreeSet<Vec<String>>,
) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    for (keyword, child) in object.iter_mut() {
        match keyword.as_str() {
            "properties" | "patternProperties" => {
                wrap_member_values(child, keys, path, excluded);
            }
            "additionalProperties" if child.is_object() => {
                wrap_node(child, keys, WrapperEdge::Member, excluded);
            }
            "items" if child.is_object() => wrap_node(child, keys, WrapperEdge::Item, excluded),
            "anyOf" | "allOf" | "oneOf" => {
                if let Some(alternatives) = child.as_array_mut() {
                    for alternative in alternatives {
                        rewrite_value_edges(alternative, keys, path, excluded);
                    }
                }
            }
            "then" | "else" => rewrite_value_edges(child, keys, path, excluded),
            "$defs" | "definitions" => {
                if let Some(definitions) = child.as_object_mut() {
                    for definition in definitions.values_mut() {
                        wrap_node(definition, keys, WrapperEdge::Unknown, excluded);
                    }
                }
            }
            _ => {}
        }
    }
}

fn wrap_member_values(
    members: &mut Value,
    keys: &[(&str, bool)],
    path: Option<&[String]>,
    excluded: &BTreeSet<Vec<String>>,
) {
    if let Some(members) = members.as_object_mut() {
        for (name, member) in members.iter_mut() {
            let member_path = path.map(|path| {
                let mut member_path = path.to_vec();
                member_path.push(name.clone());
                member_path
            });
            if member_path
                .as_ref()
                .is_some_and(|member_path| excluded.contains(member_path))
            {
                // A strict pre-rewrite consumer aborts on the raw wrapper
                // map here; children keep their alternatives.
                rewrite_value_edges(member, keys, member_path.as_deref(), excluded);
                continue;
            }
            wrap_node_at(
                member,
                keys,
                WrapperEdge::Member,
                member_path.as_deref(),
                excluded,
            );
        }
    }
}

/// Recurse into a value node's edges, then union the node with the wrapper
/// alternative. Nodes that already accept everything gain nothing. The
/// engine intercepts singleton sentinel maps unconditionally, so a node
/// whose ordinary domain accepts objects must NOT let the raw wrapper map
/// ride that domain — the wrapper alternative's program constraint is the
/// only lane a wrapper map may take.
fn wrap_node(
    node: &mut Value,
    keys: &[(&str, bool)],
    edge: WrapperEdge,
    excluded: &BTreeSet<Vec<String>>,
) {
    // Edges without a stable property path (items, additionalProperties,
    // shared definitions) descend outside the exclusion namespace.
    wrap_node_at(node, keys, edge, None, excluded);
}

/// Recurse into a value node's edges, then union the node with the wrapper
/// alternative (see [`wrap_node`]); `path` names the node's own property
/// chain for descendant exclusion checks.
fn wrap_node_at(
    node: &mut Value,
    keys: &[(&str, bool)],
    edge: WrapperEdge,
    path: Option<&[String]>,
    excluded: &BTreeSet<Vec<String>>,
) {
    rewrite_value_edges(node, keys, path, excluded);
    if schema_accepts_everything(node) {
        return;
    }
    let alternative = wrapper_schema(node, keys, edge);
    let original = std::mem::take(node);
    let ordinary = if accepted_kinds(&original).object {
        json!({ "allOf": [original, { "not": sentinel_singleton_shape(keys) }] })
    } else {
        original
    };
    *node = json!({ "anyOf": [ordinary, alternative] });
}

/// The instance shape the engine intercepts: a map whose single member is
/// one of the sentinel keys, regardless of the program's kind (`tpl`
/// aborts later on non-string programs).
fn sentinel_singleton_shape(keys: &[(&str, bool)]) -> Value {
    let names: Vec<&str> = keys.iter().map(|(key, _)| *key).collect();
    json!({
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "propertyNames": { "enum": names },
    })
}

fn schema_accepts_everything(node: &Value) -> bool {
    match node {
        Value::Bool(accepts) => *accepts,
        Value::Object(object) => object
            .keys()
            .all(|keyword| matches!(keyword.as_str(), "description" | "default")),
        _ => false,
    }
}

/// The singleton wrapper-map alternative for one node: exactly one sentinel
/// member whose value is the program string, constrained per sentinel by
/// what the engine does with the decoded result.
fn wrapper_schema(node: &Value, keys: &[(&str, bool)], edge: WrapperEdge) -> Value {
    let mut properties = Map::new();
    for (key, spread) in keys {
        let program = if *spread {
            spread_program_schema(edge)
        } else {
            replace_program_schema(node)
        };
        properties.insert((*key).to_string(), program);
    }
    json!({
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "additionalProperties": false,
        "properties": properties,
    })
}

/// A replace sentinel substitutes the decoded program result at the node.
/// A static program's YAML decoding must inhabit the node: pure
/// integer-typed nodes accept exactly the integer-literal lexemes, while
/// other typed nodes reject only programs whose decoded kind is CERTAIN to
/// fall outside the node's accepted kinds. Dynamic programs stay open.
fn replace_program_schema(node: &Value) -> Value {
    if node.get("type").and_then(Value::as_str) == Some("integer") {
        return json!({
            "type": "string",
            "pattern": "(^[+-]?(0x[0-9A-Fa-f]+|0o[0-7]+|[0-9]+)$)|\\{\\{",
        });
    }
    let kinds = accepted_kinds(node);
    if kinds == Kinds::all() {
        return json!({ "type": "string" });
    }
    let accepts_numeric = kinds.integer || kinds.number;
    let mut excluded = Vec::new();
    if !kinds.object {
        excluded.push(FLOW_MAP_START.to_string());
    }
    if !kinds.array {
        excluded.push(FLOW_SEQ_START.to_string());
        excluded.push(BLOCK_SEQ_START.to_string());
    }
    if !accepts_numeric {
        excluded.push(padded_token(PLAIN_SCALAR_NUMBER_TOKEN_PATTERN));
        excluded.push(padded_token(PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN));
    }
    if !kinds.boolean {
        excluded.push(padded_token(PLAIN_SCALAR_BOOL_TOKEN_PATTERN));
    }
    if !kinds.null {
        excluded.push(padded_token(PLAIN_SCALAR_NULL_TOKEN_PATTERN));
    }
    if !kinds.string {
        excluded.push(QUOTED_DOUBLE.to_string());
        excluded.push(QUOTED_SINGLE.to_string());
    }
    // A bare word is certainly one of string/number/bool/null, so once all
    // four are unacceptable the whole class rejects; accepted null
    // spellings are rescued below.
    let plain_word_excluded = !kinds.string && !accepts_numeric && !kinds.boolean;
    if plain_word_excluded {
        excluded.push(PLAIN_WORD.to_string());
    }
    program_schema_excluding(&excluded, plain_word_excluded && kinds.null)
}

/// A spread sentinel splices the decoded result into the parent: scalar
/// results always abort, maps only spread onto maps (member edges) and
/// slices onto slices (item edges); a null result is a no-op removal.
fn spread_program_schema(edge: WrapperEdge) -> Value {
    let mut excluded = vec![
        padded_token(PLAIN_SCALAR_NUMBER_TOKEN_PATTERN),
        padded_token(PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN),
        padded_token(PLAIN_SCALAR_BOOL_TOKEN_PATTERN),
        QUOTED_DOUBLE.to_string(),
        QUOTED_SINGLE.to_string(),
        PLAIN_WORD.to_string(),
    ];
    match edge {
        WrapperEdge::Member => {
            excluded.push(FLOW_SEQ_START.to_string());
            excluded.push(BLOCK_SEQ_START.to_string());
        }
        WrapperEdge::Item => excluded.push(FLOW_MAP_START.to_string()),
        WrapperEdge::Unknown => {}
    }
    program_schema_excluding(&excluded, true)
}

/// A program string rejecting the certainly-incompatible static lexeme
/// classes. `rescue_null` re-admits null spellings swallowed by the bare
/// word class when a null result is acceptable.
fn program_schema_excluding(excluded: &[String], rescue_null: bool) -> Value {
    if excluded.is_empty() {
        return json!({ "type": "string" });
    }
    let pattern = excluded.join("|");
    if rescue_null {
        json!({
            "type": "string",
            "anyOf": [
                { "pattern": padded_token(PLAIN_SCALAR_NULL_TOKEN_PATTERN) },
                { "not": { "pattern": pattern } },
            ],
        })
    } else {
        json!({ "type": "string", "not": { "pattern": pattern } })
    }
}

/// A flow indicator as the program's first significant character fixes the
/// decoded kind regardless of any template action later in the text; `{{`
/// is a template action, not a flow mapping.
const FLOW_MAP_START: &str = r"^[ \t\r\n]*\{(?:[^{]|$)";
const FLOW_SEQ_START: &str = r"^[ \t\r\n]*\[";
const BLOCK_SEQ_START: &str = r"^[ \t\r\n]*-(?:[ \t\r\n]|$)";
/// One bare plain-scalar word: certainly a string or a resolver token,
/// never a map, slice, or template action.
const PLAIN_WORD: &str = r"^[ \t\r\n]*[0-9A-Za-z_./+-]+[ \t\r\n]*$";
/// A complete quoted scalar decodes as a string even when a template
/// action renders inside the quotes.
const QUOTED_DOUBLE: &str = r#"^[ \t\r\n]*"[^"]*"[ \t\r\n]*$"#;
const QUOTED_SINGLE: &str = r"^[ \t\r\n]*'[^']*'[ \t\r\n]*$";

/// An anchored resolver-token grammar, re-anchored to tolerate the
/// whitespace padding the engine's reparse strips.
fn padded_token(anchored: &str) -> String {
    let inner = anchored
        .strip_prefix('^')
        .and_then(|token| token.strip_suffix('$'))
        .unwrap_or(anchored);
    format!(r"^[ \t\r\n]*(?:{inner})[ \t\r\n]*$")
}

/// The set of top-level instance kinds a node's schema can accept.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Kinds {
    object: bool,
    array: bool,
    string: bool,
    number: bool,
    integer: bool,
    boolean: bool,
    null: bool,
}

impl Kinds {
    const fn all() -> Self {
        Self {
            object: true,
            array: true,
            string: true,
            number: true,
            integer: true,
            boolean: true,
            null: true,
        }
    }

    const fn none() -> Self {
        Self {
            object: false,
            array: false,
            string: false,
            number: false,
            integer: false,
            boolean: false,
            null: false,
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            object: self.object && other.object,
            array: self.array && other.array,
            string: self.string && other.string,
            number: self.number && other.number,
            integer: self.integer && other.integer,
            boolean: self.boolean && other.boolean,
            null: self.null && other.null,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            object: self.object || other.object,
            array: self.array || other.array,
            string: self.string || other.string,
            number: self.number || other.number,
            integer: self.integer || other.integer,
            boolean: self.boolean || other.boolean,
            null: self.null || other.null,
        }
    }
}

/// Kinds accepted by a node, from its explicit `type`, `enum`/`const`
/// values, and combinator arms. Anything unrecognized — `$ref`, bare
/// keyword-less nodes — abstains to "all kinds" so the wrapper program
/// never rejects more than the node provably does.
fn accepted_kinds(node: &Value) -> Kinds {
    let object = match node {
        Value::Bool(accepts) => {
            return if *accepts {
                Kinds::all()
            } else {
                Kinds::none()
            };
        }
        Value::Object(object) => object,
        _ => return Kinds::all(),
    };
    if object.contains_key("$ref") {
        return Kinds::all();
    }
    let mut kinds = Kinds::all();
    if let Some(type_value) = object.get("type") {
        kinds = kinds.intersect(kinds_from_type(type_value));
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        kinds = kinds.intersect(
            values
                .iter()
                .fold(Kinds::none(), |acc, value| acc.union(kind_of_value(value))),
        );
    }
    if let Some(value) = object.get("const") {
        kinds = kinds.intersect(kind_of_value(value));
    }
    for combinator in ["anyOf", "oneOf"] {
        if let Some(arms) = object.get(combinator).and_then(Value::as_array)
            && !arms.is_empty()
        {
            kinds = kinds.intersect(
                arms.iter()
                    .fold(Kinds::none(), |acc, arm| acc.union(accepted_kinds(arm))),
            );
        }
    }
    if let Some(arms) = object.get("allOf").and_then(Value::as_array) {
        for arm in arms {
            kinds = kinds.intersect(accepted_kinds(arm));
        }
    }
    kinds
}

fn kinds_from_type(type_value: &Value) -> Kinds {
    match type_value {
        Value::String(name) => kinds_from_type_name(name),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .fold(Kinds::none(), |acc, name| {
                acc.union(kinds_from_type_name(name))
            }),
        _ => Kinds::all(),
    }
}

fn kinds_from_type_name(name: &str) -> Kinds {
    let mut kinds = Kinds::none();
    match name {
        "object" => kinds.object = true,
        "array" => kinds.array = true,
        "string" => kinds.string = true,
        "number" => {
            kinds.number = true;
            kinds.integer = true;
        }
        "integer" => kinds.integer = true,
        "boolean" => kinds.boolean = true,
        "null" => kinds.null = true,
        _ => return Kinds::all(),
    }
    kinds
}

fn kind_of_value(value: &Value) -> Kinds {
    let mut kinds = Kinds::none();
    match value {
        Value::Null => kinds.null = true,
        Value::Bool(_) => kinds.boolean = true,
        Value::Number(number) => {
            kinds.integer = number.is_i64() || number.is_u64();
            kinds.number = true;
        }
        Value::String(_) => kinds.string = true,
        Value::Array(_) => kinds.array = true,
        Value::Object(_) => kinds.object = true,
    }
    kinds
}
