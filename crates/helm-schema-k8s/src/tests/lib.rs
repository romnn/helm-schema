use super::*;
use helm_schema_core::ResourceRef;
use test_util::prelude::sim_assert_eq;

#[test]
fn filename_for_core_resource() {
    let r = ResourceRef {
        api_version: "v1".to_string(),
        kind: "Service".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    sim_assert_eq!(have: filename_for_resource(&r), want: "service-v1.json");
}

#[test]
fn filename_for_grouped_resource() {
    let r = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "PrometheusRule".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    sim_assert_eq!(
        have: filename_for_resource(&r),
        want: "prometheusrule-monitoring-coreos-com-v1.json"
    );
}

#[test]
fn filename_for_k8s_io_group_prefers_group_prefix() {
    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    sim_assert_eq!(
        have: filename_for_resource(&r),
        want: "networkpolicy-networking-v1.json"
    );
}
