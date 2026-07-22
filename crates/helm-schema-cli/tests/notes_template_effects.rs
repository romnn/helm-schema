//! Helm executes `templates/NOTES.txt`, so consumers and terminal effects
//! inside it are schema evidence like any other template, while its prose
//! must not feed YAML resource detection.
//!
//! Shapes mirror the audited charts: trivy-operator's notes `tpl` a
//! values string, while velero accumulates migration errors in a local and
//! fails after all checks have run.

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use vfs::VfsPath;

const CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

const VALUES_YAML: &str = "\
targetNamespaces: ~
backupStorageLocation: ~
";

const NOTES: &str = "\
Thank you for installing {{ .Chart.Name }}.

Scanning namespaces: {{ tpl .Values.targetNamespaces . }}

{{- $breaking := \"\" }}
{{- if kindIs \"map\" .Values.backupStorageLocation }}
{{- $breaking = print $breaking \"backupStorageLocation moved to the list form\" }}
{{- end }}
{{- if $breaking }}
{{- fail $breaking }}
{{- end }}
";

const TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
data:
  placeholder: \"static\"
";

fn generated_schema() -> eyre::Result<serde_json::Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, VALUES_YAML)?;
    test_util::write(&chart_dir.join("templates/NOTES.txt")?, NOTES)?;
    test_util::write(&chart_dir.join("templates/cm.yaml")?, TEMPLATE)?;

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
            crd_catalog_cache_dir: Some(
                test_util::workspace_root().join(".cache/crds-catalog-cache"),
            ),
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}

#[test]
fn notes_template_consumers_and_validators_become_schema_evidence() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema()?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // The notes' `tpl` consumer requires a string when truthy.
    assert!(
        validator.is_valid(&serde_json::json!({ "targetNamespaces": "ns-a,ns-b" })),
        "strings feed the notes' tpl call",
    );
    assert!(
        !validator.is_valid(&serde_json::json!({ "targetNamespaces": { "a": 1 } })),
        "a truthy non-string reaches the notes' tpl call and aborts rendering",
    );

    // The notes' migration `fail` rejects the legacy map form.
    assert!(
        validator.is_valid(&serde_json::json!({ "backupStorageLocation": [{ "name": "x" }] })),
        "the supported list form renders",
    );
    assert!(
        !validator.is_valid(&serde_json::json!({ "backupStorageLocation": { "name": "x" } })),
        "the notes validator terminates rendering for the legacy map form",
    );

    Ok(())
}
