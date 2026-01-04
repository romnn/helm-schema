use clap::{Args, Parser, Subcommand};
use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::schema::{
    generate_values_schema_vyt, DefaultVytSchemaProvider, UpstreamK8sSchemaProvider,
    UpstreamThenDefaultVytSchemaProvider,
};
use helm_schema_mapper::vyt;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use vfs::VfsPath;

#[derive(Parser, Debug)]
#[command(
    name = "helm-schema",
    version,
    about = "Generate an accurate JSON schema for any helm chart"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan chart and print a summary as JSON
    Scan {
        /// Include files under tests/
        #[arg(long)]
        include_tests: bool,

        /// Path to the chart root
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Generate values.schema.json from chart templates
    ValuesSchema(ValuesSchemaArgs),
}

#[derive(Args, Debug)]
struct ValuesSchemaArgs {
    /// Chart reference: local path, local .tgz, URL to .tgz, or repo/chart (requires --repo-url)
    chart: String,

    /// Write output to a file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Kubernetes JSON schema version directory (kubernetes-json-schema cache layout)
    #[arg(long, default_value = "v1.29.0-standalone-strict")]
    k8s_version: String,

    /// Kubernetes JSON schema cache directory
    #[arg(long)]
    k8s_schema_cache: Option<PathBuf>,

    /// Allow downloading missing Kubernetes JSON schemas
    #[arg(long, default_value_t = false)]
    allow_net: bool,

    /// Helm repository base URL (for repo/chart refs)
    #[arg(long)]
    repo_url: Option<String>,

    /// Chart version (for repo/chart refs). If omitted, uses latest semver.
    #[arg(long)]
    version: Option<String>,
}

fn canonicalize_json(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(val) = obj.get(k) {
                    out.insert(k.clone(), canonicalize_json(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        _ => v.clone(),
    }
}

fn json_pretty(v: &Value) -> eyre::Result<String> {
    let v = canonicalize_json(v);
    Ok(serde_json::to_string_pretty(&v).map_err(|e| eyre::eyre!(e))?)
}

fn run_vyt_on_chart_templates(chart: &helm_schema_chart::ChartSummary) -> eyre::Result<Vec<vyt::VYUse>> {
    let mut defs = vyt::DefineIndex::default();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        vyt::extend_define_index_from_str(&mut defs, &src)
            .wrap_err_with(|| format!("index defines in {}", p.as_str()))?;
    }
    let defs = std::sync::Arc::new(defs);

    let mut all: Vec<vyt::VYUse> = Vec::new();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        let parsed = helm_schema_template::parse::parse_gotmpl_document(&src)
            .ok_or_eyre("template parse returned None")
            .wrap_err_with(|| format!("parse template {}", p.as_str()))?;

        let mut uses = vyt::VYT::new(src)
            .with_defines(std::sync::Arc::clone(&defs))
            .run(&parsed.tree);
        all.append(&mut uses);
    }

    Ok(all)
}

fn use_sort_key(u: &vyt::VYUse) -> (String, String, String, Vec<String>, Option<(String, String)>) {
    let kind = match u.kind {
        vyt::VYKind::Scalar => "Scalar",
        vyt::VYKind::Fragment => "Fragment",
    }
    .to_string();

    let resource = u
        .resource
        .as_ref()
        .map(|r| (r.api_version.clone(), r.kind.clone()));

    (
        u.source_expr.clone(),
        u.path.0.join("."),
        kind,
        u.guards.clone(),
        resource,
    )
}

fn cmp_uses(a: &vyt::VYUse, b: &vyt::VYUse) -> Ordering {
    use_sort_key(a).cmp(&use_sort_key(b))
}

fn write_output(out: &str, path: Option<&Path>) -> eyre::Result<()> {
    if let Some(p) = path {
        std::fs::write(p, out).map_err(|e| eyre::eyre!(e))?;
    } else {
        use std::io::Write;
        let mut stdout = std::io::stdout().lock();
        if let Err(e) = stdout.write_all(out.as_bytes()) {
            // When piping to `head`, stdout may be closed early; treat as success.
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(eyre::eyre!(e));
            }
        }
    }
    Ok(())
}

fn vfs_root_from_local_path(path: &Path) -> eyre::Result<VfsPath> {
    let fs = vfs::PhysicalFS::new(path);
    Ok(VfsPath::new(fs))
}

fn extract_tgz_to_temp(bytes: &[u8]) -> eyre::Result<(TempDir, PathBuf)> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let td = tempfile::tempdir().map_err(|e| eyre::eyre!(e))?;
    let gz = GzDecoder::new(bytes);
    let mut ar = Archive::new(gz);

    for entry in ar.entries().map_err(|e| eyre::eyre!(e))? {
        let mut e = entry.map_err(|e| eyre::eyre!(e))?;
        let path = e.path().map_err(|e| eyre::eyre!(e))?.into_owned();
        let out_path = td.path().join(&path);
        if e.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| eyre::eyre!(e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| eyre::eyre!(e))?;
            }
            let mut f = std::fs::File::create(&out_path).map_err(|e| eyre::eyre!(e))?;
            std::io::copy(&mut e, &mut f).map_err(|e| eyre::eyre!(e))?;
        }
    }

    // Find the first Chart.yaml under the extracted tree.
    let mut chart_root = None;
    for ent in walkdir::WalkDir::new(td.path()) {
        let ent = ent.map_err(|e| eyre::eyre!(e))?;
        if ent.file_type().is_file() && ent.file_name() == "Chart.yaml" {
            chart_root = ent.path().parent().map(|p| p.to_path_buf());
            break;
        }
    }
    let chart_root = chart_root.ok_or_eyre("unable to locate Chart.yaml in tgz")?;
    Ok((td, chart_root))
}

fn fetch_url_bytes(url: &str) -> eyre::Result<Vec<u8>> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| eyre::eyre!("failed to GET {url}: {e}"))?;
    let mut r = resp.into_reader();
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut r, &mut buf).map_err(|e| eyre::eyre!(e))?;
    Ok(buf)
}

#[derive(serde::Deserialize)]
struct RepoIndex {
    entries: std::collections::HashMap<String, Vec<RepoChartVersion>>,
}

#[derive(serde::Deserialize)]
struct RepoChartVersion {
    version: String,
    urls: Vec<String>,
}

fn join_url(base: &str, rel: &str) -> String {
    if rel.starts_with("http://") || rel.starts_with("https://") {
        return rel.to_string();
    }
    let b = base.trim_end_matches('/');
    let r = rel.trim_start_matches('/');
    format!("{b}/{r}")
}

fn resolve_repo_chart_tgz_url(
    repo_url: &str,
    chart_name: &str,
    version: Option<&str>,
) -> eyre::Result<String> {
    let index_url = join_url(repo_url, "index.yaml");
    let bytes = fetch_url_bytes(&index_url)?;
    let idx: RepoIndex = serde_yaml::from_slice(&bytes).map_err(|e| eyre::eyre!(e))?;
    let vers = idx
        .entries
        .get(chart_name)
        .ok_or_eyre("chart not found in repo index")?;

    let selected = if let Some(v) = version {
        vers.iter().find(|x| x.version == v).ok_or_eyre("version not found")?
    } else {
        vers.iter()
            .filter_map(|x| semver::Version::parse(&x.version).ok().map(|sv| (sv, x)))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, x)| x)
            .ok_or_eyre("no semver versions found")?
    };

    let url = selected
        .urls
        .first()
        .ok_or_eyre("no urls for selected chart version")?;
    Ok(join_url(repo_url, url))
}

fn load_chart_from_ref(chart_ref: &str, repo_url: Option<&str>, version: Option<&str>) -> eyre::Result<(VfsPath, Option<TempDir>)> {
    let p = PathBuf::from(chart_ref);
    if p.exists() {
        if p.is_dir() {
            return Ok((vfs_root_from_local_path(&p)?, None));
        }
        if p.is_file() && chart_ref.ends_with(".tgz") {
            let bytes = std::fs::read(&p).map_err(|e| eyre::eyre!(e))?;
            let (td, root) = extract_tgz_to_temp(&bytes)?;
            return Ok((vfs_root_from_local_path(&root)?, Some(td)));
        }
    }

    if chart_ref.starts_with("http://") || chart_ref.starts_with("https://") {
        let bytes = fetch_url_bytes(chart_ref)?;
        let (td, root) = extract_tgz_to_temp(&bytes)?;
        return Ok((vfs_root_from_local_path(&root)?, Some(td)));
    }

    // repo/chart form
    if let Some((_, chart)) = chart_ref.split_once('/') {
        let repo_url = repo_url.ok_or_eyre("repo/chart ref requires --repo-url")?;
        let tgz_url = resolve_repo_chart_tgz_url(repo_url, chart, version)?;
        let bytes = fetch_url_bytes(&tgz_url)?;
        let (td, root) = extract_tgz_to_temp(&bytes)?;
        return Ok((vfs_root_from_local_path(&root)?, Some(td)));
    }

    Err(eyre::eyre!(
        "unsupported chart reference: {chart_ref} (expected path, .tgz, URL, or repo/chart)"
    ))
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_env_filter("info").init();

    let cli = Cli::parse();

    match cli.cmd {
        Command::Scan { include_tests, path } => {
            let root = vfs_root_from_local_path(&path)?;
            let summary = load_chart(
                &root,
                &LoadOptions {
                    include_tests,
                    ..Default::default()
                },
            )?;
            println!("{summary:?}");
        }
        Command::ValuesSchema(args) => {
            let (root, _temp) = load_chart_from_ref(
                &args.chart,
                args.repo_url.as_deref(),
                args.version.as_deref(),
            )?;

            let chart = load_chart(
                &root,
                &LoadOptions {
                    include_tests: false,
                    recurse_subcharts: true,
                    auto_extract_tgz: true,
                    ..Default::default()
                },
            )?;

            let mut uses = run_vyt_on_chart_templates(&chart)?;
            uses.sort_by(cmp_uses);

            let upstream = {
                let mut u = UpstreamK8sSchemaProvider::new(args.k8s_version)
                    .with_allow_download(args.allow_net);
                if let Some(dir) = args.k8s_schema_cache {
                    u = u.with_cache_dir(dir);
                }
                u
            };
            let provider = UpstreamThenDefaultVytSchemaProvider {
                upstream,
                fallback: DefaultVytSchemaProvider::default(),
            };

            let schema = generate_values_schema_vyt(&uses, &provider);
            let schema_pretty = json_pretty(&schema)?;
            write_output(&schema_pretty, args.output.as_deref())?;
        }
    }
    Ok(())
}
