//! Cache writers refuse file names that are not portable to every platform.
//!
//! A cache name outside `[a-z0-9._-]`, with a leading or trailing dot, or
//! naming a reserved Windows device is never written on any platform, so
//! bundles and user caches stay portable. The skip is best effort: one
//! warning per entry, then the lookup continues exactly as on a cache miss.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::cache::LAYOUT_MARKER_FILENAME;
use helm_schema_k8s::{
    CrdsCatalogSchemaProvider, Diagnostic, DiagnosticSink, K8sSchemaProvider,
    KubernetesJsonSchemaProvider, format_diagnostic_text,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for K8s integration tests.
pub mod common;
use common::{MockFetcher, MockResponse};

const K8S_VERSION: &str = "v1.29.0-standalone-strict";

fn schema_body() -> Value {
    json!({"type": "object", "properties": {"spec": {"type": "object"}}})
}

/// Every file below `dir` except the layout marker, relative and sorted.
fn cache_files(dir: &Path) -> eyre::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        if !next.exists() {
            continue;
        }
        for entry in
            std::fs::read_dir(&next).wrap_err_with(|| format!("read {}", next.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name != LAYOUT_MARKER_FILENAME)
            {
                out.push(path.strip_prefix(dir)?.to_path_buf());
            }
        }
    }
    out.sort();
    Ok(out)
}

fn fetcher(response: &MockResponse) -> Arc<MockFetcher> {
    Arc::new(MockFetcher::new().with_default(response.clone()))
}

fn k8s_provider(
    cache: &Path,
    version: &str,
    fetcher: Arc<MockFetcher>,
    sink: &DiagnosticSink,
) -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::new(version.to_string())
        .with_cache_dir(cache.to_path_buf())
        .with_allow_download(true)
        .with_fetcher(fetcher)
        .with_diagnostic_sink(sink.clone())
}

fn crd_provider(
    cache: &Path,
    fetcher: Arc<MockFetcher>,
    sink: &DiagnosticSink,
) -> CrdsCatalogSchemaProvider {
    CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.to_path_buf())
        .with_allow_download(true)
        .with_fetcher(fetcher)
        .with_diagnostic_sink(sink.clone())
}

/// The root schema a lookup yields, run twice: the second run proves the
/// warning stays single and the in-process state still answers.
fn root_schema_twice(
    provider: &impl K8sSchemaProvider,
    resource: &ResourceRef,
) -> [Option<Value>; 2] {
    [(), ()].map(|()| {
        provider
            .lookup(resource, &YamlPath(Vec::new()))
            .into_schema_fragment()
            .map(helm_schema_core::ProviderSchemaFragment::into_schema)
    })
}

fn skipped(cache: &Path, entry: &str, reason: &str) -> Diagnostic {
    Diagnostic::CacheWriteSkipped {
        cache_root: cache.display().to_string(),
        entry: entry.to_string(),
        reason: reason.to_string(),
    }
}

/// One non-portable lookup beside its portable twin, under one upstream
/// response.
struct Case {
    version: &'static str,
    resource: ResourceRef,
    entry: &'static str,
    reason: &'static str,
}

fn kubernetes_cases() -> Vec<Case> {
    vec![
        Case {
            version: K8S_VERSION,
            resource: ResourceRef::concrete("v1".to_string(), "Con\"fig".to_string()),
            entry: "default/v1.29.0-standalone-strict/con\"fig-v1.json",
            reason: r#"component "con\"fig-v1.json" contains '"', outside the portable set [a-z0-9._-]"#,
        },
        Case {
            version: K8S_VERSION,
            resource: ResourceRef::concrete("v1".to_string(), "Foo:Bar".to_string()),
            entry: "default/v1.29.0-standalone-strict/foo:bar-v1.json",
            reason: r#"component "foo:bar-v1.json" contains ':', outside the portable set [a-z0-9._-]"#,
        },
        Case {
            version: "V1.29.0",
            resource: ResourceRef::concrete("v1".to_string(), "ConfigMap".to_string()),
            entry: "default/V1.29.0/configmap-v1.json",
            reason: r#"component "V1.29.0" contains 'V', outside the portable set [a-z0-9._-]"#,
        },
    ]
}

/// The portable twin: the same upstream answer under a portable name.
fn portable_twin(case: &Case) -> (&'static str, ResourceRef) {
    (
        K8S_VERSION,
        ResourceRef::concrete(case.resource.api_version.clone(), "ConfigMap".to_string()),
    )
}

#[test]
fn kubernetes_cache_skips_non_portable_names_and_continues_as_a_miss() -> eyre::Result<()> {
    for response in [
        MockResponse::Body(schema_body().to_string().into_bytes()),
        MockResponse::NotFound,
    ] {
        for case in kubernetes_cases() {
            let cache = ScratchDir::new("non-portable-k8s")?;
            let sink = DiagnosticSink::new();
            let provider = k8s_provider(cache.path(), case.version, fetcher(&response), &sink);

            let have = root_schema_twice(&provider, &case.resource);

            let twin_cache = ScratchDir::new("portable-twin-k8s")?;
            let twin_sink = DiagnosticSink::new();
            let (twin_version, twin_resource) = portable_twin(&case);
            let twin = k8s_provider(
                twin_cache.path(),
                twin_version,
                fetcher(&response),
                &twin_sink,
            );
            let want = root_schema_twice(&twin, &twin_resource);

            sim_assert_eq!(have: have, want: want, "{}", case.entry);
            sim_assert_eq!(have: cache_files(cache.path())?, want: Vec::<PathBuf>::new(), "{}", case.entry);
            sim_assert_eq!(
                have: sink.snapshot(),
                want: vec![skipped(cache.path(), case.entry, case.reason)],
                "{}",
                case.entry
            );
            // The twin really did write, so the skip above is not vacuous.
            sim_assert_eq!(have: cache_files(twin_cache.path())?.len(), want: 1, "{}", case.entry);
            sim_assert_eq!(have: twin_sink.snapshot(), want: Vec::<Diagnostic>::new());
        }
    }
    Ok(())
}

#[test]
fn crd_catalog_cache_skips_non_portable_names_and_continues_as_a_miss() -> eyre::Result<()> {
    let resource = ResourceRef::concrete("example.com/v1".to_string(), "Wid\"get".to_string());
    let twin_resource = ResourceRef::concrete("example.com/v1".to_string(), "Widget".to_string());
    for response in [
        MockResponse::Body(schema_body().to_string().into_bytes()),
        MockResponse::NotFound,
    ] {
        let cache = ScratchDir::new("non-portable-crd")?;
        let sink = DiagnosticSink::new();
        let provider = crd_provider(cache.path(), fetcher(&response), &sink);

        let have = root_schema_twice(&provider, &resource);

        let twin_cache = ScratchDir::new("portable-twin-crd")?;
        let twin = crd_provider(
            twin_cache.path(),
            fetcher(&response),
            &DiagnosticSink::new(),
        );
        let want = root_schema_twice(&twin, &twin_resource);

        // The CRD catalog keeps no not-found markers, so only a fetched
        // schema has anything to write.
        let want_diagnostics = match response {
            MockResponse::Body(_) => vec![skipped(
                cache.path(),
                "default/example.com/wid\"get_v1.json",
                r#"component "wid\"get_v1.json" contains '"', outside the portable set [a-z0-9._-]"#,
            )],
            MockResponse::NotFound | MockResponse::Transport(_) | MockResponse::NetworkDisabled => {
                Vec::new()
            }
        };

        sim_assert_eq!(have: have, want: want);
        sim_assert_eq!(have: cache_files(cache.path())?, want: Vec::<PathBuf>::new());
        sim_assert_eq!(have: sink.snapshot(), want: want_diagnostics);
    }
    Ok(())
}

#[test]
fn skipped_cache_write_renders_as_a_warning() {
    let diagnostic = skipped(
        Path::new("/cache"),
        "default/v1.29.0-standalone-strict/con\"fig-v1.json",
        r#"component "con\"fig-v1.json" contains '"', outside the portable set [a-z0-9._-]"#,
    );

    sim_assert_eq!(
        have: format_diagnostic_text(&diagnostic),
        want: r#"warning: not caching default/v1.29.0-standalone-strict/con"fig-v1.json under /cache: component "con\"fig-v1.json" contains '"', outside the portable set [a-z0-9._-]; continuing as a cache miss"#
    );
}
