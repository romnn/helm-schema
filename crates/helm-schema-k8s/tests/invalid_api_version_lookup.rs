//! An apiVersion that is not `version` or `group/version` under Kubernetes
//! naming never reaches a provider: no cache read, no fetch, no cache file.
//!
//! The milvus pulsar templates once surfaced the lone `"` of
//! `apiVersion: "{{ .Values.certs.internal_issuer.apiVersion }}"` as the
//! apiVersion, and a networked run recorded `certificate-".json.not-found`
//! in the provider bundle, which Windows cannot check out.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_k8s::{
    Chain, CrdsCatalogSchemaProvider, Diagnostic, DiagnosticSink, K8sSchemaProvider,
    KubernetesJsonSchemaProvider, ProviderLookupResult,
};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for K8s integration tests.
pub mod common;
use common::{MockFetcher, MockResponse};

const K8S_VERSION: &str = "v1.29.0-standalone-strict";

/// Syntactically invalid apiVersions, each of which a template can produce.
const INVALID_API_VERSIONS: &[&str] = &[
    "\"",
    "autoscaling/",
    "/v1",
    "Cert-Manager.io/v1",
    "cert-manager.io/v1/Certificate",
    "cert-manager.io/v1 ",
];

fn provider_use(resource: ResourceRef) -> ProviderSchemaUse {
    ProviderSchemaUse {
        value_path: helm_schema_core::ValuesPath::parse("x"),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource,
        is_self_range_collection: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        source_null_tolerant: false,
        omitted_members: std::collections::BTreeMap::default(),
        outer_guards: Vec::new(),
    }
}

/// Every file below `dir`, relative to it, sorted.
fn files_below(dir: &Path) -> eyre::Result<Vec<PathBuf>> {
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
            } else {
                out.push(path.strip_prefix(dir)?.to_path_buf());
            }
        }
    }
    out.sort();
    Ok(out)
}

struct Providers {
    k8s_cache: ScratchDir,
    crd_cache: ScratchDir,
    fetcher: Arc<MockFetcher>,
}

impl Providers {
    fn new() -> eyre::Result<Self> {
        // Every fetch would succeed, so any lookup that got as far as the
        // network would write a cache file.
        let fetcher = Arc::new(
            MockFetcher::new().with_default(MockResponse::Body(br#"{"type":"object"}"#.to_vec())),
        );
        Ok(Self {
            k8s_cache: ScratchDir::new("invalid-api-version-k8s")?,
            crd_cache: ScratchDir::new("invalid-api-version-crd")?,
            fetcher,
        })
    }

    fn k8s(&self) -> KubernetesJsonSchemaProvider {
        KubernetesJsonSchemaProvider::new(K8S_VERSION.to_string())
            .with_cache_dir(self.k8s_cache.path().to_path_buf())
            .with_allow_download(true)
            .with_fetcher(self.fetcher.clone())
    }

    fn crd(&self) -> CrdsCatalogSchemaProvider {
        CrdsCatalogSchemaProvider::new()
            .with_cache_dir(self.crd_cache.path().to_path_buf())
            .with_allow_download(true)
            .with_fetcher(self.fetcher.clone())
    }

    fn written_files(&self) -> eyre::Result<Vec<PathBuf>> {
        let mut files = files_below(self.k8s_cache.path())?;
        files.extend(files_below(self.crd_cache.path())?);
        Ok(files)
    }
}

#[test]
fn chain_never_looks_up_an_invalid_api_version() -> eyre::Result<()> {
    for api_version in INVALID_API_VERSIONS {
        let providers = Providers::new()?;
        let sink = DiagnosticSink::new();
        let chain = Chain::new(vec![Box::new(providers.crd()), Box::new(providers.k8s())])
            .with_diagnostic_sink(sink.clone());
        let resource = ResourceRef::concrete((*api_version).to_string(), "Certificate".to_string());

        let fragment = chain.schema_fragment_for_use(&provider_use(resource));

        sim_assert_eq!(have: fragment.is_none(), want: true, "{api_version:?}");
        sim_assert_eq!(have: providers.fetcher.total_calls(), want: 0, "{api_version:?}");
        sim_assert_eq!(have: providers.written_files()?, want: Vec::<PathBuf>::new(), "{api_version:?}");
        sim_assert_eq!(
            have: sink.snapshot(),
            want: vec![Diagnostic::MissingSchema {
                kind: "Certificate".to_string(),
                api_version: (*api_version).to_string(),
                k8s_versions_tried: vec![K8S_VERSION.to_string()],
                tried_filenames: Vec::new(),
                available_in_cache_versions: Vec::new(),
                suggested_k8s_version: None,
                hint: None,
            }],
            "{api_version:?}"
        );
    }
    Ok(())
}

#[test]
fn providers_never_look_up_an_invalid_api_version() -> eyre::Result<()> {
    for api_version in INVALID_API_VERSIONS {
        let providers = Providers::new()?;
        let resource = ResourceRef::concrete((*api_version).to_string(), "Issuer".to_string());
        let path = YamlPath(vec!["spec".to_string()]);

        let k8s = providers.k8s().lookup(&resource, &path);
        let crd = providers.crd().lookup(&resource, &path);

        sim_assert_eq!(have: matches!(k8s, ProviderLookupResult::NotOwned), want: true, "{api_version:?}");
        sim_assert_eq!(have: matches!(crd, ProviderLookupResult::NotOwned), want: true, "{api_version:?}");
        sim_assert_eq!(have: providers.fetcher.total_calls(), want: 0, "{api_version:?}");
        let written = providers
            .written_files()?
            .into_iter()
            .filter(|file| file.as_os_str() != helm_schema_k8s::cache::LAYOUT_MARKER_FILENAME)
            .collect::<Vec<_>>();
        sim_assert_eq!(have: written, want: Vec::<PathBuf>::new(), "{api_version:?}");
    }
    Ok(())
}

/// The valid spelling of the same resource does reach the network, so the
/// assertions above are not vacuous.
#[test]
fn a_valid_api_version_does_reach_the_provider() -> eyre::Result<()> {
    let providers = Providers::new()?;
    let resource =
        ResourceRef::concrete("cert-manager.io/v1".to_string(), "Certificate".to_string());

    let crd = providers.crd().lookup(&resource, &YamlPath(Vec::new()));

    sim_assert_eq!(have: matches!(crd, ProviderLookupResult::Found { .. }), want: true);
    sim_assert_eq!(have: providers.fetcher.total_calls(), want: 1);
    sim_assert_eq!(
        have: files_below(providers.crd_cache.path())?,
        want: vec![
            PathBuf::from("CACHE_LAYOUT_VERSION"),
            PathBuf::from("default/cert-manager.io/certificate_v1.json"),
        ]
    );
    Ok(())
}
