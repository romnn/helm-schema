//! Helm-selected CRD installation evidence and opt-in native validation.
//!
//! Recorded responses carry complete original inputs; ordinary tests never start Go.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_core::ResourceRef;
use helm_schema_k8s::crds_catalog::relative_path_for_resource;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::invocation::{hex, output_within};

const PROTOCOL: &str = "helm-schema/crdcheck/v3";
const POLICY: &str = "helm-schema/chart-crds/v4";

/// Complete evidence emitted by the dependency-processed Helm install action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    protocol: String,
    kubernetes_version: String,
    namespace: String,
    release: String,
    crds: Vec<CrdFile>,
    resources: Vec<Value>,
    hooks: Value,
    problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrdFile {
    filename: String,
    raw: String,
    documents: Vec<Value>,
}

/// The narrowly decisive outcomes; native diagnostics never become allowance identities.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Native validation accepted the resource within the installation premises.
    Accepted,
    /// The unchanged catalog may judge this resource.
    Catalog,
    /// Installation or native validation did not prove a result.
    Uncertain(String),
}

/// A checker configuration fixed at the validator boundary, with data-only replay by default.
#[derive(Debug)]
pub struct Checker {
    binary: PathBuf,
    binary_sha256: String,
    records: PathBuf,
    live: bool,
}

#[derive(Serialize, Deserialize)]
struct Recorded {
    inputs: Vec<String>,
    response: Value,
}

impl Checker {
    /// Opens recorded checker results without inspecting or invoking an executable.
    #[must_use]
    pub fn replay(records: &Path) -> Self {
        Self {
            binary: PathBuf::new(),
            binary_sha256: fs::read_to_string(records.join("binary.sha256"))
                .unwrap_or_default()
                .trim()
                .to_string(),
            records: records.to_path_buf(),
            live: false,
        }
    }

    /// Enables explicit maintainer invocation; missing tooling still abstains at use.
    #[must_use]
    pub fn live(binary: &Path, records: &Path) -> Self {
        Self {
            binary: binary.to_path_buf(),
            binary_sha256: fs::read(binary).map(|raw| digest(&raw)).unwrap_or_default(),
            records: records.to_path_buf(),
            live: true,
        }
    }

    pub(crate) fn from_environment(root: &Path) -> Self {
        let binary = std::env::var_os("HELM_SCHEMA_CRDCHECK")
            .map_or_else(|| root.join("target/crdcheck"), PathBuf::from);
        let live = std::env::var("ADJUDICATE_WITH_HELM").is_ok_and(|value| value == "1");
        let records = std::env::var_os("HELM_SCHEMA_CRDCHECK_RECORDS").map_or_else(
            || {
                root.join(if live {
                    ".cache/crdcheck"
                } else {
                    "testdata/crdcheck"
                })
            },
            PathBuf::from,
        );
        let binary_sha256 = if live {
            fs::read(&binary)
                .map(|bytes| digest(&bytes))
                .unwrap_or_default()
        } else {
            fs::read_to_string(records.join("binary.sha256"))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        Self {
            binary,
            binary_sha256,
            records,
            live,
        }
    }

    pub(crate) fn identity(&self) -> String {
        format!("{POLICY}\n{PROTOCOL}\n{}", self.binary_sha256)
    }

    fn check(&self, declaration: &Value, resource: &Value, context: &Value) -> eyre::Result<Value> {
        eyre::ensure!(
            self.binary_sha256.len() == 64,
            "native checker identity unavailable"
        );
        let inputs = vec![
            serde_json::to_string(declaration)?,
            serde_json::to_string(resource)?,
            serde_json::to_string(context)?,
        ];
        let key = input_digest(&inputs);
        if self.live {
            eyre::ensure!(
                digest(&fs::read(&self.binary)?) == self.binary_sha256,
                "checker binary changed"
            );
        }
        let path = self.records.join(format!("{key}.json"));
        if let Ok(raw) = fs::read(&path) {
            let record: Recorded = serde_json::from_slice(&raw)?;
            eyre::ensure!(record.inputs == inputs, "recorded original inputs differ");
            verify_response(&record.response, &key, resource)?;
            return Ok(record.response);
        }
        eyre::ensure!(self.live, "no recorded native result for {key}");
        fs::create_dir_all(&self.records)?;
        // Record keys already bind the binary; this pin only selects offline replay identity.
        fs::write(
            self.records.join("binary.sha256"),
            format!("{}\n", self.binary_sha256),
        )?;
        let scratch = tempfile::Builder::new()
            .prefix("crdcheck-")
            .tempdir_in(&self.records)?;
        let mut command = Command::new(&self.binary);
        command.env("GOMAXPROCS", "3");
        for (index, input) in inputs.iter().enumerate() {
            let path = scratch.path().join(format!("{index}.json"));
            fs::write(&path, input)?;
            command.arg(path);
        }
        let output = output_within(&mut command, Duration::from_secs(30), "native CRD checker")?;
        eyre::ensure!(
            output.status.success(),
            "native checker failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        eyre::ensure!(
            digest(&fs::read(&self.binary)?) == self.binary_sha256,
            "checker binary changed during invocation"
        );
        let response: Value =
            serde_json::from_slice(&output.stdout).wrap_err("incomplete native response")?;
        verify_response(&response, &key, resource)?;
        fs::write(
            path,
            serde_json::to_vec_pretty(&Recorded {
                inputs,
                response: response.clone(),
            })?,
        )?;
        Ok(response)
    }
}

impl Installation {
    /// Canonical Kubernetes YAML documents, decorated by Helm's own ownership visitor.
    #[must_use]
    pub fn resources(&self) -> &[Value] {
        &self.resources
    }

    /// Finds the exact decorated resource corresponding to an older decoded document.
    #[must_use]
    pub fn resource_for(&self, document: &Value) -> Option<&Value> {
        let matches: Vec<_> = self
            .resources
            .iter()
            .filter(|candidate| {
                [
                    "/apiVersion",
                    "/kind",
                    "/metadata/name",
                    "/metadata/namespace",
                ]
                .iter()
                .all(|pointer| candidate.pointer(pointer) == document.pointer(pointer))
            })
            .collect();
        if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        }
    }

    /// Judges chart-local installation proof, retaining the existing catalog boundary.
    ///
    /// `identity` binds the complete original chart, Helm invocation and schema-bundle policy.
    /// Callers supply the content-keyed invocation receipt, never only a chart name.
    #[must_use]
    pub fn decide(
        &self,
        checker: &Checker,
        resource: &Value,
        catalog: Option<&Path>,
        identity: &str,
        created_namespaces: &[String],
    ) -> Decision {
        match self.decide_checked(checker, resource, catalog, identity, created_namespaces) {
            Ok(decision) => decision,
            Err(error) => Decision::Uncertain(format!("{error:#}")),
        }
    }

    fn decide_checked(
        &self,
        checker: &Checker,
        resource: &Value,
        catalog: Option<&Path>,
        identity: &str,
        created_namespaces: &[String],
    ) -> eyre::Result<Decision> {
        let api = string(resource, "apiVersion")?;
        let kind = string(resource, "kind")?;
        if let Some(root) = catalog {
            let reference = ResourceRef::concrete(api.to_string(), kind.to_string());
            let path = relative_path_for_resource(&reference)
                .ok_or_else(|| eyre::eyre!("catalog identity invalid"))?;
            match fs::read(root.join("default").join(path)) {
                Ok(raw) => {
                    serde_json::from_slice::<Value>(&raw).wrap_err("corrupt pinned catalog")?;
                    return Ok(Decision::Catalog);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.verify_resource(resource, created_namespaces)?;
        let (group, version) = api
            .split_once('/')
            .ok_or_else(|| eyre::eyre!("missing custom API group"))?;
        let declarations = self.declarations()?;
        let mut selected = None;
        for declaration in declarations.values() {
            if declaration.pointer("/spec/group").and_then(Value::as_str) == Some(group)
                && declaration
                    .pointer("/spec/names/kind")
                    .and_then(Value::as_str)
                    == Some(kind)
            {
                selected = Some(*declaration);
                let versions = declaration
                    .pointer("/spec/versions")
                    .and_then(Value::as_array)
                    .ok_or_else(|| eyre::eyre!("missing versions"))?;
                let matching: Vec<_> = versions
                    .iter()
                    .filter(|v| v.get("name").and_then(Value::as_str) == Some(version))
                    .collect();
                eyre::ensure!(
                    matching.len() == 1,
                    "requested version absent or ambiguous in installed CRD"
                );
                for entry in matching {
                    eyre::ensure!(
                        entry.get("served").and_then(Value::as_bool) == Some(true),
                        "requested version not served by installed CRD"
                    );
                }
            }
        }
        let Some(selected) = selected else {
            return Ok(Decision::Catalog);
        };
        let declarations: Vec<_> = declarations.into_values().cloned().collect();
        // Original installation and invocation identities bind all sources, hooks and policy.
        let context = json!({"mode":"helm-4.2.3-ssa-create-strict","reference_environment":"fresh-kubernetes-1.29.0-no-admission","release_namespace":self.namespace,"created_namespaces":created_namespaces,"declarations":declarations,"identity":format!("{}\n{identity}\n{}",checker.identity(),serde_json::to_string(self)?)});
        let response = checker.check(selected, resource, &context)?;
        match response.get("verdict").and_then(Value::as_str) {
            Some("Accepted") => Ok(Decision::Accepted),
            Some("Rejected") => Ok(Decision::Uncertain(format!(
                "native rejection lacks proven battery assertion correspondence: {response}"
            ))),
            _ => Ok(Decision::Uncertain(format!(
                "native validation abstained: {response}"
            ))),
        }
    }
    fn verify_resource(&self, resource: &Value, created_namespaces: &[String]) -> eyre::Result<()> {
        eyre::ensure!(
            self.protocol == "helm-schema/installation/v1"
                && !self.namespace.is_empty()
                && !self.release.is_empty(),
            "missing installation proof"
        );
        eyre::ensure!(
            matches!(self.kubernetes_version.as_str(), "v1.29.0" | "1.29.0"),
            "native reference requires Kubernetes 1.29.0"
        );
        eyre::ensure!(
            self.problems.is_empty(),
            "installation veto: {:?}",
            self.problems
        );
        eyre::ensure!(
            resource.get("status").is_none(),
            "supplied status requires unmodelled SSA validation"
        );
        let metadata = resource
            .get("metadata")
            .and_then(Value::as_object)
            .ok_or_else(|| eyre::eyre!("missing object metadata"))?;
        eyre::ensure!(
            metadata.keys().all(|key| matches!(
                key.as_str(),
                "name" | "namespace" | "labels" | "annotations"
            )),
            "unmodelled apply metadata"
        );
        let api = string(resource, "apiVersion")?;
        let kind = string(resource, "kind")?;
        let (group, _) = api
            .split_once('/')
            .ok_or_else(|| eyre::eyre!("missing custom API group"))?;
        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| eyre::eyre!("fixed name required"))?;
        eyre::ensure!(!name.is_empty(), "fixed name required");
        if let Some(namespace) = metadata.get("namespace") {
            let namespace = namespace
                .as_str()
                .ok_or_else(|| eyre::eyre!("malformed namespace"))?;
            eyre::ensure!(
                namespace.is_empty()
                    || namespace == self.namespace
                    || created_namespaces
                        .iter()
                        .any(|created| created == namespace),
                "namespace not proven to exist"
            );
        }
        let namespace = metadata
            .get("namespace")
            .and_then(Value::as_str)
            .filter(|namespace| !namespace.is_empty())
            .unwrap_or(&self.namespace);
        let mut count = 0;
        for other in &self.resources {
            if other
                .get("apiVersion")
                .and_then(Value::as_str)
                .and_then(|api| api.split_once('/'))
                .map(|(group, _)| group)
                == Some(group)
                && other.get("kind").and_then(Value::as_str) == Some(kind)
                && other.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
                && other
                    .pointer("/metadata/namespace")
                    .and_then(Value::as_str)
                    .filter(|namespace| !namespace.is_empty())
                    .unwrap_or(&self.namespace)
                    == namespace
            {
                count += 1;
            }
        }
        eyre::ensure!(
            count == 1,
            "duplicate or unmatched installed resource identity"
        );
        Ok(())
    }

    fn declarations(&self) -> eyre::Result<BTreeMap<&str, &Value>> {
        let mut declarations = BTreeMap::new();
        for file in &self.crds {
            eyre::ensure!(
                !file.raw.is_empty() && !file.filename.is_empty() && !file.documents.is_empty(),
                "missing CRD source bytes or resources are empty"
            );
            for declaration in &file.documents {
                eyre::ensure!(
                    string(declaration, "apiVersion")? == "apiextensions.k8s.io/v1"
                        && string(declaration, "kind")? == "CustomResourceDefinition",
                    "non-v1-CRD document in installation"
                );
                let name = declaration
                    .pointer("/metadata/name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| eyre::eyre!("CRD name missing"))?;
                if let Some(previous) = declarations.insert(name, declaration) {
                    eyre::ensure!(
                        previous == declaration,
                        "conflicting same-name CRD declarations"
                    );
                }
            }
        }
        // Kubernetes reconciles resource aliases and kind aliases in separate per-group domains.
        let mut resources = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        for (owner, declaration) in &declarations {
            let spec = declaration.get("spec").ok_or_eyre("CRD spec missing")?;
            let group = string(spec, "group")?;
            let names = spec.get("names").ok_or_eyre("CRD names missing")?;
            let kind = string(names, "kind")?;
            let plural = string(names, "plural")?;
            let singular = match names.get("singular") {
                Some(Value::String(value)) if !value.is_empty() => value.clone(),
                None | Some(Value::String(_)) => {
                    eyre::ensure!(kind.is_ascii(), "unmodelled non-ASCII CRD singular default");
                    kind.to_ascii_lowercase()
                }
                _ => eyre::bail!("malformed CRD singular"),
            };
            let list_kind = match names.get("listKind") {
                Some(Value::String(value)) if !value.is_empty() => value.clone(),
                None | Some(Value::String(_)) => format!("{kind}List"),
                _ => eyre::bail!("malformed CRD listKind"),
            };
            let mut aliases = vec![plural.to_string(), singular];
            if let Some(short_names) = names.get("shortNames") {
                for alias in short_names
                    .as_array()
                    .ok_or_eyre("malformed CRD shortNames")?
                {
                    aliases.push(
                        alias
                            .as_str()
                            .ok_or_eyre("malformed CRD shortName")?
                            .to_string(),
                    );
                }
            }
            for alias in aliases {
                if let Some(previous) = resources.insert((group, alias), *owner) {
                    eyre::ensure!(
                        previous == *owner,
                        "cross-name CRD resource naming collision"
                    );
                }
            }
            for alias in [kind.to_string(), list_kind] {
                if let Some(previous) = kinds.insert((group, alias), *owner) {
                    eyre::ensure!(previous == *owner, "cross-name CRD kind naming collision");
                }
            }
        }
        Ok(declarations)
    }
}

fn string<'a>(value: &'a Value, key: &str) -> eyre::Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("missing string {key}"))
}

fn digest(raw: &[u8]) -> String {
    hex(&Sha256::digest(raw))
}

fn input_digest(inputs: &[String]) -> String {
    let mut hash = Sha256::new();
    for input in inputs {
        hash.update(u64::try_from(input.len()).unwrap_or(u64::MAX).to_be_bytes());
        hash.update(input.as_bytes());
    }
    hex(&hash.finalize())
}

fn verify_response(response: &Value, key: &str, resource: &Value) -> eyre::Result<()> {
    eyre::ensure!(
        response.get("protocol").and_then(Value::as_str) == Some(PROTOCOL)
            && response.get("request_sha256").and_then(Value::as_str) == Some(key),
        "native response identity mismatch"
    );
    eyre::ensure!(
        matches!(
            response.get("verdict").and_then(Value::as_str),
            Some("Accepted" | "Rejected" | "Uncertain")
        ),
        "native response verdict missing"
    );
    if response.get("verdict").and_then(Value::as_str) == Some("Accepted") {
        eyre::ensure!(
            response
                .get("effective_json")
                .and_then(Value::as_str)
                .is_some(),
            "accepted native response lacks effective instance"
        );
    }
    if let Some(effective) = response.get("effective_json").and_then(Value::as_str) {
        eyre::ensure!(
            response.get("effective_sha256").and_then(Value::as_str)
                == Some(digest(effective.as_bytes()).as_str()),
            "effective response hash mismatch"
        );
        let effective: Value = serde_json::from_str(effective)?;
        eyre::ensure!(
            ["/apiVersion", "/kind", "/metadata/name"]
                .iter()
                .all(|pointer| effective.pointer(pointer) == resource.pointer(pointer)),
            "effective response resource identity mismatch"
        );
        if let Some(namespace) = resource
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .filter(|namespace| !namespace.is_empty())
        {
            eyre::ensure!(
                effective
                    .pointer("/metadata/namespace")
                    .and_then(Value::as_str)
                    == Some(namespace),
                "effective response namespace mismatch"
            );
        }
    }
    Ok(())
}
