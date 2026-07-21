use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_core::{ContractPathSchemaEvidence, ContractSchemaSignals, MetadataFieldKind};

use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs};
use crate::schema_model::{empty_schema, guard_value_to_json, is_empty_schema, type_schema};
use crate::values_yaml::{ValuesYamlPathFacts, ValuesYamlPathInfo, build_values_yaml_path_info};

pub(crate) struct ResolvedPathSchema {
    pub(crate) value_path: String,
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
    pub(crate) values_yaml_schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
    pub(crate) used_as_serialized: bool,
    pub(crate) used_as_pathless_fragment: bool,
    pub(crate) accepted_dependency_values_root_fragment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: ResourceRef,
    path: YamlPath,
    kind: ValueKind,
    is_self_range_collection: bool,
    /// These fields change the restricted schema a use resolves to, so an
    /// under-keyed cache hit would leak one use's preimage into another.
    template_supplied_member_keys: std::collections::BTreeSet<String>,
    split_segment: Option<helm_schema_core::SplitSegmentUse>,
    merge_layers: Option<helm_schema_core::MergeLayersUse>,
    range_key: bool,
    omitted_members: std::collections::BTreeMap<String, Vec<helm_schema_core::ConditionalGuard>>,
}

pub(crate) struct PathSchemaResolver<'a> {
    schema_evidence_by_value_path: &'a BTreeMap<String, ContractPathSchemaEvidence>,
    values_yaml_info: BTreeMap<String, ValuesYamlPathInfo>,
    resolve_policy: ResolvePolicy,
    provider: &'a dyn ResourceSchemaOracle,
    provider_schema_cache: HashMap<ProviderSchemaLookupKey, Option<Arc<ProviderSchemaCandidate>>>,
}

impl<'a> PathSchemaResolver<'a> {
    pub(crate) fn new(
        contract_signals: &'a ContractSchemaSignals,
        values_yaml_doc: &YamlValue,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        let values_yaml_info = build_values_yaml_path_info(
            values_yaml_doc,
            contract_signals.referenced_value_paths(),
            contract_signals.pruned_parent_value_paths(),
            contract_signals.direct_ranged_value_paths(),
        );
        Self {
            schema_evidence_by_value_path: contract_signals.schema_evidence_by_value_path(),
            values_yaml_info,
            resolve_policy: ResolvePolicy,
            provider,
            provider_schema_cache: HashMap::new(),
        }
    }

    /// Resolve overlay/branch evidence in isolation, without a values.yaml
    /// document or a shared cache.
    pub(crate) fn resolve_single_path_evidence(
        evidence: &ContractPathSchemaEvidence,
        provider: &dyn ResourceSchemaOracle,
    ) -> ResolvedPathSchema {
        let path_segments = crate::split_value_path(&evidence.value_path);
        resolve_path_evidence(
            evidence.clone(),
            path_segments,
            None,
            provider,
            &ResolvePolicy,
            &mut HashMap::new(),
        )
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn resolve_all(mut self) -> Vec<ResolvedPathSchema> {
        let resolved_value_paths = self
            .schema_evidence_by_value_path
            .iter()
            .filter(|(_, evidence)| {
                evidence.is_referenced_value_path || !evidence.fail_implications.is_empty()
            })
            .map(|(value_path, _)| value_path.clone())
            .collect::<Vec<_>>();
        resolved_value_paths
            .iter()
            .filter_map(|value_path| self.resolve_path(value_path))
            .collect()
    }

    fn resolve_path(&mut self, value_path: &str) -> Option<ResolvedPathSchema> {
        let evidence = self
            .schema_evidence_by_value_path
            .get(value_path)
            .cloned()?;
        Some(resolve_path_evidence(
            evidence,
            crate::split_value_path(value_path),
            self.values_yaml_info.get(value_path),
            self.provider,
            &self.resolve_policy,
            &mut self.provider_schema_cache,
        ))
    }
}

fn resolve_path_evidence(
    evidence: ContractPathSchemaEvidence,
    path_segments: Vec<String>,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> ResolvedPathSchema {
    let value_path = evidence.value_path.clone();
    let used_as_serialized = evidence.facts.used_as_serialized;
    let used_as_pathless_fragment = evidence.facts.used_as_pathless_fragment;
    let accepted_dependency_values_root_fragment =
        evidence.facts.accepted_dependency_values_root_fragment;
    let (policy_inputs, provider_schema_candidate) = build_path_schema_inputs(
        evidence,
        values_yaml_info,
        provider,
        resolve_policy,
        provider_schema_cache,
    );
    let mut schema = resolve_policy.resolve_schema_for_value_path(policy_inputs);
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
        value_path,
        path_segments,
        schema,
        values_yaml_schema: values_yaml_info
            .map(|path_info| path_info.schema.clone())
            .unwrap_or_else(empty_schema),
        provider_schema_candidate,
        used_as_serialized,
        used_as_pathless_fragment,
        accepted_dependency_values_root_fragment,
    }
}

fn provider_schemas_for_path_evidence(
    evidence: &ContractPathSchemaEvidence,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
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
        let lookup_key = ProviderSchemaLookupKey {
            resource: provider_use.resource.clone(),
            path: provider_use.path.clone(),
            kind: provider_use.kind,
            is_self_range_collection: provider_use.is_self_range_collection,
            template_supplied_member_keys: provider_use.template_supplied_member_keys.clone(),
            split_segment: provider_use.split_segment.clone(),
            merge_layers: provider_use.merge_layers.clone(),
            range_key: provider_use.range_key,
            omitted_members: provider_use.omitted_members.clone(),
        };
        let schema = match provider_schema_cache.entry(lookup_key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let schema = lookup_provider_schema(provider, provider_use, resolve_policy);
                entry.insert(schema.clone());
                schema
            }
        };
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
    evidence: ContractPathSchemaEvidence,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> (ValuePathSchemaInputs, Option<ProviderSchemaCandidate>) {
    let provider_schemas = provider_schemas_for_path_evidence(
        &evidence,
        provider,
        resolve_policy,
        provider_schema_cache,
    );
    let (provider_schema, provider_schema_candidate) = provider_schema_for_path(
        provider_schemas,
        metadata_schema(&evidence.metadata_field_kinds),
    );
    let values_yaml_facts =
        values_yaml_info.map_or_else(ValuesYamlPathFacts::absent, |path_info| path_info.facts());
    let facts = ValuePathSchemaFacts::new(evidence.facts, values_yaml_facts);
    let values_yaml_schema = values_yaml_info
        .map(|path_info| path_info.schema.clone())
        .unwrap_or_else(empty_schema);

    (
        ValuePathSchemaInputs {
            facts,
            provider_schema,
            values_yaml_schema,
            guard_predicate_schema: guard_predicate_schema(
                &evidence.value_path,
                &evidence.guard_predicates,
                resolve_policy,
            ),
            type_hint_schema: type_hint_schema(&evidence.type_hints),
            guarded_type_hint_schema: type_hint_schema(&evidence.guarded_type_hints),
            fallback_type_hint_schema: type_hint_schema(&evidence.fallback_type_hints),
        },
        provider_schema_candidate,
    )
}

fn lookup_provider_schema(
    provider: &dyn ResourceSchemaOracle,
    provider_use: &ProviderSchemaUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<ProviderSchemaCandidate>> {
    provider
        .schema_fragment_for_use(provider_use)
        .and_then(|fragment| {
            fragment.try_map_schema(|schema| {
                resolve_policy.provider_schema_for_value_use(schema, provider_use)
            })
        })
        .map(ProviderSchemaCandidate::from_provider_fragment)
        .map(Arc::new)
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
        merge_schema_list(
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

    (
        merge_schema_list(vec![provider_schema, metadata_schema]),
        provider_schema_candidate,
    )
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
        merge_schema_list(
            field_kinds
                .iter()
                .copied()
                .map(metadata_field_schema)
                .collect(),
        )
    }
}

/// Schema for `fail`-branch requirements. Non-member requirements accept
/// null alongside the demanded type: fail tests routinely sit behind
/// `default`-chained locals, where a null input takes the fallback and
/// renders. Member requirements stay exact (a null member value really
/// aborts).
pub(crate) fn fail_requirement_schema<'a>(
    implications: impl IntoIterator<Item = &'a helm_schema_core::ContractFailImplication>,
) -> Value {
    let mut parts = Vec::new();
    for implication in implications {
        let requirement = fail_value_requirement_schema(
            &implication.requirements,
            !matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Value
            ),
        );
        if is_empty_schema(&requirement) {
            continue;
        }
        match &implication.target {
            helm_schema_core::ContractRequirementTarget::Value => parts.push(requirement),
            helm_schema_core::ContractRequirementTarget::Members { allow_integer } => {
                let mut arms = vec![
                    serde_json::json!({ "type": "array", "items": requirement }),
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": requirement,
                    }),
                ];
                if *allow_integer {
                    let integer =
                        if requirements_allow_runtime_kind(&implication.requirements, "integer") {
                            serde_json::json!({ "type": "integer" })
                        } else {
                            // Nonpositive integer ranges execute no iterations,
                            // so no member reaches the body requirement.
                            serde_json::json!({ "type": "integer", "maximum": 0 })
                        };
                    arms.push(integer);
                }
                arms.push(serde_json::json!({ "type": "null" }));
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            helm_schema_core::ContractRequirementTarget::MembersMatchingPrefix { prefix } => {
                let pattern = format!("^{}", regex::escape(prefix));
                parts.push(serde_json::json!({
                    "anyOf": [
                        {
                            "type": "object",
                            "patternProperties": { (pattern): requirement },
                        },
                        { "type": "array", "maxItems": 0 },
                        { "type": "null" },
                    ]
                }));
            }
            helm_schema_core::ContractRequirementTarget::MembersAt {
                target_path,
                allow_integer,
            } => {
                // Nil-tolerant requirements (comparison operands, and
                // truthy-scoped types whose absent leaf is falsy and escapes
                // the consumer) hold only when the leaf is present, so the
                // wrapper must not demand the field itself. A
                // `NotSchemaType` requirement is likewise satisfied by an
                // absent leaf: the failing test it negates fired only on
                // values OF that type (a quoted-token splice constrains
                // strings; members without the field never render it).
                let tolerant_leaf = implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::ComparableKind(_)
                            | helm_schema_core::FailValueRequirement::TruthyImpliesSchemaType(_)
                            | helm_schema_core::FailValueRequirement::NotEquals(_)
                    )
                }) || implication.requirements.iter().any(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::NotSchemaType(_)
                    )
                });
                let member = if tolerant_leaf {
                    optional_leaf_object_path_schema(target_path, requirement)
                } else {
                    required_object_path_schema(target_path, requirement)
                };
                let mut arms = vec![
                    serde_json::json!({ "type": "array", "items": member }),
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": member,
                    }),
                ];
                if *allow_integer {
                    // Integer iteration yields int members, which can never
                    // host the required field; only zero iterations pass.
                    arms.push(serde_json::json!({ "type": "integer", "maximum": 0 }));
                }
                arms.push(serde_json::json!({ "type": "null" }));
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            helm_schema_core::ContractRequirementTarget::MembersWhereEquals {
                guard_path,
                value,
                target_path,
            } => {
                let Some(value) = serde_json::to_value(value).ok() else {
                    continue;
                };
                let guard =
                    required_object_path_schema(guard_path, serde_json::json!({ "const": value }));
                let target = crate::schema_tree::insert_path_schema_value(
                    empty_schema(),
                    target_path,
                    requirement,
                );
                let member = serde_json::json!({ "if": guard, "then": target });
                parts.push(serde_json::json!({
                    "anyOf": [
                        { "type": "array", "items": member },
                        { "type": "object", "additionalProperties": member },
                        { "type": "null" },
                    ]
                }));
            }
            helm_schema_core::ContractRequirementTarget::Keys => {
                let mut object =
                    if requirements_allow_runtime_kind(&implication.requirements, "string") {
                        serde_json::json!({ "type": "object" })
                    } else {
                        serde_json::json!({ "type": "object", "maxProperties": 0 })
                    };
                // Pattern requirements constrain each KEY's spelling
                // (traefik's uppercase gate); string keys are structural in
                // YAML maps, so only the pattern itself needs encoding.
                let mut key_schemas = Vec::new();
                for requirement in &implication.requirements {
                    match requirement {
                        helm_schema_core::FailValueRequirement::MatchesPattern {
                            pattern,
                            templated: false,
                        } => {
                            if let Some(pattern) = ecma_compatible_pattern(pattern) {
                                key_schemas.push(serde_json::json!({ "pattern": pattern }));
                            }
                        }
                        helm_schema_core::FailValueRequirement::NotMatchesPattern { pattern } => {
                            if let Some(pattern) = ecma_compatible_pattern(pattern) {
                                key_schemas
                                    .push(serde_json::json!({ "not": { "pattern": pattern } }));
                            }
                        }
                        helm_schema_core::FailValueRequirement::StringLengthBounds { min, max } => {
                            let mut bounds = serde_json::Map::new();
                            if let Some(min) = min {
                                bounds.insert("minLength".to_string(), serde_json::json!(min));
                            }
                            if let Some(max) = max {
                                bounds.insert("maxLength".to_string(), serde_json::json!(max));
                            }
                            key_schemas.push(Value::Object(bounds));
                        }
                        _ => {}
                    }
                }
                match key_schemas.len() {
                    0 => {}
                    1 => {
                        object["propertyNames"] =
                            key_schemas.pop().unwrap_or_else(|| serde_json::json!({}));
                    }
                    _ => {
                        object["propertyNames"] = serde_json::json!({ "allOf": key_schemas });
                    }
                }
                let array = if requirements_allow_runtime_kind(&implication.requirements, "integer")
                {
                    serde_json::json!({ "type": "array" })
                } else {
                    // An empty array never evaluates the range body, so no
                    // integer key reaches the strict consumer.
                    serde_json::json!({ "type": "array", "maxItems": 0 })
                };
                parts.push(serde_json::json!({
                    "anyOf": [object, array, { "type": "null" }]
                }));
            }
        }
    }
    merge_schema_list(parts)
}

/// Like [`required_object_path_schema`], but the LEAF member stays
/// optional: nil-tolerant requirements (comparison operands) constrain the
/// field only when it is present. Intermediate segments stay required
/// because field access through an absent parent aborts rendering with a
/// nil-pointer error before the tolerant leaf comparison runs.
fn optional_leaf_object_path_schema(path: &[String], leaf: Value) -> Value {
    let Some((last, parents)) = path.split_last() else {
        return leaf;
    };
    let leaf_host = serde_json::json!({
        "type": "object",
        "properties": { (last.clone()): leaf },
    });
    required_object_path_schema(parents, leaf_host)
}

fn required_object_path_schema(path: &[String], leaf: Value) -> Value {
    path.iter().rev().fold(leaf, |schema, segment| {
        serde_json::json!({
            "type": "object",
            "properties": { (segment.clone()): schema },
            "required": [segment],
        })
    })
}

/// Translate a Go/RE2 pattern into an ECMA 262 equivalent for the JSON
/// Schema `pattern` keyword: bare `{`/`}` braces that do not form a
/// quantifier are literal in RE2 but invalid in strict ECMA parsers, so
/// they get escaped. Constructs with no ECMA spelling (inline flags,
/// `\A`/`\z` anchors, POSIX classes) abstain.
pub(crate) fn ecma_compatible_pattern(pattern: &str) -> Option<String> {
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
        let character = characters[index];
        if character != '\\' && character != '-' {
            previous_was_class_escape = false;
        }
        match character {
            '\\' => {
                previous_was_class_escape = in_class && is_class_escape(index);
                out.push(character);
                if index + 1 < characters.len() {
                    out.push(characters[index + 1]);
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
            '{' if !in_class => {
                // A valid quantifier ({n}, {n,}, {n,m}) passes through.
                let mut end = index + 1;
                while end < characters.len()
                    && (characters[end].is_ascii_digit() || characters[end] == ',')
                {
                    end += 1;
                }
                let quantifier = end > index + 1
                    && characters.get(end) == Some(&'}')
                    && characters[index + 1].is_ascii_digit();
                if quantifier {
                    out.extend(&characters[index..=end]);
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

fn requirements_allow_runtime_kind(
    requirements: &[helm_schema_core::FailValueRequirement],
    schema_type: &str,
) -> bool {
    use helm_schema_core::FailValueRequirement;

    requirements.iter().all(|requirement| match requirement {
        FailValueRequirement::SchemaType(required) => required == schema_type,
        // Null is asserted away before Sprig's missing-key handling runs.
        FailValueRequirement::SchemaTypeEvenNull(required) => required == schema_type,
        // Every runtime kind has a Helm-falsy spelling that escapes the
        // consumer, so the truthy-scoped requirement excludes no kind.
        FailValueRequirement::TruthyImpliesSchemaType(_) => true,
        // Every runtime kind except null has truthy inhabitants.
        FailValueRequirement::HelmTruthy => schema_type != "null",
        // Every runtime kind has a Helm-falsy spelling.
        FailValueRequirement::HelmFalsy => true,
        FailValueRequirement::NotEquals(_) => true,
        // Applies only to present fields on objects; every other kind
        // passes vacuously (a missing field differs from every literal).
        FailValueRequirement::FieldNotEquals { .. } => true,
        FailValueRequirement::ComparableKind(required) => {
            required == schema_type || schema_type == "null"
        }
        FailValueRequirement::NotSchemaType(rejected) => rejected != schema_type,
        FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. } => schema_type == "string",
        FailValueRequirement::Iterable { allow_integer } => {
            matches!(schema_type, "array" | "object" | "null")
                || schema_type == "integer" && *allow_integer
        }
        FailValueRequirement::HasMember(_) => schema_type == "object",
        FailValueRequirement::MemberHost { handled_kinds } => {
            schema_type == "object" || handled_kinds.iter().any(|kind| kind == schema_type)
        }
        FailValueRequirement::IndexableAt(_) => matches!(schema_type, "array" | "string"),
        FailValueRequirement::SplitSegmentsAtLeast {
            allow_non_string, ..
        } => schema_type == "string" || *allow_non_string,
        // The requirement constrains rendered CONTENT, not the value's kind.
        FailValueRequirement::QuotedSerializationSafe { .. } => true,
        // The field constraint applies only to objects carrying the field;
        // every other kind passes vacuously.
        FailValueRequirement::FieldHelmFalsy { .. } => true,
        FailValueRequirement::FieldEquals { .. } => schema_type == "object",
        // Presence of a (truthy or non-null) field needs an object host.
        FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. } => schema_type == "object",
        FailValueRequirement::AnyOf(alternatives) => alternatives
            .iter()
            .any(|alternative| requirements_allow_runtime_kind(alternative, schema_type)),
    })
}

fn fail_value_requirement_schema(
    requirements: &[helm_schema_core::FailValueRequirement],
    per_member: bool,
) -> Value {
    use helm_schema_core::FailValueRequirement;
    let mut parts = Vec::new();
    let mut required_members: Vec<&str> = Vec::new();
    for requirement in requirements {
        match requirement {
            FailValueRequirement::SchemaType(schema_type) => {
                let type_schema = type_schema(schema_type);
                if per_member {
                    parts.push(type_schema);
                } else {
                    parts.push(serde_json::json!({
                        "anyOf": [type_schema, { "type": "null" }]
                    }));
                }
            }
            // No null tolerance: the consumer type-asserts before its nil
            // handling, so an explicit null aborts (absence stays open
            // through the arm's properties anchoring).
            FailValueRequirement::SchemaTypeEvenNull(schema_type) => {
                parts.push(type_schema(schema_type));
            }
            // Only truthy values reach the consumer; every Helm-falsy
            // spelling escapes through the selection and stays accepted.
            FailValueRequirement::TruthyImpliesSchemaType(schema_type) => {
                parts.push(serde_json::json!({
                    "anyOf": [
                        type_schema(schema_type),
                        { "not": { "$ref": format!(
                            "#/$defs/{}",
                            crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                        ) } },
                    ]
                }));
            }
            FailValueRequirement::HelmTruthy => {
                parts.push(serde_json::json!({ "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) }));
            }
            FailValueRequirement::HelmFalsy => {
                parts.push(serde_json::json!({ "not": { "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) } }));
            }
            // `properties` constrains only PRESENT keys on objects — an
            // absent or null field differs from every literal, so no
            // `required` rides along.
            FailValueRequirement::FieldNotEquals { path, value } => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                let mut node = serde_json::json!({ "not": { "const": value } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({ "properties": { segment: node } });
                }
                parts.push(node);
            }
            // `properties` constrains only PRESENT keys on objects, which
            // is exactly the tolerance the negated truthiness test needs:
            // an absent or falsy field renders, a truthy one aborts.
            FailValueRequirement::FieldHelmFalsy { path } => {
                let mut node = serde_json::json!({ "not": { "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({ "properties": { segment: node } });
                }
                parts.push(node);
            }
            FailValueRequirement::NotEquals(value) => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                parts.push(serde_json::json!({ "not": { "const": value } }));
            }
            // Nil compares, so a null member is as valid as an absent one.
            FailValueRequirement::ComparableKind(schema_type) => {
                parts.push(serde_json::json!({
                    "anyOf": [type_schema(schema_type), { "type": "null" }]
                }));
            }
            FailValueRequirement::NotSchemaType(schema_type) => {
                parts.push(serde_json::json!({ "not": type_schema(schema_type) }));
            }
            FailValueRequirement::HasMember(member) => {
                required_members.push(member);
            }
            FailValueRequirement::MatchesPattern { pattern, templated } => {
                // JSON Schema patterns are ECMA 262; abstaining on an
                // untranslatable Go/RE2 pattern only widens the arm back
                // to its other requirements.
                if let Some(pattern) = ecma_compatible_pattern(pattern) {
                    let matches = serde_json::json!({ "type": "string", "pattern": pattern });
                    if *templated {
                        // The pattern constrains `tpl`'s OUTPUT: a raw value
                        // carrying a template action renders to something
                        // that may match, so admit it alongside the
                        // action-free strings the pattern already accepts.
                        parts.push(serde_json::json!({
                            "anyOf": [
                                matches,
                                { "type": "string", "pattern": "\\{\\{" },
                            ]
                        }));
                    } else {
                        parts.push(matches);
                    }
                }
            }
            FailValueRequirement::NotMatchesPattern { pattern } => {
                // Abstaining on an untranslatable pattern only widens the
                // arm back to its other requirements, as for MatchesPattern.
                if let Some(pattern) = ecma_compatible_pattern(pattern) {
                    parts.push(serde_json::json!({
                        "type": "string",
                        "not": { "pattern": pattern },
                    }));
                }
            }
            FailValueRequirement::StringLengthBounds { min, max } => {
                let mut bounds = serde_json::Map::new();
                bounds.insert("type".to_string(), serde_json::json!("string"));
                if let Some(min) = min {
                    bounds.insert("minLength".to_string(), serde_json::json!(min));
                }
                if let Some(max) = max {
                    bounds.insert("maxLength".to_string(), serde_json::json!(max));
                }
                parts.push(Value::Object(bounds));
            }
            FailValueRequirement::MemberHost { handled_kinds } => {
                let mut arms = vec![type_schema("object")];
                arms.extend(
                    handled_kinds
                        .iter()
                        .filter(|kind| kind.as_str() != "object")
                        .map(|kind| type_schema(kind)),
                );
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            FailValueRequirement::Iterable { allow_integer } => {
                parts.push(crate::runtime_iterable_schema(*allow_integer));
            }
            FailValueRequirement::IndexableAt(index) => {
                parts.push(serde_json::json!({
                    "anyOf": [
                        { "type": "array", "minItems": index + 1 },
                        { "type": "string" },
                    ]
                }));
            }
            FailValueRequirement::SplitSegmentsAtLeast {
                separator,
                segments,
                allow_non_string,
            } => {
                let occurrences = segments.saturating_sub(1);
                let pattern = format!(
                    "^(?:[\\s\\S]*{}){{{occurrences}}}",
                    regex::escape(separator)
                );
                let string = serde_json::json!({ "type": "string", "pattern": pattern });
                if *allow_non_string {
                    parts.push(serde_json::json!({
                        "anyOf": [string, { "not": { "type": "string" } }]
                    }));
                } else {
                    parts.push(string);
                }
            }
            FailValueRequirement::QuotedSerializationSafe { style } => {
                parts.push(crate::quoted_serialization::reference_schema(*style));
            }
            // Presence rides the equality (Go's `eq` aborts on nil), so
            // every wrapping level requires its segment and the host must
            // be an object.
            FailValueRequirement::FieldEquals { path, value } => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                let mut node = serde_json::json!({ "const": value });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            // Presence rides along both field forms: an absent field is
            // null-rendered (not-null fails) and falsy (truthy fails), so
            // every wrapping level requires its segment.
            FailValueRequirement::FieldPresentNotNull { path } => {
                let mut node = serde_json::json!({ "not": { "type": "null" } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            FailValueRequirement::FieldHelmTruthy { path } => {
                let mut node = serde_json::json!({ "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            FailValueRequirement::AnyOf(alternatives) => {
                let arms: Vec<Value> = alternatives
                    .iter()
                    .map(|alternative| fail_value_requirement_schema(alternative, per_member))
                    .collect();
                // Field-based alternatives all READ a member field, and a
                // field read aborts on non-object members, so the
                // alternation nests inside one object schema. Descendant
                // property insertions then merge as siblings of the inner
                // `anyOf` (a conjunction); emitting a bare `anyOf` would
                // let the union combiner treat the carrier as one more
                // arm and make the alternation vacuous.
                let field_based = alternatives.iter().flatten().all(|requirement| {
                    matches!(
                        requirement,
                        FailValueRequirement::HasMember(_)
                            | FailValueRequirement::FieldEquals { .. }
                            | FailValueRequirement::FieldNotEquals { .. }
                            | FailValueRequirement::FieldHelmFalsy { .. }
                            | FailValueRequirement::FieldPresentNotNull { .. }
                            | FailValueRequirement::FieldHelmTruthy { .. }
                    )
                });
                if field_based {
                    parts.push(serde_json::json!({ "type": "object", "anyOf": arms }));
                } else {
                    parts.push(serde_json::json!({ "anyOf": arms }));
                }
            }
        }
    }
    if !required_members.is_empty() {
        required_members.sort_unstable();
        required_members.dedup();
        parts.push(serde_json::json!({
            "type": "object",
            "required": required_members,
        }));
    }
    // Requirements are CONJUNCTIVE: they must all hold for the tested
    // value. `merge_schema_list` is the evidence-union combiner whose
    // fallback is `anyOf`, which would silently weaken a multi-requirement
    // implication (two not-equals arms union into a tautology).
    let mut conjuncts: Vec<Value> = Vec::new();
    for part in parts {
        if !conjuncts.contains(&part) {
            conjuncts.push(part);
        }
    }
    match conjuncts.len() {
        0 => empty_schema(),
        1 => conjuncts.remove(0),
        _ => serde_json::json!({ "allOf": conjuncts }),
    }
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

fn guard_predicate_schema(
    value_path: &str,
    guard_predicates: &[helm_schema_ir::ConditionalGuard],
    resolve_policy: &ResolvePolicy,
) -> Value {
    merge_schema_list(
        guard_predicates
            .iter()
            .filter_map(|predicate| resolve_policy.guard_predicate_schema(value_path, predicate))
            .collect(),
    )
}

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}
