use crate::load_budget::LoadBudget;
use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use flate2::{Compression, write::GzEncoder};
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

use super::*;

const DEMO_MANIFEST: &str = indoc! {"
    name: demo
    version: 0.1.0
"};

fn load_directory(root: &VfsPath, include_tests: bool) -> EngineResult<LoadedChart> {
    LoadedChart::read_directory(Arc::new(Directory::new(root.clone(), None)), include_tests)
}

fn tarball(wrapper: &str, entries: &[(&str, impl AsRef<[u8]>)]) -> eyre::Result<Vec<u8>> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, contents) in entries {
        let contents = contents.as_ref();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(contents.len().try_into()?);
        tar.append_data(&mut header, format!("{wrapper}/{name}"), contents)?;
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
        ("charts/child/Chart.yaml", "name: child\nversion: 0.1.0"),
        ("Chart.yaml", DEMO_MANIFEST),
    ];
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, contents) in entries {
        test_util::write(&root.join(name)?, contents)?;
    }
    let directory = load_directory(&root, false)?;
    let archive = LoadedChart::read_archive(
        tarball("demo", &entries)?.as_slice(),
        SourcePath::Native(PathBuf::from("demo.tgz")),
        false,
    )?;
    let mut names = entries
        .map(|(name, _)| name)
        .into_iter()
        .filter(|name| !name.starts_with("charts/"))
        .collect::<Vec<_>>();
    names.sort_unstable();
    let names_and_bytes = |chart: &LoadedChart| -> eyre::Result<_> {
        names
            .iter()
            .map(|name| {
                let file = chart.file(name).ok_or_eyre("expected chart file")?;
                Ok((file.name.clone(), Arc::clone(&file.bytes)))
            })
            .collect::<eyre::Result<Vec<_>>>()
    };
    sim_assert_eq!(have: names_and_bytes(&directory)?, want: names_and_bytes(&archive)?);
    Ok(())
}

#[test]
fn archive_keeps_duplicate_members_in_input_order() -> eyre::Result<()> {
    let entries = [
        ("Chart.yaml", DEMO_MANIFEST),
        ("values.yaml", "old: true"),
        ("templates/z.yaml", "kind: Secret"),
        ("values.yaml", "new: true"),
        ("templates/z.yaml", "kind: ConfigMap"),
    ];
    let chart = LoadedChart::read_archive(
        tarball("demo", &entries)?.as_slice(),
        SourcePath::Native(PathBuf::from("demo.tgz")),
        false,
    )?;
    let members = chart.entries.files.iter().map(|entry| {
        let SourcePath::ArchiveMember { archive, original_name } = &entry.source else {
            eyre::bail!("expected archive member");
        };
        assert!(matches!(archive.as_ref(), SourcePath::Native(path) if path == Path::new("demo.tgz")));
        Ok((
            std::str::from_utf8(original_name)?.trim_start_matches("demo/"),
            std::str::from_utf8(&entry.file.bytes)?,
        ))
    }).collect::<eyre::Result<Vec<_>>>()?;
    sim_assert_eq!(have: members, want: entries.to_vec());
    let selected = chart
        .file("values.yaml")
        .map(|file| file.source().map(str::to_owned))
        .transpose()?;
    sim_assert_eq!(have: selected, want: Some("new: true".to_string()));
    let templates = chart
        .files_with_role(FileRole::ManifestTemplate)
        .map(LoadedFile::source)
        .collect::<EngineResult<Vec<_>>>()?;
    sim_assert_eq!(have: templates, want: ["kind: ConfigMap"]);
    Ok(())
}

fn role_paths(chart: &LoadedChart, role: FileRole) -> Vec<String> {
    let mut paths = chart
        .files_with_role(role)
        .map(|file| {
            file.text_name()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths
}

#[test]
fn file_roles_follow_chart_file_classification() -> eyre::Result<()> {
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
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "name: child\nversion: 0.1.0",
    )?;
    test_util::write(&chart_dir.join("charts/child/config/child.json")?, "{}\n")?;

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.schema.json")?, "{}")?;
    test_util::write(&chart_dir.join("charts/leaf.prov")?, "signed")?;
    test_util::write(&chart_dir.join("files/templates/values.yaml")?, "nested")?;
    test_util::write(&chart_dir.join("requirements.yaml")?, "dependencies: []")?;
    test_util::write(&chart_dir.join("templates/.ignored.yaml")?, "ignored")?;
    test_util::write(&chart_dir.join("templates/selected.json")?, "{}")?;
    test_util::write(&chart_dir.join(".helmignore")?, "# comment")?;
    let charts = discover_directory(chart_dir.clone(), false)?;
    let chart = &charts.first().ok_or_eyre("root chart")?.files;
    assert!(chart.file("values.schema.json").is_some());
    assert!(chart.file(".helmignore").is_some());
    sim_assert_eq!(
        have: role_paths(chart, FileRole::ManifestTemplate),
        want: vec!["deployment.yaml", "selected.json"]
    );
    sim_assert_eq!(
        have: role_paths(chart, FileRole::DefineIndexTemplate),
        want: vec!["_helpers.tpl", "_script.txt", "deployment.yaml", "selected.json"]
    );
    sim_assert_eq!(
        have: role_paths(chart, FileRole::StaticCrd),
        want: vec!["example.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(chart, FileRole::FilesGetSource),
        want: vec![".helmignore", "client-auth.json", "config.yaml", "example.yaml", "leaf.prov", "values.yaml"]
    );

    assert!(chart.file("templates/tests/test-job.yaml").is_some());
    let chart = load_directory(&chart_dir, true)?;
    sim_assert_eq!(
        have: role_paths(&chart, FileRole::ManifestTemplate),
        want: vec!["deployment.yaml", "selected.json", "test-job.yaml"]
    );

    Ok(())
}

#[test]
fn loaded_chart_corpus_owns_the_classified_source_snapshot() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, DEMO_MANIFEST)?;
    let template = chart_dir.join("templates/configmap.yaml")?;
    test_util::write(&template, "kind: ConfigMap\n")?;
    let values = chart_dir.join("values.yaml")?;
    test_util::write(&values, "original: true")?;
    let chart = load_directory(&chart_dir, false)?;
    chart.file("values.yaml").ok_or_eyre("captured values")?;
    test_util::write(&template, "kind: Secret\n")?;
    test_util::write(&values, "changed: true")?;
    let sources = chart
        .files_with_role(FileRole::ManifestTemplate)
        .map(|file| file.source().map(str::to_owned))
        .collect::<EngineResult<Vec<_>>>()?;
    sim_assert_eq!(have: sources, want: vec!["kind: ConfigMap\n"]);
    let values = chart
        .file("values.yaml")
        .map(|file| file.source().map(str::to_owned))
        .transpose()?;
    sim_assert_eq!(have: values, want: Some("original: true".to_string()));
    Ok(())
}

#[test]
fn directory_chart_roles_use_case_sensitive_member_names() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-casing")?;
    let root = VfsPath::new(vfs::PhysicalFS::new(scratch.path()));
    test_util::write(&root.join("Chart.yaml")?, DEMO_MANIFEST)?;
    for name in [
        "VALUES.YAML",
        "CRDS/widget.yaml",
        "charts/child/Chart.yaml",
        "charts/child/VALUES.YAML",
    ] {
        test_util::write(&root.join(name)?, "{}")?;
    }
    let chart = load_directory(&root, false)?;
    assert!(chart.file("values.yaml").is_none());
    let static_crds = chart
        .files_with_role(FileRole::StaticCrd)
        .filter_map(LoadedFile::text_name)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: static_crds, want: Vec::<&str>::new());
    let mut assets = chart
        .files_with_role(FileRole::FilesGetSource)
        .filter_map(LoadedFile::text_name)
        .collect::<Vec<_>>();
    assets.sort_unstable();
    sim_assert_eq!(have: assets, want: vec!["CRDS/widget.yaml", "VALUES.YAML"]);
    Ok(())
}

#[test]
fn case_sensitive_template_selection_keeps_distinct_directory_entries() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, DEMO_MANIFEST)?;
    test_util::write(&root.join("templates/a.yaml")?, "a")?;
    test_util::write(&root.join("Templates/b.yaml")?, "b")?;
    let chart = load_directory(&root, false)?;
    sim_assert_eq!(have: role_paths(&chart, FileRole::ManifestTemplate), want: vec!["a.yaml"]);
    assert!(chart.file("Templates/b.yaml").is_some());
    Ok(())
}

#[test]
fn vendored_metadata_failure_precedes_later_invalid_package() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("charts/a/Chart.yaml")?, "name: [")?;
    test_util::write(&root.join("charts/z.tgz")?, "invalid archive")?;
    let error = super::super::discover_chart_contexts(&root.into(), false)
        .expect_err("child metadata is invalid");
    assert!(matches!(error, CliError::ChartDecode { .. }), "{error:?}");
    Ok(())
}

#[test]
fn prepared_config_bytes_are_the_same_bytes_seen_by_files_get() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, DEMO_MANIFEST)?;
    let config = root.join("helm-schema.yaml")?;
    test_util::write(&config, "version: 1")?;
    let mut source = crate::chart_source::RootChartSource::directory(root);
    sim_assert_eq!(have: source.read_root_file("helm-schema.yaml")?, want: Some("version: 1".to_string()));
    test_util::write(&config, "version: 2")?;
    let chart = source.load(false)?;
    let files = chart
        .files_with_role(FileRole::FilesGetSource)
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
    let charts = crate::chart::discover_chart_contexts(&root.into(), false)?;
    let defines = crate::chart::build_define_index(&charts)?;
    for (name, template) in &expected {
        sim_assert_eq!(have: defines.get_file(name), want: Some(template.as_str()));
    }
    Ok(())
}

#[test]
fn archive_normalizes_members_without_losing_original_spelling() -> eyre::Result<()> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    let name = b"demo/./sub/../Chart.yaml";
    let manifest = DEMO_MANIFEST.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(manifest.len().try_into()?);
    let raw_name = header
        .as_mut_bytes()
        .get_mut(..name.len())
        .ok_or_eyre("archive name does not fit")?;
    raw_name.copy_from_slice(name);
    header.set_cksum();
    tar.append(&header, manifest)?;
    let bytes = tar.into_inner()?.finish()?;
    let chart = LoadedChart::read_archive(
        bytes.as_slice(),
        SourcePath::Native(PathBuf::from("charts/demo.tgz")),
        false,
    )?;
    let entry = chart.entries.files.first().ok_or_eyre("archive entry")?;
    let SourcePath::ArchiveMember {
        archive,
        original_name,
    } = &entry.source
    else {
        eyre::bail!("expected archive member");
    };
    assert!(
        matches!(archive.as_ref(), SourcePath::Native(path) if path == Path::new("charts/demo.tgz"))
    );
    sim_assert_eq!(have: original_name.as_ref(), want: name.as_slice());
    sim_assert_eq!(have: chart.file("Chart.yaml").map(|file| file.source().map(str::to_owned)).transpose()?, want: Some(DEMO_MANIFEST.to_string()));
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
) -> EngineResult<Vec<crate::chart::ChartContext>> {
    crate::chart::discover_chart_contexts(&root.into(), include_tests)
}

#[test]
fn directory_reads_helmignore_before_the_walk() -> eyre::Result<()> {
    let (root, opens) = ObservedFs::with_denied(&["/.helmignore"]);
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join(".helmignore")?, "*")?;
    let error = discover_directory(root, false).expect_err("unreadable .helmignore");
    assert!(matches!(error, CliError::Vfs(_)), "{error:?}");
    sim_assert_eq!(have: opens.lock().unwrap().as_slice(), want: ["/.helmignore"]);
    Ok(())
}

#[test]
fn mount_with_files_but_without_a_manifest_is_an_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("charts/no-manifest/file.yaml")?, "opaque")?;
    let error = discover_directory(root, false).expect_err("missing child manifest");
    assert!(matches!(error, CliError::Vfs(_)), "{error:?}");
    assert!(
        error.to_string().contains("/charts/no-manifest/Chart.yaml"),
        "{error}"
    );
    Ok(())
}

#[test]
fn directory_prunes_ignored_paths_before_opening_members() -> eyre::Result<()> {
    let skipped = "/templates/ignored/fifo";
    let retained = "/templates/retained/control.yaml";
    let (root, opens) = ObservedFs::with_denied(&[skipped]);
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join(".helmignore")?, "templates/ignored/")?;
    test_util::write(&root.join(skipped)?, "")?;
    test_util::write(&root.join(retained)?, "retained")?;
    let charts = discover_directory(root, false)?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/templates/retained/control.yaml"), want: Some("retained"));
    assert!(!opens.lock().unwrap().iter().any(|path| path == skipped));
    Ok(())
}

#[test]
fn legacy_snapshot_archive_cap_does_not_truncate_templates() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    let template = "{{ define \"my-helper\" }}my answer{{ end }}";
    test_util::write(&root.join("templates/charts/helper.tgz")?, template)?;
    let charts = discover_directory(root, false)?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/templates/charts/helper.tgz"), want: Some(template));
    Ok(())
}

#[test]
fn direct_input_lookup_caches_backend_hit_and_miss() -> eyre::Result<()> {
    let upper = VfsPath::new(ObservedFs {
        folded: BTreeMap::from([("/values.yaml".to_string(), "/VALUES.YAML".to_string())]),
        ..ObservedFs::default()
    });
    let lower = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&upper.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&upper.join("VALUES.YAML")?, "answer: upper")?;
    test_util::write(&lower.join("Values.Yaml")?, "answer: lower")?;
    let root = VfsPath::new(vfs::OverlayFS::new(&[upper.clone(), lower]));
    let mut source = crate::chart_source::RootChartSource::directory(root);
    sim_assert_eq!(have: source.read_root_file("values.yaml")?, want: Some("answer: upper".into()));
    sim_assert_eq!(have: source.read_root_file("missing")?, want: None);
    test_util::write(&upper.join("VALUES.YAML")?, "answer: changed")?;
    test_util::write(&upper.join("missing")?, "present after lookup")?;
    sim_assert_eq!(have: source.read_root_file("values.yaml")?, want: Some("answer: upper".into()));
    sim_assert_eq!(have: source.read_root_file("missing")?, want: None);
    Ok(())
}

#[test]
fn legacy_snapshot_reuses_chart_local_values_flag_bytes() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-local-values")?;
    let values = scratch.path().join("values.yaml");
    std::fs::write(
        scratch.path().join("Chart.yaml"),
        "name: root\nversion: 0.1.0",
    )?;
    std::fs::write(&values, "answer: original")?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false)?;
    crate::chart::load_values_files_and_check_keys(&charts, &[])?;
    std::fs::write(&values, "answer: changed")?;
    let sources = crate::chart::load_values_files_and_check_keys(&charts, &[values])?;
    sim_assert_eq!(have: sources.first().map(AsRef::as_ref), want: Some("answer: original"));
    Ok(())
}

#[test]
fn legacy_snapshot_vendored_archive_keeps_its_containing_source() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-package-parent")?;
    std::fs::write(
        scratch.path().join("Chart.yaml"),
        "name: root\nversion: 0.1.0",
    )?;
    std::fs::create_dir(scratch.path().join("charts"))?;
    let package = scratch.path().join("charts/leaf.tgz");
    std::fs::write(
        &package,
        tarball("leaf", &[("Chart.yaml", "name: leaf\nversion: 0.1.0")])?,
    )?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false)?;
    let leaf = charts.get(1).ok_or_eyre("leaf chart")?;
    let entry = leaf
        .files
        .entries
        .files
        .first()
        .ok_or_eyre("archive entry")?;
    assert!(
        matches!(&entry.source, SourcePath::ArchiveMember { archive, .. }
            if matches!(archive.as_ref(), SourcePath::Native(path) if *path == package.canonicalize()?)),
        "{:?}",
        entry.source
    );
    Ok(())
}

#[test]
fn missing_manifest_is_a_vfs_file_not_found() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    let error = discover_directory(root, false).expect_err("missing manifest");
    let CliError::Vfs(error) = error else {
        eyre::bail!("expected a VFS error, got {error:?}");
    };
    assert!(
        matches!(error.kind(), vfs::error::VfsErrorKind::FileNotFound),
        "{error:?}"
    );
    assert!(error.to_string().contains("/Chart.yaml"), "{error}");
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
    assert!(error.to_string().contains(path), "{error}");
    Ok(())
}

#[test]
fn non_utf8_manifest_is_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, [0xff])?;
    assert_vfs_invalid_data(
        discover_directory(root, false).expect_err("invalid manifest"),
        "/Chart.yaml",
    )
}

#[test]
fn non_utf8_requirements_are_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("requirements.yaml")?, [0xff])?;
    assert_vfs_invalid_data(
        discover_directory(root, false).expect_err("invalid requirements"),
        "/requirements.yaml",
    )
}

#[test]
fn non_utf8_defaults_are_a_vfs_read_error() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("values.yaml")?, [0xff])?;
    let charts = discover_directory(root, false)?;
    assert_vfs_invalid_data(
        crate::chart::build_composed_values_document(&charts, false).expect_err("invalid defaults"),
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
    std::fs::write(
        scratch.path().join("Chart.yaml"),
        "name: root\nversion: 0.1.0",
    )?;
    std::fs::write(&values, "answer: original")?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let charts = crate::chart::discover_chart_contexts(&source, false)?;
    let first =
        crate::chart::load_values_files_and_check_keys(&charts, std::slice::from_ref(&values))?;
    std::fs::write(&values, "answer: changed")?;
    let second = crate::chart::load_values_files_and_check_keys(&charts, &[values])?;
    sim_assert_eq!(have: first.first().map(AsRef::as_ref), want: Some("answer: original"));
    sim_assert_eq!(have: second, want: first);
    crate::chart::build_define_index(&charts)?;
    Ok(())
}

#[test]
fn selection_law_directory_filters_but_archive_does_not() -> eyre::Result<()> {
    let entries = [
        ("Chart.yaml", "name: root\nversion: 0.1.0"),
        (".helmignore", "charts/[^x]eaf/\n"),
        ("charts/leaf/Chart.yaml", "name: leaf\nversion: 0.1.0"),
        ("charts/leaf/templates/run.yaml", "leaf"),
        ("charts/xeaf/Chart.yaml", "name: xeaf\nversion: 0.1.0"),
        ("charts/xeaf/templates/run.yaml", "xeaf"),
    ];
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, contents) in entries {
        test_util::write(&root.join(name)?, contents)?;
    }
    let charts = discover_directory(root, false)?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/charts/xeaf/templates/run.yaml"), want: Some("xeaf"));
    sim_assert_eq!(have: index.get_file("root/charts/leaf/templates/run.yaml"), want: None);

    let archive = LoadedChart::read_archive(
        tarball("root", &entries)?.as_slice(),
        SourcePath::Native(PathBuf::from("root.tgz")),
        false,
    )?;
    let mut seen = Vec::new();
    archive.visit_vendored_charts(|child| {
        seen.push(child.path.rsplit('/').next().unwrap_or_default().to_owned());
        Ok(())
    })?;
    sim_assert_eq!(have: seen, want: vec!["leaf", "xeaf"]);
    assert!(
        role_paths(&archive, FileRole::FilesGetSource)
            .iter()
            .any(|name| name == ".helmignore")
    );
    Ok(())
}

#[test]
fn selection_law_directory_filters_below_retained_child() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, contents) in [
        ("Chart.yaml", "name: root\nversion: 0.1.0"),
        (
            ".helmignore",
            indoc! {"
                charts/leaf/values.yaml
                charts/leaf/templates/skip.yaml
                charts/leaf/files/skip.txt
                charts/leaf/crds/skip.yaml
                charts/leaf/requirements.yaml
            "},
        ),
        ("charts/leaf/Chart.yaml", "name: leaf\nversion: 0.1.0"),
        ("charts/leaf/.helmignore", "templates/keep.yaml\n"),
        ("charts/leaf/values.yaml", "enabled: true"),
        ("charts/leaf/requirements.yaml", "dependencies: []"),
        ("charts/leaf/templates/skip.yaml", "skipped"),
        ("charts/leaf/templates/keep.yaml", "retained"),
        ("charts/leaf/files/skip.txt", "skipped"),
        ("charts/leaf/files/keep.txt", "retained"),
        ("charts/leaf/crds/skip.yaml", "skipped"),
        ("charts/leaf/crds/keep.yaml", "retained"),
    ] {
        test_util::write(&root.join(name)?, contents)?;
    }
    let charts = discover_directory(root, false)?;
    let leaf = charts.get(1).ok_or_eyre("retained child")?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/charts/leaf/templates/keep.yaml"), want: Some("retained"));
    sim_assert_eq!(have: index.get_file("root/charts/leaf/templates/skip.yaml"), want: None);
    assert!(leaf.files.file("values.yaml").is_none());
    assert!(leaf.files.file("requirements.yaml").is_none());
    sim_assert_eq!(have: role_paths(&leaf.files, FileRole::StaticCrd), want: vec!["keep.yaml"]);
    sim_assert_eq!(have: role_paths(&leaf.files, FileRole::FilesGetSource), want: vec![".helmignore", "keep.txt", "keep.yaml"]);
    Ok(())
}

#[test]
fn selection_law_skips_prefixed_nested_mounts_and_keeps_packages() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    let package = |name: &str, template: &str| {
        tarball(
            "wrapper",
            &[
                ("Chart.yaml", format!("name: {name}\nversion: 0.1.0")),
                ("templates/run.yaml", template.to_owned()),
            ],
        )
    };
    let hidden = package("hidden", "dot skipped")?;
    let leaf = package("leaf", "retained")?;
    let duplicate = package("leaf", "duplicate skipped")?;
    let parent = tarball(
        "wrapper",
        &[
            ("Chart.yaml", b"name: parent\nversion: 0.1.0".as_slice()),
            (
                "charts/_hidden/Chart.yaml",
                b"name: hidden\nversion: 0.1.0".as_slice(),
            ),
            (
                "charts/_hidden/templates/run.yaml",
                b"underscore skipped".as_slice(),
            ),
            ("charts/.hidden.tgz", hidden.as_slice()),
            ("charts/leaf.tgz", leaf.as_slice()),
            ("charts/leaf.tgz", duplicate.as_slice()),
        ],
    )?;
    test_util::write(&root.join("charts/parent.tgz")?, parent)?;
    let charts = discover_directory(root, false)?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/charts/parent/charts/leaf/templates/run.yaml"), want: Some("retained"));
    sim_assert_eq!(have: index.get_file("root/charts/parent/charts/hidden/templates/run.yaml"), want: None);
    sim_assert_eq!(have: charts.len(), want: 3);
    let leaf = charts.get(2).ok_or_eyre("retained leaf")?;
    let manifest = leaf
        .files
        .entries
        .files
        .first()
        .ok_or_eyre("leaf manifest")?;
    let SourcePath::ArchiveMember {
        archive: parent,
        original_name,
    } = &manifest.source
    else {
        eyre::bail!("leaf member");
    };
    sim_assert_eq!(have: original_name.as_ref(), want: b"wrapper/Chart.yaml");
    let SourcePath::ArchiveMember {
        archive: root,
        original_name,
    } = parent.as_ref()
    else {
        eyre::bail!("packaged leaf member");
    };
    sim_assert_eq!(have: original_name.as_ref(), want: b"wrapper/charts/leaf.tgz");
    assert!(
        matches!(root.as_ref(), SourcePath::Virtual(path) if path.as_str() == "/charts/parent.tgz")
    );
    Ok(())
}

#[test]
fn archive_normalization_is_independent_of_host_paths() -> eyre::Result<()> {
    for (raw, normalized) in [
        ("chart/Chart.yaml", "Chart.yaml"),
        ("chart/./files/../values.yaml", "values.yaml"),
        (r"chart\templates\run.yaml", "templates/run.yaml"),
    ] {
        sim_assert_eq!(have: archive_name("chart.tgz", raw.as_bytes())?, want: normalized.as_bytes());
    }
    for raw in [
        "Chart.yaml",
        "chart/../evil",
        r"chart\..\evil",
        "chart//etc/passwd",
        "chart/C:/file",
    ] {
        assert!(
            matches!(archive_name("chart.tgz", raw.as_bytes()), Err(CliError::UnsafeArchiveEntryPath { entry_path, .. }) if entry_path == raw)
        );
    }
    Ok(())
}

#[test]
fn selection_law_uses_exact_mount_extension_and_never_searches_archive_roots() -> eyre::Result<()> {
    let read = |contents: &[u8]| {
        LoadedChart::read_archive(
            contents,
            SourcePath::Native(PathBuf::from("root.tgz")),
            false,
        )
    };
    let displaced = tarball(
        "wrapper",
        &[("nested/Chart.yaml", "name: root\nversion: 0.1.0")],
    )?;
    assert!(matches!(
        read(&displaced),
        Err(CliError::NoChartYamlInArchive { .. })
    ));

    let valid = tarball(
        "wrapper",
        &[
            ("Chart.yaml", "name: leaf\nversion: 0.1.0"),
            ("templates/run.yaml", "retained"),
        ],
    )?;
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    let good = root.join("charts/leaf.tgz")?;
    test_util::write(&good, &valid)?;
    let charts = discover_directory(root.clone(), false)?;
    sim_assert_eq!(have: crate::chart::build_define_index(&charts)?.get_file("root/charts/leaf/templates/run.yaml"), want: Some("retained"));

    good.remove_file()?;
    test_util::write(&root.join("charts/leaf.tar.gz")?, valid)?;
    assert!(discover_directory(root, false).is_err());
    Ok(())
}

#[test]
fn ignore_patterns_match_go_filepath_rules() -> eyre::Result<()> {
    use crate::chart::ignore::Rules;

    let rules = Rules::parse(b"/root.yaml\ntmp/\nfiles/[^x]et.?\n")?;
    assert!(rules.ignores(b"root.yaml", false));
    assert!(!rules.ignores(b"other/root.yaml", false));
    assert!(rules.ignores(b"other/tmp", true));
    assert!(!rules.ignores(b"other/tmp", false));
    assert!(rules.ignores(b"files/pet.a", false));
    assert!(!rules.ignores(b"files/xet.a", false));

    let go_byte_star = Rules::parse(b"*??")?;
    assert!(go_byte_star.ignores("☃".as_bytes(), false));
    assert!(Rules::parse(b"**/temp").is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_directory_selects_symlink_targets_and_prunes_oversized_files() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-native-walk")?;
    let root = scratch.path().join("root");
    let target = scratch.path().join("outside");
    std::fs::create_dir_all(root.join("templates"))?;
    std::fs::create_dir_all(&target)?;
    std::fs::write(root.join("Chart.yaml"), "name: root\nversion: 0.1.0")?;
    let controls = target.join("control.yaml");
    std::fs::write(&controls, "retained")?;
    std::os::unix::fs::symlink(&target, root.join("templates/link"))?;
    std::fs::write(root.join(".helmignore"), "oversized.bin\n")?;
    std::fs::File::create(root.join("oversized.bin"))?.set_len(MAX_FILE_SIZE as u64 + 1)?;
    let load = || {
        let source = crate::chart_source::RootChartSource::open(&root, LoadBudget::default())?;
        crate::chart::discover_chart_contexts(&source, false)
    };
    let charts = load()?;
    let index = crate::chart::build_define_index(&charts)?;
    sim_assert_eq!(have: index.get_file("root/templates/link/control.yaml"), want: Some("retained"));
    assert!(index.get_file("root/oversized.bin").is_none());
    std::fs::write(controls, "changed")?;
    let chart = &charts.first().ok_or_eyre("root chart")?.files;
    let captured = chart.load_local_file(&root.join("templates/link/control.yaml"))?;
    sim_assert_eq!(have: captured.as_deref(), want: Some(b"retained".as_slice()));

    std::fs::remove_file(root.join(".helmignore"))?;
    assert!(matches!(
        load(),
        Err(CliError::LoadBudgetExceeded {
            limit_bytes: MAX_FILE_SIZE,
            ..
        })
    ));
    Ok(())
}

#[test]
fn directory_and_archive_strip_bom_from_selected_payloads() -> eyre::Result<()> {
    let entries = [
        ("Chart.yaml", "name: root\nversion: 0.1.0"),
        ("payload.txt", "\u{feff}retained"),
    ];
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, contents) in entries {
        test_util::write(&root.join(name)?, contents)?;
    }
    let directory = load_directory(&root, false)?;
    let archive = LoadedChart::read_archive(
        tarball("wrapper", &entries)?.as_slice(),
        SourcePath::Native(PathBuf::from("root.tgz")),
        false,
    )?;
    for chart in [directory, archive] {
        let file = chart.file("payload.txt").ok_or_eyre("retained payload")?;
        sim_assert_eq!(have: file.source()?, want: "retained");
    }
    Ok(())
}

#[test]
fn directory_input_prepares_a_source_manifest_before_selection() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join(".helmignore")?, "Chart.template.yaml\n")?;
    test_util::write(
        &root.join("Chart.template.yaml")?,
        "name: prepared\nversion: 0.1.0",
    )?;
    test_util::write(&root.join("templates/control.yaml")?, "retained")?;
    let charts = discover_directory(root.clone(), false)?;
    let chart = charts.first().ok_or_eyre("prepared chart")?;
    assert!(chart.files.file("Chart.template.yaml").is_none());
    sim_assert_eq!(have: crate::chart::build_define_index(&charts)?.get_file("prepared/templates/control.yaml"), want: Some("retained"));

    test_util::write(&root.join("Chart.yaml")?, "name: existing\nversion: 0.1.0")?;
    let charts = discover_directory(root, false)?;
    sim_assert_eq!(have: crate::chart::build_define_index(&charts)?.get_file("existing/templates/control.yaml"), want: Some("retained"));
    Ok(())
}

#[test]
fn native_snapshot_bytes_belong_to_one_analysis_session() -> eyre::Result<()> {
    let scratch = test_util::scratch::ScratchDir::new("chart-loader-sessions")?;
    std::fs::write(
        scratch.path().join("Chart.yaml"),
        "name: root\nversion: 0.1.0",
    )?;
    let values_path = scratch.path().join("values.yaml");
    std::fs::write(&values_path, "answer: first")?;
    let source = crate::chart_source::RootChartSource::open(scratch.path(), LoadBudget::default())?;
    let first = source.load(false)?;
    std::fs::write(&values_path, "answer: second")?;
    let second = source.load(false)?;
    sim_assert_eq!(have: first.file("values.yaml").ok_or_eyre("first snapshot")?.source()?, want: "answer: first");
    sim_assert_eq!(have: second.file("values.yaml").ok_or_eyre("second snapshot")?.source()?, want: "answer: second");
    Ok(())
}

#[test]
fn directory_loader_rejects_a_directory_in_place_of_helmignore() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("templates/control.yaml")?, "retained")?;
    let ignored = root.join(".helmignore")?;
    ignored.create_dir()?;
    assert!(discover_directory(root.clone(), false).is_err());
    ignored.remove_dir()?;
    test_util::write(&ignored, "# comments are accepted")?;
    let charts = discover_directory(root, false)?;
    sim_assert_eq!(have: crate::chart::build_define_index(&charts)?.get_file("root/templates/control.yaml"), want: Some("retained"));
    Ok(())
}

#[test]
fn review_law_validates_ignore_rule_before_removing_directory_suffix() -> eyre::Result<()> {
    use crate::chart::ignore::Rules;
    let rules = Rules::parse(br"data\/")?;
    assert!(!rules.ignores(b"data", true));
    Ok(())
}

#[test]
fn review_law_reads_large_self_ignored_rules() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    let mut rules = b"/.helmignore\n".to_vec();
    rules.extend(b"# short comment\n".repeat(MAX_FILE_SIZE / 8));
    test_util::write(&root.join(".helmignore")?, rules)?;
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    let chart = load_directory(&root, true)?;
    assert!(chart.file(".helmignore").is_none());
    assert!(chart.file("Chart.yaml").is_some());
    Ok(())
}

#[test]
fn review_law_preserves_distinct_non_utf8_archive_names() -> eyre::Result<()> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, data) in [
        (
            b"root/Chart.yaml".as_slice(),
            b"name: root\nversion: 0.1.0".as_slice(),
        ),
        (b"root/files/\xff.txt".as_slice(), b"first".as_slice()),
        (b"root/files/\xfe.txt".as_slice(), b"second".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(data.len().try_into()?);
        header
            .as_mut_bytes()
            .get_mut(..name.len())
            .ok_or_eyre("member name")?
            .copy_from_slice(name);
        header.set_cksum();
        tar.append(&header, data)?;
    }
    let archive = tar.into_inner()?.finish()?;
    let chart = LoadedChart::read_archive(
        archive.as_slice(),
        SourcePath::Native(PathBuf::from("root.tgz")),
        true,
    )?;
    let files: Vec<_> = chart
        .files_with_role(FileRole::FilesGetSource)
        .map(|file| file.bytes.as_ref())
        .collect();
    sim_assert_eq!(have: files, want: [b"first".as_slice(), b"second".as_slice()]);
    Ok(())
}

#[test]
fn review_law_archive_member_count_is_not_a_helm_limit() -> eyre::Result<()> {
    let mut entries = vec![("Chart.yaml".to_owned(), "name: root\nversion: 0.1.0")];
    entries.extend((0..4096).map(|i| (format!("files/{i}"), "x")));
    let entries: Vec<_> = entries
        .iter()
        .map(|(name, bytes)| (name.as_str(), *bytes))
        .collect();
    let archive = tarball("root", &entries)?;
    let chart = LoadedChart::read_archive(
        archive.as_slice(),
        SourcePath::Native(PathBuf::from("root.tgz")),
        true,
    )?;
    sim_assert_eq!(have: chart.files_with_role(FileRole::FilesGetSource).count(), want: 4096);
    Ok(())
}

#[test]
fn review_law_requirements_uses_the_overlaid_version_and_notes_use_suffix() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    for (name, data) in [
        (
            "Chart.yaml",
            indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies: []
        "},
        ),
        ("requirements.yaml", "apiVersion: v1"),
        ("templates/help-NOTES.txt", "arbitrary text"),
        ("templates/manifest.yaml", "kind: ConfigMap"),
    ] {
        test_util::write(&root.join(name)?, data)?;
    }
    let charts = discover_directory(root, true)?;
    let chart = &charts.first().ok_or_eyre("root")?.files;
    let actual = [
        FileRole::FilesGetSource,
        FileRole::NotesTemplate,
        FileRole::ManifestTemplate,
    ]
    .map(|role| role_paths(chart, role));
    sim_assert_eq!(have: actual, want: [vec!["requirements.yaml"], vec!["help-NOTES.txt"], vec!["manifest.yaml"]]);
    Ok(())
}

#[test]
fn review_law_helm_archive_byte_limits_use_payload_size() -> eyre::Result<()> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::none()));
    let file = vec![0; MAX_FILE_SIZE];
    for number in 0..13 {
        let mut header = tar::Header::new_gnu();
        header.set_size(file.len().try_into()?);
        header.set_mode(0o644);
        tar.append_data(&mut header, format!("root/files/{number}"), file.as_slice())?;
    }
    let manifest = b"name: root\nversion: 0.1.0";
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest.len().try_into()?);
    header.set_mode(0o644);
    tar.append_data(&mut header, "root/Chart.yaml", manifest.as_slice())?;
    let archive = tar.into_inner()?.finish()?;
    assert!(archive.len() > 64 * 1024 * 1024);
    let chart = LoadedChart::read_archive(
        archive.as_slice(),
        SourcePath::Native(PathBuf::from("root.tgz")),
        true,
    )?;
    sim_assert_eq!(have: chart.files_with_role(FileRole::FilesGetSource).count(), want: 13);

    let mut compressed = vec![("Chart.yaml", manifest.as_slice()); 20];
    compressed.fill(("files/data", &file));
    let exactly_at_limit = tarball("root", &compressed)?;
    let error = LoadedChart::read_archive(
        exactly_at_limit.as_slice(),
        SourcePath::Native(PathBuf::from("exact.tgz")),
        true,
    )
    .expect_err("Helm rejects an archive at the expanded limit");
    assert!(matches!(
        error,
        CliError::LoadBudgetExceeded {
            limit_bytes: MAX_ARCHIVE_SIZE,
            ..
        }
    ));

    let mut oversized = file;
    oversized.push(0);
    let compressed = tarball(
        "root",
        &[
            ("Chart.yaml", manifest.as_slice()),
            ("files/oversized", oversized.as_slice()),
        ],
    )?;
    let error = LoadedChart::read_archive(
        compressed.as_slice(),
        SourcePath::Native(PathBuf::from("per-file.tgz")),
        true,
    )
    .expect_err("Helm rejects a member larger than its limit");
    assert!(matches!(
        error,
        CliError::LoadBudgetExceeded {
            limit_bytes: MAX_FILE_SIZE,
            ..
        }
    ));
    Ok(())
}

#[test]
fn review_law_ignore_literal_keeps_the_filename_bytes() -> eyre::Result<()> {
    use crate::chart::ignore::Rules;
    let rules = Rules::parse(b"/files/\xff.txt")?;
    assert!(rules.ignores(b"files/\xff.txt", false));
    assert!(!rules.ignores(b"files/\xfe.txt", false));
    assert!(!rules.ignores("files/�.txt".as_bytes(), false));
    Ok(())
}

#[test]
fn slice3_duplicate_manifests_overlay_in_archive_order() -> eyre::Result<()> {
    let archive = tarball(
        "root",
        &[
            (
                "Chart.yaml",
                indoc! {"
            apiVersion: v2
            name: earlier
            version: 0.1.0
            description: carry-forward
            dependencies:
              - {name: first}
              - {name: second, condition: gates.second}
        "},
            ),
            (
                "Chart.yaml",
                indoc! {"
                name: later
                dependencies: [{name: first}]
            "},
            ),
            (
                "requirements.yaml",
                indoc! {"
                dependencies: [{name: first}, {name: added}]
            "},
            ),
        ],
    )?;
    let chart = LoadedChart::read_archive(
        archive.as_slice(),
        SourcePath::Native(PathBuf::from("chart.tgz")),
        false,
    )?;
    sim_assert_eq!(
        have: ["name", "description", "apiVersion"].map(|field| chart.metadata()[field].as_str()),
        want: [Some("later"), Some("carry-forward"), Some("v2")],
    );
    sim_assert_eq!(have: chart.metadata()["dependencies"][1]["name"].as_str(), want: Some("added"));
    sim_assert_eq!(have: chart.metadata()["dependencies"][1]["condition"].as_str(), want: Some("gates.second"));
    Ok(())
}
