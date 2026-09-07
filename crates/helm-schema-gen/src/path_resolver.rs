use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use helm_schema_core::{ConditionalGuard, ValuesPath};
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_core::{ContractPathSchemaEvidence, ContractSchemaSignals, MetadataFieldKind};

use crate::merge::{intersect_schema_list, merge_schema_list};
use crate::provider_resolution::ProviderSchemaResolutions;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs};
use crate::schema_model::{empty_schema, guard_value_to_json, is_empty_schema, type_schema};
use crate::schema_node::SchemaNode;
use crate::values_yaml::{ValuesYamlPathFacts, ValuesYamlPathInfo, build_values_yaml_path_info};

#[derive(Clone)]
pub(crate) struct ResolvedPathSchema {
    pub(crate) value_path: ValuesPath,
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
    /// Runtime evidence resolved without the chart's declared default shape.
    ///
    /// Conditional-base ownership must consult this form: a whole-path
    /// reference makes `values.yaml` shape available to ordinary inference,
    /// but does not prove that shape is required while every consumer is
    /// dormant.
    pub(crate) structural_schema: Value,
    /// Independently executing consumers, without declaration or guard hints.
    /// Wildcard contracts remain in their range-scoped requirement lane.
    pub(crate) independent_base_contract: Option<IndependentBaseContract>,
    pub(crate) values_yaml_schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
    pub(crate) used_as_serialized: bool,
    pub(crate) used_as_pathless_fragment: bool,
    pub(crate) accepted_dependency_values_root_fragment: bool,
}

/// A literal-path contract qualified independently of declaration and guard hints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndependentBaseContract {
    literal_path: Vec<String>,
    schema: Value,
}

impl IndependentBaseContract {
    pub(crate) fn literal_path(&self) -> &[String] {
        &self.literal_path
    }

    pub(crate) fn schema(&self) -> SchemaNode {
        SchemaNode::foreign(self.schema.clone())
    }
}

pub(crate) struct PathSchemaResolver<'a> {
    schema_evidence_by_value_path: &'a BTreeMap<ValuesPath, ContractPathSchemaEvidence>,
    values_yaml_info: BTreeMap<ValuesPath, ValuesYamlPathInfo>,
    dependency_default_paths: BTreeSet<ValuesPath>,
    provider_resolutions: &'a ProviderSchemaResolutions,
}

impl<'a> PathSchemaResolver<'a> {
    pub(crate) fn new(
        contract_signals: &'a ContractSchemaSignals,
        values_yaml_doc: &YamlValue,
        dependency_values_yaml_doc: &YamlValue,
        provider_resolutions: &'a ProviderSchemaResolutions,
    ) -> Self {
        let values_yaml_info = build_values_yaml_path_info(
            values_yaml_doc,
            contract_signals.referenced_value_paths(),
            contract_signals.pruned_parent_value_paths(),
            contract_signals.unconditionally_omitted_value_paths(),
            contract_signals.direct_ranged_value_paths(),
        );
        Self {
            schema_evidence_by_value_path: contract_signals.schema_evidence_by_value_path(),
            values_yaml_info,
            dependency_default_paths: contract_signals
                .referenced_value_paths()
                .iter()
                .filter(|path| {
                    crate::values_yaml::yaml_value_at_values_path(dependency_values_yaml_doc, path)
                        .is_some()
                })
                .cloned()
                .collect(),
            provider_resolutions,
        }
    }

    /// Resolve overlay/branch evidence in isolation, without a values.yaml
    /// document.
    pub(crate) fn resolve_single_path_evidence(
        value_path: &ValuesPath,
        evidence: &ContractPathSchemaEvidence,
        provider_resolutions: &ProviderSchemaResolutions,
    ) -> ResolvedPathSchema {
        resolve_path_evidence(value_path, evidence, None, false, provider_resolutions)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn resolve_all(self) -> Vec<ResolvedPathSchema> {
        let Self {
            schema_evidence_by_value_path,
            values_yaml_info,
            dependency_default_paths,
            provider_resolutions,
        } = self;
        schema_evidence_by_value_path
            .iter()
            .filter(|(_, evidence)| {
                evidence.is_referenced_value_path || !evidence.requirement_implications.is_empty()
            })
            .map(|(value_path, evidence)| {
                resolve_path_evidence(
                    value_path,
                    evidence,
                    values_yaml_info.get(value_path),
                    dependency_default_paths.contains(value_path),
                    provider_resolutions,
                )
            })
            .collect()
    }
}

fn resolve_path_evidence(
    value_path: &ValuesPath,
    evidence: &ContractPathSchemaEvidence,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    has_dependency_default: bool,
    provider_resolutions: &ProviderSchemaResolutions,
) -> ResolvedPathSchema {
    let path_segments = value_path
        .segments()
        .map(helm_schema_core::Segment::encode_component)
        .collect();
    let used_as_serialized = evidence.facts.used_as_serialized;
    let used_as_pathless_fragment = evidence.facts.used_as_pathless_fragment;
    let accepted_dependency_values_root_fragment =
        evidence.facts.accepted_dependency_values_root_fragment;
    let (policy_inputs, provider_schema_candidate) = build_path_schema_inputs(
        value_path,
        evidence,
        values_yaml_info,
        has_dependency_default,
        provider_resolutions,
    );
    let (structural_policy_inputs, _) =
        build_path_schema_inputs(value_path, evidence, None, false, provider_resolutions);
    let independent_base_contract =
        independent_base_contract(value_path, &structural_policy_inputs);
    let structural_schema = ResolvePolicy::resolve_schema_for_value_path(structural_policy_inputs);
    let mut schema = ResolvePolicy::resolve_schema_for_value_path(policy_inputs);
    if let Some(values_yaml_info) = values_yaml_info {
        for declared_default in &values_yaml_info.declared_defaults {
            schema = crate::resolve_policy::open_objects_rejecting_declared_members(
                schema,
                declared_default,
            );
        }
    }
    let provider_schema_candidate =
        provider_schema_candidate.filter(|provider_schema| provider_schema.survives_as(&schema));

    ResolvedPathSchema {
        value_path: value_path.clone(),
        path_segments,
        schema,
        structural_schema,
        independent_base_contract,
        values_yaml_schema: values_yaml_info
            .map(|path_info| path_info.schema.clone())
            .unwrap_or_else(empty_schema),
        provider_schema_candidate,
        used_as_serialized,
        used_as_pathless_fragment,
        accepted_dependency_values_root_fragment,
    }
}

fn independent_base_contract(
    value_path: &ValuesPath,
    inputs: &ValuePathSchemaInputs,
) -> Option<IndependentBaseContract> {
    let literal_path = value_path
        .segments()
        .map(|segment| segment.literal().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let ValuePathSchemaInputs::Complete {
        facts,
        provider_schema,
        ..
    } = inputs;
    let strict_string = facts.contract.has_non_self_guarded_string_contract;
    if !strict_string && provider_schema.is_empty_schema() {
        return None;
    }
    // Provider transformations resolve separately from the raw-string
    // obligation: independent consumers intersect, while hints merge alternatives.
    // Tested kinds and literal fallback hints do not qualify.
    let provider_contract =
        ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
            facts: *facts,
            provider_schema: provider_schema.clone(),
            values_yaml_schema: SchemaNode::empty(),
            guard_predicate_schema: SchemaNode::empty(),
            type_hint_schema: SchemaNode::empty(),
            guarded_type_hint_schema: SchemaNode::empty(),
            fallback_type_hint_schema: SchemaNode::empty(),
        });
    let schema = if strict_string {
        intersect_schema_list(vec![provider_contract, type_schema("string")])
    } else {
        provider_contract
    };
    (!is_empty_schema(&schema) && schema != Value::Bool(true)).then_some(IndependentBaseContract {
        literal_path,
        schema,
    })
}

fn provider_schemas_for_path_evidence(
    evidence: &ContractPathSchemaEvidence,
    provider_resolutions: &ProviderSchemaResolutions,
) -> Vec<Arc<ProviderSchemaCandidate>> {
    let mut provider_schemas = Vec::new();

    for provider_use in &evidence.provider_schema_uses {
        // Merge-layer uses resolve through synthesized arms instead: the
        // preferred layer as a whole-payload arm under its own truthiness,
        // a shadowed layer as per-key arms scoped to unshadowed keys.
        // Range-KEY uses likewise: their slot constrains the key domain,
        // never the collection's value schema.
        if provider_use.merge_layers.is_some() || provider_use.range_key {
            continue;
        }
        let schema = provider_resolutions.candidate_for_use(provider_use);
        if let Some(schema) = schema
            && !provider_schemas
                .iter()
                .any(|existing| Arc::ptr_eq(existing, &schema))
        {
            provider_schemas.push(schema);
        }
    }

    provider_schemas
}

fn build_path_schema_inputs(
    value_path: &ValuesPath,
    evidence: &ContractPathSchemaEvidence,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    has_dependency_default: bool,
    provider_resolutions: &ProviderSchemaResolutions,
) -> (ValuePathSchemaInputs, Option<ProviderSchemaCandidate>) {
    let provider_schemas = provider_schemas_for_path_evidence(evidence, provider_resolutions);
    let (provider_schema, provider_schema_candidate) = provider_schema_for_path(
        provider_schemas,
        metadata_schema(&evidence.metadata_field_kinds),
    );
    let mut values_yaml_facts = values_yaml_info.map_or_else(
        ValuesYamlPathFacts::absent,
        super::values_yaml::ValuesYamlPathInfo::facts,
    );
    values_yaml_facts.has_dependency_default = has_dependency_default;
    let facts = ValuePathSchemaFacts::new(evidence.facts, values_yaml_facts);
    let values_yaml_schema = values_yaml_info
        .map(|path_info| path_info.schema.clone())
        .unwrap_or_else(empty_schema);

    (
        ValuePathSchemaInputs::Complete {
            facts,
            provider_schema: SchemaNode::from_value(provider_schema),
            values_yaml_schema: SchemaNode::from_value(values_yaml_schema),
            guard_predicate_schema: SchemaNode::from_value(guard_predicate_schema(
                &value_path.encode(),
                &evidence.guard_predicates,
            )),
            type_hint_schema: SchemaNode::from_value(type_hint_schema(&evidence.type_hints)),
            guarded_type_hint_schema: SchemaNode::from_value(type_hint_schema(
                &evidence.guarded_type_hints,
            )),
            fallback_type_hint_schema: SchemaNode::from_value(type_hint_schema(
                &evidence.fallback_type_hints,
            )),
        },
        provider_schema_candidate,
    )
}

fn provider_schema_for_path(
    provider_schemas: Vec<Arc<ProviderSchemaCandidate>>,
    metadata_schema: Value,
) -> (Value, Option<ProviderSchemaCandidate>) {
    let single_provider_schema = match provider_schemas.as_slice() {
        [schema] => Some(schema.clone()),
        _ => None,
    };
    let provider_schema = if let Some(provider_schema) = single_provider_schema.as_deref() {
        provider_schema.schema().clone()
    } else {
        intersect_schema_list(
            provider_schemas
                .into_iter()
                .map(|schema| schema.schema().clone())
                .collect(),
        )
    };
    let provider_schema_candidate = if is_empty_schema(&metadata_schema) {
        single_provider_schema.as_deref().cloned()
    } else {
        None
    };

    let provider_schema = intersect_schema_list(vec![provider_schema, metadata_schema]);

    (provider_schema, provider_schema_candidate)
}

fn metadata_field_schema(field: MetadataFieldKind) -> Value {
    match field {
        MetadataFieldKind::StringMap => string_map_schema(),
        MetadataFieldKind::Name | MetadataFieldKind::Namespace => type_schema("string"),
    }
}

fn metadata_schema(field_kinds: &BTreeSet<MetadataFieldKind>) -> Value {
    if field_kinds.is_empty() {
        empty_schema()
    } else {
        intersect_schema_list(
            field_kinds
                .iter()
                .copied()
                .map(metadata_field_schema)
                .collect(),
        )
    }
}

mod fail_requirement;

pub(crate) use fail_requirement::fail_requirement_schema;

/// Like [`required_object_path_schema`], but the LEAF member stays
/// optional: nil-tolerant requirements (comparison operands) constrain the
/// field only when it is present. Intermediate segments stay required
/// because field access through an absent parent aborts rendering with a
/// nil-pointer error before the tolerant leaf comparison runs.
fn optional_leaf_object_path_schema(path: &[String], leaf: Value) -> Value {
    let Some((last, parents)) = path.split_last() else {
        return leaf;
    };
    if last == "*" {
        // A ranged member exists by construction; only named fields can be
        // absent and therefore receive the nil-tolerant optional-leaf rule.
        return required_object_path_schema(path, leaf);
    }
    let leaf_host = serde_json::json!({
        "type": "object",
        "properties": { (last.clone()): leaf },
    });
    required_object_path_schema(parents, leaf_host)
}

fn required_object_path_schema(path: &[String], leaf: Value) -> Value {
    path.iter().rev().fold(leaf, |schema, segment| {
        if segment == "*" {
            serde_json::json!({
                "anyOf": [
                    { "type": "array", "items": schema },
                    { "type": "object", "additionalProperties": schema },
                    { "type": "null" },
                ],
            })
        } else {
            serde_json::json!({
                "type": "object",
                "properties": { (segment.clone()): schema },
                "required": [segment],
            })
        }
    })
}

/// Translate a Go/RE2 pattern into an ECMA 262 equivalent for the JSON
/// Schema `pattern` keyword: bare `{`/`}` braces that do not form a
/// quantifier are literal in RE2 but invalid in strict ECMA parsers, so
/// they get escaped. A leading global multiline flag is exact when the only
/// affected anchor is the pattern's initial `^`; it lowers to an explicit
/// start-of-input-or-line prefix. Constructs with no bounded ECMA spelling
/// (other inline flags, later multiline anchors, `\A`/`\z` anchors, POSIX
/// classes) abstain.
pub(crate) fn ecma_compatible_pattern(pattern: &str) -> Option<String> {
    let (pattern, multiline_start_anchor, multiline) =
        if let Some(pattern) = pattern.strip_prefix("(?m)") {
            if let Some(pattern) = pattern.strip_prefix('^') {
                (pattern, true, true)
            } else {
                (pattern, false, true)
            }
        } else {
            (pattern, false, false)
        };
    if pattern.contains("(?i") && !pattern.contains("(?i:")
        || pattern.contains("(?m")
        || pattern.contains("(?s")
        || pattern.contains("(?U")
        || pattern.contains("(?P<")
        || pattern.contains("\\A")
        || pattern.contains("\\z")
        || pattern.contains("[[:")
    {
        return None;
    }
    let characters: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    if multiline_start_anchor {
        out.push_str("(?:^|\\n)");
    }
    let mut in_class = false;
    let mut previous_was_class_escape = false;
    let is_class_escape = |index: usize| {
        characters.get(index) == Some(&'\\')
            && matches!(
                characters.get(index + 1),
                Some('w' | 'W' | 'd' | 'D' | 's' | 'S')
            )
    };
    let mut index = 0;
    while index < characters.len() {
        let Some(character) = characters.get(index).copied() else {
            break;
        };
        if character != '\\' && character != '-' {
            previous_was_class_escape = false;
        }
        match character {
            '\\' => {
                previous_was_class_escape = in_class && is_class_escape(index);
                out.push(character);
                if let Some(next) = characters.get(index + 1).copied() {
                    out.push(next);
                    index += 1;
                }
            }
            // In-class `-` adjacent to a class escape (`[\w-\.]`) is a
            // literal in RE2 but an invalid range in strict ECMA parsers.
            '-' if in_class && (previous_was_class_escape || is_class_escape(index + 1)) => {
                out.push_str("\\-");
            }
            '[' if !in_class => {
                in_class = true;
                out.push(character);
            }
            ']' if in_class => {
                in_class = false;
                out.push(character);
            }
            '^' | '$' if multiline && !in_class => return None,
            '{' if !in_class => {
                // A valid quantifier ({n}, {n,}, {n,m}) passes through.
                let mut end = index + 1;
                while end < characters.len()
                    && characters
                        .get(end)
                        .is_some_and(|character| character.is_ascii_digit() || *character == ',')
                {
                    end += 1;
                }
                let quantifier = end > index + 1
                    && characters.get(end) == Some(&'}')
                    && characters.get(index + 1).is_some_and(char::is_ascii_digit);
                if quantifier {
                    if let Some(quantifier) = characters.get(index..=end) {
                        out.extend(quantifier);
                    }
                    index = end;
                } else {
                    out.push_str("\\{");
                }
            }
            '}' if !in_class => out.push_str("\\}"),
            _ => out.push(character),
        }
        index += 1;
    }
    Some(out)
}

/// A value whose text must keep an unquoted YAML token intact. Only strings
/// carry the risk — every other scalar formats as plain digits or words — and a
/// `tpl` render escapes through any value carrying a template action, whose
/// rendered text is not the raw value at all.
fn plain_scalar_safe_schema(token_initial: bool, templated: bool) -> Value {
    let mut safe = serde_json::json!({ "type": "string" });
    if let Some(object) = safe.as_object_mut() {
        object.insert(
            "allOf".to_string(),
            Value::Array(crate::resolve_policy::plain_scalar_structural_exclusions(
                token_initial,
            )),
        );
    }
    let mut arms = vec![safe, serde_json::json!({ "not": { "type": "string" } })];
    if templated {
        arms.push(serde_json::json!({ "type": "string", "pattern": "\\{\\{" }));
    }
    serde_json::json!({ "anyOf": arms })
}

fn type_hint_schema(schema_types: &BTreeSet<String>) -> Value {
    if schema_types.is_empty() {
        return empty_schema();
    }

    merge_schema_list(
        schema_types
            .iter()
            .map(|schema_type| type_schema(schema_type))
            .collect(),
    )
}

fn guard_predicate_schema(value_path: &str, guard_predicates: &[ConditionalGuard]) -> Value {
    merge_schema_list(
        guard_predicates
            .iter()
            .filter_map(|predicate| ResolvePolicy::guard_predicate_schema(value_path, predicate))
            .collect(),
    )
}

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}
