use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::FailValueRequirement;
use helm_schema_ir::ValueKind;
use indoc::formatdoc;
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;

use super::{
    expected_values_schema, navigated_host_condition, parse_ir, root_property_schema, schema_for,
};

#[derive(Clone, Copy)]
enum BehaviorCase {
    IdentityProjection { stringified: bool },
    MemberIdentity { json_decoded: bool },
    RangeSubject { json_decoded: bool },
    HasKeyHost { parsed_map: bool },
    SpliceLowering { yaml_serialized: bool },
}

impl BehaviorCase {
    const ALL: [Self; 10] = [
        Self::IdentityProjection { stringified: false },
        Self::IdentityProjection { stringified: true },
        Self::MemberIdentity {
            json_decoded: false,
        },
        Self::MemberIdentity { json_decoded: true },
        Self::RangeSubject {
            json_decoded: false,
        },
        Self::RangeSubject { json_decoded: true },
        Self::HasKeyHost { parsed_map: false },
        Self::HasKeyHost { parsed_map: true },
        Self::SpliceLowering {
            yaml_serialized: false,
        },
        Self::SpliceLowering {
            yaml_serialized: true,
        },
    ];

    const fn value_path(self) -> &'static str {
        match self {
            Self::IdentityProjection { stringified: false } => "identityRaw",
            Self::IdentityProjection { stringified: true } => "identityStringified",
            Self::MemberIdentity {
                json_decoded: false,
            } => "memberRaw",
            Self::MemberIdentity { json_decoded: true } => "memberJson",
            Self::RangeSubject {
                json_decoded: false,
            } => "rangeRaw",
            Self::RangeSubject { json_decoded: true } => "rangeJson",
            Self::HasKeyHost { parsed_map: false } => "hasKeyRaw",
            Self::HasKeyHost { parsed_map: true } => "hasKeyParsed",
            Self::SpliceLowering {
                yaml_serialized: false,
            } => "spliceRaw",
            Self::SpliceLowering {
                yaml_serialized: true,
            } => "spliceYaml",
        }
    }

    fn source(self) -> String {
        let path = self.value_path();
        match self {
            Self::IdentityProjection { stringified: false } => formatdoc! {"
                {{{{- $value := .Values.{path} -}}}}
                value: {{{{ $value }}}}
            "},
            Self::IdentityProjection { stringified: true } => formatdoc! {"
                {{{{- $value := toString .Values.{path} -}}}}
                value: {{{{ $value }}}}
            "},
            Self::MemberIdentity {
                json_decoded: false,
            } => formatdoc! {"
                {{{{- $value := .Values.{path} -}}}}
                value: {{{{ $value.name }}}}
            "},
            Self::MemberIdentity { json_decoded: true } => formatdoc! {"
                {{{{- $value := .Values.{path} | toJson | fromJson -}}}}
                value: {{{{ $value.name }}}}
            "},
            Self::RangeSubject {
                json_decoded: false,
            } => formatdoc! {"
                {{{{- $value := .Values.{path} -}}}}
                {{{{- range $value }}}}
                value: item
                {{{{- end }}}}
            "},
            Self::RangeSubject { json_decoded: true } => formatdoc! {"
                {{{{- $value := .Values.{path} | toJson | fromJson -}}}}
                {{{{- range $value }}}}
                value: item
                {{{{- end }}}}
            "},
            Self::HasKeyHost { parsed_map: false } => formatdoc! {r#"
                {{{{- $value := .Values.{path} -}}}}
                {{{{- if hasKey $value "enabled" }}}}
                value: set
                {{{{- end }}}}
            "#},
            Self::HasKeyHost { parsed_map: true } => formatdoc! {r#"
                {{{{- $value := .Values.{path} | toYaml | fromYaml -}}}}
                {{{{- if hasKey $value "enabled" }}}}
                value: set
                {{{{- end }}}}
            "#},
            Self::SpliceLowering {
                yaml_serialized: false,
            } => {
                format!("value: {{{{ .Values.{path} }}}}\n")
            }
            Self::SpliceLowering {
                yaml_serialized: true,
            } => formatdoc! {"
                value:
                {{{{ toYaml .Values.{path} | nindent 2 }}}}
            "},
        }
    }

    fn expected_schema(self) -> Value {
        let path = self.value_path();
        match self {
            Self::IdentityProjection { .. } | Self::SpliceLowering { .. } => {
                root_schema(path, json!({}), Vec::new())
            }
            Self::MemberIdentity { .. } => {
                let host_present = json!({ "not": navigated_host_condition(&[path]) });
                let host_object = root_property_schema(path, json!({ "type": "object" }));
                root_schema(
                    path,
                    json!({
                        "additionalProperties": {},
                        "properties": { "name": {} },
                    }),
                    vec![json!({ "if": host_present, "then": host_object })],
                )
            }
            Self::RangeSubject { json_decoded } => {
                let types = if json_decoded {
                    json!(["array", "null", "object"])
                } else {
                    json!(["array", "integer", "null", "object"])
                };
                let range_schema = json!({ "type": types });
                root_schema(
                    path,
                    range_schema.clone(),
                    vec![root_property_schema(path, range_schema)],
                )
            }
            Self::HasKeyHost { parsed_map: false } => root_schema(
                path,
                json!({
                    "additionalProperties": {},
                    "properties": { "enabled": {} },
                }),
                vec![root_property_schema(
                    path,
                    json!({ "type": ["null", "object"] }),
                )],
            ),
            Self::HasKeyHost { parsed_map: true } => {
                root_schema(path, json!({ "additionalProperties": {} }), Vec::new())
            }
        }
    }
}

fn root_schema(path: &str, property: Value, all_of: Vec<Value>) -> Value {
    expected_values_schema(
        [(path.to_string(), property)]
            .into_iter()
            .collect::<Map<_, _>>(),
        all_of,
        false,
    )
}

#[test]
fn transforms_keep_their_position_specific_facts_and_schemas() -> eyre::Result<()> {
    for case in BehaviorCase::ALL {
        let path = case.value_path();
        let finalized = parse_ir(&case.source()).finalize();
        let evidence = finalized
            .schema_signals()
            .evidence_for(path)
            .ok_or_eyre("generated transform case must produce path evidence")?;

        match case {
            BehaviorCase::IdentityProjection { stringified } => {
                let placed = finalized
                    .uses()
                    .iter()
                    .find(|use_| use_.source_expr == path && !use_.path.0.is_empty())
                    .ok_or_eyre("identity case must produce a placed use")?;
                sim_assert_eq!(have: placed.stringified, want: stringified);
                let expected_kind = if stringified {
                    ValueKind::Serialized
                } else {
                    ValueKind::Scalar
                };
                sim_assert_eq!(have: placed.kind, want: expected_kind);
            }
            BehaviorCase::MemberIdentity { .. } => {
                assert!(evidence.facts.has_referenced_descendants);
                assert!(finalized.uses().iter().any(|use_| {
                    use_.source_expr == format!("{path}.name")
                        && use_.path.0 == ["value".to_string()]
                }));
            }
            BehaviorCase::RangeSubject { json_decoded } => {
                assert!(evidence.facts.is_direct_ranged_source);
                sim_assert_eq!(
                    have: evidence.facts.has_json_decoded_range_use,
                    want: json_decoded
                );
            }
            BehaviorCase::HasKeyHost { parsed_map } => {
                let has_object_requirement = evidence
                    .requirement_implications
                    .iter()
                    .flat_map(|implication| &implication.requirements)
                    .any(|requirement| {
                        matches!(requirement, FailValueRequirement::SchemaType(schema_type) if schema_type == "object")
                    });
                sim_assert_eq!(have: has_object_requirement, want: !parsed_map);
                sim_assert_eq!(have: evidence.facts.used_as_fragment, want: parsed_map);
                sim_assert_eq!(have: evidence.facts.used_as_serialized, want: parsed_map);
            }
            BehaviorCase::SpliceLowering { yaml_serialized } => {
                let placed = finalized
                    .uses()
                    .iter()
                    .find(|use_| use_.source_expr == path)
                    .ok_or_eyre("splice case must produce a placed use")?;
                let expected_kind = if yaml_serialized {
                    ValueKind::YamlSerialized
                } else {
                    ValueKind::Scalar
                };
                sim_assert_eq!(have: placed.kind, want: expected_kind);
                sim_assert_eq!(
                    have: evidence.facts.used_as_yaml_serialized,
                    want: yaml_serialized
                );
            }
        }

        let schema = schema_for(finalized.into_schema_signals());
        sim_assert_eq!(have: schema, want: case.expected_schema());
    }
    Ok(())
}
