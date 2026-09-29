//! Generation of every registered artifact, and the producer that writes them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::GenerateOptions;
use helm_schema::output::{
    EmitRequest, FetchPolicy, LoadBudget, OutputPipelineOptions, PolicyInputOptions,
};
use helm_schema::provider::ProviderOptions;
use helm_schema_ast::DefineIndex;
use helm_schema_core::ResourceSchemaOracle;
use helm_schema_gen::{PreparedValuesDocuments, ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::{ContractIr, SymbolicIrContext};
use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider, KubernetesJsonSchemaProvider};
use serde_json::Value;
use test_util::scratch::ScratchDir;
use vfs::VfsPath;

use crate::manifest::{
    self, BuildProvenance, CompanionFile, HARNESS_VERSION, HelmReadyFiles, INTERNAL_DIR,
    InputDigester, LOCK_FILE, MANIFEST_FILE, Manifest, ManifestEntry,
};
use crate::registry::{
    self, ArtifactSpec, ChartRecipe, GenerationRecipe, IrRecipe, PROVIDER_BUNDLE, PolicyRecipe,
    TemplateProvider, TemplateRecipe,
};
use crate::source_digest::sha256_hex;

/// The directory of a chart given relative to `testdata/charts`.
#[must_use]
pub fn chart_dir(chart_relative_path: &str) -> PathBuf {
    test_util::workspace_testdata()
        .join("charts")
        .join(chart_relative_path)
}

/// The analysis options of a whole-chart recipe.
#[must_use]
pub fn generate_options(recipe: &ChartRecipe) -> GenerateOptions {
    generate_options_at(&chart_dir(recipe.chart), recipe)
}

/// The analysis options of `recipe`, reading the chart at `chart_dir` instead.
#[must_use]
pub fn generate_options_at(chart_dir: &Path, recipe: &ChartRecipe) -> GenerateOptions {
    let bundle = test_util::workspace_testdata().join(PROVIDER_BUNDLE);
    let chart_dir = chart_dir.to_string_lossy().to_string();
    GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: recipe.include_tests,
        include_subchart_values: recipe.include_subchart_values,
        values_files: recipe
            .values_files
            .iter()
            .map(|path| test_util::workspace_testdata().join(path))
            .collect(),
        infer_required: recipe.infer_required,
        emission: recipe.profile.into(),
        authoring: recipe.authoring,
        provider: ProviderOptions {
            k8s_versions: vec![recipe.k8s_version.to_string()],
            // Provider availability is a deterministic test INPUT: the
            // committed bundle under testdata/ pins exactly which upstream
            // schemas generation can see, so cold and warm runs produce the
            // same fixtures. The gitignored .cache/ dirs must not be used
            // here — an empty cache silently strips provider-backed facts
            // from every expected schema.
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            allow_net: false,
            crd_catalog_cache_dir: Some(bundle.join("crds-catalog-cache")),
            disable_k8s_schemas: false,
            crd_override_dir: Some(bundle.join("crds-catalog-cache")),
            ..Default::default()
        },
    }
}

/// Generates the whole-chart schema of `recipe`.
///
/// # Errors
///
/// Returns an error when analysis or generation fails.
pub fn chart_schema(recipe: &ChartRecipe) -> eyre::Result<Value> {
    let generated = AnalysisSession::new(generate_options(recipe))
        .generated_schema()
        .wrap_err_with(|| format!("generate {:?} schema for {}", recipe.profile, recipe.chart))?;
    if recipe.minimize {
        // Mirror the CLI's default output policy: repeated schema subtrees
        // are interned into root-level `$defs` under readable source paths.
        // Fixtures always keep those names; shortening is a separate
        // transform applied only to what is handed to Helm.
        Ok(helm_schema_json_schema_minify::minimize_schema(
            generated.schema,
            &generated.definition_origins,
            helm_schema_json_schema_minify::DefinitionNames::Source,
        ))
    } else {
        Ok(generated.schema)
    }
}

/// An analysis session for a final-output policy recipe.
#[must_use]
pub fn policy_session(recipe: &PolicyRecipe) -> AnalysisSession {
    AnalysisSession::new(generate_options(&recipe.chart))
}

/// Emits the final output of `recipe` from `session`.
///
/// A recipe with an override file writes it to a scratch directory first.
///
/// # Errors
///
/// Returns an error when the override cannot be written or emission fails.
pub fn emit_policy(session: &AnalysisSession, recipe: &PolicyRecipe) -> eyre::Result<Value> {
    let request = EmitRequest {
        reference_policy: recipe.reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: recipe.chart.minimize,
            definition_names: helm_schema::output::DefinitionNames::Source,
        },
    };
    let Some(file) = recipe.override_file else {
        return Ok(session.emit(request)?);
    };
    let dir = ScratchDir::new("overrides").wrap_err("create override directory")?;
    let path = dir.path().join(file.name);
    std::fs::write(&path, file.contents)
        .wrap_err_with(|| format!("write override {}", path.display()))?;
    Ok(session.emit_with_policy_paths(
        &[path],
        PolicyInputOptions {
            fetch_policy: FetchPolicy::input_assembly(false),
            load_budget: LoadBudget::default(),
        },
        request,
    )?)
}

/// Indexes the helper sources of a template or IR recipe.
///
/// # Errors
///
/// Returns an error when a helper source cannot be read.
pub fn define_index(sources: test_util::DefineSourceSpec<'_>) -> eyre::Result<DefineIndex> {
    let mut index = DefineIndex::new();
    for source in sources.load()? {
        index.add_file_source(&source.path, &source.source);
    }
    Ok(index)
}

/// Generates the template-level schema of `recipe` over its registered values.
///
/// # Errors
///
/// Returns an error when an input file cannot be read.
pub fn template_schema(recipe: &TemplateRecipe) -> eyre::Result<Value> {
    let values_yaml = match recipe.inline_values {
        Some(values_yaml) => values_yaml.to_string(),
        None => test_util::read_testdata(recipe.values_path)?,
    };
    template_schema_with_values(recipe, &values_yaml)
}

/// Generates the template-level schema of `recipe` over `values_yaml`.
///
/// # Errors
///
/// Returns an error when an input file cannot be read.
pub fn template_schema_with_values(
    recipe: &TemplateRecipe,
    values_yaml: &str,
) -> eyre::Result<Value> {
    let source = test_util::read_testdata(recipe.template_path)?;
    let index = define_index(recipe.define_sources)?;
    let contract = SymbolicIrContext::new(&index).generate_contract_ir(&source);
    let provider = match recipe.provider {
        TemplateProvider::K8s(version) => production_k8s_chain(version),
        TemplateProvider::CrdK8s(version) => production_crd_k8s_chain(version),
    };
    Ok(values_schema(contract, &provider, Some(values_yaml)))
}

/// Lowers `contract` to a values schema the way template-level fixtures do.
#[must_use]
pub fn values_schema(
    contract: ContractIr,
    provider: &dyn ResourceSchemaOracle,
    values_yaml: Option<&str>,
) -> Value {
    let schema_signals = contract.finalize().into_schema_signals();
    let composed = values_yaml
        .and_then(|source| serde_yaml::from_str(source).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let documents = PreparedValuesDocuments::new(composed, serde_yaml::Value::Null);
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, provider).with_values_documents(&documents),
    )
}

/// Extracts the finalized contract-IR document of `recipe`.
///
/// # Errors
///
/// Returns an error when an input file cannot be read.
pub fn ir_document(recipe: &IrRecipe) -> eyre::Result<Value> {
    let source = test_util::read_testdata(recipe.template_path)?;
    let index = define_index(recipe.define_sources)?;
    let document = SymbolicIrContext::new(&index)
        .generate_contract_ir(&source)
        .finalize()
        .document();
    serde_json::to_value(document).wrap_err("serialize contract IR")
}

/// K8s provider reading only the vendored bundle.
///
/// Provider availability is a deterministic test INPUT: the bundle pins which
/// upstream schemas a test can see. Reaching the ambient user cache with
/// downloads enabled made results depend on cache warmth and on
/// `raw.githubusercontent.com` being reachable — the exact failure mode the
/// bundle exists to remove. Every provider a test builds must come from here.
#[must_use]
pub fn bundled_k8s_provider(version: &str) -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::new(version.to_string())
        .with_cache_dir(
            test_util::workspace_testdata()
                .join(PROVIDER_BUNDLE)
                .join("kubernetes-json-schema-cache"),
        )
        .with_allow_download(false)
}

/// CRD catalog provider reading only the vendored bundle. See
/// [`bundled_k8s_provider`] for why downloads stay off.
#[must_use]
pub fn bundled_crd_provider() -> CrdsCatalogSchemaProvider {
    CrdsCatalogSchemaProvider::new()
        .with_cache_dir(
            test_util::workspace_testdata()
                .join(PROVIDER_BUNDLE)
                .join("crds-catalog-cache"),
        )
        .with_allow_download(false)
}

/// Production-like K8s provider path for chart-level generator tests.
///
/// These tests are meant to approximate what end users run through the CLI,
/// so they use the chain layer plus apiVersion inference instead of the older
/// single-provider shortcut.
#[must_use]
pub fn production_k8s_chain(version: &str) -> Chain {
    let k8s_provider = bundled_k8s_provider(version).with_api_version_guess(true);
    Chain::new(vec![Box::new(k8s_provider)]).with_inference_enabled(true)
}

/// Production-like CRD + K8s provider path for chart-level generator tests.
///
/// This keeps real-world CRD-consuming chart tests on the same resolution path
/// as the CLI while leaving lower-layer provider-specific tests free to pin a
/// single provider when that is the actual subject under test.
#[must_use]
pub fn production_crd_k8s_chain(version: &str) -> Chain {
    let crds = bundled_crd_provider();
    let k8s_provider = bundled_k8s_provider(version).with_api_version_guess(true);
    Chain::new(vec![Box::new(crds), Box::new(k8s_provider)]).with_inference_enabled(true)
}

/// Generates `recipe` and serializes it exactly as its fixture family is stored.
///
/// Whole-chart, lean and final-output schemas are pretty JSON plus a newline;
/// template schemas and IR documents are pretty JSON without one.
///
/// # Errors
///
/// Returns an error when generation or serialization fails.
pub fn generate(recipe: &GenerationRecipe) -> eyre::Result<Vec<u8>> {
    let (value, trailing_newline) = match recipe {
        GenerationRecipe::Chart(chart) => (chart_schema(chart)?, true),
        GenerationRecipe::FinalPolicy(policy) => {
            (emit_policy(&policy_session(policy), policy)?, true)
        }
        GenerationRecipe::Template(template) => (template_schema(template)?, false),
        GenerationRecipe::Ir(ir) => (ir_document(ir)?, false),
    };
    let mut bytes = serde_json::to_vec_pretty(&value).wrap_err("serialize artifact")?;
    if trailing_newline {
        bytes.push(b'\n');
    }
    Ok(bytes)
}

/// Generates the whole registry into `out` and then publishes its manifest.
///
/// With `helm_ready`, every schema artifact Helm reads also gets its
/// Helm-ready copy and name map under `internal/` (see
/// [`manifest::helm_ready_files`]), recorded in its manifest entry.
///
/// `build` is the provenance compiled into the running producer; the run is
/// refused when it differs from the tree on disk, so a stale executable cannot
/// certify current sources. The manifest is removed first and written last,
/// through a rename, and only after [`manifest::verify_all`] has rechecked the
/// source tree, every recipe's inputs as captured before dispatch, and every
/// artifact's presence and bytes. A lock file keeps a second producer out of
/// `out` for the duration.
///
/// # Errors
///
/// Returns an error when the build is stale, `out` is owned by another run,
/// any artifact fails, or any check fails.
pub fn produce(
    out: &Path,
    jobs: usize,
    build: &BuildProvenance,
    helm_ready: bool,
) -> eyre::Result<Manifest> {
    build.check_current()?;
    let specs = registry::registry();
    registry::validate(&specs)?;
    std::fs::create_dir_all(out.join(INTERNAL_DIR))
        .wrap_err_with(|| format!("create {}", out.display()))?;
    let lock = out.join(LOCK_FILE);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .wrap_err_with(|| {
            format!(
                "{} exists: another producer owns {} (remove it if that run died)",
                lock.display(),
                out.display()
            )
        })?;
    let produced = produce_locked(out, jobs, build, &specs, helm_ready);
    let released =
        std::fs::remove_file(&lock).wrap_err_with(|| format!("remove {}", lock.display()));
    let manifest = produced?;
    released?;
    Ok(manifest)
}

fn produce_locked(
    out: &Path,
    jobs: usize,
    build: &BuildProvenance,
    specs: &[ArtifactSpec],
    helm_ready: bool,
) -> eyre::Result<Manifest> {
    let manifest_path = out.join(MANIFEST_FILE);
    if manifest_path.exists() {
        std::fs::remove_file(&manifest_path)
            .wrap_err_with(|| format!("remove stale {}", manifest_path.display()))?;
    }
    let artifacts = produce_entries(specs, out, jobs, helm_ready)?;
    let executable = std::env::current_exe().wrap_err("locate the producer executable")?;
    let manifest = Manifest {
        harness_version: HARNESS_VERSION,
        build: build.clone(),
        producer_binary_sha256: sha256_hex(
            &std::fs::read(&executable)
                .wrap_err_with(|| format!("read {}", executable.display()))?,
        ),
        testdata: manifest::testdata_location(),
        artifacts,
    };
    manifest::verify_all(out, &manifest)?;
    let mut bytes = serde_json::to_vec_pretty(&manifest).wrap_err("serialize manifest")?;
    bytes.push(b'\n');
    write_atomically(&manifest_path, &bytes)?;
    Ok(manifest)
}

/// Generates `specs` into `out` with up to `jobs` concurrent workers and returns
/// their entries in `specs` order, without writing a manifest.
///
/// Every recipe's input digest is captured before any generation starts. Each
/// worker owns its analysis session and provider; nothing mutable is shared.
/// This is also the `--only` path, whose partial output is never a manifest.
///
/// # Errors
///
/// Returns an error listing every artifact that failed.
pub fn produce_entries(
    specs: &[ArtifactSpec],
    out: &Path,
    jobs: usize,
    helm_ready: bool,
) -> eyre::Result<Vec<ManifestEntry>> {
    std::fs::create_dir_all(out.join(INTERNAL_DIR))
        .wrap_err_with(|| format!("create {}", out.display()))?;
    let mut digester = InputDigester::new(test_util::workspace_testdata());
    let mut inputs = Vec::with_capacity(specs.len());
    for spec in specs {
        inputs.push(digester.digest(&spec.recipe)?);
    }

    let next = AtomicUsize::new(0);
    let mut outcomes: Vec<(usize, eyre::Result<ManifestEntry>)> = Vec::new();
    std::thread::scope(|scope| {
        let workers = (0..jobs.max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let (Some(spec), Some(inputs)) = (specs.get(index), inputs.get(index))
                        else {
                            return done;
                        };
                        done.push((index, produce_one(spec, inputs, out, helm_ready)));
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            match worker.join() {
                Ok(done) => outcomes.extend(done),
                Err(_) => {
                    outcomes.push((usize::MAX, Err(eyre::eyre!("a producer worker panicked"))));
                }
            }
        }
    });
    outcomes.sort_by_key(|(index, _)| *index);

    let mut entries = Vec::with_capacity(specs.len());
    let mut failures = Vec::new();
    for (_, outcome) in outcomes {
        match outcome {
            Ok(entry) => entries.push(entry),
            Err(error) => failures.push(format!("{error:?}")),
        }
    }
    eyre::ensure!(
        failures.is_empty() && entries.len() == specs.len(),
        "{} of {} artifacts failed:\n{}",
        specs.len() - entries.len(),
        specs.len(),
        failures.join("\n")
    );
    Ok(entries)
}

fn produce_one(
    spec: &ArtifactSpec,
    inputs: &str,
    out: &Path,
    helm_ready: bool,
) -> eyre::Result<ManifestEntry> {
    let bytes = generate(&spec.recipe).wrap_err_with(|| format!("generate {}", spec.id.key()))?;
    let mut entry = ManifestEntry::new(spec, inputs.to_string(), &bytes);
    if helm_ready
        && let Some([schema, defs_map]) = manifest::helm_ready_files(spec, &bytes)
            .wrap_err_with(|| format!("shorten {}", spec.id.key()))?
    {
        write_atomically(&out.join(&schema.file), &schema.bytes)?;
        write_atomically(&out.join(&defs_map.file), &defs_map.bytes)?;
        entry.helm_ready = Some(HelmReadyFiles {
            schema: CompanionFile::new(schema.file, &schema.bytes),
            defs_map: CompanionFile::new(defs_map.file, &defs_map.bytes),
        });
    }
    write_atomically(&out.join(&entry.file), &bytes)?;
    Ok(entry)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> eyre::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes).wrap_err_with(|| format!("write {}", temporary.display()))?;
    std::fs::rename(&temporary, path).wrap_err_with(|| format!("rename to {}", path.display()))
}
