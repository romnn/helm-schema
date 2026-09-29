//! Adjudicates values against pinned Helm execution and offline Kubernetes schemas.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use color_eyre::eyre::{self, OptionExt as _};
use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use helm_schema::output::LoadBudget;
use helm_schema_core::{ApiPresenceQuery, ResourceRef, YamlPath};
use helm_schema_k8s::{
    CrdsCatalogSchemaProvider, K8sSchemaProvider, KubernetesJsonSchemaProvider,
    ProviderLookupResult, is_k8s_builtin_group,
};
use indoc::indoc;
use serde::Deserialize as _;
use serde_json::Value;
use test_util::helm_values::{AcceptanceDocument, ValuesError, acceptance_values};
use test_util::scratch::ScratchDir;

use crate::helm::cache_policy::render_cacheability;
use crate::helm::invocation::{
    Cacheability, HelmExecution, HelmRunner, InvocationRecord, PreparedTree, Replay,
    TemplateRequest, tree_sha256,
};
use crate::helm::kubernetes_version::chart_kubernetes_version;

/// Values obtained from template-free Helm execution, before chart mutation.
pub struct CoalescedValues(Value);

impl CoalescedValues {
    /// The coalesced values document.
    #[must_use]
    pub const fn as_json(&self) -> &Value {
        &self.0
    }
}

/// One adjudicated values file: Helm's coalesced values and its render.
pub struct HelmProbe {
    /// `None` when Helm aborts while coalescing values, e.g. on a non-table subchart scope.
    pub values: Option<CoalescedValues>,
    /// The render of the same values file.
    pub rendered: HelmExecution,
    /// The case directory holding both executions' inputs and outputs.
    pub evidence_dir: PathBuf,
}

/// A defaults document with the violations it carries.
type EvaluatedDocument = (Value, Vec<ViolationKey>);

/// Private copies share chart inputs and differ only in their template bodies.
pub struct PinnedHelmChart {
    /// The Helm every execution of this chart runs on.
    runner: &'static HelmRunner,
    evidence_dir: PathBuf,
    render_chart: PreparedTree,
    coalesce_chart: PreparedTree,
    /// The `--kube-version` both executions run under.
    kubernetes_version: &'static str,
    /// Whether renders of this chart may be replayed.
    render_cacheability: Cacheability,
    /// The defaults render's documents with their violations, computed once
    /// by the first probe that needs them; empty when Helm refuses the
    /// defaults, an error when the attempt itself failed.
    defaults_evaluation: OnceLock<Result<Vec<EvaluatedDocument>, String>>,
    /// Every Helm child this chart's adjudication ran, in execution order.
    invocations: Mutex<Vec<InvocationRecord>>,
    /// The largest peak resident set size among those children.
    peak_child_rss: AtomicU64,
}

impl PinnedHelmChart {
    /// Prepares the chart at `chart_path` for `runner`: a render copy without
    /// shipped `values.schema.json` files or `templates/tests`, and a
    /// template-free copy that dumps Helm's coalesced values.
    ///
    /// # Errors
    ///
    /// Returns an error when the chart cannot be copied, has no manifest, or
    /// no pinned Kubernetes version satisfies it.
    pub fn prepare(runner: &'static HelmRunner, chart_path: &Path) -> eyre::Result<Self> {
        // Retain successful and failed cases so a verdict remains reproducible.
        let evidence_dir = ScratchDir::new("adjudication")?.keep();
        eprintln!("Helm adjudication evidence: {}", evidence_dir.display());
        let render_chart = runner.staging_dir()?;
        let coalesce_chart = runner.staging_dir()?;
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
        let kubernetes_version = chart_kubernetes_version(&render_chart)?;
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
        let render_cacheability = render_cacheability(&render_chart)?;
        let render_chart = runner.publish_tree(&render_chart)?;
        let coalesce_chart = runner.publish_tree(&coalesce_chart)?;
        fs::write(
            evidence_dir.join("prepared.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "source": chart_path,
                "render": render_chart.path(),
                "coalesce": coalesce_chart.path(),
                "kubernetes_version": kubernetes_version,
                "render_cacheability": render_cacheability,
            }))?,
        )?;
        Ok(Self {
            runner,
            evidence_dir,
            render_chart,
            coalesce_chart,
            kubernetes_version,
            render_cacheability,
            defaults_evaluation: OnceLock::new(),
            invocations: Mutex::new(Vec::new()),
            peak_child_rss: AtomicU64::new(0),
        })
    }

    /// The prepared chart Helm renders.
    #[must_use]
    pub const fn render_tree(&self) -> &PreparedTree {
        &self.render_chart
    }

    /// The template-free copy that dumps Helm's coalesced values.
    #[must_use]
    pub const fn coalesce_tree(&self) -> &PreparedTree {
        &self.coalesce_chart
    }

    /// The Helm children run so far, in execution order.
    pub fn invocations(&self) -> Vec<InvocationRecord> {
        self.invocations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The largest peak resident set size of any Helm child run so far.
    pub fn peak_child_rss(&self) -> u64 {
        self.peak_child_rss.load(Ordering::Relaxed)
    }

    fn record(&self, record: &InvocationRecord) {
        self.peak_child_rss
            .fetch_max(record.max_rss_bytes, Ordering::Relaxed);
        self.invocations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(record.clone());
    }

    /// The Kubernetes version the chart renders under by default.
    #[must_use]
    pub const fn kubernetes_version(&self) -> &'static str {
        self.kubernetes_version
    }

    /// Both executions read the same saved overlay, including when rendering aborts.
    ///
    /// # Errors
    ///
    /// Returns an error when Helm cannot be run or its values dump cannot be read.
    pub fn adjudicate(&self, overlay: &Value) -> eyre::Result<HelmProbe> {
        self.adjudicate_as(
            &serde_json::to_vec_pretty(overlay)?,
            "render",
            self.kubernetes_version,
        )
    }

    /// Coalesces and renders the exact values file `values` under
    /// `kubernetes_version`: Helm reads these bytes, not a re-encoding.
    ///
    /// # Errors
    ///
    /// Returns an error when Helm cannot be run or its values dump cannot be read.
    pub fn adjudicate_file(
        &self,
        values: &[u8],
        kubernetes_version: &str,
    ) -> eyre::Result<HelmProbe> {
        self.adjudicate_as(values, "render", kubernetes_version)
    }

    fn adjudicate_as(
        &self,
        values: &[u8],
        stage: &str,
        kubernetes_version: &str,
    ) -> eyre::Result<HelmProbe> {
        let evidence_dir = self.new_case()?;
        fs::write(evidence_dir.join("values.json"), values)?;
        let (values, record) = coalesce_in(
            self.runner,
            &self.coalesce_chart,
            &evidence_dir,
            kubernetes_version,
        )?;
        self.record(&record);
        let rendered = run_helm(
            self.runner,
            &self.render_chart,
            &evidence_dir,
            stage,
            kubernetes_version,
            &self.render_cacheability,
        )?;
        self.record(&rendered.record);
        // Rendering coalesces the same values first, so it cannot survive a coalescence abort.
        eyre::ensure!(
            values.is_some() || !rendered.success(),
            "Helm rendered values it could not coalesce; evidence={}",
            crate::helm::invocation::preserve_failure(&evidence_dir).display()
        );
        Ok(HelmProbe {
            values,
            rendered,
            evidence_dir,
        })
    }

    /// The documents Helm's schema checks validate for `overlay`, composed
    /// by the Rust port over the same private chart copy Helm renders.
    /// `helm template` prints only the template document; lint's two are
    /// never printed.
    pub fn acceptance_documents(
        &self,
        overlay: &Value,
    ) -> BTreeMap<AcceptanceDocument, Result<Value, ValuesError>> {
        let mut documents = BTreeMap::new();
        for kind in AcceptanceDocument::ALL {
            let document = acceptance_values(self.render_chart.path(), overlay.clone(), kind)
                .map(|values| values.root);
            documents.insert(kind, document);
        }
        documents
    }

    fn new_case(&self) -> eyre::Result<PathBuf> {
        Ok(tempfile::Builder::new()
            .prefix("probe-")
            .tempdir_in(&self.evidence_dir)?
            .keep())
    }
}

/// Coalesces the values file saved in `case` in the template-free `chart`.
/// Returns `None` when Helm aborts before values reach templates.
///
/// Helm v4.2.3 `coalesceDeps` (pkg/chart/common/util/coalesce.go:118-119) aborts with "type
/// mismatch on <subchart>" when a subchart scope is not a table.
/// A non-table over a nested default table only prints "cannot overwrite table with non
/// table" (coalesce.go:350), keeps the user's value and continues, so that warning never
/// decides a verdict.
fn coalesce_in(
    runner: &HelmRunner,
    chart: &PreparedTree,
    case: &Path,
    kubernetes_version: &str,
) -> eyre::Result<(Option<CoalescedValues>, InvocationRecord)> {
    // The template-free copy renders only the fixed values dump, a chart of ours.
    let output = run_helm(
        runner,
        chart,
        case,
        "coalesce",
        kubernetes_version,
        &Cacheability::trusted(Replay::Cacheable),
    )?;
    if !output.success() {
        return Ok((None, output.record));
    }
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
    Ok((Some(CoalescedValues(values)), output.record))
}

/// Renders `chart` with the values file saved in `case`.
fn run_helm(
    runner: &HelmRunner,
    chart: &PreparedTree,
    case: &Path,
    stage: &str,
    kubernetes_version: &str,
    cacheability: &Cacheability,
) -> eyre::Result<HelmExecution> {
    let values = fs::read(case.join("values.json"))?;
    let request = TemplateRequest {
        chart,
        values: &values,
        kubernetes_version,
    };
    runner.template(&request, case, stage, cacheability)
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
    let scratch = ScratchDir::new("chart-archive")?;
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
    let sanitized = ScratchDir::new("chart-sanitized")?;
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
    // A fixed gzip header and sorted, normalized tar headers make the bytes a
    // function of the sanitized content alone.
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(file, Compression::default());
    let mut archive = tar::Builder::new(encoder);
    append_sorted(&mut archive, sanitized.path(), Path::new(&root.file_name()))?;
    archive.into_inner()?.finish()?;
    Ok(())
}

/// Appends `directory` as `name` with its entries in sorted order, every
/// header owned by uid and gid 0 with no owner names, a zero mtime, and mode
/// 0755 for directories and 0644 for files.
fn append_sorted<W: std::io::Write>(
    archive: &mut tar::Builder<W>,
    directory: &Path,
    name: &Path,
) -> eyre::Result<()> {
    let mut header = normalized_header(tar::EntryType::Directory, 0o755, 0);
    archive.append_data(&mut header, name, std::io::empty())?;
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = name.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            append_sorted(archive, &entry.path(), &path)?;
        } else {
            eyre::ensure!(
                kind.is_file(),
                "unsupported chart entry: {}",
                path.display()
            );
            let file = fs::File::open(entry.path())?;
            let mut header =
                normalized_header(tar::EntryType::Regular, 0o644, file.metadata()?.len());
            archive.append_data(&mut header, &path, file)?;
        }
    }
    Ok(())
}

fn normalized_header(kind: tar::EntryType, mode: u32, size: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_mode(mode);
    header.set_size(size);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header
}

/// Whether rendered resources are valid under the pinned Kubernetes schemas.
#[derive(Debug, PartialEq, Eq)]
pub enum KubernetesVerdict {
    /// Every resource was validated and none violates its schema.
    Valid,
    /// These violations are proven.
    Invalid(Vec<KubernetesViolation>),
    /// No violation is proven, but these resources could not be decided.
    Uncertain(Vec<String>),
}

/// A violated assertion of a validated document, independent of the
/// document's position in a render and of its name, which a probe may change
/// without changing what the document violates. A values document has no
/// `api_version` or `kind`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViolationKey {
    /// The resource's apiVersion; empty for a values document.
    pub api_version: String,
    /// The resource's kind; empty for a values document.
    pub kind: String,
    /// Pointer to the violating value.
    pub instance_path: String,
    /// Keyword location of the violated assertion.
    pub schema_path: String,
    /// The violated keyword.
    pub keyword: String,
    /// What tells violations of one assertion apart: the property a
    /// `required` misses, the properties `additionalProperties` rejects,
    /// else the offending value when it is a scalar and its type when not.
    pub detail: String,
}

impl ViolationKey {
    /// The key of `error` in a document of `api_version`/`kind`.
    #[must_use]
    pub fn new(api_version: &str, kind: &str, error: &jsonschema::ValidationError<'_>) -> Self {
        let detail = match error.kind() {
            jsonschema::error::ValidationErrorKind::Required { property } => property.to_string(),
            jsonschema::error::ValidationErrorKind::AdditionalProperties { unexpected } => {
                unexpected.join(", ")
            }
            _ => match error.instance().as_ref() {
                Value::Object(_) => "object".to_string(),
                Value::Array(_) => "array".to_string(),
                scalar => scalar.to_string(),
            },
        };
        Self {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            instance_path: error.instance_path().to_string(),
            schema_path: error.schema_path().to_string(),
            keyword: error.kind().keyword().to_string(),
            detail,
        }
    }
}

impl fmt::Display for ViolationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {} {} at {}",
            self.instance_path, self.keyword, self.detail, self.schema_path
        )
    }
}

/// One schema violation of a rendered resource.
#[derive(Debug, PartialEq, Eq)]
pub struct KubernetesViolation {
    /// Position in the rendered stream, such as `document 2/items/0`.
    pub location: String,
    /// The resource's namespace, if it names one.
    pub namespace: Option<String>,
    /// The resource's name, if it has one.
    pub name: Option<String>,
    /// What is violated.
    pub key: ViolationKey,
    /// The validator's message.
    pub message: String,
}

impl KubernetesViolation {
    /// A document without a string apiVersion and kind.
    fn unidentified(location: &str) -> Self {
        Self {
            location: location.to_string(),
            namespace: None,
            name: None,
            key: ViolationKey {
                api_version: String::new(),
                kind: String::new(),
                instance_path: String::new(),
                schema_path: "/required".to_string(),
                keyword: "required".to_string(),
                detail: "apiVersion, kind".to_string(),
            },
            message: "a resource needs a string apiVersion and kind".to_string(),
        }
    }
}

impl fmt::Display for KubernetesViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = &self.key;
        let name = self.name.as_deref().unwrap_or("<unnamed>");
        match &self.namespace {
            Some(namespace) => write!(
                formatter,
                "{}: {}/{} {namespace}/{name}: {}: {}",
                self.location, key.api_version, key.kind, key.instance_path, self.message
            ),
            None => write!(
                formatter,
                "{}: {}/{} {name}: {}: {}",
                self.location, key.api_version, key.kind, key.instance_path, self.message
            ),
        }
    }
}

/// A probe render judged against the chart's defaults render.
#[derive(Debug, Default)]
pub struct DefaultsComparison {
    /// Violations of new or changed documents beyond what the unpaired
    /// defaults documents carry.
    pub new_violations: Vec<KubernetesViolation>,
    /// Violations of new or changed documents that the unpaired defaults
    /// documents carry too, counted with multiplicity.
    pub inherited_violations: Vec<KubernetesViolation>,
    /// New or changed documents whose validity could not be decided.
    pub uncertain: Vec<String>,
}

#[derive(Default)]
struct ResourceEvidence {
    invalid: Vec<KubernetesViolation>,
    uncertain: Vec<String>,
}

/// The Kubernetes release the corpus's offline bundle validates against.
pub const KUBERNETES_RELEASE: &str = "v1.29.0-standalone-strict";

/// How resource schemas are compiled and judged. Changing the compilation
/// options or the identity proof must change this.
const VALIDATOR_POLICY: &str = "helm-schema/offline-kubernetes-validator/v1";

/// A compiled resource schema, or why the resource cannot be decided.
type CompiledSchema = Arc<OnceLock<Result<Arc<jsonschema::Validator>, String>>>;

/// Compiled schemas by `(apiVersion, kind)`, compiled once each.
#[derive(Default)]
struct CompiledSchemas(Mutex<BTreeMap<(String, String), CompiledSchema>>);

/// Compiled schemas shared by every validator of this process, keyed by the
/// validator policy, the Kubernetes release, and the content of the pinned
/// bundles, never by their location.
static COMPILED_SCHEMAS: OnceLock<Mutex<BTreeMap<String, Arc<CompiledSchemas>>>> = OnceLock::new();

/// Validates rendered resources against pinned, offline Kubernetes (and CRD)
/// schema bundles.
pub struct OfflineKubernetesValidator {
    provider: KubernetesJsonSchemaProvider,
    /// The pinned CRD catalog that decides non-built-in kinds, if any.
    crds: Option<CrdsCatalogSchemaProvider>,
    /// The pinned bundle directories and the content identity they had when
    /// this validator was made.
    bundles: Vec<(PathBuf, String)>,
    schemas: Arc<CompiledSchemas>,
}

impl OfflineKubernetesValidator {
    /// Judges resources by the offline Kubernetes bundle cached at `cache`
    /// for `release` (such as [`KUBERNETES_RELEASE`]).
    ///
    /// # Errors
    ///
    /// Returns an error when the bundle cannot be read.
    pub fn new(cache: &Path, release: &str) -> eyre::Result<Self> {
        Self::with_bundles(cache, None, release)
    }

    /// Also judges CRD kinds by the schemas pinned in the CRD catalog cache
    /// `crds`.
    ///
    /// # Errors
    ///
    /// Returns an error when either bundle cannot be read.
    pub fn with_crd_catalog(cache: &Path, crds: &Path, release: &str) -> eyre::Result<Self> {
        Self::with_bundles(cache, Some(crds), release)
    }

    fn with_bundles(cache: &Path, crds: Option<&Path>, release: &str) -> eyre::Result<Self> {
        let mut bundles = vec![(cache.to_path_buf(), tree_sha256(cache)?)];
        if let Some(crds) = crds {
            bundles.push((crds.to_path_buf(), tree_sha256(crds)?));
        }
        let mut identity = format!("{VALIDATOR_POLICY}\n{release}\n");
        for (_, sha256) in &bundles {
            identity.push_str(sha256);
            identity.push('\n');
        }
        if crds.is_none() {
            identity.push_str("no CRD catalog\n");
        }
        let schemas = COMPILED_SCHEMAS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(identity)
            .or_default()
            .clone();
        Ok(Self {
            provider: KubernetesJsonSchemaProvider::new(release)
                .with_cache_dir(cache)
                .with_allow_download(false),
            crds: crds.map(|crds| {
                CrdsCatalogSchemaProvider::new()
                    .with_cache_dir(crds)
                    .with_allow_download(false)
            }),
            bundles,
            schemas,
        })
    }

    /// Fails when a pinned bundle changed after this validator was made,
    /// which would make its shared compiled schemas stale.
    ///
    /// # Errors
    ///
    /// Returns an error naming the changed bundle.
    pub fn verify_bundles_unchanged(&self) -> eyre::Result<()> {
        for (path, sha256) in &self.bundles {
            eyre::ensure!(
                tree_sha256(path)? == *sha256,
                "pinned schema bundle {} changed during the run",
                path.display()
            );
        }
        Ok(())
    }

    /// A proven violation is decisive even when another resource lacks a schema.
    /// `runner` decodes the YAML as Kubernetes would, through Helm's `fromYaml`.
    ///
    /// # Errors
    ///
    /// Returns an error when the decoder cannot be run or rejects `rendered`.
    pub fn validate(
        &self,
        runner: &HelmRunner,
        rendered: &[u8],
    ) -> eyre::Result<KubernetesVerdict> {
        let case = ScratchDir::new("yaml-decoder")?.keep();
        let documents = decode(runner, rendered, &case)?
            .documents
            .map_err(|rejection| eyre::eyre!("{rejection}"))?;
        let mut evidence = ResourceEvidence::default();
        for (index, document) in documents.iter().enumerate() {
            if !document.is_null() {
                self.validate_document(document, &format!("document {index}"), &mut evidence);
            }
        }
        Ok(if !evidence.invalid.is_empty() {
            KubernetesVerdict::Invalid(evidence.invalid)
        } else if !evidence.uncertain.is_empty() {
            KubernetesVerdict::Uncertain(evidence.uncertain)
        } else {
            KubernetesVerdict::Valid
        })
    }

    /// Judges `rendered` against the chart's defaults render, which a schema
    /// must accept even when it violates Kubernetes.
    ///
    /// A probe document equal to a still unpaired defaults document is
    /// unchanged and needs no proof. Every other document must be validated
    /// completely; its violations are inherited only as far as the unpaired
    /// defaults documents carry the same ones, counted with multiplicity. A
    /// failed defaults render leaves no baseline, so every document is new.
    ///
    /// # Errors
    ///
    /// Returns an error when Helm cannot render or decode the defaults or the probe.
    pub fn compare_with_defaults(
        &self,
        chart: &PinnedHelmChart,
        probe: &HelmProbe,
    ) -> eyre::Result<DefaultsComparison> {
        let defaults = self.defaults_evaluation(chart)?;
        let mut unpaired: Vec<&(Value, Vec<ViolationKey>)> = defaults.iter().collect();
        let mut changed = ResourceEvidence::default();
        let decoded = decode(chart.runner, &probe.rendered.stdout, &probe.evidence_dir)?;
        if let Some(record) = &decoded.record {
            chart.record(record);
        }
        let documents = decoded
            .documents
            .map_err(|rejection| eyre::eyre!("{rejection}"))?;
        for (index, document) in documents.iter().enumerate() {
            if document.is_null() {
                continue;
            }
            // A decoder error or a rounded number hides what the document was.
            let comparable = document.get("Error").is_none() && !has_inexact_number(document);
            let pair = unpaired
                .iter()
                .position(|(default, _)| comparable && default == document);
            match pair {
                Some(position) => {
                    unpaired.remove(position);
                }
                None => {
                    self.validate_document(document, &format!("document {index}"), &mut changed);
                }
            }
        }
        let mut allowed = BTreeMap::<&ViolationKey, usize>::new();
        for (_, keys) in &unpaired {
            for key in keys {
                *allowed.entry(key).or_default() += 1;
            }
        }
        let mut comparison = DefaultsComparison {
            uncertain: changed.uncertain,
            ..DefaultsComparison::default()
        };
        for violation in changed.invalid {
            match allowed.get_mut(&violation.key) {
                Some(remaining) if *remaining > 0 => {
                    *remaining -= 1;
                    comparison.inherited_violations.push(violation);
                }
                _ => comparison.new_violations.push(violation),
            }
        }
        Ok(comparison)
    }

    /// The defaults render's documents with their violations, computed once
    /// per chart by the first probe that needs them, inline. Helm refusing
    /// the defaults leaves no baseline; failing to run Helm is an error.
    fn defaults_evaluation<'c>(
        &self,
        chart: &'c PinnedHelmChart,
    ) -> eyre::Result<&'c [(Value, Vec<ViolationKey>)]> {
        chart
            .defaults_evaluation
            .get_or_init(|| {
                self.evaluate_defaults(chart)
                    .map_err(|error| format!("{error:?}"))
            })
            .as_deref()
            .map_err(|error| eyre::eyre!("defaults render could not be adjudicated: {error}"))
    }

    fn evaluate_defaults(
        &self,
        chart: &PinnedHelmChart,
    ) -> eyre::Result<Vec<(Value, Vec<ViolationKey>)>> {
        let probe = chart.adjudicate_as(
            &serde_json::to_vec_pretty(&serde_json::json!({}))?,
            "control",
            chart.kubernetes_version,
        )?;
        if !probe.rendered.success() {
            return Ok(Vec::new());
        }
        let decoded = decode(chart.runner, &probe.rendered.stdout, &probe.evidence_dir)?;
        if let Some(record) = &decoded.record {
            chart.record(record);
        }
        let Ok(control) = decoded.documents else {
            return Ok(Vec::new());
        };
        let mut evaluated = Vec::new();
        for document in control {
            if document.is_null() {
                continue;
            }
            let mut evidence = ResourceEvidence::default();
            self.validate_document(&document, "defaults", &mut evidence);
            let keys = evidence
                .invalid
                .into_iter()
                .map(|violation| violation.key)
                .collect();
            evaluated.push((document, keys));
        }
        Ok(evaluated)
    }

    /// The shared compiled schema of `apiVersion`/`kind`, compiled once.
    fn compiled_validator(
        &self,
        api_version: &str,
        kind: &str,
    ) -> Result<Arc<jsonschema::Validator>, String> {
        let key = (api_version.to_string(), kind.to_string());
        let group = api_version.split_once('/').map_or("", |(group, _)| group);
        // Hold the map only to find the cell; compile outside it.
        let cell = self
            .schemas
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key)
            .or_default()
            .clone();
        cell.get_or_init(|| {
            match &self.crds {
                Some(crds) if !is_k8s_builtin_group(group) => {
                    compile_crd_validator(crds, api_version, kind)
                }
                _ => compile_resource_validator(&self.provider, api_version, kind),
            }
            .map(Arc::new)
        })
        .clone()
    }

    fn validate_document(&self, document: &Value, location: &str, evidence: &mut ResourceEvidence) {
        if let Some(error) = document.get("Error") {
            evidence.uncertain.push(format!(
                "{location}: Helm fromYaml reported an error: {error}"
            ));
            return;
        }
        if has_inexact_number(document) {
            evidence.uncertain.push(format!(
                "{location}: numeric magnitude exceeds Helm fromYaml's exact integer range"
            ));
            return;
        }
        // A document without a string apiVersion and kind is no resource
        // Kubernetes can accept, such as the `{}` an empty list item renders.
        let (Some(api_version), Some(kind)) = (
            document.get("apiVersion").and_then(Value::as_str),
            document.get("kind").and_then(Value::as_str),
        ) else {
            evidence
                .invalid
                .push(KubernetesViolation::unidentified(location));
            return;
        };
        if api_version == "v1"
            && kind == "List"
            && let Some(items) = document.get("items").and_then(Value::as_array)
        {
            for (index, item) in items.iter().enumerate() {
                self.validate_document(item, &format!("{location}/items/{index}"), evidence);
            }
        }
        let validator = self.compiled_validator(api_version, kind);
        let name = document
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .map(str::to_string);
        let namespace = document
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .map(str::to_string);
        match &validator {
            Ok(validator) => {
                for error in validator.iter_errors(document) {
                    evidence.invalid.push(KubernetesViolation {
                        location: location.to_string(),
                        namespace: namespace.clone(),
                        name: name.clone(),
                        key: ViolationKey::new(api_version, kind, &error),
                        message: error.to_string(),
                    });
                }
            }
            Err(error) => {
                // The bundle records a built-in group's kind as absent
                // upstream: the pinned server does not serve it. Any other
                // missing schema, a CRD's above all, stays undecided.
                let query = ApiPresenceQuery::Resource {
                    api_version: api_version.to_string(),
                    kind: kind.to_string(),
                };
                if self
                    .provider
                    .capability_has_query_at_primary_version(&query)
                    == Some(false)
                {
                    evidence.invalid.push(KubernetesViolation {
                        location: location.to_string(),
                        namespace,
                        name,
                        key: ViolationKey {
                            api_version: api_version.to_string(),
                            kind: kind.to_string(),
                            instance_path: "/apiVersion".to_string(),
                            schema_path: "/served".to_string(),
                            keyword: "served".to_string(),
                            detail: String::new(),
                        },
                        message: format!(
                            "{api_version}/{kind} is not served by the pinned Kubernetes version"
                        ),
                    });
                    return;
                }
                let name = name.as_deref().unwrap_or("<unnamed>");
                evidence
                    .uncertain
                    .push(format!("{location}: {api_version}/{kind} {name}: {error}"));
            }
        }
    }
}

/// Documents decoded by Helm's `fromYaml`, or why Helm could not decode them,
/// with the Helm child's record when one ran.
pub struct Decoded {
    /// The decoded documents, or Helm's reason for rejecting the YAML.
    pub documents: Result<Vec<Value>, String>,
    record: Option<InvocationRecord>,
}

/// Decodes `rendered` through Helm's `fromYaml` in the directory `decode`
/// under `case`.
///
/// # Errors
///
/// Only a failure to run the decoder is an error.
pub fn decode(runner: &HelmRunner, rendered: &[u8], case: &Path) -> eyre::Result<Decoded> {
    let rejected = |rejection: String| Decoded {
        documents: Err(rejection),
        record: None,
    };
    let Ok(source) = std::str::from_utf8(rendered) else {
        return Ok(rejected("rendered YAML is not UTF-8".to_string()));
    };
    let documents = match yaml_documents(source) {
        Ok(documents) => documents,
        Err(error) => return Ok(rejected(error.to_string())),
    };
    if documents.is_empty() {
        return Ok(Decoded {
            documents: Ok(Vec::new()),
            record: None,
        });
    }
    // A document of comments alone parses as null: Kubernetes' decoder
    // skips it, where Helm's `fromYaml` would read it as `{}`.
    let mut empty = Vec::new();
    for document in &documents {
        empty.push(matches!(
            serde_yaml::from_str::<serde_yaml::Value>(document),
            Ok(serde_yaml::Value::Null)
        ));
    }
    let case = case.join("decode");
    fs::create_dir(&case)?;
    let chart = runner.staging_dir()?;
    write_yaml_decoder(&chart)?;
    fs::create_dir(chart.join("documents"))?;
    let mut filenames = Vec::new();
    // Helm parses values files as YAML, which can fold raw Unicode line breaks in strings.
    // Transport document bytes through chart files and pass only ASCII filenames as values.
    for (index, document) in documents.into_iter().enumerate() {
        let filename = format!("documents/{index}.yaml");
        fs::write(chart.join(&filename), document)?;
        filenames.push(filename);
    }
    let kubernetes_version = chart_kubernetes_version(&chart)?;
    let chart = runner.publish_tree(&chart)?;
    fs::write(
        case.join("values.json"),
        serde_json::to_vec(&serde_json::json!({"documents": filenames}))?,
    )?;
    // The runner records the decoder chart (documents included); a preserved
    // failure also needs the YAML those documents were split from.
    fs::write(case.join("rendered.yaml"), rendered)?;
    // The decoder chart is fixed; its documents are part of its content.
    let values = fs::read(case.join("values.json"))?;
    let request = TemplateRequest {
        chart: &chart,
        values: &values,
        kubernetes_version,
    };
    let output = runner.template(
        &request,
        &case,
        "decode",
        &Cacheability::trusted(Replay::Cacheable),
    )?;
    let documents = if output.success() {
        decoder_documents(&output.stdout, &case, empty)
    } else {
        Err(format!(
            "Helm YAML decoding failed; evidence={}: {}",
            crate::helm::invocation::preserve_failure(&case).display(),
            String::from_utf8_lossy(&output.stderr)
        ))
    };
    Ok(Decoded {
        documents,
        record: Some(output.record),
    })
}

fn decoder_documents(stdout: &[u8], case: &Path, empty: Vec<bool>) -> Result<Vec<Value>, String> {
    let document: Value = serde_yaml::from_slice(stdout).map_err(|error| error.to_string())?;
    let json = document
        .pointer("/data/documents")
        .and_then(Value::as_str)
        .ok_or("Helm YAML decoder did not return documents")?;
    let mut decoded: Vec<Value> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    fs::write(case.join("documents.json"), json).map_err(|error| error.to_string())?;
    for (document, empty) in decoded.iter_mut().zip(empty) {
        if empty {
            *document = Value::Null;
        }
    }
    Ok(decoded)
}

/// A CRD catalog schema carries no identity of its own; the catalog
/// addresses it by exact group, kind and version, which is its identity. The
/// provider hands out the document root with the `ObjectMeta` typing every
/// custom resource's `metadata` gets, and the document has no references to
/// lose. It is a structural `OpenAPI` schema, judged under the corpus's Draft 7.
fn compile_crd_validator(
    crds: &CrdsCatalogSchemaProvider,
    api_version: &str,
    kind: &str,
) -> Result<jsonschema::Validator, String> {
    let resource = ResourceRef::concrete(api_version.to_string(), kind.to_string());
    let ProviderLookupResult::Found { schema, .. } = crds.lookup(&resource, &YamlPath::default())
    else {
        return Err("pinned CRD schema not found".to_string());
    };
    jsonschema::options()
        .with_retriever(NoRemoteSchemas)
        .with_draft(jsonschema::Draft::Draft7)
        .build(schema.schema())
        .map_err(|error| format!("pinned CRD schema did not compile: {error}"))
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

fn write_yaml_decoder(chart: &Path) -> eyre::Result<()> {
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
    Ok(())
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
