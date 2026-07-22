//! Golden CST dumps over real corpus templates: the charts exercised by the
//! IR corpus (cert-manager, bitnami-redis, signoz zookeeper/postgresql,
//! zalando, nats, surveyor) plus a helper file full of define blocks.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_syntax::TemplatedDocument;
use test_util::prelude::sim_assert_eq;

fn assert_corpus_dump(template_path: &str, expected: &str) -> eyre::Result<()> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(template_path);
    let source = std::fs::read_to_string(&path)
        .wrap_err_with(|| format!("read golden fixture {}", path.display()))?;
    let document = TemplatedDocument::parse(&source);
    sim_assert_eq!(have: document.dump(), want: expected, "{template_path}");
    Ok(())
}

#[test]
fn cert_manager_service_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/cert-manager/templates/service.yaml",
        include_str!("fixtures/cert_manager_service.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn cert_manager_helpers_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/cert-manager/templates/_helpers.tpl",
        include_str!("fixtures/cert_manager_helpers.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn bitnami_redis_networkpolicy_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/bitnami-redis/templates/networkpolicy.yaml",
        include_str!("fixtures/bitnami_redis_networkpolicy.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn signoz_zookeeper_svc_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
        include_str!("fixtures/signoz_zookeeper_svc.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn signoz_postgresql_secrets_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
        include_str!("fixtures/signoz_postgresql_secrets.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn zalando_postgres_pod_priority_class_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
        include_str!("fixtures/zalando_postgres_pod_priority_class.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn nats_service_account_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/nats/templates/service-account.yaml",
        include_str!("fixtures/nats_service_account.cst.txt"),
    )?;
    Ok(())
}

#[test]
fn surveyor_hpa_dump_matches() -> eyre::Result<()> {
    assert_corpus_dump(
        "charts/surveyor/templates/hpa.yaml",
        include_str!("fixtures/surveyor_hpa.cst.txt"),
    )?;
    Ok(())
}
