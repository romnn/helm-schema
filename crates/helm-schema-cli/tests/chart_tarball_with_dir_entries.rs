//! Regression test for chart-discovery on wrapper charts whose vendored
//! subchart tarballs contain explicit directory entries (paths ending in `/`).
//!
//! `chart::extract_chart_archive` joins every tar entry path into a vfs
//! `MemoryFS` via `VfsPath::join`, which rejects any path argument ending in
//! `/` (vfs-0.12.2 `src/path.rs:62-64`, `VfsErrorKind::InvalidPath`). The
//! standard `tar` tooling that packages real-world Helm charts — including
//! Bitnami's charts on the public chart catalog — emits explicit directory
//! entries, so the discovery step fails before any template analysis can run.
//!
//! Discovered against `deployment/charts/minio/charts/minio-17.0.21.tgz` in
//! the airtype repo, whose bundled bitnami minio tarball starts with
//! `minio/`, `minio/templates/`, `minio/charts/`, … entries.
//!
//! Everything runs against an in-memory `vfs::MemoryFS` — no temp dirs, no
//! disk I/O — matching the abstraction `helm-schema-cli` already uses
//! internally for chart extraction.

use std::io;

use color_eyre::eyre::WrapErr;
use flate2::Compression;
use flate2::write::GzEncoder;
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const SUBCHART_NAME: &str = "subchart";

const WRAPPER_CHART_YAML: &str = "\
apiVersion: v2
name: wrapper
version: 0.1.0
dependencies:
  - name: subchart
    version: 0.1.0
";

const WRAPPER_VALUES_YAML: &str = "{}\n";

const SUBCHART_CHART_YAML: &str = "\
apiVersion: v2
name: subchart
version: 0.1.0
";

const SUBCHART_VALUES_YAML: &str = "enabled: true\n";

const SUBCHART_CONFIGMAP_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
data:
  enabled: \"{{ .Values.enabled }}\"
";

fn into_eyre(e: helm_schema_cli::CliError) -> color_eyre::eyre::Report {
    e.into()
}

/// Build a gzipped tar archive of a minimal synthetic subchart entirely in
/// memory. The archive deliberately contains explicit *directory* entries
/// (paths ending in `/`) — the shape `tar`/`helm package` emit by default —
/// because that's what the regression hinges on.
fn build_subchart_tarball() -> color_eyre::eyre::Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let gz = GzEncoder::new(&mut buf, Compression::default());
        let mut builder = tar::Builder::new(gz);

        append_dir_entry(&mut builder, &format!("{SUBCHART_NAME}/"))?;
        append_dir_entry(&mut builder, &format!("{SUBCHART_NAME}/templates/"))?;
        append_file_entry(
            &mut builder,
            &format!("{SUBCHART_NAME}/Chart.yaml"),
            SUBCHART_CHART_YAML.as_bytes(),
        )?;
        append_file_entry(
            &mut builder,
            &format!("{SUBCHART_NAME}/values.yaml"),
            SUBCHART_VALUES_YAML.as_bytes(),
        )?;
        append_file_entry(
            &mut builder,
            &format!("{SUBCHART_NAME}/templates/configmap.yaml"),
            SUBCHART_CONFIGMAP_TEMPLATE.as_bytes(),
        )?;

        builder.finish().wrap_err("finalize tarball")?;
    }
    Ok(buf)
}

/// Append an explicit directory entry to the tar archive. `tar::Builder`'s
/// higher-level helpers don't reliably emit directory entries across crate
/// versions, so the entry is assembled by hand to make the failure shape
/// deterministic.
fn append_dir_entry<W: io::Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
) -> color_eyre::eyre::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).wrap_err("set dir path")?;
    header.set_mode(0o755);
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Directory);
    header.set_cksum();
    builder
        .append(&header, io::empty())
        .wrap_err("append dir entry")?;
    Ok(())
}

fn append_file_entry<W: io::Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    contents: &[u8],
) -> color_eyre::eyre::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).wrap_err("set file path")?;
    header.set_mode(0o644);
    header.set_size(u64::try_from(contents.len()).wrap_err("file size fits in u64")?);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    builder
        .append(&header, contents)
        .wrap_err("append file entry")?;
    Ok(())
}

#[test]
fn wrapper_chart_with_subchart_tarball_containing_dir_entries() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join(format!("charts/{SUBCHART_NAME}-0.1.0.tgz"))?,
        build_subchart_tarball()?,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    // `global: {}` is mirrored into every subchart slot to match
    // Helm's chartutil.MergeValues, which writes `global: {}` into
    // each subchart at render time even when no chart in the tree
    // declares its own `global`. Without the mirror, `helm lint
    // --strict` rejects the merged values against an otherwise-correct
    // inferred schema.
    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "subchart": {
                "additionalProperties": false,
                "properties": {
                    "enabled": { "type": "boolean" },
                    "global": { "additionalProperties": {}, "type": "object" }
                },
                "type": "object"
            }
        },
        "type": "object"
    });

    sim_assert_eq!(have: actual, want: expected);
    Ok(())
}
