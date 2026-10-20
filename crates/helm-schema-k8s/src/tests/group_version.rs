use test_util::prelude::sim_assert_eq;

use crate::group_version::GroupVersion;

#[test]
fn parses_core_and_grouped_api_versions() {
    for (api_version, group, version) in [
        ("v1", "", "v1"),
        ("v2alpha3", "", "v2alpha3"),
        ("apps/v1", "apps", "v1"),
        ("networking.k8s.io/v1beta1", "networking.k8s.io", "v1beta1"),
        ("cert-manager.io/v1", "cert-manager.io", "v1"),
        ("k8s.cni.cncf.io/v1", "k8s.cni.cncf.io", "v1"),
        ("3scale.net/v1alpha1", "3scale.net", "v1alpha1"),
    ] {
        sim_assert_eq!(
            have: GroupVersion::parse(api_version),
            want: Some(GroupVersion { group, version }),
            "{api_version:?}"
        );
    }
}

#[test]
fn rejects_api_versions_kubernetes_naming_forbids() {
    let long_label = format!("{}.io/v1", "a".repeat(64));
    for api_version in [
        "",
        "\"",
        "'",
        "autoscaling/",
        "/v1",
        "/",
        "Apps/v1",
        "apps/V1",
        "apps/v1/Deployment",
        "apps/v1 ",
        " v1",
        "cert_manager.io/v1",
        "-cert-manager.io/v1",
        "cert-manager-.io/v1",
        "cert-manager..io/v1",
        "cert-manager.io/1v",
        "cert-manager.io/v1-",
        "cert-manager.io/v1.0",
        "apps/v1:x",
        long_label.as_str(),
    ] {
        sim_assert_eq!(have: GroupVersion::parse(api_version), want: None, "{api_version:?}");
    }
}
