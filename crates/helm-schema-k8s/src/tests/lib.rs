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

/// A malformed apiVersion names no file at all, so no lookup can cache one.
#[test]
fn candidate_filenames_for_malformed_api_versions_are_empty() {
    for api_version in [
        "\"",
        "autoscaling/",
        "Cert-Manager.io/v1",
        "apps/v1/Deployment",
    ] {
        let r = ResourceRef::concrete(api_version.to_string(), "Certificate".to_string());
        sim_assert_eq!(
            have: candidate_filenames_for_resource(&r),
            want: Vec::<String>::new(),
            "{api_version:?}"
        );
        sim_assert_eq!(
            have: crate::filename::group_relative_path_for_resource(&r),
            want: None,
            "{api_version:?}"
        );
    }
}
