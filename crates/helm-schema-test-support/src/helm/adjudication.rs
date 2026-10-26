//! Adjudicates values against pinned Helm execution and offline Kubernetes schemas.

use std::collections::{BTreeMap, BTreeSet};
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
use serde_json::{Value, json};
use test_util::helm_values::{AcceptanceDocument, ValuesError, acceptance_values};
use test_util::scratch::ScratchDir;

use crate::helm::assertion_identity::{
    InstanceSegment, assertion_provenance, canonical_instance_path, canonical_schema_location,
    verify_compiler_locations, verify_execution, verify_registry_integrity,
};
use crate::helm::cache_policy::render_cacheability;
use crate::helm::invocation::{
    Cacheability, HelmExecution, HelmRunner, InvocationRecord, PreparedTree, Replay,
    TemplateRequest, tree_sha256,
};
use crate::helm::kubernetes_version::chart_kubernetes_version;

/// A defaults or context Kubernetes control the oracle cannot use: its
/// render holds resources no pinned schema decides, or new assertions
/// versus the defaults. The pinned oracle cannot decide such a document;
/// nothing failed to execute.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct UnusableControl(pub String);

/// Values obtained from template-free Helm execution, before chart mutation.
#[derive(Clone)]
pub struct CoalescedValues(Value);

impl CoalescedValues {
    /// The coalesced values document.
    #[must_use]
    pub const fn as_json(&self) -> &Value {
        &self.0
    }
}

/// One adjudicated values file: Helm's coalesced values and its render.
#[derive(Clone)]
pub struct HelmProbe {
    /// `None` when Helm aborts while coalescing values, e.g. on a non-table subchart scope.
    pub values: Option<CoalescedValues>,
    /// The render of the same values file.
    pub rendered: HelmExecution,
    /// The case directory holding both executions' inputs and outputs.
    pub evidence_dir: PathBuf,
}

impl HelmProbe {
    /// Reloads a values-file observation without retaining its rendered or coalesced bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or malformed evidence.
    pub fn read(evidence_dir: &Path) -> eyre::Result<Self> {
        let coalesce = InvocationRecord::read(evidence_dir, "coalesce")?;
        let values = if coalesce.exit_code == 0 {
            Some(CoalescedValues(serde_json::from_slice(&fs::read(
                evidence_dir.join("coalesced.json"),
            )?)?))
        } else {
            None
        };
        Ok(Self {
            values,
            rendered: HelmExecution::read(evidence_dir, "render")?,
            evidence_dir: evidence_dir.to_owned(),
        })
    }
}

/// Exact inputs for comparing Helm lint with two root schemas.
pub struct LintProbeRequest<'a> {
    /// The values file bytes read unchanged by all three executions.
    pub values: &'a [u8],
    /// The explicit Kubernetes context used for all executions.
    pub kubernetes_version: &'a str,
    /// The root baseline `values.schema.json` bytes.
    pub baseline_schema: &'a [u8],
    /// The root candidate `values.schema.json` bytes.
    pub candidate_schema: &'a [u8],
}

/// Three real lint executions over identical chart and values inputs.
pub struct HelmLintProbe {
    /// Lint with the root baseline schema installed.
    pub baseline: HelmExecution,
    /// Lint with the root candidate schema installed.
    pub candidate: HelmExecution,
    /// Lint with all chart and dependency schemas removed.
    pub schema_free: HelmExecution,
    /// The directory retaining the exact overlay and invocation evidence.
    pub evidence_dir: PathBuf,
}

/// One schema-free lint control and its exact execution evidence.
pub struct SchemaFreeLintProbe {
    /// Lint with chart and dependency schemas removed.
    pub execution: HelmExecution,
    /// The directory retaining the exact values file and invocation record.
    pub evidence_dir: PathBuf,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct LintProbeKey {
    values: Vec<u8>,
    kubernetes_version: String,
    baseline_tree: String,
    candidate_tree: String,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct DefaultsControlKey {
    kubernetes_version: String,
    schema_identity: String,
}

/// Defaults evidence shared only under the same Kubernetes version and validator inputs.
type DefaultsEvaluation = Result<Arc<ResourceEvidence>, DefaultsFailure>;

/// A memoized defaults-control failure: its message, and whether the
/// control was unusable rather than failed.
#[derive(Clone, Debug)]
struct DefaultsFailure {
    message: String,
    unusable: bool,
}

/// Private copies share chart inputs and differ only in their template bodies.
pub struct PinnedHelmChart {
    /// The Helm every execution of this chart runs on.
    runner: &'static HelmRunner,
    evidence_dir: PathBuf,
    render_chart: PreparedTree,
    coalesce_chart: PreparedTree,
    /// The `--kube-version` both executions run under by default: the corpus policy's choice for the
    /// chart, or none for a chart prepared for explicit versions only.
    kubernetes_version: Option<&'static str>,
    /// Whether renders of this chart may be replayed.
    render_cacheability: Cacheability,
    /// Defaults controls bound to their render version and validator policy/bundle content.
    defaults_controls: Mutex<BTreeMap<DefaultsControlKey, DefaultsEvaluation>>,
    /// Immutable schema-bearing trees, prepared only when lint needs them.
    lint_trees: Mutex<BTreeMap<Vec<u8>, PreparedTree>>,
    /// Successful harness executions shared across document kinds in this process.
    lint_probes: Mutex<BTreeMap<LintProbeKey, Arc<HelmLintProbe>>>,
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
        Self::prepare_with(runner, chart_path, true)
    }

    /// Like [`PinnedHelmChart::prepare`], without choosing a default Kubernetes version: every
    /// execution names its own ([`PinnedHelmChart::adjudicate_file`]), and whether the chart's
    /// `kubeVersion` admits it is Helm's decision.
    ///
    /// # Errors
    ///
    /// Returns an error when the chart cannot be copied or has no manifest.
    pub fn prepare_for_explicit_versions(
        runner: &'static HelmRunner,
        chart_path: &Path,
    ) -> eyre::Result<Self> {
        Self::prepare_with(runner, chart_path, false)
    }

    fn prepare_with(
        runner: &'static HelmRunner,
        chart_path: &Path,
        select_version: bool,
    ) -> eyre::Result<Self> {
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
        let kubernetes_version = if select_version {
            Some(chart_kubernetes_version(&render_chart)?)
        } else {
            None
        };
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
            defaults_controls: Mutex::new(BTreeMap::new()),
            lint_trees: Mutex::new(BTreeMap::new()),
            lint_probes: Mutex::new(BTreeMap::new()),
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

    /// Whether renders of this chart may be replayed.
    #[must_use]
    pub const fn render_cacheability(&self) -> &Cacheability {
        &self.render_cacheability
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

    /// The Kubernetes version the chart renders under by default, if it has one.
    #[must_use]
    pub const fn kubernetes_version(&self) -> Option<&'static str> {
        self.kubernetes_version
    }

    fn default_version(&self) -> eyre::Result<&'static str> {
        self.kubernetes_version
            .ok_or_eyre("the chart was prepared for explicit Kubernetes versions only")
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
            self.default_version()?,
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

    /// Runs pinned Helm lint with baseline, candidate, and no root schema.
    ///
    /// Dependency schemas are absent in every execution.
    /// Deterministic charts share the complete result for identical values bytes,
    /// Kubernetes version, and schema bytes within this prepared chart's lifetime.
    /// Each actual invocation is included in [`Self::invocations`].
    ///
    /// # Errors
    ///
    /// Returns an error when chart preparation or a lint invocation fails operationally.
    /// A normal lint rejection remains an execution result for rule classification.
    pub fn adjudicate_lint(
        &self,
        request: &LintProbeRequest<'_>,
    ) -> eyre::Result<Arc<HelmLintProbe>> {
        let baseline_chart = self.lint_tree(request.baseline_schema)?;
        let candidate_chart = self.lint_tree(request.candidate_schema)?;
        let key = LintProbeKey {
            values: request.values.to_vec(),
            kubernetes_version: request.kubernetes_version.to_owned(),
            baseline_tree: baseline_chart.sha256().to_owned(),
            candidate_tree: candidate_chart.sha256().to_owned(),
        };
        // Only replayable operations need coordinated lookup and execution.
        // Fresh nondeterministic observations of unrelated inputs can run concurrently.
        let mut probes =
            matches!(self.render_cacheability.replay(), Replay::Cacheable).then(|| {
                self.lint_probes
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
            });
        if let Some(probe) = probes.as_ref().and_then(|probes| probes.get(&key)) {
            return Ok(Arc::clone(probe));
        }
        let evidence_dir = self.new_case()?;
        fs::write(evidence_dir.join("values.json"), request.values)?;
        let baseline = self.lint_in(&baseline_chart, request, &evidence_dir, "lint-baseline")?;
        let candidate = self.lint_in(&candidate_chart, request, &evidence_dir, "lint-candidate")?;
        let schema_free = self.lint_in(
            &self.render_chart,
            request,
            &evidence_dir,
            "lint-schema-free",
        )?;
        let probe = Arc::new(HelmLintProbe {
            baseline,
            candidate,
            schema_free,
            evidence_dir,
        });
        if let Some(probes) = &mut probes {
            probes.insert(key, Arc::clone(&probe));
        }
        Ok(probe)
    }

    /// Runs the schema-free lint needed to establish a defaults or context control.
    ///
    /// The control does not consult either compared root schema.
    ///
    /// # Errors
    ///
    /// Returns an error when evidence cannot be written or Helm fails operationally.
    pub fn adjudicate_schema_free_lint(
        &self,
        values: &[u8],
        kubernetes_version: &str,
    ) -> eyre::Result<SchemaFreeLintProbe> {
        let evidence_dir = self.new_case()?;
        fs::write(evidence_dir.join("values.json"), values)?;
        let execution = self.runner.lint(
            &TemplateRequest {
                chart: &self.render_chart,
                values,
                kubernetes_version,
            },
            &evidence_dir,
            "lint-schema-free",
            &self.render_cacheability,
        )?;
        self.record(&execution.record);
        Ok(SchemaFreeLintProbe {
            execution,
            evidence_dir,
        })
    }

    fn lint_tree(&self, schema: &[u8]) -> eyre::Result<PreparedTree> {
        let mut trees = self
            .lint_trees
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(tree) = trees.get(schema) {
            return Ok(tree.clone());
        }
        let staged = self.runner.staging_dir()?;
        copy_chart_tree(
            self.render_chart.path(),
            &staged,
            self.render_chart.path(),
            false,
            &mut ArchiveBudget::default(),
            0,
        )?;
        fs::write(staged.join("values.schema.json"), schema)?;
        let tree = self.runner.publish_tree(&staged)?;
        trees.insert(schema.to_vec(), tree.clone());
        Ok(tree)
    }

    fn lint_in(
        &self,
        chart: &PreparedTree,
        request: &LintProbeRequest<'_>,
        case: &Path,
        stage: &str,
    ) -> eyre::Result<HelmExecution> {
        let execution = self.runner.lint(
            &TemplateRequest {
                chart,
                values: request.values,
                kubernetes_version: request.kubernetes_version,
            },
            case,
            stage,
            &self.render_cacheability,
        )?;
        self.record(&execution.record);
        Ok(execution)
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
    /// Resource validity or assertion correspondence could not be decided.
    Uncertain(Vec<String>),
}

/// A values-schema diagnostic, including its concrete input and offending value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValuesViolationKey {
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

impl ValuesViolationKey {
    /// The values-schema diagnostic for `error`.
    #[must_use]
    pub fn new(error: &jsonschema::ValidationError<'_>) -> Self {
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
            instance_path: error.instance_path().to_string(),
            schema_path: error.schema_path().to_string(),
            keyword: error.kind().keyword().to_string(),
            detail,
        }
    }
}

impl fmt::Display for ValuesViolationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {} {} at {}",
            self.instance_path, self.keyword, self.detail, self.schema_path
        )
    }
}

/// Canonical constraint and instance identity shared by all Kubernetes comparisons.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KubernetesAssertion {
    /// The resource's apiVersion.
    pub api_version: String,
    /// The resource's kind.
    pub kind: String,
    /// Absolute keyword URI, including its schema resource, independent of reference routes.
    pub schema_location: String,
    /// The violated keyword.
    pub keyword: String,
    /// Structural property identity; offending scalar values are excluded.
    pub detail: String,
    instance: Vec<InstanceSegment>,
}

/// One schema violation of a rendered resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KubernetesViolation {
    /// Position in the rendered stream, such as `document 2/items/0`.
    pub location: String,
    /// The resource's namespace, if it names one.
    pub namespace: Option<String>,
    /// The resource's name, if it has one.
    pub name: Option<String>,
    /// What is violated.
    pub key: KubernetesAssertion,
    /// Concrete instance pointer retained for diagnostics and per-resource report deduplication.
    pub instance_path: String,
    /// The validator's message.
    pub message: String,
}

impl fmt::Display for KubernetesViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = &self.key;
        let name = self.name.as_deref().unwrap_or("<unnamed>");
        match &self.namespace {
            Some(namespace) => write!(
                formatter,
                "{}: {}/{} {namespace}/{name}: {}: {}",
                self.location, key.api_version, key.kind, self.instance_path, self.message
            ),
            None => write!(
                formatter,
                "{}: {}/{} {name}: {}: {}",
                self.location, key.api_version, key.kind, self.instance_path, self.message
            ),
        }
    }
}

/// A probe render judged against an applicable defaults or context render.
#[derive(Debug, Default)]
pub struct DefaultsComparison {
    /// Canonical assertion occurrences beyond the control's multiset.
    pub new_violations: Vec<KubernetesViolation>,
    /// Canonical assertion occurrences also carried by the control.
    pub inherited_violations: Vec<KubernetesViolation>,
    /// Resources whose validity could not be decided.
    pub uncertain: Vec<String>,
}

/// Applicable context assertions after checking their usability against defaults.
///
/// Each context is decidable and introduces no assertions beyond defaults.
/// Payload inheritance uses each context's own multiset independently.
pub struct UsableKubernetesControls {
    contexts: Vec<ResourceEvidence>,
}

#[derive(Clone, Default)]
struct ResourceEvidence {
    invalid: Vec<KubernetesViolation>,
    uncertain: Vec<String>,
}

impl ResourceEvidence {
    fn compare(&self, control: &Self) -> DefaultsComparison {
        let mut remaining = BTreeMap::<&KubernetesAssertion, usize>::new();
        for violation in &control.invalid {
            *remaining.entry(&violation.key).or_default() += 1;
        }
        let mut comparison = DefaultsComparison {
            uncertain: self.uncertain.clone(),
            ..DefaultsComparison::default()
        };
        for violation in &self.invalid {
            match remaining.get_mut(&violation.key) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    comparison.inherited_violations.push(violation.clone());
                }
                _ => comparison.new_violations.push(violation.clone()),
            }
        }
        comparison
    }

    fn collect_violations(
        &mut self,
        document: &Value,
        location: &str,
        schema: &ResourceSchema,
    ) -> eyre::Result<()> {
        use jsonschema::error::ValidationErrorKind;

        let mut occurrences: Vec<KubernetesViolation> = Vec::new();
        for error in schema.validator.iter_errors(document) {
            let schema_location = canonical_schema_location(
                &schema.registry,
                ANONYMOUS_SCHEMA_URI,
                assertion_provenance(&error),
            )
            .map_err(|reason| {
                eyre::eyre!(
                    "unresolved assertion identity: {location} via {}: {reason}",
                    error.evaluation_path()
                )
            })?;
            let members = match error.kind() {
                ValidationErrorKind::AdditionalProperties { unexpected } => unexpected.clone(),
                ValidationErrorKind::AdditionalItems { limit } => {
                    let items = error.instance();
                    let items = items
                        .as_array()
                        .ok_or_eyre("additionalItems instance is not an array")?;
                    (*limit..items.len())
                        .map(|index| index.to_string())
                        .collect()
                }
                ValidationErrorKind::PropertyNames { error: name_error } => vec![
                    name_error
                        .instance()
                        .as_str()
                        .ok_or_eyre("propertyNames has no property name")?
                        .to_string(),
                ],
                _ => vec![String::new()],
            };
            for member in members {
                let structural_member = matches!(
                    error.kind(),
                    ValidationErrorKind::AdditionalProperties { .. }
                        | ValidationErrorKind::PropertyNames { .. }
                );
                let instance_path = if structural_member
                    || matches!(error.kind(), ValidationErrorKind::AdditionalItems { .. })
                {
                    error.instance_path().join(member.as_str()).to_string()
                } else {
                    error.instance_path().to_string()
                };
                let instance = canonical_instance_path(document, &error, &instance_path)
                    .map_err(|reason| eyre::eyre!("unresolved assertion identity: {location} at {instance_path} via {}: {reason}", error.evaluation_path()))?;
                let detail = match error.kind() {
                    ValidationErrorKind::Required { property } => property.to_string(),
                    _ if structural_member => Value::String(member).to_string(),
                    _ => String::new(),
                };
                let key = KubernetesAssertion {
                    api_version: document
                        .get("apiVersion")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    kind: document
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    schema_location: schema_location.clone(),
                    keyword: error.kind().keyword().to_string(),
                    detail,
                    instance,
                };
                // Repeated reference routes report one constraint occurrence more than once.
                // Different canonical roles for that same occurrence do not prove correspondence.
                if let Some(previous) = occurrences.iter().find(|previous| {
                    previous.instance_path == instance_path
                        && previous.key.schema_location == key.schema_location
                        && previous.key.keyword == key.keyword
                        && previous.key.detail == key.detail
                }) {
                    eyre::ensure!(
                        previous.key == key,
                        "unresolved assertion identity: {location} at {instance_path} has conflicting schema roles"
                    );
                    continue;
                }
                occurrences.push(KubernetesViolation {
                    location: location.to_string(),
                    namespace: document
                        .pointer("/metadata/namespace")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    name: document
                        .pointer("/metadata/name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    key,
                    instance_path,
                    message: error.to_string(),
                });
            }
        }
        self.invalid.extend(occurrences);
        Ok(())
    }

    fn unidentified(&mut self, document: &Value, location: &str) {
        for property in ["apiVersion", "kind"] {
            if document.get(property).and_then(Value::as_str).is_some() {
                continue;
            }
            let present = document.get(property).is_some();
            self.invalid.push(KubernetesViolation {
                location: location.to_string(),
                namespace: None,
                name: None,
                key: KubernetesAssertion {
                    api_version: String::new(),
                    kind: String::new(),
                    schema_location: if present {
                        format!("urn:helm-schema:resource-identity#/properties/{property}/type")
                    } else {
                        "urn:helm-schema:resource-identity#/required".to_string()
                    },
                    keyword: if present { "type" } else { "required" }.to_string(),
                    detail: if present {
                        String::new()
                    } else {
                        Value::String(property.to_string()).to_string()
                    },
                    instance: vec![InstanceSegment::Fixed(property.to_string())],
                },
                instance_path: format!("/{property}"),
                message: format!("a resource needs a string {property}"),
            });
        }
    }
}

/// The Kubernetes release the corpus's offline bundle validates against.
pub const KUBERNETES_RELEASE: &str = "v1.29.0-standalone-strict";

/// How resource schemas are compiled and judged. Changing the compilation
/// options or the identity proof must change this.
const VALIDATOR_POLICY: &str = "helm-schema/offline-kubernetes-validator/v6";

/// Base for anonymous schemas; resource API/kind separately qualify every assertion.
const ANONYMOUS_SCHEMA_URI: &str = "https://helm-schema.invalid/resource";

/// The validator and the same offline registry used to prove constraint locations.
pub(crate) struct ResourceSchema {
    pub(crate) validator: jsonschema::Validator,
    pub(crate) registry: jsonschema::Registry<'static>,
    pattern_execution: Option<jsonschema::Validator>,
    pattern_count: usize,
}

impl ResourceSchema {
    pub(crate) fn compile(schema: &Value, draft: jsonschema::Draft) -> eyre::Result<Self> {
        verify_registry_integrity(schema, draft, ANONYMOUS_SCHEMA_URI)?;
        let registry = jsonschema::Registry::new()
            .draft(draft)
            .retriever(NoRemoteSchemas)
            .add(ANONYMOUS_SCHEMA_URI, draft.create_resource(schema.clone()))?
            .prepare()?;
        eyre::ensure!(
            registry
                .resolver(jsonschema::uri::from_str(ANONYMOUS_SCHEMA_URI)?)
                .lookup(ANONYMOUS_SCHEMA_URI)?
                .contents()
                == schema,
            "registry retrieval URI does not identify the pinned root"
        );
        let patterns = verify_compiler_locations(&registry, ANONYMOUS_SCHEMA_URI)?;
        let validator = jsonschema::options()
            .with_base_uri(ANONYMOUS_SCHEMA_URI)
            .with_draft(draft)
            .with_retriever(NoRemoteSchemas)
            .with_registry(&registry)
            .build(schema)?;
        let pattern_count = patterns.len();
        let pattern_execution = if patterns.is_empty() {
            None
        } else {
            let checks: Vec<_> = patterns
                .into_iter()
                .map(|pattern| json!({"pattern": pattern}))
                .collect();
            Some(
                jsonschema::options()
                    .with_draft(draft)
                    .build(&json!({"allOf": checks}))?,
            )
        };
        Ok(Self {
            validator,
            registry,
            pattern_execution,
            pattern_count,
        })
    }

    pub(crate) fn verify_pattern_execution(&self, document: &Value) -> eyre::Result<usize> {
        let Some(checker) = &self.pattern_execution else {
            return Ok(0);
        };
        // Boolean applicators discard engine errors, so certify every possible string/key pairing.
        // Ordinary mismatches are irrelevant here; this pass never decides schema acceptance.
        let mut pending = vec![document];
        let mut strings = BTreeSet::new();
        while let Some(node) = pending.pop() {
            match node {
                Value::Object(properties) => {
                    strings.extend(properties.keys().map(String::as_str));
                    pending.extend(properties.values());
                }
                Value::Array(items) => pending.extend(items),
                Value::String(value) => {
                    strings.insert(value.as_str());
                }
                _ => {}
            }
        }
        let pairs = strings.len().saturating_mul(self.pattern_count);
        eyre::ensure!(
            pairs <= 1_000_000,
            "unresolved pattern execution budget exceeded"
        );
        for value in strings {
            for error in checker.iter_errors(&Value::String(value.to_string())) {
                verify_execution(&error)?;
            }
        }
        Ok(pairs)
    }
}

/// A compiled resource schema, or why the resource cannot be decided.
type CompiledSchema = Arc<OnceLock<Result<Arc<ResourceSchema>, String>>>;

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
    schema_identity: String,
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
            .entry(identity.clone())
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
            schema_identity: identity,
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
    /// Ambiguous assertion correspondence remains unresolved, including beside proven violations.
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
        Ok(self.validate_documents(&documents))
    }

    /// Judges documents Helm's `fromYaml` decoded ([`decode`]).
    #[must_use]
    pub fn validate_documents(&self, documents: &[Value]) -> KubernetesVerdict {
        let evidence = match self.document_evidence(documents) {
            Ok(evidence) => evidence,
            Err(err) => return KubernetesVerdict::Uncertain(vec![err.to_string()]),
        };
        if !evidence.invalid.is_empty() {
            KubernetesVerdict::Invalid(evidence.invalid)
        } else if !evidence.uncertain.is_empty() {
            KubernetesVerdict::Uncertain(evidence.uncertain)
        } else {
            KubernetesVerdict::Valid
        }
    }

    /// Judges `probe` against the chart's defaults render, which a schema
    /// must accept even when it violates Kubernetes.
    ///
    /// Defaults must be decidable. Both renders use the same canonical assertion
    /// multiset comparison as explicit contexts, including unchanged documents.
    ///
    /// # Errors
    ///
    /// Returns an error on unusable defaults, ambiguous assertion identity, or failed execution
    /// or decoding of either render.
    pub fn compare_with_defaults(
        &self,
        chart: &PinnedHelmChart,
        probe: &HelmProbe,
    ) -> eyre::Result<DefaultsComparison> {
        self.compare_with_defaults_at(chart, probe, chart.default_version()?)
    }

    /// Judges the probe against defaults rendered under `kubernetes_version`.
    ///
    /// Defaults evidence is shared only under that exact version and validator bundle/policy.
    /// The probe must have been rendered under the same version.
    ///
    /// # Errors
    ///
    /// Returns an error on unusable defaults, ambiguous assertion identity, or failed execution
    /// or decoding of either render.
    pub fn compare_with_defaults_at(
        &self,
        chart: &PinnedHelmChart,
        probe: &HelmProbe,
        kubernetes_version: &str,
    ) -> eyre::Result<DefaultsComparison> {
        let defaults = {
            let mut controls = chart
                .defaults_controls
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let evaluated = controls
                .entry(DefaultsControlKey {
                    kubernetes_version: kubernetes_version.to_owned(),
                    schema_identity: self.schema_identity.clone(),
                })
                .or_insert_with(|| {
                    self.evaluate_defaults(chart, kubernetes_version)
                        .map(Arc::new)
                        .map_err(|err| DefaultsFailure {
                            message: format!("{err:#}"),
                            unusable: err.downcast_ref::<UnusableControl>().is_some(),
                        })
                });
            evaluated.as_ref().map(Arc::clone).map_err(|failure| {
                let message = format!(
                    "defaults render could not be adjudicated: {}",
                    failure.message
                );
                if failure.unusable {
                    eyre::Report::new(UnusableControl(message))
                } else {
                    eyre::eyre!(message)
                }
            })?
        };
        Ok(self.probe_evidence(chart, probe)?.compare(&defaults))
    }

    /// Establishes control usability before attributing any lint or template rejection.
    ///
    /// Defaults may violate Kubernetes, but must have no uncertainty.
    /// Contexts must have no uncertainty or new violated assertion relative to defaults.
    /// Assertions retain resource API/kind, canonical schema location and structural
    /// property identity.
    /// Homogeneous array indices and schema-declared map positions are abstracted;
    /// tuple and prefix-item positions remain fixed.
    /// Fixed property names and occurrence multiplicity remain distinct, independent of offending values.
    /// Empty `contexts` uses defaults; otherwise only the supplied contexts are applicable.
    ///
    /// # Errors
    ///
    /// Returns control uncertainty or new violations, or an error rendering or decoding a control.
    pub fn check_controls(
        &self,
        chart: &PinnedHelmChart,
        defaults: &HelmProbe,
        contexts: &[&HelmProbe],
    ) -> eyre::Result<UsableKubernetesControls> {
        let defaults_evidence = self.control_evidence(chart, defaults, "defaults")?;
        let mut applicable = Vec::new();
        for context in contexts {
            // An explicit defaults context reuses the decoded usability control.
            let evidence = if std::ptr::eq(*context, defaults) {
                defaults_evidence.clone()
            } else {
                self.control_evidence(chart, context, "context")?
            };
            let comparison = evidence.compare(&defaults_evidence);
            if !comparison.new_violations.is_empty() {
                return Err(UnusableControl(format!(
                    "unusable context Kubernetes control: new assertions versus defaults: {}",
                    comparison
                        .new_violations
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                ))
                .into());
            }
            applicable.push(evidence);
        }
        if applicable.is_empty() {
            applicable.push(defaults_evidence);
        }
        Ok(UsableKubernetesControls {
            contexts: applicable,
        })
    }

    fn control_evidence(
        &self,
        chart: &PinnedHelmChart,
        control: &HelmProbe,
        label: &str,
    ) -> eyre::Result<ResourceEvidence> {
        eyre::ensure!(
            control.rendered.success(),
            "unusable {label} render control"
        );
        let evidence = self.probe_evidence(chart, control)?;
        if !evidence.uncertain.is_empty() {
            return Err(UnusableControl(format!(
                "unusable {label} Kubernetes control: uncertainty: {}",
                evidence.uncertain.join("; ")
            ))
            .into());
        }
        Ok(evidence)
    }

    /// Judges a probe against previously checked defaults/context controls.
    /// All renders must use the same Kubernetes version.
    /// Returns one comparison per applicable context, in the supplied order.
    /// Defaults establish usability and supply no allowances to a non-default context.
    ///
    /// # Errors
    ///
    /// Returns an error when the probe cannot be decoded or assertion correspondence is ambiguous.
    pub fn compare_with_controls(
        &self,
        chart: &PinnedHelmChart,
        probe: &HelmProbe,
        controls: &UsableKubernetesControls,
    ) -> eyre::Result<Vec<DefaultsComparison>> {
        let evidence = self.probe_evidence(chart, probe)?;
        Ok(controls
            .contexts
            .iter()
            .map(|context| evidence.compare(context))
            .collect())
    }

    fn probe_evidence(
        &self,
        chart: &PinnedHelmChart,
        probe: &HelmProbe,
    ) -> eyre::Result<ResourceEvidence> {
        self.document_evidence(&Self::decode_probe(chart, probe)?)
    }

    fn document_evidence(&self, documents: &[Value]) -> eyre::Result<ResourceEvidence> {
        let mut evidence = ResourceEvidence::default();
        for (index, document) in documents.iter().enumerate() {
            if !document.is_null() {
                self.validate_document(document, &format!("document {index}"), &mut evidence)?;
            }
        }
        Ok(evidence)
    }

    fn decode_probe(chart: &PinnedHelmChart, probe: &HelmProbe) -> eyre::Result<Vec<Value>> {
        let decoded = decode(chart.runner, &probe.rendered.stdout, &probe.evidence_dir)?;
        if let Some(record) = &decoded.record {
            chart.record(record);
        }
        decoded
            .documents
            .map_err(|rejection| eyre::eyre!("{rejection}"))
    }

    fn evaluate_defaults(
        &self,
        chart: &PinnedHelmChart,
        kubernetes_version: &str,
    ) -> eyre::Result<ResourceEvidence> {
        let probe = chart.adjudicate_as(
            &serde_json::to_vec_pretty(&serde_json::json!({}))?,
            "control",
            kubernetes_version,
        )?;
        self.control_evidence(chart, &probe, "defaults")
    }

    /// The shared compiled schema of `apiVersion`/`kind`, compiled once.
    fn compiled_validator(
        &self,
        api_version: &str,
        kind: &str,
    ) -> Result<Arc<ResourceSchema>, String> {
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

    fn validate_document(
        &self,
        document: &Value,
        location: &str,
        evidence: &mut ResourceEvidence,
    ) -> eyre::Result<()> {
        if let Some(error) = document.get("Error") {
            evidence.uncertain.push(format!(
                "{location}: Helm fromYaml reported an error: {error}"
            ));
            return Ok(());
        }
        if has_inexact_number(document) {
            evidence.uncertain.push(format!(
                "{location}: numeric magnitude exceeds Helm fromYaml's exact integer range"
            ));
            return Ok(());
        }
        // A document without a string apiVersion and kind is no resource
        // Kubernetes can accept, such as the `{}` an empty list item renders.
        let (Some(api_version), Some(kind)) = (
            document.get("apiVersion").and_then(Value::as_str),
            document.get("kind").and_then(Value::as_str),
        ) else {
            evidence.unidentified(document, location);
            return Ok(());
        };
        if api_version == "v1"
            && kind == "List"
            && let Some(items) = document.get("items").and_then(Value::as_array)
        {
            for (index, item) in items.iter().enumerate() {
                self.validate_document(item, &format!("{location}/items/{index}"), evidence)?;
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
                validator.verify_pattern_execution(document)?;
                evidence.collect_violations(document, location, validator)?;
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
                        key: KubernetesAssertion {
                            api_version: api_version.to_string(),
                            kind: kind.to_string(),
                            instance: vec![InstanceSegment::Fixed("apiVersion".to_string())],
                            schema_location: "urn:helm-schema:api-presence#/served".to_string(),
                            keyword: "served".to_string(),
                            detail: String::new(),
                        },
                        message: format!(
                            "{api_version}/{kind} is not served by the pinned Kubernetes version"
                        ),
                        instance_path: "/apiVersion".to_string(),
                    });
                    return Ok(());
                }
                let name = name.as_deref().unwrap_or("<unnamed>");
                evidence
                    .uncertain
                    .push(format!("{location}: {api_version}/{kind} {name}: {error}"));
            }
        }
        Ok(())
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
            "Helm YAML decoding failed: {}; evidence={}",
            String::from_utf8_lossy(&output.stderr),
            crate::helm::invocation::preserve_failure(&case).display()
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
) -> Result<ResourceSchema, String> {
    let resource = ResourceRef::concrete(api_version.to_string(), kind.to_string());
    let ProviderLookupResult::Found { schema, .. } = crds.lookup(&resource, &YamlPath::default())
    else {
        return Err("pinned CRD schema not found".to_string());
    };
    ResourceSchema::compile(schema.schema(), jsonschema::Draft::Draft7)
        .map_err(|error| format!("pinned CRD schema did not compile: {error}"))
}

fn compile_resource_validator(
    provider: &KubernetesJsonSchemaProvider,
    api_version: &str,
    kind: &str,
) -> Result<ResourceSchema, String> {
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
    }
    if !schema_declares_resource(source.source_schema(), api_version, kind, draft) {
        return Err(format!(
            "original schema does not prove exact identity {api_version}/{kind}"
        ));
    }
    ResourceSchema::compile(source.source_schema(), draft)
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
