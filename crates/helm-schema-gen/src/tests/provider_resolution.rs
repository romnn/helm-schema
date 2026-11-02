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
    observations: Mutex<Vec<ResourceRef>>,
}

impl ResourceSchemaOracle for CountingProvider {
    fn observe_resource(&self, resource: &ResourceRef) {
        if let Ok(mut observations) = self.observations.lock() {
            observations.push(resource.clone());
        }
    }

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
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        text_preimage: None,
        strip_marker: false,
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
    let observed_resource = use_.resource.clone();
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
    let signals = signals.with_observed_resources([observed_resource]);
    let provider = CountingProvider::default();

    let resolutions = ProviderSchemaResolutions::resolve(&signals, &provider);
    sim_assert_eq!(have: provider.observations.lock().map_err(|err| eyre::eyre!("{err}"))?.len(), want: 0);
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

#[test]
fn resource_observations_do_not_add_value_constraints() -> eyre::Result<()> {
    let resource = ResourceRef::concrete(
        "autoscaling/v2beta1".to_owned(),
        "HorizontalPodAutoscaler".to_owned(),
    );
    let baseline = ContractSchemaSignals::default();
    let observed = baseline
        .clone()
        .with_observed_resources([resource.clone(), resource.clone()]);
    let provider = CountingProvider::default();
    let schema = crate::generate_values_schema(crate::ValuesSchemaInput::new(&observed, &provider));
    sim_assert_eq!(have: schema, want: crate::generate_values_schema(crate::ValuesSchemaInput::new(&baseline, &CountingProvider::default())));
    sim_assert_eq!(have: provider.calls.into_inner().map_err(|err| eyre::eyre!("{err}"))?, want: Vec::<ProviderSchemaUse>::new());
    sim_assert_eq!(have: provider.observations.into_inner().map_err(|err| eyre::eyre!("{err}"))?, want: vec![resource]);
    Ok(())
}
