use std::{
    cell::OnceCell,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use helm_schema_core::{
    ContractSchemaSignals, ProviderSchemaFragment, ProviderSchemaUse, ResourceRef,
    ResourceSchemaOracle, YamlPath,
};

use crate::merge::union_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ProviderValueUsePolicy, ResolvePolicy};
use crate::schema_model::type_schema;

pub(crate) struct ProviderSchemaResolutions {
    by_use: BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>,
}

struct ProviderSchemaResolution {
    fragment: Option<ProviderSchemaFragment>,
    candidate: Option<Arc<ProviderSchemaCandidate>>,
    rejects_null: OnceCell<bool>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CandidateKey {
    resource: ResourceRef,
    path: YamlPath,
    policy: ProviderValueUsePolicy,
}

impl ProviderSchemaResolutions {
    pub(crate) fn resolve(
        contract_schema_signals: &ContractSchemaSignals,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        let mut by_use = BTreeMap::new();
        for evidence in contract_schema_signals
            .schema_evidence_by_value_path()
            .values()
            .filter(|evidence| {
                evidence.is_referenced_value_path || !evidence.requirement_implications.is_empty()
            })
        {
            for use_ in &evidence.provider_schema_uses {
                insert_path_resolutions(&mut by_use, use_, provider);
            }
        }
        for evidence in contract_schema_signals
            .schema_evidence_by_value_path()
            .values()
        {
            for use_ in &evidence.provider_schema_uses {
                insert_resolution(&mut by_use, use_.clone(), provider);
            }
            for overlay in &evidence.conditional_overlays {
                for use_ in &overlay.evidence.provider_schema_uses {
                    insert_resolution(&mut by_use, use_.clone(), provider);
                }
            }
        }
        for evidence in contract_schema_signals
            .schema_evidence_by_value_path()
            .values()
        {
            for overlay in &evidence.conditional_overlays {
                for use_ in &overlay.evidence.provider_schema_uses {
                    insert_path_resolutions(&mut by_use, use_, provider);
                }
            }
        }
        let mut candidates = HashMap::<CandidateKey, Option<Arc<ProviderSchemaCandidate>>>::new();
        for evidence in contract_schema_signals
            .schema_evidence_by_value_path()
            .values()
            .filter(|evidence| {
                evidence.is_referenced_value_path || !evidence.requirement_implications.is_empty()
            })
        {
            for use_ in &evidence.provider_schema_uses {
                insert_candidate(&mut by_use, &mut candidates, use_);
            }
        }
        for evidence in contract_schema_signals
            .schema_evidence_by_value_path()
            .values()
        {
            for overlay in &evidence.conditional_overlays {
                let mut overlay_candidates = HashMap::new();
                for use_ in &overlay.evidence.provider_schema_uses {
                    insert_candidate(&mut by_use, &mut overlay_candidates, use_);
                }
            }
        }
        Self { by_use }
    }

    pub(crate) fn fragment_for_use(
        &self,
        use_: &ProviderSchemaUse,
    ) -> Option<&ProviderSchemaFragment> {
        self.by_use
            .get(use_)
            .and_then(|resolution| resolution.fragment.as_ref())
    }

    pub(crate) fn candidate_for_use(
        &self,
        use_: &ProviderSchemaUse,
    ) -> Option<Arc<ProviderSchemaCandidate>> {
        self.by_use
            .get(use_)
            .and_then(|resolution| resolution.candidate.clone())
    }

    pub(crate) fn use_rejects_null(&self, use_: &ProviderSchemaUse) -> bool {
        let Some(resolution) = self.by_use.get(use_) else {
            return false;
        };
        *resolution.rejects_null.get_or_init(|| {
            resolution.fragment.as_ref().is_some_and(|fragment| {
                jsonschema::validator_for(fragment.schema())
                    .is_ok_and(|validator| !validator.is_valid(&serde_json::Value::Null))
            })
        })
    }
}

fn insert_resolution(
    resolutions: &mut BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>,
    use_: ProviderSchemaUse,
    provider: &dyn ResourceSchemaOracle,
) {
    resolutions
        .entry(use_)
        .or_insert_with_key(|use_| ProviderSchemaResolution {
            fragment: provider.schema_fragment_for_use(use_),
            candidate: None,
            rejects_null: OnceCell::new(),
        });
}

fn insert_candidate(
    resolutions: &mut BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>,
    candidates: &mut HashMap<CandidateKey, Option<Arc<ProviderSchemaCandidate>>>,
    use_: &ProviderSchemaUse,
) {
    if use_.merge_layers.is_some() || use_.range_key {
        return;
    }
    let policy = ProviderValueUsePolicy::new(
        use_.kind,
        use_.stringified,
        use_.is_self_range_collection,
        use_.template_supplied_member_keys.clone(),
        use_.split_segment.clone(),
        use_.omitted_members.clone(),
    );
    let key = CandidateKey {
        resource: use_.resource.clone(),
        path: use_.path.clone(),
        policy: policy.clone(),
    };
    let candidate = candidates
        .entry(key)
        .or_insert_with(|| resolve_candidate(resolutions, use_, &policy))
        .clone();
    if let Some(resolution) = resolutions.get_mut(use_) {
        resolution.candidate = candidate;
    }
}

fn resolve_candidate(
    resolutions: &BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>,
    use_: &ProviderSchemaUse,
    policy: &ProviderValueUsePolicy,
) -> Option<Arc<ProviderSchemaCandidate>> {
    let mut kinds = vec![use_.resource.kind.clone()];
    for kind in &use_.resource.kind_candidates {
        if !kind.is_empty() && !kinds.contains(kind) {
            kinds.push(kind.clone());
        }
    }
    let fragment = if kinds.len() == 1 {
        resolutions
            .get(use_)?
            .fragment
            .clone()?
            .try_map_schema(|schema| ResolvePolicy::provider_schema_for_value_use(schema, policy))?
    } else {
        let mut schemas = Vec::with_capacity(kinds.len());
        let mut required_in_parent = true;
        for kind in kinds {
            let mut concrete_use = use_.clone();
            concrete_use.resource.kind = kind;
            concrete_use.resource.kind_candidates.clear();
            let fragment = resolutions
                .get(&concrete_use)?
                .fragment
                .clone()?
                .try_map_schema(|schema| {
                    ResolvePolicy::provider_schema_for_value_use(schema, policy)
                })?;
            required_in_parent &= fragment.required_in_parent();
            schemas.push(fragment.into_schema());
        }
        let schema = if matches!(use_.path.0.as_slice(), [segment] if segment == "kind") {
            ResolvePolicy::provider_schema_for_value_use(&type_schema("string"), policy)?
        } else {
            union_schema_list(schemas)
        };
        ProviderSchemaFragment::new(schema).with_required_in_parent(required_in_parent)
    };
    Some(Arc::new(ProviderSchemaCandidate::from_provider_fragment(
        fragment,
    )))
}

fn insert_path_resolutions(
    resolutions: &mut BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>,
    use_: &ProviderSchemaUse,
    provider: &dyn ResourceSchemaOracle,
) {
    let mut kinds = vec![use_.resource.kind.clone()];
    for kind in &use_.resource.kind_candidates {
        if !kind.is_empty() && !kinds.contains(kind) {
            kinds.push(kind.clone());
        }
    }
    if kinds.len() == 1 {
        insert_resolution(resolutions, use_.clone(), provider);
        return;
    }
    for kind in kinds {
        let mut concrete_use = use_.clone();
        concrete_use.resource.kind = kind;
        concrete_use.resource.kind_candidates.clear();
        insert_resolution(resolutions, concrete_use, provider);
    }
}
