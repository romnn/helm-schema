use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::K8sSchemaProvider;
use crate::local::{descend_schema_path, expand_local_refs};

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct CrdsCatalogSchemaProvider {
    pub cache_dir: PathBuf,
    pub allow_download: bool,
    pub base_url: String,

    mem: std::sync::Mutex<HashMap<String, Value>>,
}

impl CrdsCatalogSchemaProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache_dir: default_crd_schema_cache_dir(),
            allow_download: std::env::var("HELM_SCHEMA_ALLOW_NET")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            base_url: "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main".to_string(),
            mem: std::sync::Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    #[must_use]
    pub fn with_allow_download(mut self, allow: bool) -> Self {
        self.allow_download = allow;
        self
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn relative_path_for_resource(resource: &ResourceRef) -> Option<String> {
        let api_version = resource.api_version.trim();
        let kind = resource.kind.trim();
        if api_version.is_empty() || kind.is_empty() {
            return None;
        }

        let (group, version) = api_version.split_once('/')?;
        let group = group.trim();
        let version = version.trim();
        if group.is_empty() || version.is_empty() {
            return None;
        }

        let group_lc = group.to_ascii_lowercase();
        if group_lc == "apps"
            || group_lc == "batch"
            || group_lc == "autoscaling"
            || group_lc == "policy"
            || group_lc == "extensions"
            || group_lc.ends_with(".k8s.io")
        {
            return None;
        }

        let kind = kind.to_ascii_lowercase();
        Some(format!("{group}/{kind}_{version}.json"))
    }

    fn local_path_for(&self, relative_path: &str) -> PathBuf {
        self.cache_dir.join(relative_path)
    }

    fn download_to_cache(
        &self,
        relative_path: &str,
        local: &Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let parent = local.parent().ok_or("no parent dir")?;
        fs::create_dir_all(parent)?;

        let url = format!("{}/{relative_path}", self.base_url.trim_end_matches('/'));
        let resp = ureq::get(&url).call()?;
        let mut reader = resp.into_body().into_reader();

        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = local.with_extension(format!("json.tmp.{}.{}", std::process::id(), n));
        {
            let mut f = fs::File::create(&tmp)?;
            std::io::copy(&mut reader, &mut f)?;
        }

        match fs::rename(&tmp, local) {
            Ok(()) => Ok(()),
            Err(e) => {
                if local.exists() {
                    let _ = fs::remove_file(&tmp);
                    Ok(())
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }

    fn load_schema_doc(&self, resource: &ResourceRef) -> Option<Value> {
        let relative_path = Self::relative_path_for_resource(resource)?;
        if let Some(v) = self.mem.lock().ok()?.get(&relative_path).cloned() {
            return Some(v);
        }

        let local = self.local_path_for(&relative_path);
        if !local.exists() {
            if !self.allow_download {
                return None;
            }
            if self.download_to_cache(&relative_path, &local).is_err() {
                return None;
            }
        }

        let bytes = fs::read(&local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        if let Ok(mut guard) = self.mem.lock() {
            guard.insert(relative_path, doc.clone());
        }
        Some(doc)
    }

    #[must_use]
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let root = self.load_schema_doc(resource)?;
        let mut stack = std::collections::HashSet::new();
        Some(expand_local_refs(&root, &root, 0, &mut stack))
    }
}

impl Default for CrdsCatalogSchemaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl K8sSchemaProvider for CrdsCatalogSchemaProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let root = self.materialize_schema_for_resource(resource)?;
        descend_schema_path(&root, &path.0)
    }
}

fn default_crd_schema_cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("HELM_SCHEMA_CRD_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("helm-schema").join("crds-catalog");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("crds-catalog");
    }
    PathBuf::from(".cache")
        .join("helm-schema")
        .join("crds-catalog")
}
