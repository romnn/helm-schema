//! Adjudicates values against pinned Helm execution and offline Kubernetes schemas.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use helm_schema::output::LoadBudget;
use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{K8sSchemaProvider as _, KubernetesJsonSchemaProvider, ProviderLookupResult};
use indoc::indoc;
use serde::Deserialize as _;
use serde_json::Value;

/// Values obtained from template-free Helm execution, before chart mutation.
pub(crate) struct CoalescedValues(Value);

impl CoalescedValues {
    pub(crate) const fn as_json(&self) -> &Value {
        &self.0
    }
}

pub(crate) struct HelmProbe {
    pub(crate) values: CoalescedValues,
    pub(crate) rendered: Output,
    pub(crate) evidence_dir: PathBuf,
}

/// Private copies share chart inputs and differ only in their template bodies.
pub(crate) struct PinnedHelmChart {
    evidence_dir: PathBuf,
    render_chart: PathBuf,
    coalesce_chart: PathBuf,
}

impl PinnedHelmChart {
    pub(crate) fn prepare(chart_path: &Path) -> eyre::Result<Self> {
        require_pinned_helm()?;
        // Retain successful and failed cases so a verdict remains reproducible.
        let evidence_dir = tempfile::Builder::new()
            .prefix("helm-schema-adjudication-")
            .tempdir()?
            .keep();
        eprintln!("Helm adjudication evidence: {}", evidence_dir.display());
        let render_chart = evidence_dir.join("render");
        let coalesce_chart = evidence_dir.join("coalesce");
        let mut archive_budget = ArchiveBudget::default();
        copy_chart_tree(
            chart_path,
            &render_chart,
            chart_path,
            false,
            &mut archive_budget,
            0,
        )?;
        archive_budget = ArchiveBudget::default();
        copy_chart_tree(
            &render_chart,
            &coalesce_chart,
            &render_chart,
            true,
            &mut archive_budget,
            0,
        )?;
        eyre::ensure!(
            render_chart.join("Chart.yaml").is_file(),
            "chart has no manifest"
        );
        fs::create_dir_all(coalesce_chart.join("templates"))?;
        fs::write(
            coalesce_chart.join("templates/adjudication-values.yaml"),
            indoc! {r"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: adjudication-values
                data:
                  values: {{ .Values | toJson | quote }}
            "},
        )?;
        Ok(Self {
            evidence_dir,
            render_chart,
            coalesce_chart,
        })
    }

    /// Both executions read the same saved overlay, including when rendering aborts.
    pub(crate) fn adjudicate(&self, overlay: &Value) -> eyre::Result<HelmProbe> {
        let evidence_dir = self.new_case()?;
        let values = coalesce_in(&self.coalesce_chart, &evidence_dir, overlay)?;
        let rendered = run_helm(&self.render_chart, &evidence_dir, "render")?;
        Ok(HelmProbe {
            values,
            rendered,
            evidence_dir,
        })
    }

    fn new_case(&self) -> eyre::Result<PathBuf> {
        Ok(tempfile::Builder::new()
            .prefix("probe-")
            .tempdir_in(&self.evidence_dir)?
            .keep())
    }
}

fn require_pinned_helm() -> eyre::Result<()> {
    let version = Command::new("helm")
        .args(["version", "--template", "{{.Version}}"])
        .output()
        .wrap_err("read Helm version")?;
    eyre::ensure!(
        version.status.success() && version.stdout == b"v4.2.3",
        "adjudication requires Helm v4.2.3: {}",
        String::from_utf8_lossy(&version.stderr)
    );
    Ok(())
}

fn coalesce_in(chart: &Path, case: &Path, overlay: &Value) -> eyre::Result<CoalescedValues> {
    fs::write(
        case.join("values.json"),
        serde_json::to_vec_pretty(overlay)?,
    )?;
    let output = run_helm(chart, case, "coalesce")?;
    eyre::ensure!(
        output.status.success(),
        "Helm coalescence failed; evidence={}: {}",
        case.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut documents = serde_yaml::Deserializer::from_slice(&output.stdout);
    let document = Value::deserialize(documents.next().ok_or_eyre("missing values dump")?)?;
    eyre::ensure!(
        documents.next().is_none(),
        "multiple documents in values dump"
    );
    let json = document
        .pointer("/data/values")
        .and_then(Value::as_str)
        .ok_or_eyre("missing JSON values in Helm dump")?;
    let values: Value = serde_json::from_str(json)?;
    eyre::ensure!(values.is_object(), "Helm dumped a non-object values root");
    fs::write(
        case.join("coalesced.json"),
        serde_json::to_vec_pretty(&values)?,
    )?;
    Ok(CoalescedValues(values))
}

fn run_helm(chart: &Path, case: &Path, stage: &str) -> eyre::Result<Output> {
    let output = Command::new("helm")
        .args(["template", "adjudication"])
        .arg(chart)
        .args(["--kube-version", "1.29.0", "--skip-schema-validation", "-f"])
        .arg(case.join("values.json"))
        .output()
        .wrap_err_with(|| format!("run Helm {stage}; evidence={}", case.display()))?;
    fs::write(case.join(format!("{stage}.yaml")), &output.stdout)?;
    fs::write(case.join(format!("{stage}.stderr")), &output.stderr)?;
    fs::write(
        case.join(format!("{stage}.status")),
        output.status.to_string(),
    )?;
    Ok(output)
}

#[derive(Default)]
struct ArchiveBudget {
    entries: usize,
    bytes: u64,
}

fn copy_chart_tree(
    from: &Path,
    to: &Path,
    chart_root: &Path,
    template_free: bool,
    budget: &mut ArchiveBudget,
    depth: usize,
) -> eyre::Result<()> {
    eyre::ensure!(
        depth < 64,
        "chart directory nesting exceeds adjudication budget"
    );
    fs::create_dir_all(to)?;
    let mut entries = fs::read_dir(from)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        if (from == chart_root && name == "values.schema.json")
            || (from == chart_root && template_free && name == "templates")
            || (from == chart_root.join("templates") && name == "tests")
        {
            continue;
        }
        let source = entry.path();
        let destination = to.join(&name);
        let kind = entry.file_type()?;
        eyre::ensure!(
            !kind.is_symlink(),
            "chart symlink is unsupported: {}",
            source.display()
        );
        if kind.is_dir() {
            let child_root = if from == chart_root.join("charts")
                && (source.join("Chart.yaml").is_file()
                    || source.join("Chart.template.yaml").is_file())
            {
                &source
            } else {
                chart_root
            };
            copy_chart_tree(
                &source,
                &destination,
                child_root,
                template_free,
                budget,
                depth + 1,
            )?;
        } else if kind.is_file() {
            let archive = name.to_string_lossy();
            if from == chart_root.join("charts")
                && (archive.ends_with(".tgz") || archive.ends_with(".tar.gz"))
            {
                copy_chart_archive(&source, &destination, template_free, budget, depth + 1)?;
            } else {
                eyre::ensure!(
                    !destination.exists(),
                    "duplicate chart file: {}",
                    destination.display()
                );
                fs::copy(&source, &destination)?;
            }
        }
    }
    if from == chart_root
        && !to.join("Chart.yaml").exists()
        && to.join("Chart.template.yaml").is_file()
    {
        fs::copy(to.join("Chart.template.yaml"), to.join("Chart.yaml"))?;
    }
    Ok(())
}

fn copy_chart_archive(
    source: &Path,
    destination: &Path,
    template_free: bool,
    budget: &mut ArchiveBudget,
    depth: usize,
) -> eyre::Result<()> {
    let limits = LoadBudget::default();
    eyre::ensure!(
        source.metadata()?.len() <= u64::try_from(limits.max_chart_archive_bytes)?,
        "compressed chart exceeds adjudication budget: {}",
        source.display()
    );
    let scratch = tempfile::tempdir()?;
    let mut archive = tar::Archive::new(GzDecoder::new(fs::File::open(source)?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        budget.entries += 1;
        eyre::ensure!(
            budget.entries <= limits.max_chart_archive_entries,
            "chart archive entry budget exceeded"
        );
        let path = entry.path()?.into_owned();
        eyre::ensure!(
            path.components().all(|part| matches!(part, Component::Normal(name) if !name.to_string_lossy().contains('\\'))),
            "unsafe archive member: {}", path.display()
        );
        let kind = entry.header().entry_type();
        eyre::ensure!(
            kind.is_file() || kind.is_dir(),
            "unsupported archive member: {}",
            path.display()
        );
        let target = scratch.path().join(&path);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        let remaining =
            u64::try_from(limits.max_chart_archive_unpacked_bytes)?.saturating_sub(budget.bytes);
        eyre::ensure!(
            entry.size() <= remaining,
            "chart archive byte budget exceeded"
        );
        fs::create_dir_all(target.parent().ok_or_eyre("archive member has no parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        budget.bytes += std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut file)?;
        eyre::ensure!(
            budget.bytes <= u64::try_from(limits.max_chart_archive_unpacked_bytes)?,
            "chart archive byte budget exceeded"
        );
    }
    let mut roots = fs::read_dir(scratch.path())?.collect::<std::io::Result<Vec<_>>>()?;
    eyre::ensure!(
        roots.len() == 1,
        "chart archive must have one root directory"
    );
    let root = roots.pop().ok_or_eyre("chart archive has no root")?;
    let source_root = root.path();
    eyre::ensure!(
        source_root.is_dir()
            && (source_root.join("Chart.yaml").is_file()
                || source_root.join("Chart.template.yaml").is_file()),
        "chart archive root has no manifest"
    );
    let sanitized = tempfile::tempdir()?;
    copy_chart_tree(
        &source_root,
        sanitized.path(),
        &source_root,
        template_free,
        budget,
        depth,
    )?;
    // Preserve the archive boundary so parent .helmignore rules cannot filter child files.
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut archive = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    archive.append_dir_all(root.file_name(), sanitized.path())?;
    archive.into_inner()?.finish()?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KubernetesVerdict {
    Valid,
    Invalid(Vec<String>),
    Uncertain(Vec<String>),
}

pub(crate) struct OfflineKubernetesValidator {
    provider: KubernetesJsonSchemaProvider,
    validators: BTreeMap<(String, String), Result<jsonschema::Validator, String>>,
}

impl OfflineKubernetesValidator {
    pub(crate) fn new(cache: &Path) -> Self {
        Self {
            provider: KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
                .with_cache_dir(cache)
                .with_allow_download(false),
            validators: BTreeMap::new(),
        }
    }

    /// A proven violation is decisive even when another resource lacks a schema.
    pub(crate) fn validate(&mut self, rendered: &[u8]) -> eyre::Result<KubernetesVerdict> {
        let mut invalid = Vec::new();
        let mut uncertain = Vec::new();
        for (index, document) in Self::decode(rendered)?.into_iter().enumerate() {
            if document.is_null() {
                continue;
            }
            self.validate_document(
                &document,
                &format!("document {index}"),
                &mut invalid,
                &mut uncertain,
            );
        }
        Ok(if !invalid.is_empty() {
            KubernetesVerdict::Invalid(invalid)
        } else if !uncertain.is_empty() {
            KubernetesVerdict::Uncertain(uncertain)
        } else {
            KubernetesVerdict::Valid
        })
    }

    fn decode(rendered: &[u8]) -> eyre::Result<Vec<Value>> {
        let source = std::str::from_utf8(rendered).wrap_err("rendered YAML is not UTF-8")?;
        let documents = yaml_documents(source)?;
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let case = prepare_yaml_decoder()?;
        let chart = case.join("chart");
        fs::create_dir(chart.join("documents"))?;
        let mut filenames = Vec::new();
        // Helm parses values files as YAML, which can fold raw Unicode line breaks in strings.
        // Transport document bytes through chart files and pass only ASCII filenames as values.
        for (index, document) in documents.into_iter().enumerate() {
            let filename = format!("documents/{index}.yaml");
            fs::write(chart.join(&filename), document)?;
            filenames.push(filename);
        }
        fs::write(
            case.join("values.json"),
            serde_json::to_vec(&serde_json::json!({"documents": filenames}))?,
        )?;
        let output = run_helm(&chart, &case, "decode")?;
        eyre::ensure!(
            output.status.success(),
            "Helm YAML decoding failed; evidence={}: {}",
            case.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let document: Value = serde_yaml::from_slice(&output.stdout)?;
        let json = document
            .pointer("/data/documents")
            .and_then(Value::as_str)
            .ok_or_eyre("Helm YAML decoder did not return documents")?;
        let decoded = serde_json::from_str(json)?;
        fs::write(case.join("documents.json"), json)?;
        Ok(decoded)
    }

    fn validate_document(
        &mut self,
        document: &Value,
        location: &str,
        invalid: &mut Vec<String>,
        uncertain: &mut Vec<String>,
    ) {
        if let Some(error) = document.get("Error") {
            uncertain.push(format!(
                "{location}: Helm fromYaml reported an error: {error}"
            ));
            return;
        }
        if has_inexact_number(document) {
            uncertain.push(format!(
                "{location}: numeric magnitude exceeds Helm fromYaml's exact integer range"
            ));
            return;
        }
        let Some(api_version) = document.get("apiVersion").and_then(Value::as_str) else {
            uncertain.push(format!("{location}: no concrete apiVersion"));
            return;
        };
        let Some(kind) = document.get("kind").and_then(Value::as_str) else {
            uncertain.push(format!("{location}: no concrete kind"));
            return;
        };
        if api_version == "v1"
            && kind == "List"
            && let Some(items) = document.get("items").and_then(Value::as_array)
        {
            for (index, item) in items.iter().enumerate() {
                self.validate_document(
                    item,
                    &format!("{location}/items/{index}"),
                    invalid,
                    uncertain,
                );
            }
        }
        let key = (api_version.to_string(), kind.to_string());
        let validator = self
            .validators
            .entry(key)
            .or_insert_with(|| compile_resource_validator(&self.provider, api_version, kind));
        let name = document
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or("<unnamed>");
        let resource = format!("{location}: {api_version}/{kind} {name}");
        match validator {
            Ok(validator) => {
                for error in validator.iter_errors(document) {
                    invalid.push(format!("{resource}: {}: {error}", error.instance_path()));
                }
            }
            Err(error) => uncertain.push(format!("{resource}: {error}")),
        }
    }
}

fn compile_resource_validator(
    provider: &KubernetesJsonSchemaProvider,
    api_version: &str,
    kind: &str,
) -> Result<jsonschema::Validator, String> {
    let resource = ResourceRef::concrete(api_version.to_string(), kind.to_string());
    let fragment = match provider.lookup(&resource, &YamlPath::default()) {
        ProviderLookupResult::Found { schema, .. } => schema,
        ProviderLookupResult::NotOwned => {
            return Err("pinned resource schema not found".to_string());
        }
        ProviderLookupResult::PathUnresolved => {
            return Err("provider could not resolve the resource schema root".to_string());
        }
        ProviderLookupResult::ResourceDocMissing {
            io_error,
            source_path,
        } => {
            return Err(format!(
                "resource schema unavailable at {source_path}: {io_error}"
            ));
        }
    };
    // Materialized inference fragments may discard unresolved references.
    // The oracle requires the intact source, so a missing reference abstains.
    let (_, source) = fragment.into_source_parts();
    let source =
        source.ok_or_else(|| "provider did not preserve the original schema source".to_string())?;
    let mut options = jsonschema::options().with_retriever(NoRemoteSchemas);
    let mut draft = jsonschema::Draft::default().detect(source.source_schema());
    // The pinned Kubernetes bundle uses an unversioned legacy meta-schema URI.
    // Interpret that URI under the corpus's fixed Draft 7 policy.
    if source
        .source_schema()
        .get("$schema")
        .and_then(Value::as_str)
        == Some("http://json-schema.org/schema#")
    {
        draft = jsonschema::Draft::Draft7;
        options = options.with_draft(draft);
    }
    if !schema_declares_resource(source.source_schema(), api_version, kind, draft) {
        return Err(format!(
            "original schema does not prove exact identity {api_version}/{kind}"
        ));
    }
    options
        .build(source.source_schema())
        .map_err(|error| format!("original resource schema did not compile: {error}"))
}

fn schema_declares_resource(
    schema: &Value,
    api_version: &str,
    kind: &str,
    draft: jsonschema::Draft,
) -> bool {
    if draft == jsonschema::Draft::Unknown {
        return false;
    }
    let mut proven = false;
    if let Some(identities) = schema.get("x-kubernetes-group-version-kind") {
        let Some(identities) = identities.as_array() else {
            return false;
        };
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        let mut found = false;
        for identity in identities {
            let (Some(declared_group), Some(declared_version), Some(declared_kind)) = (
                identity.get("group").and_then(Value::as_str),
                identity.get("version").and_then(Value::as_str),
                identity.get("kind").and_then(Value::as_str),
            ) else {
                return false;
            };
            found |=
                declared_group == group && declared_version == version && declared_kind == kind;
        }
        if !found {
            return false;
        }
        proven = true;
    }
    // Older dialects ignore validation siblings of $ref, including identity constraints.
    if schema.get("$ref").is_some()
        && matches!(
            draft,
            jsonschema::Draft::Draft4 | jsonschema::Draft::Draft6 | jsonschema::Draft::Draft7
        )
    {
        return proven;
    }
    if draft != jsonschema::Draft::Draft4
        && let Some(constant) = schema.get("const")
    {
        if constant.get("apiVersion").and_then(Value::as_str) != Some(api_version)
            || constant.get("kind").and_then(Value::as_str) != Some(kind)
        {
            return false;
        }
        proven = true;
    }
    let mut declared_fields = 0;
    for (field, expected) in [("apiVersion", api_version), ("kind", kind)] {
        let Some(property) = schema
            .get("properties")
            .and_then(|properties| properties.get(field))
        else {
            continue;
        };
        let property_draft = draft.detect(property);
        if property_draft == jsonschema::Draft::Unknown {
            return false;
        }
        if property.get("$ref").is_some()
            && matches!(
                property_draft,
                jsonschema::Draft::Draft4 | jsonschema::Draft::Draft6 | jsonschema::Draft::Draft7
            )
        {
            continue;
        }
        let mut constrained = false;
        if property_draft != jsonschema::Draft::Draft4
            && let Some(constant) = property.get("const")
        {
            if constant.as_str() != Some(expected) {
                return false;
            }
            constrained = true;
        }
        if let Some(alternatives) = property.get("enum") {
            let Some(alternatives) = alternatives.as_array() else {
                return false;
            };
            if !alternatives
                .iter()
                .any(|value| value.as_str() == Some(expected))
            {
                return false;
            }
            constrained = true;
        }
        declared_fields += usize::from(constrained);
    }
    proven || declared_fields == 2
}

fn prepare_yaml_decoder() -> eyre::Result<PathBuf> {
    require_pinned_helm()?;
    let directory = tempfile::Builder::new()
        .prefix("helm-schema-yaml-decoder-")
        .tempdir()?
        .keep();
    let chart = directory.join("chart");
    fs::create_dir_all(chart.join("templates"))?;
    fs::write(
        chart.join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: yaml-decoder
        version: 1.0.0
    "},
    )?;
    // Helm's fromYaml uses the YAML 1.1 resolver used by Kubernetes decoding.
    // Keep original scalar spellings until this boundary, including mapping keys.
    fs::write(
        chart.join("templates/documents.yaml"),
        indoc! {r"
        {{- $documents := list -}}
        {{- range .Values.documents -}}
        {{- $documents = append $documents (fromYaml ($.Files.Get .)) -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: decoded
        data:
          documents: {{ $documents | toJson | quote }}
    "},
    )?;
    eprintln!("Helm YAML decoding evidence: {}", directory.display());
    Ok(directory)
}

fn has_inexact_number(value: &Value) -> bool {
    match value {
        // fromYaml decodes numbers into float64, unlike Kubernetes' integer-preserving decoder.
        // Include the boundary because an adjacent unrepresentable integer rounds onto it.
        Value::Number(number) => number
            .as_f64()
            .is_none_or(|number| number.abs() >= 9_007_199_254_740_992.0),
        Value::Array(values) => values.iter().any(has_inexact_number),
        Value::Object(values) => values.values().any(has_inexact_number),
        _ => false,
    }
}

fn yaml_documents(source: &str) -> eyre::Result<Vec<String>> {
    // Mirror Kubernetes YAMLReader's physical-line framing, not YAML's lexical document grammar.
    // Unicode breaks remain inside a chunk; Helm alone decodes its YAML semantics.
    let mut documents = Vec::new();
    let mut document = String::new();
    for line in source.lines() {
        if let Some(suffix) = line.strip_prefix("---") {
            let suffix = suffix.trim();
            eyre::ensure!(
                suffix.is_empty() || suffix.starts_with('#'),
                "invalid Kubernetes YAML document separator: {suffix}"
            );
            if !document.is_empty() {
                documents.push(std::mem::take(&mut document));
                continue;
            }
        }
        document.push_str(line);
        document.push('\n');
    }
    if !document.is_empty() {
        documents.push(document);
    }
    Ok(documents)
}

struct NoRemoteSchemas;

impl jsonschema::Retrieve for NoRemoteSchemas {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("offline adjudication cannot retrieve {uri}").into())
    }
}
