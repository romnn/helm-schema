use color_eyre::eyre::{self, OptionExt as _};
use flate2::{Compression, write::GzEncoder};
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

use super::*;

fn tarball(wrapper: &str, entries: &[(&str, &str)]) -> eyre::Result<Vec<u8>> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, contents) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(contents.len().try_into()?);
        tar.append_data(
            &mut header,
            format!("{wrapper}/{name}"),
            contents.as_bytes(),
        )?;
    }
    Ok(tar.into_inner()?.finish()?)
}

#[test]
fn directory_and_archive_twins_expose_the_same_ordered_entries() -> eyre::Result<()> {
    let entries = [
        ("values.yaml", "message: Hello"),
        ("templates/message.yaml", "kind: ConfigMap"),
        ("templates/_helpers.tpl", "{{ define \"banner\" }}{{ end }}"),
        ("files/banner.txt", "Hello"),
        ("crds/record.yaml", "kind: CustomResourceDefinition"),
        ("charts/child/Chart.yaml", "name: child"),
        ("Chart.yaml", "name: demo"),
    ];
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, contents) in entries {
        test_util::write(&root.join(name)?, contents)?;
    }
    let directory = LoadedChart::read_directory(&root, None, None, false)?;
    let archive = LoadedChart::read_archive(
        &tarball("demo", &entries)?,
        "demo.tgz",
        SourcePath::Native(PathBuf::from("demo.tgz")),
        LoadBudget::default(),
        false,
    )?;
    assert!(matches!(
        directory.entries.as_ref(),
        LoadedEntries::Directory(_)
    ));
    assert!(matches!(
        archive.entries.as_ref(),
        LoadedEntries::Archive { .. }
    ));

    let mut names = entries.map(|(name, _)| name);
    names.sort_unstable();
    let names_and_bytes = |chart: &LoadedChart| -> eyre::Result<_> {
        names
            .iter()
            .map(|name| {
                let file = chart.file(name)?.ok_or_eyre("expected chart file")?;
                Ok((file.name, file.bytes))
            })
            .collect::<eyre::Result<Vec<_>>>()
    };
    sim_assert_eq!(have: names_and_bytes(&directory)?, want: names_and_bytes(&archive)?);
    Ok(())
}

#[test]
fn archive_keeps_duplicate_members_in_input_order() -> eyre::Result<()> {
    let entries = [
        ("Chart.yaml", "name: demo"),
        ("values.yaml", "old: true"),
        ("templates/z.yaml", "kind: ConfigMap"),
        ("values.yaml", "new: true"),
    ];
    let chart = LoadedChart::read_archive(
        &tarball("demo", &entries)?,
        "demo.tgz",
        SourcePath::Native(PathBuf::from("demo.tgz")),
        LoadBudget::default(),
        false,
    )?;
    let LoadedEntries::Archive {
        source_path,
        entries: raw,
        visible: _,
    } = chart.entries.as_ref()
    else {
        eyre::bail!("expected archive entries");
    };
    assert!(matches!(source_path, SourcePath::Native(path) if path == Path::new("demo.tgz")));
    let members = raw
        .iter()
        .map(|entry| {
            Ok((
                std::str::from_utf8(&entry.original_archive_name)?.trim_start_matches("demo/"),
                std::str::from_utf8(&entry.bytes)?,
            ))
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    sim_assert_eq!(have: members, want: entries.to_vec());
    let selected = chart
        .file("values.yaml")?
        .map(|file| file.source().map(str::to_owned))
        .transpose()?;
    sim_assert_eq!(have: selected, want: Some("new: true".to_string()));
    Ok(())
}

fn role_paths(chart: &LoadedChart, role: FileRole) -> EngineResult<Vec<String>> {
    chart.prepare_roles()?;
    let mut paths = chart
        .files_with_role(role)?
        .into_iter()
        .map(|file| file.name.rsplit('/').next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    Ok(paths)
}

#[test]
fn file_roles_preserve_existing_chart_file_classification() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        "kind: Deployment\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        "{{ define \"x\" }}{{ end }}\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/tests/test-job.yaml")?,
        "kind: Job\n",
    )?;
    test_util::write(
        &chart_dir.join("crds/example.yaml")?,
        "kind: CustomResourceDefinition\n",
    )?;
    test_util::write(&chart_dir.join("files/config.yaml")?, "answer: 42\n")?;
    test_util::write(
        &chart_dir.join("config/client-auth.json")?,
        "{\"users\": []}\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/_script.txt")?,
        "{{ .Values.script }}\n",
    )?;
    test_util::write(&chart_dir.join("charts/child/config/child.json")?, "{}\n")?;

    test_util::write(&chart_dir.join("values.schema.json")?, "{}")?;
    test_util::write(&chart_dir.join(".helmignore")?, "*")?;
    let chart = LoadedChart::read_directory(&chart_dir, None, None, false)?;
    assert!(chart.file("values.schema.json")?.is_some());
    assert!(chart.file(".helmignore")?.is_some());
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::ManifestTemplate)?,
        want: vec!["deployment.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::DefineIndexTemplate)?,
        want: vec!["_helpers.tpl", "_script.txt", "deployment.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::StaticCrd)?,
        want: vec!["example.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::FilesGetSource)?,
        want: vec!["client-auth.json", "config.yaml", "example.yaml"]
    );

    assert!(chart.file("templates/tests/test-job.yaml")?.is_some());
    let chart = LoadedChart::read_directory(&chart_dir, None, None, true)?;
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::ManifestTemplate)?,
        want: vec!["deployment.yaml", "test-job.yaml"]
    );

    Ok(())
}

#[test]
fn loaded_chart_corpus_owns_the_classified_source_snapshot() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    let template = chart_dir.join("templates/configmap.yaml")?;
    test_util::write(&template, "kind: ConfigMap\n")?;
    let values = chart_dir.join("values.yaml")?;
    test_util::write(&values, "original: true")?;
    let chart = LoadedChart::read_directory(&chart_dir, None, None, false)?;
    chart.prepare_roles()?;
    chart.file("values.yaml")?.ok_or_eyre("captured values")?;
    test_util::write(&template, "kind: Secret\n")?;
    test_util::write(&values, "changed: true")?;
    let sources = chart
        .files_with_role(FileRole::ManifestTemplate)?
        .into_iter()
        .map(|file| file.source().map(str::to_owned))
        .collect::<EngineResult<Vec<_>>>()?;
    sim_assert_eq!(have: sources, want: vec!["kind: ConfigMap\n"]);
    let values = chart
        .file("values.yaml")?
        .map(|file| file.source().map(str::to_owned))
        .transpose()?;
    sim_assert_eq!(have: values, want: Some("original: true".to_string()));
    Ok(())
}

#[test]
fn directory_lookups_follow_backend_casing_and_files_keep_the_original_names() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-casing")?;
    let root = VfsPath::new(vfs::PhysicalFS::new(scratch.path()));
    for name in [
        "VALUES.YAML",
        "CRDS/widget.yaml",
        "charts/child/Chart.yaml",
        "charts/child/VALUES.YAML",
    ] {
        test_util::write(&root.join(name)?, "{}")?;
    }
    let folded = root.join("values.yaml")?.is_file()?;
    let chart = LoadedChart::read_directory(&root, None, None, false)?;
    sim_assert_eq!(have: chart.file("values.yaml")?.is_some(), want: folded);
    chart.prepare_roles()?;
    let static_crds = chart
        .files_with_role(FileRole::StaticCrd)?
        .into_iter()
        .map(|file| file.name)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: static_crds, want: if folded { vec!["crds/widget.yaml".to_string()] } else { Vec::new() });
    let mut assets = chart
        .files_with_role(FileRole::FilesGetSource)?
        .into_iter()
        .map(|file| file.name)
        .collect::<Vec<_>>();
    assets.sort_unstable();
    sim_assert_eq!(have: assets, want: vec!["CRDS/widget.yaml", "VALUES.YAML"]);
    Ok(())
}

#[test]
fn case_sensitive_template_selection_keeps_distinct_directory_entries() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("templates/a.yaml")?, "a")?;
    test_util::write(&root.join("Templates/b.yaml")?, "b")?;
    let chart = LoadedChart::read_directory(&root, None, None, false)?;
    sim_assert_eq!(have: role_paths(&chart, FileRole::ManifestTemplate)?, want: vec!["a.yaml"]);
    assert!(chart.file("Templates/b.yaml")?.is_some());
    Ok(())
}

#[test]
fn vendored_metadata_failure_precedes_later_invalid_package() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("charts/a/Chart.yaml")?, "name: [")?;
    test_util::write(&root.join("charts/z.tgz")?, "invalid archive")?;
    let error = super::super::discover_chart_contexts(&root.into(), false, LoadBudget::default())
        .expect_err("child metadata is invalid");
    assert!(matches!(error, CliError::Yaml(_)), "{error:?}");
    Ok(())
}

#[test]
fn prepared_config_bytes_are_the_same_bytes_seen_by_files_get() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    let config = root.join("helm-schema.yaml")?;
    test_util::write(&config, "version: 1")?;
    let mut source = crate::chart_source::RootChartSource::directory(root);
    sim_assert_eq!(have: source.read_root_file("helm-schema.yaml")?, want: Some("version: 1".to_string()));
    test_util::write(&config, "version: 2")?;
    let chart = source.load(false)?;
    chart.prepare_roles()?;
    let files = chart
        .files_with_role(FileRole::FilesGetSource)?
        .into_iter()
        .map(|file| file.source().map(str::to_owned))
        .collect::<EngineResult<Vec<_>>>()?;
    sim_assert_eq!(have: files, want: vec!["version: 1"]);
    Ok(())
}

#[test]
fn nested_packages_with_the_same_wrapper_keep_distinct_templates() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &root.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    let mut expected = Vec::new();
    for parent in ["left", "right"] {
        test_util::write(
            &root.join(format!("charts/{parent}/Chart.yaml"))?,
            formatdoc! {"
                apiVersion: v2
                name: {parent}
                version: 0.1.0
            "},
        )?;
        let template = formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: from-{parent}
        "};
        test_util::write(
            &root.join(format!("charts/{parent}/charts/leaf.tgz"))?,
            tarball(
                "leaf",
                &[
                    (
                        "Chart.yaml",
                        indoc! {"
                    apiVersion: v2
                    name: leaf
                    version: 0.1.0
                "},
                    ),
                    ("templates/configmap.yaml", &template),
                ],
            )?,
        )?;
        expected.push((
            format!("root/charts/{parent}/charts/leaf/templates/configmap.yaml"),
            template,
        ));
    }
    let charts = crate::chart::discover_chart_contexts(&root.into(), false, LoadBudget::default())?;
    let defines = crate::chart::build_define_index(&charts)?;
    for (name, template) in &expected {
        sim_assert_eq!(have: defines.get_file(name), want: Some(template.as_str()));
    }
    Ok(())
}

#[test]
fn archive_keeps_original_member_spelling_outside_the_legacy_view() -> eyre::Result<()> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    let name = b"./demo//Chart.yaml";
    let manifest = b"name: demo";
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(manifest.len().try_into()?);
    let raw_name = header
        .as_mut_bytes()
        .get_mut(..name.len())
        .ok_or_eyre("archive name does not fit")?;
    raw_name.copy_from_slice(name);
    header.set_cksum();
    tar.append(&header, &manifest[..])?;
    let bytes = tar.into_inner()?.finish()?;
    let chart = LoadedChart::read_archive(
        &bytes,
        "demo.tgz",
        SourcePath::Native(PathBuf::from("charts/demo.tgz")),
        LoadBudget::default(),
        false,
    )?;
    let LoadedEntries::Archive {
        source_path,
        entries,
        visible: _,
    } = chart.entries.as_ref()
    else {
        eyre::bail!("expected archive");
    };
    assert!(
        matches!(source_path, SourcePath::Native(path) if path == Path::new("charts/demo.tgz"))
    );
    sim_assert_eq!(have: entries.first().map(|entry| entry.original_archive_name.as_ref()), want: Some(name.as_slice()));
    sim_assert_eq!(have: chart.file("Chart.yaml")?.map(|file| file.source().map(str::to_owned)).transpose()?, want: Some("name: demo".to_string()));
    Ok(())
}

#[derive(Debug, Default)]
struct ObservedFs {
    inner: vfs::MemoryFS,
    denied: BTreeSet<String>,
    folded: BTreeMap<String, String>,
    opens: Arc<std::sync::Mutex<Vec<String>>>,
}

impl ObservedFs {
    fn target<'a>(&'a self, path: &'a str) -> &'a str {
        self.folded.get(path).map_or(path, String::as_str)
    }

    fn with_denied(paths: &[&str]) -> (VfsPath, Arc<std::sync::Mutex<Vec<String>>>) {
        let fs = Self {
            denied: paths.iter().map(|path| (*path).to_owned()).collect(),
            ..Self::default()
        };
        let opens = Arc::clone(&fs.opens);
        (VfsPath::new(fs), opens)
    }
}

impl vfs::FileSystem for ObservedFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        if self.denied.contains(path) {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into());
        }
        self.inner.read_dir(self.target(path))
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        self.opens.lock().unwrap().push(path.to_string());
        if self.denied.contains(path) {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into());
        }
        self.inner.open_file(self.target(path))
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        self.inner.metadata(self.target(path))
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        self.inner.exists(self.target(path))
    }

    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner.create_dir(path)
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.inner.create_file(path)
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.inner.append_file(path)
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner.remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner.remove_dir(path)
    }
}

fn discover_directory(
    root: VfsPath,
    include_tests: bool,
    budget: LoadBudget,
) -> EngineResult<Vec<crate::chart::ChartContext>> {
    crate::chart::discover_chart_contexts(&root.into(), include_tests, budget)
}

#[test]
fn legacy_snapshot_skips_unreadable_helmignore() -> eyre::Result<()> {
    let (root, opens) = ObservedFs::with_denied(&["/.helmignore"]);
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join(".helmignore")?, "*")?;
    let charts = discover_directory(root, false, LoadBudget::default())?;
    crate::chart::build_define_index(&charts)?;
    assert!(
        !opens
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/.helmignore")
    );
    Ok(())
}

#[test]
fn legacy_snapshot_skips_directory_without_a_manifest() -> eyre::Result<()> {
    let (root, _) = ObservedFs::with_denied(&["/charts/no-manifest"]);
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("charts/no-manifest/file.yaml")?, "opaque")?;
    let charts = discover_directory(root, false, LoadBudget::default())?;
    sim_assert_eq!(have: charts.len(), want: 1);
    crate::chart::build_define_index(&charts)?;
    Ok(())
}

#[test]
fn legacy_snapshot_never_opens_excluded_test_templates() -> eyre::Result<()> {
    // Reject an attempted open instead of blocking, as a FIFO would.
    let (root, opens) = ObservedFs::with_denied(&["/templates/tests/fifo"]);
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("templates/tests/fifo")?, "")?;
    let charts = discover_directory(root, false, LoadBudget::default())?;
    crate::chart::build_define_index(&charts)?;
    assert!(
        !opens
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/templates/tests/fifo")
    );
    Ok(())
}

#[test]
fn legacy_snapshot_reports_malformed_metadata_before_template_io() -> eyre::Result<()> {
    let (root, _) = ObservedFs::with_denied(&["/templates/broken.yaml"]);
    test_util::write(&root.join("Chart.yaml")?, "name: [")?;
    test_util::write(&root.join("templates/broken.yaml")?, "")?;
    let error =
        discover_directory(root, false, LoadBudget::default()).expect_err("invalid manifest");
    assert!(matches!(error, CliError::Yaml(_)), "{error:?}");
    Ok(())
}

#[test]
fn legacy_snapshot_reports_values_keys_before_template_io() -> eyre::Result<()> {
    let (root, opens) = ObservedFs::with_denied(&["/templates/broken.yaml"]);
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("values.yaml")?, "on: alias")?;
    test_util::write(&root.join("templates/broken.yaml")?, "")?;
    let charts = discover_directory(root, false, LoadBudget::default())?;
    let error =
        crate::chart::load_values_files_and_check_keys(&charts, &[]).expect_err("invalid keys");
    assert!(
        matches!(error, CliError::YamlBooleanAliasKeys { .. }),
        "{error:?}"
    );
    assert!(
        !opens
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/templates/broken.yaml")
    );
    Ok(())
}

#[test]
fn legacy_snapshot_archive_cap_does_not_truncate_templates() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    let template = "{{ define \"my-helper\" }}my answer{{ end }}";
    test_util::write(&root.join("templates/charts/helper.tgz")?, template)?;
    let charts = discover_directory(
        root,
        false,
        LoadBudget {
            max_chart_archive_bytes: 8,
            ..LoadBudget::default()
        },
    )?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/templates/charts/helper.tgz"), want: Some(template));
    Ok(())
}

#[test]
fn legacy_snapshot_uses_the_overlay_backends_direct_lookup() -> eyre::Result<()> {
    let upper = VfsPath::new(ObservedFs {
        folded: BTreeMap::from([("/values.yaml".to_string(), "/VALUES.YAML".to_string())]),
        ..ObservedFs::default()
    });
    let lower = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&upper.join("Chart.yaml")?, "name: root")?;
    test_util::write(&upper.join("VALUES.YAML")?, "answer: upper")?;
    test_util::write(&lower.join("Values.Yaml")?, "answer: lower")?;
    let root = VfsPath::new(vfs::OverlayFS::new(&[upper, lower]));
    let charts = discover_directory(root, false, LoadBudget::default())?;
    let chart = charts.first().ok_or_eyre("root chart")?;
    sim_assert_eq!(have: crate::chart::chart_own_values(chart)?, want: serde_yaml::from_str::<serde_yaml::Value>("answer: upper")?);
    Ok(())
}

#[test]
fn legacy_snapshot_reuses_chart_local_values_flag_bytes() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-local-values")?;
    let values = scratch.path().join("values.yaml");
    std::fs::write(scratch.path().join("Chart.yaml"), "name: root")?;
    std::fs::write(&values, "answer: original")?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false, LoadBudget::default())?;
    crate::chart::load_values_files_and_check_keys(&charts, &[])?;
    std::fs::write(&values, "answer: changed")?;
    let sources = crate::chart::load_values_files_and_check_keys(&charts, &[values])?;
    sim_assert_eq!(have: sources.first().map(AsRef::as_ref), want: Some("answer: original"));
    Ok(())
}

#[test]
fn legacy_snapshot_vendored_archive_keeps_its_containing_source() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-package-parent")?;
    std::fs::write(scratch.path().join("Chart.yaml"), "name: root")?;
    std::fs::create_dir(scratch.path().join("charts"))?;
    let package = scratch.path().join("charts/leaf.tgz");
    std::fs::write(&package, tarball("leaf", &[("Chart.yaml", "name: leaf")])?)?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false, LoadBudget::default())?;
    let leaf = charts.get(1).ok_or_eyre("leaf chart")?;
    let LoadedEntries::Archive { source_path, .. } = leaf.files.entries.as_ref() else {
        eyre::bail!("leaf is an archive");
    };
    assert!(
        matches!(source_path, SourcePath::Native(path) if *path == package.canonicalize()?),
        "{source_path:?}"
    );
    Ok(())
}

#[test]
fn missing_manifest_is_a_vfs_file_not_found() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    let error =
        discover_directory(root, false, LoadBudget::default()).expect_err("missing manifest");
    let CliError::Vfs(error) = error else {
        eyre::bail!("expected a VFS error, got {error:?}");
    };
    assert!(
        matches!(error.kind(), vfs::error::VfsErrorKind::FileNotFound),
        "{error:?}"
    );
    sim_assert_eq!(have: error.path().as_str(), want: "/Chart.yaml");
    Ok(())
}

fn assert_vfs_invalid_data(error: CliError, path: &str) -> eyre::Result<()> {
    let CliError::Vfs(error) = error else {
        eyre::bail!("expected a VFS error, got {error:?}");
    };
    assert!(
        matches!(error.kind(), vfs::error::VfsErrorKind::IoError(io) if io.kind() == std::io::ErrorKind::InvalidData),
        "{error:?}",
    );
    sim_assert_eq!(have: error.path().as_str(), want: path);
    Ok(())
}

#[test]
fn non_utf8_manifest_is_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, [0xff])?;
    assert_vfs_invalid_data(
        discover_directory(root, false, LoadBudget::default()).expect_err("invalid manifest"),
        "/Chart.yaml",
    )
}

#[test]
fn non_utf8_requirements_are_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("requirements.yaml")?, [0xff])?;
    assert_vfs_invalid_data(
        discover_directory(root, false, LoadBudget::default()).expect_err("invalid requirements"),
        "/requirements.yaml",
    )
}

#[test]
fn non_utf8_defaults_are_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root")?;
    test_util::write(&root.join("values.yaml")?, [0xff])?;
    let charts = discover_directory(root, false, LoadBudget::default())?;
    let chart = charts.first().ok_or_eyre("root chart")?;
    assert_vfs_invalid_data(
        crate::chart::chart_own_values(chart).expect_err("invalid defaults"),
        "/values.yaml",
    )?;
    assert_vfs_invalid_data(
        crate::chart::load_values_files_and_check_keys(&charts, &[])
            .expect_err("invalid defaults during key validation"),
        "/values.yaml",
    )
}

// Apple filesystems reject the non-UTF-8 filename used by this law.
#[cfg(all(unix, not(target_vendor = "apple")))]
#[test]
fn native_values_flag_preserves_non_utf8_filename_and_reuses_bytes() -> eyre::Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let scratch = test_util::scratch::ScratchDir::new("chart-loader-native-values")?;
    let skipped = scratch.path().join("templates/tests");
    std::fs::create_dir_all(&skipped)?;
    let values = skipped.join(OsStr::from_bytes(b"values-\xff.yaml"));
    std::fs::write(scratch.path().join("Chart.yaml"), "name: root")?;
    std::fs::write(&values, "answer: original")?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false, LoadBudget::default())?;
    let first =
        crate::chart::load_values_files_and_check_keys(&charts, std::slice::from_ref(&values))?;
    std::fs::write(&values, "answer: changed")?;
    let second = crate::chart::load_values_files_and_check_keys(&charts, &[values])?;
    sim_assert_eq!(have: first.first().map(AsRef::as_ref), want: Some("answer: original"));
    sim_assert_eq!(have: second, want: first);
    crate::chart::build_define_index(&charts)?;
    Ok(())
}
