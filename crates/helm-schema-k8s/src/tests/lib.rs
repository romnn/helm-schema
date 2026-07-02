use super::*;
use helm_schema_core::ResourceRef;
use test_util::prelude::sim_assert_eq;

#[test]
fn filename_for_core_resource() {
    let r = ResourceRef::concrete("v1".to_string(), "Service".to_string());
    sim_assert_eq!(have: filename_for_resource(&r), want: "service-v1.json");
}

#[test]
fn filename_for_grouped_resource() {
    let r = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );
    sim_assert_eq!(
        have: filename_for_resource(&r),
        want: "prometheusrule-monitoring-coreos-com-v1.json"
    );
}

#[test]
fn filename_for_k8s_io_group_prefers_group_prefix() {
    let r = ResourceRef::concrete(
        "networking.k8s.io/v1".to_string(),
        "NetworkPolicy".to_string(),
    );
    sim_assert_eq!(
        have: filename_for_resource(&r),
        want: "networkpolicy-networking-v1.json"
    );
}
