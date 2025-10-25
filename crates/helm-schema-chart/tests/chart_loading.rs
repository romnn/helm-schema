use color_eyre::eyre;
use helm_schema_chart::{ChartSummary, LoadOptions, load_chart};
use helm_schema_core::ChartIdentity;
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn loads_basic_chart() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());

    write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: demo
            version: 0.1.0
        "#},
    )?;
    write(&root.join("values.yaml")?, "replicaCount: 1\n")?;
    write(&root.join("templates/deploy.yaml")?, "kind: Deployment\n")?;
    write(
        &root.join("crds/mycrd.yaml")?,
        "apiVersion: apiextensions.k8s.io/v1\n",
    )?;

    let sum = load_chart(&root, &LoadOptions::default())?;
    assert_that!(
        &sum,
        matches_pattern!(ChartSummary {
            identity: matches_pattern!(ChartIdentity {
                name: eq("demo"),
                version: eq("0.1.0"),
                app_version: none(),
            }),
            templates: contains_path("/templates/deploy.yaml"),
            crds: contains_path("/crds/mycrd.yaml"),
            values_files: contains_path("/values.yaml"),
            ..
        })
    );
    Ok(())
}

#[test]
fn detects_helpers_and_subcharts() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());

    write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            name: parent
            version: 0.1.0
        "#},
    )?;
    write(&root.join("templates/_helpers.tpl")?, "{{/* helpers */}}\n")?;
    // unpacked subchart
    write(
        &root.join("charts/child/Chart.yaml")?,
        "name: child\nversion: 0.1.0\n",
    )?;

    let sum = load_chart(&root, &LoadOptions::default())?;
    assert!(sum.has_helpers);
    assert_eq!(sum.subcharts.len(), 1);
    assert_eq!(sum.subcharts[0].name, "child");
    Ok(())
}

#[test]
fn honors_tests_exclusion() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());

    write(&root.join("Chart.yaml")?, "name: t\nversion: 0.1.0\n")?;
    write(&root.join("tests/unit/foo.yaml")?, "kind: Pod\n")?;

    let sum = load_chart(
        &root,
        &LoadOptions {
            include_tests: false,
            ..Default::default()
        },
    )?;
    assert_that!(sum.templates, is_empty());

    let sum2 = load_chart(
        &root,
        &LoadOptions {
            include_tests: true,
            ..Default::default()
        },
    )?;
    assert_that!(sum2.templates, len(eq(1)));
    Ok(())
}

#[test]
fn errors_without_chart_yaml() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());
    let err = load_chart(&root, &LoadOptions::default()).unwrap_err();
    assert_that!(err.to_string(), contains_regex(r"Missing .*/Chart\.yaml"));
    Ok(())
}

#[test]
fn respects_gitignore_and_hidden() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());
    // chart skeleton
    write(&root.join("g/Chart.yaml")?, "name: g\nversion: 0.1.0\n")?;
    write(&root.join("g/templates/a.yaml")?, "kind: Pod\n")?;
    write(&root.join("g/templates/.hidden.yaml")?, "kind: Pod\n")?;
    write(&root.join("g/templates/ignored.yaml")?, "kind: Pod\n")?;
    // .gitignore excludes templates/ignored.yaml
    write(&root.join("g/.gitignore")?, "templates/ignored.yaml\n")?;

    let chart = root.join("g")?;
    // default opts: respect_gitignore=true, include_hidden=false
    let sum = load_chart(
        &chart,
        &LoadOptions::default()
            .with_respect_gitignore(true)
            .with_include_hidden(false),
    )?;
    assert_that!(
        &sum,
        matches_pattern!(ChartSummary {
            templates: unordered_elements_are![matches_path("/g/templates/a.yaml")],
            ..
        })
    );

    // Include hidden: hidden file shows up
    let sum2 = load_chart(
        &chart,
        &LoadOptions::default()
            .with_respect_gitignore(true)
            .with_include_hidden(true),
    )?;
    assert_that!(
        &sum2,
        matches_pattern!(ChartSummary {
            templates: unordered_elements_are![
                matches_path("/g/templates/a.yaml"),
                matches_path("/g/templates/.hidden.yaml"),
            ],
            ..
        })
    );
    Ok(())
}

#[test]
fn values_file_variants_detected() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());
    write(&root.join("x/Chart.yaml")?, "name: x\nversion: 0.1.0\n")?;
    write(&root.join("x/values.yaml")?, "a: 1\n")?;
    write(&root.join("x/values.dev.yaml")?, "a: 2\n")?;
    write(&root.join("x/values-production.yaml")?, "a: 3\n")?;

    let chart = root.join("x")?;
    let sum = load_chart(&chart, &LoadOptions::default())?;
    assert_that!(
        &sum,
        matches_pattern!(ChartSummary {
            values_files: unordered_elements_are![
                matches_path("/x/values.yaml"),
                matches_path("/x/values.dev.yaml"),
                // matches_path("/x/values-production.yaml"),
            ],
            ..
        })
    );
    Ok(())
}

#[test]
fn ignores_root_yaml_not_under_templates_or_crds() -> eyre::Result<()> {
    test_util::Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());
    write(&root.join("Chart.yaml")?, "name: y\nversion: 0.1.0\n")?;
    write(&root.join("random.yaml")?, "kind: Pod\n")?;
    let sum = load_chart(&root, &LoadOptions::default())?;
    assert_that!(sum.templates, is_empty());
    assert_that!(sum.crds, is_empty());
    Ok(())
}

#[cfg(feature = "extract_tgz")]
#[test]
fn extracts_packed_subchart_tgz() -> eyre::Result<()> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    use tar::{Builder, EntryType, Header};

    test_util::Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());
    write(&root.join("app/Chart.yaml")?, "name: app\nversion: 1.0.0\n")?;

    // Build child.tgz entirely in memory (NO real FS)
    let mut tar_buf = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_buf);

        // Directory entry: child/
        let mut dir_header = Header::new_gnu();
        dir_header.set_entry_type(EntryType::Directory);
        dir_header.set_mode(0o755);
        dir_header.set_size(0);
        dir_header.set_cksum();
        tar.append_data(&mut dir_header, "child/", std::io::empty())?;

        // File entry: child/Chart.yaml
        let chart_yaml = b"name: child\nversion: 0.1.0\n";
        let mut file_header = Header::new_gnu();
        file_header.set_entry_type(EntryType::Regular);
        file_header.set_mode(0o644);
        file_header.set_size(chart_yaml.len() as u64);
        file_header.set_cksum();
        tar.append_data(
            &mut file_header,
            "child/Chart.yaml",
            std::io::Cursor::new(&chart_yaml[..]),
        )?;

        tar.finish()?;
    }

    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&tar_buf)?;
    let tgz_bytes = gz.finish()?;
    assert_that!(tgz_bytes, len(gt(0)));

    // Place tgz under VFS and run loader
    let tgz = root.join("app/charts/child-0.1.0.tgz")?;
    tgz.parent().create_dir_all()?;

    {
        let mut f = tgz.create_file()?;
        f.write_all(&tgz_bytes)?;
        f.flush()?;
    }

    let chart = root.join("app")?;
    let sum = load_chart(
        &chart,
        &LoadOptions {
            auto_extract_tgz: true,
            ..Default::default()
        },
    )?;
    assert_that!(sum.subcharts, len(gt(0)));
    assert!(sum.subcharts.iter().any(|s| s.name == "child"));
    Ok(())
}
