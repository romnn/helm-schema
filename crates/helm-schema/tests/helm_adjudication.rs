//! Checks exact Helm values and offline rendered-resource adjudication.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use helm_schema_k8s::cache::{CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, k8s_cache_path};
use indoc::{formatdoc, indoc};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[path = "common/helm_adjudication.rs"]
mod helm_adjudication;

use helm_adjudication::{KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart};

fn write_chart(root: &Path, name: &str, values: &str) -> eyre::Result<()> {
    fs::create_dir_all(root.join("templates"))?;
    fs::write(
        root.join("Chart.yaml"),
        formatdoc! {"
            apiVersion: v2
            name: {name}
            version: 1.0.0
        "},
    )?;
    fs::write(root.join("values.yaml"), values)?;
    Ok(())
}

#[test]
fn unchanged_unknown_resources_are_differential_evidence_only() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    let cache = tempfile::tempdir()?;
    write_chart(root.path(), "differential", "token: original\n")?;
    fs::write(
        root.path().join("templates/resource.yaml"),
        indoc! {r"
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: sample
        spec:
          token: {{ .Values.token }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    assert!(matches!(
        validator.validate(&probe.rendered.stdout)?,
        KubernetesVerdict::Uncertain(_)
    ));
    let differential = validator.validate_differential(&chart, &probe.rendered.stdout)?;
    assert!(
        matches!(differential, KubernetesVerdict::UnchangedUnknown(_)),
        "{differential:?}"
    );
    let changed = chart.adjudicate(&json!({"token": "changed"}))?;
    assert!(matches!(
        validator.validate_differential(&chart, &changed.rendered.stdout)?,
        KubernetesVerdict::Uncertain(_)
    ));
    Ok(())
}

#[test]
fn duplicate_unknown_identities_on_either_side_remain_uncertain() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    for (defaults, overlay) in [(1, 2), (2, 1), (2, 2)] {
        let root = tempfile::tempdir()?;
        write_chart(root.path(), "duplicates", &format!("copies: {defaults}\n"))?;
        fs::write(
            root.path().join("templates/resources.yaml"),
            indoc! {r"
            {{ range until (int .Values.copies) }}
            ---
            apiVersion: example.test/v1
            kind: Unknown
            metadata:
              namespace: example
              name: duplicate
            {{ end }}
        "},
        )?;
        let chart = PinnedHelmChart::prepare(root.path())?;
        let probe = chart.adjudicate(&json!({"copies": overlay}))?;
        let verdict = OfflineKubernetesValidator::new(cache.path())
            .validate_differential(&chart, &probe.rendered.stdout)?;
        assert!(
            matches!(verdict, KubernetesVerdict::Uncertain(_)),
            "{verdict:?}"
        );
    }
    Ok(())
}

#[test]
fn differential_matching_never_hides_known_invalid_resources() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    let cache = tempfile::tempdir()?;
    write_chart(root.path(), "invalid", "{}")?;
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({
            "type": "object", "properties": {
                "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
                "data": {"type": "object", "additionalProperties": {"type": "string"}}
            }
        }),
    )?;
    fs::write(
        root.path().join("templates/resources.yaml"),
        indoc! {r"
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: same
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: invalid
        data:
          token: true
    "},
    )?;
    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let absolute = validator.validate(&probe.rendered.stdout)?;
    assert!(matches!(absolute, KubernetesVerdict::Invalid(_)));
    sim_assert_eq!(have: validator.validate_differential(&chart, &probe.rendered.stdout)?, want: absolute);
    Ok(())
}

#[test]
fn nonresource_and_inexact_unknown_documents_cannot_be_matched() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    for template in [
        "value: scalar\n",
        indoc! {"
            apiVersion: example.test/v1
            kind: Unknown
            metadata:
              name: inexact
            value: 9007199254740993
        "},
    ] {
        let root = tempfile::tempdir()?;
        write_chart(root.path(), "uncertain", "{}")?;
        fs::write(root.path().join("templates/resource.yaml"), template)?;
        let chart = PinnedHelmChart::prepare(root.path())?;
        let probe = chart.adjudicate(&json!({}))?;
        let mut validator = OfflineKubernetesValidator::new(cache.path());
        let absolute = validator.validate(&probe.rendered.stdout)?;
        assert!(matches!(absolute, KubernetesVerdict::Uncertain(_)));
        sim_assert_eq!(have: validator.validate_differential(&chart, &probe.rendered.stdout)?, want: absolute);
    }
    Ok(())
}

#[test]
fn coalescence_preserves_null_ownership_before_render_mutation() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "parent", "owned: 1\n")?;
    write_chart(&root.path().join("charts/child"), "child", "owned: 2\n")?;
    fs::write(
        root.path().join("templates/config.yaml"),
        indoc! {r#"
        {{- $_ := set .Values "mutated" true -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          mutated: {{ .Values.mutated | quote }}
          kubernetes: {{ .Capabilities.KubeVersion.Version | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(root.path())?;
    sim_assert_eq!(have: chart.adjudicate(&json!({}))?.values.as_json(), want: &json!({"owned": 1, "child": {"owned": 2, "global": {}}}));

    // Only defaults owned by the consuming chart delete user nulls.
    let overlay = json!({"owned": null, "absent": null, "child": {"owned": null, "absent": null}});
    let expected = json!({"absent": null, "child": {"absent": null, "global": {}}});
    let probe = chart.adjudicate(&overlay)?;
    sim_assert_eq!(have: probe.values.as_json(), want: &expected);
    eyre::ensure!(
        probe.rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    let rendered: serde_json::Value = serde_yaml::from_slice(&probe.rendered.stdout)?;
    sim_assert_eq!(have: rendered, want: json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "example"}, "data": {"mutated": "true", "kubernetes": "v1.29.0"}}));
    sim_assert_eq!(have: serde_json::from_slice::<serde_json::Value>(&fs::read(probe.evidence_dir.join("coalesced.json"))?)?, want: expected);
    sim_assert_eq!(have: serde_json::from_slice::<serde_json::Value>(&fs::read(probe.evidence_dir.join("values.json"))?)?, want: overlay);
    Ok(())
}

#[test]
fn packed_dependencies_are_sanitized_recursively_without_changing_sources() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "parent", "{}")?;
    fs::rename(
        root.path().join("Chart.yaml"),
        root.path().join("Chart.template.yaml"),
    )?;
    fs::write(root.path().join("values.schema.json"), "false")?;
    fs::create_dir_all(root.path().join("templates/tests"))?;
    fs::write(
        root.path().join("templates/tests/fail.yaml"),
        "{{ fail \"excluded root test\" }}",
    )?;

    let child = tempfile::tempdir()?;
    write_chart(child.path(), "child", "childDefault: 3\n")?;
    fs::write(child.path().join("values.schema.json"), "false")?;
    fs::create_dir_all(child.path().join("templates/tests"))?;
    fs::write(
        child.path().join("templates/tests/fail.yaml"),
        "{{ fail \"excluded child test\" }}",
    )?;
    let grandchild = tempfile::tempdir()?;
    write_chart(grandchild.path(), "grandchild", "leaf: 4\n")?;
    fs::write(grandchild.path().join("values.schema.json"), "false")?;
    pack_chart(
        grandchild.path(),
        &child.path().join("charts/grandchild.tgz"),
        "grandchild",
    )?;
    let archive = root.path().join("charts/child.tgz");
    pack_chart(child.path(), &archive, "child")?;
    let original = fs::read(&archive)?;

    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    sim_assert_eq!(have: probe.values.as_json(), want: &json!({"child": {"childDefault": 3, "global": {}, "grandchild": {"leaf": 4, "global": {}}}}));
    sim_assert_eq!(have: fs::read(&archive)?, want: original);
    sim_assert_eq!(have: fs::read_to_string(root.path().join("values.schema.json"))?, want: "false");
    assert!(!root.path().join("Chart.yaml").exists());
    let evidence = probe
        .evidence_dir
        .parent()
        .ok_or_eyre("probe has no evidence root")?;
    assert!(!evidence.join("render/charts/child").exists());
    let render = archive_files(&fs::read(evidence.join("render/charts/child.tgz"))?)?;
    assert!(!render.contains_key(Path::new("child/values.schema.json")));
    assert!(
        !render
            .keys()
            .any(|path| path.starts_with("child/templates/tests"))
    );
    let grandchild = archive_files(
        render
            .get(Path::new("child/charts/grandchild.tgz"))
            .ok_or_eyre("missing nested archive")?,
    )?;
    assert!(!grandchild.contains_key(Path::new("grandchild/values.schema.json")));
    let coalesce = archive_files(&fs::read(evidence.join("coalesce/charts/child.tgz"))?)?;
    assert!(
        !coalesce
            .keys()
            .any(|path| path.starts_with("child/templates"))
    );
    Ok(())
}

fn archive_files(bytes: &[u8]) -> eyre::Result<BTreeMap<std::path::PathBuf, Vec<u8>>> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut files = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_file() {
            let path = entry.path()?.into_owned();
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            files.insert(path, contents);
        }
    }
    Ok(files)
}

fn pack_chart(root: &Path, archive: &Path, name: &str) -> eyre::Result<()> {
    fs::create_dir_all(archive.parent().ok_or_eyre("archive has no parent")?)?;
    let encoder = GzEncoder::new(fs::File::create(archive)?, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.append_dir_all(name, root)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

#[test]
fn render_abort_keeps_the_exact_document_and_evidence() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "aborting", "{}")?;
    fs::write(
        root.path().join("templates/fail.yaml"),
        "{{ fail \"intentional abort\" }}",
    )?;
    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({"unknown": null}))?;
    sim_assert_eq!(have: probe.values.as_json(), want: &json!({"unknown": null}));
    assert!(!probe.rendered.status.success());
    assert!(
        fs::read_to_string(probe.evidence_dir.join("render.stderr"))?.contains("intentional abort")
    );
    Ok(())
}

#[test]
fn offline_validator_distinguishes_invalid_resources_from_missing_schemas() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let schema = json!({"type": "object", "properties": {
        "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
        "metadata": {"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}
    }, "required": ["apiVersion", "kind", "metadata"]});
    write_cached_schema(cache.path(), "configmap-v1.json", &schema)?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let valid = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "valid"}});
    sim_assert_eq!(have: validator.validate(&serde_json::to_vec(&valid)?)?, want: KubernetesVerdict::Valid);

    // A List wrapper must not hide its contained resource's typed violation.
    let list = json!({"apiVersion": "v1", "kind": "List", "items": [
        {"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": true}}
    ]});
    let KubernetesVerdict::Invalid(errors) = validator.validate(&serde_json::to_vec(&list)?)?
    else {
        eyre::bail!("wrongly typed ConfigMap name was not rejected");
    };
    eyre::ensure!(
        errors.len() == 1
            && errors
                .iter()
                .any(|error| error.contains("document 0/items/0")
                    && error.contains("/metadata/name")),
        "unexpected evidence: {errors:?}"
    );
    let missing = json!({"apiVersion": "example.test/v1", "kind": "Missing", "metadata": {"name": "unknown"}});
    let KubernetesVerdict::Uncertain(errors) =
        validator.validate(&serde_json::to_vec(&missing)?)?
    else {
        eyre::bail!("missing offline schema was treated as a verdict");
    };
    sim_assert_eq!(have: errors, want: vec!["document 0: example.test/v1/Missing unknown: pinned resource schema not found".to_string()]);
    Ok(())
}

#[test]
fn unresolved_schema_references_do_not_fetch_or_prove_validity() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}, "allOf": [{"$ref": "https://invalid.example.test/not-cached.json"}]}),
    )?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let document =
        json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "unknown"}});
    let KubernetesVerdict::Uncertain(errors) =
        validator.validate(&serde_json::to_vec(&document)?)?
    else {
        eyre::bail!("missing reference was treated as a resource verdict");
    };
    eyre::ensure!(
        errors.iter().any(
            |error| error.contains("original resource schema did not compile")
                && error.contains("not-cached.json")
        ),
        "missing reference evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn helm_yaml_decoding_preserves_boolean_keys_octal_scalars_and_unicode_boundaries()
-> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "café"}, "data": {"true": 10, "plain": true, "quoted": "on", "config": "---\nyes\n"}});
    // An exact schema makes every decoded scalar and key part of the assertion.
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let source = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: café
        data:
          yes: 012
          plain: on
          quoted: 'on'
          config: |
            ---
            yes
    "};
    sim_assert_eq!(have: validator.validate(source.as_bytes())?, want: KubernetesVerdict::Valid);
    let repeated = format!("{source}{source}");
    sim_assert_eq!(have: validator.validate(repeated.as_bytes())?, want: KubernetesVerdict::Valid);
    let typed_name = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: yes
    "};
    let KubernetesVerdict::Invalid(errors) =
        validator.validate(format!("{source}{typed_name}").as_bytes())?
    else {
        eyre::bail!("YAML 1.1 boolean name was not rejected");
    };
    eyre::ensure!(
        errors.len() == 1 && errors.iter().all(|error| error.contains("document 1:")),
        "unexpected evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn helm_yaml_decoder_errors_remain_uncertain() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    // Helm fromYaml reports sequence roots through its Error member.
    let source = "[one, two]";
    let KubernetesVerdict::Uncertain(errors) = validator.validate(source.as_bytes())? else {
        eyre::bail!("unsupported YAML document was treated as a resource verdict");
    };
    eyre::ensure!(
        errors
            .iter()
            .any(|error| error.contains("Helm fromYaml reported an error")),
        "missing decoding evidence: {errors:?}"
    );
    Ok(())
}

#[test]
fn raw_document_transport_preserves_unicode_line_breaks() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "transport"}, "data": {"nel": "before\nafter\n", "ls": "before\u{2028}after\n", "ps": "before\u{2029}after\n"}});
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let source = formatdoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: transport
        data:
          nel: |
            before{nel}    after
          ls: |
            before{ls}    after
          ps: |
            before{ps}    after
    ", nel = '\u{85}', ls = '\u{2028}', ps = '\u{2029}'};
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    // A values-file round trip folds these characters before fromYaml can read the document.
    sim_assert_eq!(have: validator.validate(source.as_bytes())?, want: KubernetesVerdict::Valid);
    Ok(())
}

#[test]
fn kubernetes_document_framing_preserves_unicode_yaml_and_physical_boundaries() -> eyre::Result<()>
{
    let cache = tempfile::tempdir()?;
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"properties": {
            "apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"},
            "metadata": {"properties": {"name": {"type": "string"}}}
        }}),
    )?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let lines = [
        "# comment",
        "apiVersion: v1",
        "kind: ConfigMap",
        "metadata:",
        "  name: false",
    ];
    let invalid = validator.validate(lines.join("\n").as_bytes())?;
    eyre::ensure!(
        matches!(invalid, KubernetesVerdict::Invalid(_)),
        "control must reject"
    );

    // Unicode YAML breaks terminate comments even though they do not frame separate resources.
    for separator in [
        "\u{85}",
        "\u{2028}",
        "\u{2029}",
        "\r\u{85}",
        "\r\u{2028}",
        "\r\u{2029}",
    ] {
        sim_assert_eq!(have: validator.validate(lines.join(separator).as_bytes())?, want: invalid);
        let source = formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata: {{name: valid}}{separator}---{separator}apiVersion: v1{separator}kind: ConfigMap{separator}metadata: {{name: false}}
        "};
        sim_assert_eq!(have: validator.validate(source.as_bytes())?, want: KubernetesVerdict::Valid);
    }

    // A physical separator introduces the second resource; its trailing comment is allowed.
    let source = indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata: {name: valid}
        --- # second
        apiVersion: v1
        kind: ConfigMap
        metadata: {name: false}
    "};
    let KubernetesVerdict::Invalid(errors) = validator.validate(source.as_bytes())? else {
        eyre::bail!("physical separator hid the second resource");
    };
    eyre::ensure!(
        errors.iter().all(|error| error.contains("document 1")),
        "{errors:?}"
    );
    assert!(validator.validate(b"---invalid\n").is_err());
    Ok(())
}

#[test]
fn integers_that_helm_normalization_can_round_remain_uncertain() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "number"}, "value": 9_007_199_254_740_993_u64});
    write_cached_schema(
        cache.path(),
        "configmap-v1.json",
        &json!({"const": expected}),
    )?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    let source = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: number
        value: 9007199254740993
    "};
    sim_assert_eq!(have: validator.validate(source.as_bytes())?, want: KubernetesVerdict::Uncertain(vec!["document 0: numeric magnitude exceeds Helm fromYaml's exact integer range".to_string()]));
    Ok(())
}

#[test]
fn archive_links_are_rejected_before_helm_execution() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "parent", "{}")?;
    fs::create_dir_all(root.path().join("charts"))?;
    let archive = fs::File::create(root.path().join("charts/unsafe.tgz"))?;
    let mut builder = tar::Builder::new(GzEncoder::new(archive, Compression::default()));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_mode(0o777);
    header.set_size(0);
    header.set_link_name("../../outside")?;
    header.set_cksum();
    builder.append_data(&mut header, "unsafe/link", std::io::empty())?;
    builder.into_inner()?.finish()?;
    let result = PinnedHelmChart::prepare(root.path());
    let Err(error) = result else {
        eyre::bail!("archive symlink was accepted");
    };
    assert!(error.to_string().contains("unsupported archive member"));
    Ok(())
}

#[test]
fn schema_identity_must_match_the_exact_resource_group() -> eyre::Result<()> {
    let cache = tempfile::tempdir()?;
    let schema = json!({"properties": {
        "apiVersion": {"enum": ["apps/v1"]}, "kind": {"enum": ["Deployment"]},
        "spec": {"properties": {"replicas": {"type": "integer"}}}
    }});
    write_cached_schema(cache.path(), "deployment-apps-v1.json", &schema)?;
    let mut validator = OfflineKubernetesValidator::new(cache.path());
    // The inference provider's short filename fallback must not establish schema ownership.
    let custom = json!({"apiVersion": "apps.example.test/v1", "kind": "Deployment", "metadata": {"name": "custom"}, "spec": {"replicas": "many"}});
    sim_assert_eq!(have: validator.validate(&serde_json::to_vec(&custom)?)?, want: KubernetesVerdict::Uncertain(vec!["document 0: apps.example.test/v1/Deployment custom: original schema does not prove exact identity apps.example.test/v1/Deployment".to_string()]));
    let builtin = json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "builtin"}, "spec": {"replicas": "many"}});
    assert!(matches!(
        validator.validate(&serde_json::to_vec(&builtin)?)?,
        KubernetesVerdict::Invalid(_)
    ));
    Ok(())
}

#[test]
fn schema_identity_metadata_must_be_complete_and_consistent() -> eyre::Result<()> {
    let resource = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": false}});
    let metadata = json!([{"group": "", "version": "v1", "kind": "ConfigMap"}]);
    for (schema, proves_identity) in [
        (
            json!({"x-kubernetes-group-version-kind": metadata, "properties": {"metadata": {"properties": {"name": {"type": "string"}}}}}),
            true,
        ),
        (
            json!({"x-kubernetes-group-version-kind": metadata, "properties": {"apiVersion": {"const": "other/v1"}}}),
            false,
        ),
        (
            json!({"x-kubernetes-group-version-kind": [{"version": "v1", "kind": "ConfigMap"}], "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"properties": {"metadata": {"properties": {"name": {"type": "string"}}}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "$ref": "#/definitions/shape", "definitions": {"shape": {}}, "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-04/schema#", "properties": {"apiVersion": {"const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "definitions": {"shape": {}}, "properties": {"apiVersion": {"$ref": "#/definitions/shape", "const": "v1"}, "kind": {"const": "ConfigMap"}}}),
            false,
        ),
    ] {
        let cache = tempfile::tempdir()?;
        write_cached_schema(cache.path(), "configmap-v1.json", &schema)?;
        let mut validator = OfflineKubernetesValidator::new(cache.path());
        let verdict = validator.validate(&serde_json::to_vec(&resource)?)?;
        if proves_identity {
            assert!(
                matches!(verdict, KubernetesVerdict::Invalid(_)),
                "{verdict:?}"
            );
        } else {
            assert!(
                matches!(verdict, KubernetesVerdict::Uncertain(_)),
                "{verdict:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn packed_dependencies_preserve_the_parent_ignore_boundary() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "parent", "{}")?;
    fs::write(root.path().join(".helmignore"), "payload.txt")?;
    let child = tempfile::tempdir()?;
    write_chart(child.path(), "child", "{}")?;
    fs::write(child.path().join("payload.txt"), "kept")?;
    fs::write(
        child.path().join("templates/configmap.yaml"),
        indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: child
        data:
          payload: {{ required "child file disappeared" (.Files.Get "payload.txt") | quote }}
    "#},
    )?;
    pack_chart(child.path(), &root.path().join("charts/child.tgz"), "child")?;
    let original = Command::new("helm")
        .args(["template", "adjudication"])
        .arg(root.path())
        .args(["--kube-version", "1.29.0"])
        .output()?;
    eyre::ensure!(
        original.status.success(),
        "{}",
        String::from_utf8_lossy(&original.stderr)
    );
    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    let expected = json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "child"}, "data": {"payload": "kept"}});
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&original.stdout)?, want: expected);
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&probe.rendered.stdout)?, want: expected);
    Ok(())
}

#[test]
fn template_exclusions_only_apply_to_actual_chart_roots() -> eyre::Result<()> {
    let root = tempfile::tempdir()?;
    write_chart(root.path(), "files", "{}")?;
    fs::create_dir_all(root.path().join("files/templates/tests"))?;
    fs::write(
        root.path().join("files/templates/tests/payload.txt"),
        "kept",
    )?;
    fs::write(root.path().join("files/values.schema.json"), "asset")?;
    fs::write(
        root.path().join("templates/configmap.yaml"),
        indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: files
        data:
          payload: {{ required "data file disappeared" (.Files.Get "files/templates/tests/payload.txt") | quote }}
          schema: {{ required "schema asset disappeared" (.Files.Get "files/values.schema.json") | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(root.path())?;
    let probe = chart.adjudicate(&json!({}))?;
    eyre::ensure!(
        probe.rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.rendered.stderr)
    );
    sim_assert_eq!(have: serde_yaml::from_slice::<serde_json::Value>(&probe.rendered.stdout)?, want: json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "files"}, "data": {"payload": "kept", "schema": "asset"}}));
    Ok(())
}

fn write_cached_schema(
    cache: &Path,
    filename: &str,
    schema: &serde_json::Value,
) -> eyre::Result<()> {
    fs::write(
        cache.join(LAYOUT_MARKER_FILENAME),
        CACHE_LAYOUT_VERSION.to_string(),
    )?;
    let path = k8s_cache_path(
        cache,
        helm_schema_k8s::default_source_id(),
        "v1.29.0-standalone-strict",
        filename,
    );
    fs::create_dir_all(path.parent().ok_or_eyre("schema has no parent")?)?;
    fs::write(path, serde_json::to_vec(schema)?)?;
    Ok(())
}
