use crate::filename::candidate_filenames_for_resource;
use helm_schema_core::ResourceRef;
use test_util::prelude::sim_assert_eq;

#[test]
fn candidate_filenames_for_core_resource() {
    let r = ResourceRef::concrete("v1".to_string(), "Service".to_string());
    sim_assert_eq!(
        have: candidate_filenames_for_resource(&r),
        want: vec!["service-v1.json".to_string()]
    );
}

#[test]
fn candidate_filenames_for_grouped_resource() {
    let r = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );
    sim_assert_eq!(
        have: candidate_filenames_for_resource(&r),
        want: vec![
            "prometheusrule-monitoring-coreos-com-v1.json".to_string(),
            "prometheusrule-monitoring-v1.json".to_string(),
        ]
    );
}

#[test]
fn candidate_filenames_for_k8s_io_group_prefer_group_prefix() {
    let r = ResourceRef::concrete(
        "networking.k8s.io/v1".to_string(),
        "NetworkPolicy".to_string(),
    );
    sim_assert_eq!(
        have: candidate_filenames_for_resource(&r),
        want: vec![
            "networkpolicy-networking-v1.json".to_string(),
            "networkpolicy-networking-k8s-io-v1.json".to_string(),
        ]
    );
}
