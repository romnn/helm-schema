//! Golden CST dumps over real corpus templates: the charts exercised by the
//! IR corpus (cert-manager, bitnami-redis, signoz zookeeper/postgresql,
//! zalando, nats, surveyor) plus a helper file full of define blocks.

use std::path::Path;

use helm_schema_syntax::TemplatedDocument;
use test_util::prelude::sim_assert_eq;

fn assert_corpus_dump(template_path: &str, expected: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(template_path);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let document = TemplatedDocument::parse(&source);
    sim_assert_eq!(have: document.dump(), want: expected, "{template_path}");
}

#[test]
fn cert_manager_service_dump_matches() {
    assert_corpus_dump(
        "charts/cert-manager/templates/service.yaml",
        include_str!("fixtures/cert_manager_service.cst.txt"),
    );
}

#[test]
fn cert_manager_helpers_dump_matches() {
    assert_corpus_dump(
        "charts/cert-manager/templates/_helpers.tpl",
        include_str!("fixtures/cert_manager_helpers.cst.txt"),
    );
}

#[test]
fn bitnami_redis_networkpolicy_dump_matches() {
    assert_corpus_dump(
        "charts/bitnami-redis/templates/networkpolicy.yaml",
        include_str!("fixtures/bitnami_redis_networkpolicy.cst.txt"),
    );
}

#[test]
fn signoz_zookeeper_svc_dump_matches() {
    assert_corpus_dump(
        "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
        include_str!("fixtures/signoz_zookeeper_svc.cst.txt"),
    );
}

#[test]
fn signoz_postgresql_secrets_dump_matches() {
    assert_corpus_dump(
        "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
        include_str!("fixtures/signoz_postgresql_secrets.cst.txt"),
    );
}

#[test]
fn zalando_postgres_pod_priority_class_dump_matches() {
    assert_corpus_dump(
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
        include_str!("fixtures/zalando_postgres_pod_priority_class.cst.txt"),
    );
}

#[test]
fn nats_service_account_dump_matches() {
    assert_corpus_dump(
        "charts/nats/templates/service-account.yaml",
        include_str!("fixtures/nats_service_account.cst.txt"),
    );
}

#[test]
fn surveyor_hpa_dump_matches() {
    assert_corpus_dump(
        "charts/surveyor/templates/hpa.yaml",
        include_str!("fixtures/surveyor_hpa.cst.txt"),
    );
}
