use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

use crate::generation::{AuthoringPolicy, EmissionSelection};
use crate::provider_builder::ProviderOptions;

use super::{GenerateOptions, PreparedSession, PreparedValuesDocuments};

#[test]
fn review_law_lint_observes_ignored_physical_root_defaults() -> eyre::Result<()> {
    let root = vfs::VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&root.join("Chart.yaml")?, "name: root\nversion: 0.1.0")?;
    test_util::write(&root.join("values.yaml")?, "ignored: 7")?;
    test_util::write(&root.join(".helmignore")?, "values.yaml")?;
    let session = PreparedSession::from_generate_options(&GenerateOptions {
        chart_source: root.into(),
        include_tests: false,
        include_subchart_values: false,
        values_files: Vec::new(),
        infer_required: false,
        emission: EmissionSelection::default(),
        authoring: AuthoringPolicy::default(),
        provider: ProviderOptions::default(),
    })?;
    let want = PreparedValuesDocuments::new(serde_yaml::from_str("{}")?, serde_yaml::Value::Null)
        .with_uncoalesced_root(serde_yaml::from_str("ignored: 7")?);
    sim_assert_eq!(have: session.values_documents, want: want);
    Ok(())
}
