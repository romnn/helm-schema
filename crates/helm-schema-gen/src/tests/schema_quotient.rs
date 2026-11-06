//! Complete input quotients for a restricted structural Draft 7 schema grammar.

use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::{Map, Value};

const MAX_DOCUMENTS: usize = 50_000;
const MAX_SCHEMA_VISITS: usize = 100_000;
const MAX_SCHEMA_DEPTH: usize = 64;
const MAX_TOTAL_REPRESENTATIVES: usize = 250_000;

#[derive(Default)]
struct Features {
    keys: BTreeSet<String>,
    constrained: bool,
}

struct Quotient {
    nodes: BTreeMap<Vec<String>, Features>,
    remaining: usize,
}

pub(super) fn assert_equivalent(have: &Value, want: &Value) -> eyre::Result<()> {
    let mut quotient = Quotient {
        nodes: BTreeMap::new(),
        remaining: MAX_SCHEMA_VISITS,
    };
    for schema in [have, want] {
        quotient.visit(schema, schema, Some(&[]), &mut BTreeSet::new(), 0)?;
    }
    let actual = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(have)
        .map_err(|err| eyre::eyre!("invalid actual schema: {err}"))?;
    let expected = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(want)
        .map_err(|err| eyre::eyre!("invalid expected schema: {err}"))?;
    let mut remaining = MAX_TOTAL_REPRESENTATIVES;
    let representatives = quotient.values(&[], &mut BTreeMap::new(), &mut remaining)?;
    for document in representatives {
        let actual_accepts = actual.is_valid(&document);
        let expected_accepts = expected.is_valid(&document);
        if actual_accepts != expected_accepts {
            eyre::bail!(
                "schema languages differ at {document}: actual={actual_accepts}, expected={expected_accepts}"
            );
        }
    }
    Ok(())
}

impl Quotient {
    fn visit(
        &mut self,
        schema: &Value,
        root: &Value,
        path: Option<&[String]>,
        references: &mut BTreeSet<String>,
        depth: usize,
    ) -> eyre::Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_eyre("schema proof visit cap exceeded")?;
        if depth > MAX_SCHEMA_DEPTH {
            eyre::bail!("schema proof depth cap exceeded");
        }
        if let Some(path) = path {
            self.nodes.entry(path.to_vec()).or_default();
        }
        if let Some(value) = schema.as_bool() {
            if !value {
                self.constrain(path);
            }
            return Ok(());
        }
        let schema = schema
            .as_object()
            .ok_or_eyre("expected a schema object or Boolean")?;
        if let Some(reference) = schema.get("$ref") {
            for (keyword, value) in schema {
                if keyword != "$ref" {
                    check_annotation(keyword, value)?;
                }
            }
            let reference = reference
                .as_str()
                .ok_or_eyre("expected a string schema reference")?;
            let pointer = reference
                .strip_prefix('#')
                .ok_or_eyre("schema proof requires local references")?;
            if pointer.contains('%') {
                eyre::bail!("unsupported percent-encoded schema reference");
            }
            let target = root
                .pointer(pointer)
                .ok_or_eyre("unresolved local schema reference")?;
            if !references.insert(reference.to_owned()) {
                eyre::bail!("recursive schema reference: {reference}");
            }
            self.visit(target, root, path, references, depth + 1)?;
            references.remove(reference);
            return Ok(());
        }
        for (keyword, value) in schema {
            match keyword.as_str() {
                "$schema" | "title" | "description" => check_annotation(keyword, value)?,
                "$defs" | "definitions" => {
                    for definition in value
                        .as_object()
                        .ok_or_eyre("expected schema definitions")?
                        .values()
                    {
                        // Unused definitions are checked without assigning them an input position.
                        self.visit(definition, root, None, references, depth + 1)?;
                    }
                }
                "properties" => {
                    for (key, child) in
                        value.as_object().ok_or_eyre("expected schema properties")?
                    {
                        if let Some(path) = path {
                            self.nodes
                                .entry(path.to_vec())
                                .or_default()
                                .keys
                                .insert(key.clone());
                        }
                        let child_path = path.map(|path| {
                            let mut child_path = path.to_vec();
                            child_path.push(key.clone());
                            child_path
                        });
                        self.visit(child, root, child_path.as_deref(), references, depth + 1)?;
                    }
                }
                "allOf" | "anyOf" | "oneOf" => {
                    for child in value
                        .as_array()
                        .ok_or_eyre("expected a schema applicator array")?
                    {
                        self.visit(child, root, path, references, depth + 1)?;
                    }
                }
                "if" | "then" | "else" | "not" => {
                    self.visit(value, root, path, references, depth + 1)?;
                }
                _ => self.constraint(keyword, value, path)?,
            }
        }
        Ok(())
    }

    fn constraint(
        &mut self,
        keyword: &str,
        value: &Value,
        path: Option<&[String]>,
    ) -> eyre::Result<()> {
        match keyword {
            "required" => {
                self.constrain(path);
                for key in value
                    .as_array()
                    .ok_or_eyre("expected a required-key array")?
                {
                    let key = key.as_str().ok_or_eyre("expected a required-key string")?;
                    if let Some(path) = path {
                        self.nodes
                            .entry(path.to_vec())
                            .or_default()
                            .keys
                            .insert(key.to_owned());
                        let mut child_path = path.to_vec();
                        child_path.push(key.to_owned());
                        self.nodes.entry(child_path).or_default();
                    }
                }
            }
            "additionalProperties" => {
                if value.as_bool().is_none()
                    && value.as_object().is_none_or(|schema| !schema.is_empty())
                {
                    eyre::bail!("schema proof requires unconstrained additional-property values");
                }
                if value == &Value::Bool(false) {
                    self.constrain(path);
                }
            }
            "items" => {
                if value != &Value::Bool(true)
                    && value.as_object().is_none_or(|schema| !schema.is_empty())
                {
                    eyre::bail!("schema proof requires unconstrained array items");
                }
            }
            "type" => {
                if let Some(types) = value.as_array() {
                    if types.is_empty() {
                        eyre::bail!("empty schema type array");
                    }
                    for kind in types {
                        check_type(kind)?;
                    }
                } else {
                    check_type(value)?;
                }
                self.constrain(path);
            }
            "enum" => {
                if value != &serde_json::json!([null]) && value != &serde_json::json!([""]) {
                    eyre::bail!("unsupported schema enumeration");
                }
                self.constrain(path);
            }
            "const" => {
                if value != &Value::Bool(true)
                    && value.as_number().and_then(serde_json::Number::as_f64) != Some(0.0)
                {
                    eyre::bail!("unsupported schema constant");
                }
                self.constrain(path);
            }
            "minLength" | "minItems" | "minProperties" => {
                if value.as_u64() != Some(1) {
                    eyre::bail!("unsupported schema size threshold");
                }
                self.constrain(path);
            }
            _ => eyre::bail!("unsupported schema proof keyword: {keyword}"),
        }
        Ok(())
    }

    fn constrain(&mut self, path: Option<&[String]>) {
        if let Some(path) = path {
            self.nodes.entry(path.to_vec()).or_default().constrained = true;
        }
    }

    fn values(
        &self,
        path: &[String],
        cache: &mut BTreeMap<Vec<String>, Vec<Value>>,
        remaining: &mut usize,
    ) -> eyre::Result<Vec<Value>> {
        if let Some(values) = cache.get(path) {
            charge_representatives(remaining, values.len())?;
            return Ok(values.clone());
        }
        let Features { keys, constrained } = self
            .nodes
            .get(path)
            .ok_or_eyre("missing schema proof position")?;
        if keys.is_empty() && !constrained {
            charge_representatives(remaining, 1)?;
            return Ok(vec![Value::Null]);
        }
        // These representatives separate every supported scalar atom and array size threshold.
        // Item schemas admit every value, so nonempty arrays need no mixed-element representatives.
        charge_representatives(remaining, 10)?;
        let mut values = vec![
            Value::Null,
            serde_json::json!(false),
            serde_json::json!(true),
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(1.5),
            serde_json::json!(""),
            serde_json::json!("x"),
            serde_json::json!([]),
            serde_json::json!([null]),
        ];
        let mut objects = vec![Map::new()];
        for key in keys {
            let mut child_path = path.to_vec();
            child_path.push(key.clone());
            let children = self.values(&child_path, cache, remaining)?;
            let count = objects
                .len()
                .checked_mul(children.len() + 1)
                .ok_or_eyre("schema proof product overflow")?;
            if count > MAX_DOCUMENTS {
                eyre::bail!("schema proof document cap exceeded");
            }
            charge_representatives(remaining, count)?;
            let mut next = Vec::with_capacity(count);
            for object in objects {
                next.push(object.clone());
                for value in &children {
                    let mut present = object.clone();
                    present.insert(key.clone(), value.clone());
                    next.push(present);
                }
            }
            objects = next;
        }
        let count = objects
            .len()
            .checked_mul(2)
            .and_then(|count| count.checked_add(values.len()))
            .ok_or_eyre("schema proof product overflow")?;
        if count > MAX_DOCUMENTS {
            eyre::bail!("schema proof document cap exceeded");
        }
        charge_representatives(remaining, count)?;
        let mut extra_key = "__schema_proof_extra".to_owned();
        while keys.contains(&extra_key) {
            extra_key.push('_');
        }
        // Named-key subsets retain every presence and child class.
        // Extra keys have unconstrained values, and only empty/nonempty object size is observed.
        for mut object in objects {
            values.push(Value::Object(object.clone()));
            object.insert(extra_key.clone(), Value::Null);
            values.push(Value::Object(object));
        }
        charge_representatives(remaining, values.len())?;
        cache.insert(path.to_vec(), values.clone());
        Ok(values)
    }
}

fn charge_representatives(remaining: &mut usize, count: usize) -> eyre::Result<()> {
    *remaining = remaining
        .checked_sub(count)
        .ok_or_eyre("schema proof representative cap exceeded")?;
    Ok(())
}

fn check_annotation(keyword: &str, value: &Value) -> eyre::Result<()> {
    match keyword {
        "$schema" if value.as_str() == Some("http://json-schema.org/draft-07/schema#") => Ok(()),
        "title" | "description" if value.is_string() => Ok(()),
        _ => eyre::bail!("unsupported schema annotation or reference sibling: {keyword}"),
    }
}

fn check_type(value: &Value) -> eyre::Result<()> {
    match value.as_str() {
        Some("null" | "boolean" | "string" | "number" | "integer" | "object" | "array") => Ok(()),
        Some(_) | None => eyre::bail!("unsupported schema type"),
    }
}
