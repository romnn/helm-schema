use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use color_eyre::eyre;
use helm_schema_core::{
    ConditionalOverlayEvidence, ConditionalOverlayFlavor, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractSchemaSignals, ProviderSchemaFragment, ProviderSchemaUse,
    ResourceRef, ResourceSchemaOracle, ValueKind, ValuesPath, YamlPath,
};
use test_util::prelude::sim_assert_eq;

use crate::provider_resolution::ProviderSchemaResolutions;

#[derive(Debug, Default)]
struct CountingProvider {
    calls: Mutex<Vec<ProviderSchemaUse>>,
}

impl ResourceSchemaOracle for CountingProvider {
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        let Ok(mut calls) = self.calls.lock() else {
            return None;
        };
        calls.push(use_.clone());
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "const": use_.resource.kind,
        })))
    }
}

#[test]
fn provider_resolution_phase_resolves_each_exact_and_concrete_use_once() -> eyre::Result<()> {
    let value_path = ValuesPath::parse("service.value");
    let mut resource = ResourceRef::concrete("example/v1".to_string(), "Primary".to_string());
    resource.kind_candidates = vec!["Alternative".to_string(), "Primary".to_string()];
    let use_ = ProviderSchemaUse {
        value_path: value_path.clone(),
        path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource,
        is_self_range_collection: false,
        source_null_tolerant: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        outer_guards: Vec::new(),
    };
    let overlay = ConditionalPathOverlay::new(
        Vec::new(),
        ConditionalOverlayEvidence {
            provider_schema_uses: vec![use_.clone()],
            ..ConditionalOverlayEvidence::default()
        },
        false,
        ConditionalOverlayFlavor::Ordinary,
    );
    let signals = ContractSchemaSignals::new(
        BTreeMap::from([(
            value_path,
            ContractPathSchemaEvidence {
                is_referenced_value_path: true,
                provider_schema_uses: vec![use_.clone(), use_],
                conditional_overlays: vec![overlay],
                ..ContractPathSchemaEvidence::default()
            },
        )]),
        Vec::new(),
    );
    let provider = CountingProvider::default();

    let resolutions = ProviderSchemaResolutions::resolve(&signals, &provider);
    let calls = provider
        .calls
        .into_inner()
        .map_err(|error| eyre::eyre!("provider call mutex was poisoned: {error}"))?;

    sim_assert_eq!(have: calls.len(), want: 3);
    sim_assert_eq!(
        have: calls
            .iter()
            .map(|use_| (
                use_.resource.kind.as_str(),
                use_.resource.kind_candidates.is_empty(),
            ))
            .collect::<Vec<_>>(),
        want: vec![
            ("Primary", true),
            ("Alternative", true),
            ("Primary", false),
        ]
    );
    for use_ in &calls {
        assert!(resolutions.fragment_for_use(use_).is_some());
    }
    Ok(())
}
