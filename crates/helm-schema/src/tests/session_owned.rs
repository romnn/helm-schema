use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ContractUse, Guard, ValueKind, ValuesPath, YamlPath};
use helm_schema_gen::{
    AuthoringPolicy, EmissionReport, GenerationDecisions, PreparedValuesDocuments, SchemaProfile,
};
use helm_schema_ir::ContractIr;
use helm_schema_k8s::LocalSchemaUniverse;
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

use super::{Analysis, AnalysisSession, GenerateOptions, PreparedSession, ResolvedContract};
use crate::provider::ProviderOptions;

#[test]
fn consuming_generation_applies_required_inference_to_resolved_schema() -> eyre::Result<()> {
    let setting = ValuesPath::parse("setting");
    let header = ContractUse::new(
        setting.clone(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: setting.clone(),
        }],
        None,
    );
    let rendered = ContractUse::new(
        setting.clone(),
        YamlPath(vec!["data".to_string(), "value".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    );
    let session = AnalysisSession::new(GenerateOptions {
        chart_source: VfsPath::new(vfs::MemoryFS::new()).into(),
        include_tests: false,
        include_subchart_values: false,
        values_files: Vec::new(),
        infer_required: true,
        emission: SchemaProfile::default().into(),
        authoring: AuthoringPolicy::default(),
        provider: ProviderOptions {
            allow_net: false,
            disable_k8s_schemas: true,
            ..Default::default()
        },
    });
    drop(
        session
            .prepared
            .get_or_try_init(&session.initializing, || {
                Ok(PreparedSession {
                    analysis: Analysis {
                        contract: ContractIr::from_contract_uses(vec![header, rendered]),
                        local_schemas: LocalSchemaUniverse::default(),
                    },
                    values_documents: PreparedValuesDocuments::new(
                        serde_yaml::Value::Null,
                        serde_yaml::Value::Null,
                    ),
                    shadowed_input_paths: BTreeSet::new(),
                    explicit_value_paths: BTreeSet::new(),
                    values_descriptions: BTreeMap::new(),
                })
            })?,
    );
    drop(
        session
            .resolved_contract
            .get_or_try_init(&session.initializing, || {
                Ok(ResolvedContract {
                    schema: json!({"type": "object", "properties": {"setting": {"type": "string"}}}),
                    emission_report: EmissionReport::default(),
                    definition_origins: BTreeMap::new(),
                    generation_decisions: GenerationDecisions::default(),
                })
            })?,
    );
    {
        let finalized = session.finalized_contract()?;
        let evidence = finalized
            .schema_signals()
            .evidence_for(&setting)
            .ok_or_eyre("missing setting evidence")?;
        assert!(evidence.is_required_inference_candidate());
    }
    let schema = session.into_generated_schema()?.schema;
    sim_assert_eq!(
        have: &schema,
        want: &json!({
            "type": "object",
            "properties": {"setting": {"type": "string"}},
            "required": ["setting"],
        }),
    );
    let validator = jsonschema::validator_for(&schema)?;
    assert!(!validator.is_valid(&json!({})));
    assert!(validator.is_valid(&json!({"setting": "example"})));
    Ok(())
}
