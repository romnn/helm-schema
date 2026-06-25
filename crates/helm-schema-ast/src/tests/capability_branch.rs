use helm_schema_core::{ApiPresenceQuery, CapabilityGuard, CapabilityPresencePredicate};
use test_util::prelude::sim_assert_eq;

use super::decode_guard;

#[test]
fn decode_guard_recognises_capability_has() {
    sim_assert_eq!(
        have: decode_guard(".Capabilities.APIVersions.Has \"policy/v1\""),
        want: CapabilityGuard::Has {
            api: "policy/v1".to_string(),
        }
    );
    sim_assert_eq!(
        have: decode_guard("$.Capabilities.APIVersions.Has \"networking.k8s.io/v1/Ingress\""),
        want: CapabilityGuard::Has {
            api: "networking.k8s.io/v1/Ingress".to_string(),
        }
    );
}

#[test]
fn decode_guard_recognises_negated_capability_has() {
    sim_assert_eq!(
        have: decode_guard("not .Capabilities.APIVersions.Has \"extensions/v1beta1\""),
        want: CapabilityGuard::NotHas {
            api: "extensions/v1beta1".to_string(),
        }
    );
}

#[test]
fn decode_guard_falls_back_to_opaque_for_values_refs() {
    let guard = decode_guard("$.Values.podDisruptionBudget.apiVersion");
    assert!(matches!(guard, CapabilityGuard::Opaque { .. }));
}

#[test]
fn presence_predicate_uses_core_query_parser() {
    let guard = CapabilityGuard::Has {
        api: "policy/v1/PodDisruptionBudget".to_string(),
    };
    sim_assert_eq!(
        have: guard.presence_predicate(),
        want: Some(CapabilityPresencePredicate::Has(
            ApiPresenceQuery::Resource {
                api_version: "policy/v1".to_string(),
                kind: "PodDisruptionBudget".to_string(),
            }
        ))
    );
}
