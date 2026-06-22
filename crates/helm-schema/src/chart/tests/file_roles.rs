use super::*;
use test_util::prelude::sim_assert_eq;

fn role_paths(files: &[ChartFile], role: FileRole) -> Vec<String> {
    let mut paths = files
        .iter()
        .filter(|file| file.has_role(role))
        .map(|file| {
            file.path
                .as_str()
                .rsplit_once('/')
                .map(|(_, path)| path.to_string())
                .unwrap_or_else(|| file.path.as_str().to_string())
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn file_roles_preserve_existing_chart_file_classification() -> color_eyre::eyre::Result<()> {
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

    let files_without_tests = list_chart_files(&chart_dir, false)?;
    sim_assert_eq!(
        have: role_paths(&files_without_tests, FileRole::ManifestTemplate),
        want: vec!["deployment.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&files_without_tests, FileRole::DefineIndexTemplate),
        want: vec!["_helpers.tpl", "deployment.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&files_without_tests, FileRole::StaticCrd),
        want: vec!["example.yaml"]
    );
    sim_assert_eq!(
        have: role_paths(&files_without_tests, FileRole::FilesGetSource),
        want: vec!["config.yaml"]
    );

    let files_with_tests = list_chart_files(&chart_dir, true)?;
    sim_assert_eq!(
        have: role_paths(&files_with_tests, FileRole::ManifestTemplate),
        want: vec!["deployment.yaml", "test-job.yaml"]
    );

    Ok(())
}
