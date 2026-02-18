use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdsCatalogSchemaProvider, K8sSchemaProvider, LocalSchemaProvider};
use std::sync::atomic::{AtomicUsize, Ordering};

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn make_temp_dir(group_dir: &str) -> std::path::PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "helm-schema.crds-catalog-test.{}.{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(dir.join(group_dir)).expect("create temp dir");
    dir
}

#[test]
fn materialize_prometheusrule() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let r = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "PrometheusRule".to_string(),
        api_version_candidates: Vec::new(),
    };

    let upstream_materialized = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize");

    let relative_path = "monitoring.coreos.com/prometheusrule_v1.json";
    let cached = provider.cache_dir.join(relative_path);
    assert!(
        cached.exists(),
        "expected schema to be cached at {cached:?}"
    );

    let root_dir = make_temp_dir("monitoring.coreos.com");
    std::fs::copy(&cached, root_dir.join(relative_path)).expect("copy cached schema");

    let local_provider = LocalSchemaProvider::new(&root_dir);
    let local_materialized = local_provider
        .materialize_schema_for_resource(&r)
        .expect("materialize");

    similar_asserts::assert_eq!(upstream_materialized, local_materialized);
}

#[test]
fn prometheusrule_leaf_schema_rules_items() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let r = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "PrometheusRule".to_string(),
        api_version_candidates: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "groups[*]".to_string(),
        "rules[*]".to_string(),
    ]);

    let upstream_leaf = provider
        .schema_for_resource_path(&r, &path)
        .expect("leaf schema");

    let relative_path = "monitoring.coreos.com/prometheusrule_v1.json";
    let cached = provider.cache_dir.join(relative_path);
    assert!(
        cached.exists(),
        "expected schema to be cached at {cached:?}"
    );

    let root_dir = make_temp_dir("monitoring.coreos.com");
    std::fs::copy(&cached, root_dir.join(relative_path)).expect("copy cached schema");

    let local_provider = LocalSchemaProvider::new(&root_dir);
    let local_leaf = local_provider
        .schema_for_resource_path(&r, &path)
        .expect("leaf schema");

    similar_asserts::assert_eq!(upstream_leaf, local_leaf);
}
