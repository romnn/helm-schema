//! Lint constraints compiled part by part, each distinct part once per
//! generation.
//!
//! A constraint `if condition then then_value` is judged against the
//! document `{$defs: D, allOf: [{if: condition, then: then_value}]}`,
//! where `D` holds the provider and condition definitions the constraint
//! reaches and Helm truthiness when either references it. Its verdict is
//! a boolean expression over parts: draft-07 `if`/`then` holds unless the
//! condition holds and `then` fails; a sole `allOf`, `anyOf` or `not`
//! keyword is the conjunction, disjunction or negation of its members; a
//! sole `$ref` (draft-07 ignores its siblings anyway) is its target in `D`
//! when that body is a valid draft-07 schema, since meta-validation never
//! looks inside `$defs`.
//! Every other schema is a part, compiled as `{$defs: D', allOf: [part]}`
//! with `D'` the definitions of `D` it reaches. A part therefore depends
//! only on its text and on the names and sources of those definitions,
//! whose bodies never change once named, and is shared by every constraint
//! that reaches it alike.
//!
//! The constraint fails to compile exactly when one of its parts does:
//! draft-07 meta-validation checks the members of `allOf`, `anyOf` and
//! `not` as independent schemas and never looks inside `$defs`, every
//! reference a part resolves resolves to the same body, and every
//! definition of `D` is reached from some part. A constraint whose text or
//! definitions hold another reference or an identifier resolves it against
//! the whole document, so it is compiled whole.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use serde_json::Value;

use crate::condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};

/// The map a reachable definition's body is read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DefinitionSource {
    Provider,
    Condition,
    HelmTruthy,
}

/// The definitions of one constraint's document by name.
type Scope = BTreeMap<String, DefinitionSource>;

/// Everything a part's verdict and compilation depend on: its text and
/// each definition of the constraint's document it reaches.
#[derive(PartialEq, Eq, Hash)]
struct PartKey {
    schema: String,
    definitions: Vec<(String, DefinitionSource)>,
}

/// `if condition then then_value`, as the boolean structure of its parts,
/// or as one part when it addresses its own document.
pub(crate) enum ConstraintValidator {
    Split {
        condition: Rc<Judge>,
        then: Rc<Judge>,
    },
    Whole(Rc<Judge>),
}

impl ConstraintValidator {
    pub(crate) fn is_valid(&self, instance: &Value) -> bool {
        match self {
            Self::Split { condition, then } => {
                !condition.is_valid(instance) || then.is_valid(instance)
            }
            Self::Whole(constraint) => constraint.is_valid(instance),
        }
    }
}

/// A schema's verdict over its compiled parts.
enum Judge {
    Part(jsonschema::Validator),
    AllOf(Vec<Rc<Judge>>),
    AnyOf(Vec<Rc<Judge>>),
    Not(Rc<Judge>),
}

impl Judge {
    fn is_valid(&self, instance: &Value) -> bool {
        match self {
            Self::Part(validator) => validator.is_valid(instance),
            Self::AllOf(members) => members.iter().all(|member| member.is_valid(instance)),
            Self::AnyOf(members) => members.iter().any(|member| member.is_valid(instance)),
            Self::Not(inner) => !inner.is_valid(instance),
        }
    }
}

/// A boolean keyword standing alone in its schema.
enum Combinator<'v> {
    Junction(Junction, &'v [Value]),
    Not(&'v Value),
}

/// `allOf` or `anyOf`.
#[derive(Clone, Copy)]
enum Junction {
    AllOf,
    AnyOf,
}

impl Junction {
    fn keyword(self) -> &'static str {
        match self {
            Self::AllOf => "allOf",
            Self::AnyOf => "anyOf",
        }
    }

    fn judge(self, members: Vec<Rc<Judge>>) -> Judge {
        match self {
            Self::AllOf => Judge::AllOf(members),
            Self::AnyOf => Judge::AnyOf(members),
        }
    }
}

/// What a definition body references.
#[derive(Clone)]
struct BodyReferences {
    /// The root definition names it references.
    names: Rc<[String]>,
    /// Whether it holds a reference other than to a root definition or an
    /// identifier, which resolve against the document it sits in.
    document_relative: bool,
}

type DefinitionKey = (String, DefinitionSource);

/// The compiled parts of one generation's lint constraints.
pub(crate) struct LintValidators<'a> {
    providers: &'a BTreeMap<String, Value>,
    helm_truthy: Value,
    references: RefCell<HashMap<DefinitionKey, BodyReferences>>,
    /// Whether each definition body references Helm truthiness.
    references_helm_truthy: RefCell<HashMap<DefinitionKey, bool>>,
    /// Whether each definition body is a valid draft-07 schema.
    meta_valid: RefCell<HashMap<DefinitionKey, bool>>,
    parts: RefCell<HashMap<PartKey, Option<Rc<Judge>>>>,
}

impl<'a> LintValidators<'a> {
    pub(crate) fn new(providers: &'a BTreeMap<String, Value>) -> Self {
        Self {
            providers,
            helm_truthy: helm_truthy_definition_schema(),
            references: RefCell::new(HashMap::new()),
            references_helm_truthy: RefCell::new(HashMap::new()),
            meta_valid: RefCell::new(HashMap::new()),
            parts: RefCell::new(HashMap::new()),
        }
    }

    /// The validator of `if condition then then_value`, or `None` when it
    /// does not compile.
    ///
    /// A constraint whose text or definitions hold a reference other than
    /// to a root definition, or an identifier, resolves it against its
    /// whole document, so it is compiled whole.
    #[tracing::instrument(skip_all, fields(definitions = tracing::field::Empty))]
    pub(crate) fn constraint_validator(
        &self,
        condition: &Value,
        then_value: &Value,
        conditions: &BTreeMap<String, Value>,
    ) -> Option<ConstraintValidator> {
        let (scope, document_relative) = self.scope(condition, then_value, conditions);
        tracing::Span::current().record("definitions", scope.len());
        if document_relative {
            let constraint = serde_json::json!({ "if": condition, "then": then_value });
            return Some(ConstraintValidator::Whole(self.part(
                &constraint,
                &scope,
                conditions,
            )?));
        }
        let mut expanding = Vec::new();
        Some(ConstraintValidator::Split {
            condition: self.judge(condition, &scope, conditions, &mut expanding)?,
            then: self.judge(then_value, &scope, conditions, &mut expanding)?,
        })
    }

    /// The definitions of the constraint's document: the provider and the
    /// condition definitions it reaches, each within its own map, the
    /// condition's taking precedence, and Helm truthiness when the
    /// constraint or one of them references it; and whether the constraint
    /// or one of them is document-relative (see [`BodyReferences`]).
    fn scope(
        &self,
        condition: &Value,
        then_value: &Value,
        conditions: &BTreeMap<String, Value>,
    ) -> (Scope, bool) {
        let mut referenced = Vec::new();
        let mut document_relative = collect_references(condition, &mut referenced);
        document_relative |= collect_references(then_value, &mut referenced);
        let mut scope = Scope::new();
        for (source, definitions) in [
            (DefinitionSource::Provider, self.providers),
            (DefinitionSource::Condition, conditions),
        ] {
            let mut reached = BTreeSet::new();
            let mut pending = Vec::new();
            for name in &referenced {
                if definitions.contains_key(name) && reached.insert(name.clone()) {
                    pending.push(name.clone());
                }
            }
            while let Some(name) = pending.pop() {
                for referenced in self.body_references(&name, source, conditions).names.iter() {
                    if definitions.contains_key(referenced) && reached.insert(referenced.clone()) {
                        pending.push(referenced.clone());
                    }
                }
            }
            for name in reached {
                scope.insert(name, source);
            }
        }
        let references_helm_truthy = value_references_helm_truthy(condition)
            || value_references_helm_truthy(then_value)
            || scope
                .iter()
                .any(|(name, source)| self.body_references_helm_truthy(name, *source, conditions));
        if references_helm_truthy {
            scope.insert(
                HELM_TRUTHY_DEFINITION_NAME.to_string(),
                DefinitionSource::HelmTruthy,
            );
        }
        document_relative |= scope.iter().any(|(name, source)| {
            self.body_references(name, *source, conditions)
                .document_relative
        });
        (scope, document_relative)
    }

    fn judge(
        &self,
        schema: &Value,
        scope: &Scope,
        conditions: &BTreeMap<String, Value>,
        expanding: &mut Vec<String>,
    ) -> Option<Rc<Judge>> {
        if let Some((name, source)) =
            self.expandable_reference(schema, scope, conditions, expanding)
        {
            let key = self.part_key(schema, scope, conditions);
            if let Some(judge) = self.parts.borrow().get(&key) {
                return judge.clone();
            }
            let body = self.body(name, source, conditions)?;
            expanding.push(name.to_string());
            let judge = self.judge(body, scope, conditions, expanding);
            expanding.pop();
            self.parts.borrow_mut().insert(key, judge.clone());
            return judge;
        }
        let (junction, members) = match combinator(schema) {
            Some(Combinator::Not(inner)) if self.splits(inner, scope, conditions, expanding) => {
                let inner = self.judge(inner, scope, conditions, expanding)?;
                return Some(Rc::new(Judge::Not(inner)));
            }
            Some(Combinator::Junction(junction, members))
                if members
                    .iter()
                    .any(|member| self.splits(member, scope, conditions, expanding)) =>
            {
                (junction, members)
            }
            Some(Combinator::Not(_) | Combinator::Junction(..)) | None => {
                return self.part(schema, scope, conditions);
            }
        };
        let mut judges = Vec::new();
        let mut rest = Vec::new();
        for member in members {
            if self.splits(member, scope, conditions, expanding) {
                judges.push(self.judge(member, scope, conditions, expanding)?);
            } else {
                rest.push(member.clone());
            }
        }
        if !rest.is_empty() {
            let rest = Value::Object(serde_json::Map::from_iter([(
                junction.keyword().to_string(),
                Value::Array(rest),
            )]));
            judges.push(self.part(&rest, scope, conditions)?);
        }
        Some(Rc::new(junction.judge(judges)))
    }

    fn part(
        &self,
        schema: &Value,
        scope: &Scope,
        conditions: &BTreeMap<String, Value>,
    ) -> Option<Rc<Judge>> {
        let key = self.part_key(schema, scope, conditions);
        if let Some(judge) = self.parts.borrow().get(&key) {
            return judge.clone();
        }
        let mut definitions = serde_json::Map::new();
        for (name, source) in &key.definitions {
            if let Some(body) = self.body(name, *source, conditions) {
                definitions.insert(name.clone(), body.clone());
            }
        }
        let judge =
            compile_part(schema, definitions).map(|validator| Rc::new(Judge::Part(validator)));
        self.parts.borrow_mut().insert(key, judge.clone());
        judge
    }

    /// `schema`'s text and the definitions of `scope` it reaches.
    fn part_key(
        &self,
        schema: &Value,
        scope: &Scope,
        conditions: &BTreeMap<String, Value>,
    ) -> PartKey {
        let mut referenced = Vec::new();
        collect_references(schema, &mut referenced);
        let mut reached = BTreeMap::new();
        let mut pending = Vec::new();
        for name in referenced {
            if let Some(source) = scope.get(&name)
                && !reached.contains_key(&name)
            {
                reached.insert(name.clone(), *source);
                pending.push((name, *source));
            }
        }
        while let Some((name, source)) = pending.pop() {
            for referenced in self.body_references(&name, source, conditions).names.iter() {
                if let Some(source) = scope.get(referenced)
                    && !reached.contains_key(referenced)
                {
                    reached.insert(referenced.clone(), *source);
                    pending.push((referenced.clone(), *source));
                }
            }
        }
        PartKey {
            schema: schema.to_string(),
            definitions: reached.into_iter().collect(),
        }
    }

    fn body<'v>(
        &'v self,
        name: &str,
        source: DefinitionSource,
        conditions: &'v BTreeMap<String, Value>,
    ) -> Option<&'v Value> {
        match source {
            DefinitionSource::Provider => self.providers.get(name),
            DefinitionSource::Condition => conditions.get(name),
            DefinitionSource::HelmTruthy => Some(&self.helm_truthy),
        }
    }

    fn body_references(
        &self,
        name: &str,
        source: DefinitionSource,
        conditions: &BTreeMap<String, Value>,
    ) -> BodyReferences {
        let key = (name.to_string(), source);
        if let Some(references) = self.references.borrow().get(&key) {
            return references.clone();
        }
        let mut names = Vec::new();
        let document_relative = self
            .body(name, source, conditions)
            .is_some_and(|body| collect_references(body, &mut names));
        let references = BodyReferences {
            names: Rc::from(names),
            document_relative,
        };
        self.references.borrow_mut().insert(key, references.clone());
        references
    }

    /// The definition a sole `$ref` names when the constraint's document
    /// holds it, it is not already being expanded, and its body is a valid
    /// draft-07 schema: the document's definitions are never meta-validated,
    /// while an expanded body is compiled as a schema of its own.
    fn expandable_reference<'s>(
        &self,
        schema: &Value,
        scope: &'s Scope,
        conditions: &BTreeMap<String, Value>,
        expanding: &[String],
    ) -> Option<(&'s str, DefinitionSource)> {
        let Value::Object(object) = schema else {
            return None;
        };
        let (keyword, Value::String(reference)) = object.iter().next()? else {
            return None;
        };
        if object.len() != 1 || keyword != "$ref" {
            return None;
        }
        let reference = helm_schema_json_schema_walk::parse_definition_reference(reference)?;
        if !reference.suffix.is_empty() || expanding.contains(&reference.name) {
            return None;
        }
        let (name, source) = scope.get_key_value(&reference.name)?;
        self.body_meta_valid(name, *source, conditions)
            .then_some((name.as_str(), *source))
    }

    /// Whether `schema` judges as more than one compiled part.
    fn splits(
        &self,
        schema: &Value,
        scope: &Scope,
        conditions: &BTreeMap<String, Value>,
        expanding: &[String],
    ) -> bool {
        if self
            .expandable_reference(schema, scope, conditions, expanding)
            .is_some()
        {
            return true;
        }
        match combinator(schema) {
            Some(Combinator::Junction(_, members)) => members
                .iter()
                .any(|member| self.splits(member, scope, conditions, expanding)),
            Some(Combinator::Not(inner)) => self.splits(inner, scope, conditions, expanding),
            None => false,
        }
    }

    fn body_meta_valid(
        &self,
        name: &str,
        source: DefinitionSource,
        conditions: &BTreeMap<String, Value>,
    ) -> bool {
        let key = (name.to_string(), source);
        if let Some(valid) = self.meta_valid.borrow().get(&key) {
            return *valid;
        }
        let valid = self
            .body(name, source, conditions)
            .is_some_and(jsonschema::draft7::meta::is_valid);
        self.meta_valid.borrow_mut().insert(key, valid);
        valid
    }

    fn body_references_helm_truthy(
        &self,
        name: &str,
        source: DefinitionSource,
        conditions: &BTreeMap<String, Value>,
    ) -> bool {
        let key = (name.to_string(), source);
        if let Some(references) = self.references_helm_truthy.borrow().get(&key) {
            return *references;
        }
        let references = self
            .body(name, source, conditions)
            .is_some_and(value_references_helm_truthy);
        self.references_helm_truthy
            .borrow_mut()
            .insert(key, references);
        references
    }
}

#[tracing::instrument(skip_all, fields(definitions = definitions.len()))]
fn compile_part(
    schema: &Value,
    definitions: serde_json::Map<String, Value>,
) -> Option<jsonschema::Validator> {
    let document = Value::Object(serde_json::Map::from_iter([
        (
            "$schema".to_string(),
            Value::String("http://json-schema.org/draft-07/schema#".to_string()),
        ),
        ("$defs".to_string(), Value::Object(definitions)),
        ("allOf".to_string(), Value::Array(vec![schema.clone()])),
    ]));
    jsonschema::validator_for(&document).ok()
}

fn combinator(schema: &Value) -> Option<Combinator<'_>> {
    let Value::Object(object) = schema else {
        return None;
    };
    if object.len() != 1 {
        return None;
    }
    let (keyword, value) = object.iter().next()?;
    match (keyword.as_str(), value) {
        ("allOf", Value::Array(members)) if !members.is_empty() => {
            Some(Combinator::Junction(Junction::AllOf, members))
        }
        ("anyOf", Value::Array(members)) if !members.is_empty() => {
            Some(Combinator::Junction(Junction::AnyOf, members))
        }
        ("not", inner) => Some(Combinator::Not(inner)),
        _ => None,
    }
}

/// Adds every root definition name a `$ref` below `value` names, at any
/// position; returns whether `value` is document-relative: it holds another
/// `$ref`, or an `$id` or `id` identifier.
fn collect_references(value: &Value, names: &mut Vec<String>) -> bool {
    match value {
        Value::Object(object) => {
            let mut document_relative = ["$id", "id"]
                .into_iter()
                .any(|keyword| object.get(keyword).is_some_and(Value::is_string));
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                match helm_schema_json_schema_walk::parse_definition_reference(reference) {
                    Some(reference) => names.push(reference.name),
                    None => document_relative = true,
                }
            }
            for child in object.values() {
                document_relative |= collect_references(child, names);
            }
            document_relative
        }
        Value::Array(items) => {
            let mut document_relative = false;
            for item in items {
                document_relative |= collect_references(item, names);
            }
            document_relative
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}
