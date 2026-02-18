use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn from_yaml_then_field_access_becomes_nested_values_path_in_schema() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());
    let _ = write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: struct
            version: 0.1.0
        "#},
    )?;

    let tpl = indoc! {r#"
        {{- $o := fromYaml .Values.rawYaml }}
        out: {{ $o.someField }}

        {{- with (fromYaml .Values.otherYaml) }}
        out2: {{ .nested }}
        {{- end }}
    "#};
    let _ = write(&root.join("templates/t.yaml")?, tpl)?;

    let chart = load_chart(&root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    let some_field_ty = schema
        .pointer("/properties/rawYaml/properties/someField/type")
        .ok_or_eyre("missing rawYaml.someField.type")?;
    assert_eq!(some_field_ty.as_str(), Some("string"));

    let nested_ty = schema
        .pointer("/properties/otherYaml/properties/nested/type")
        .ok_or_eyre("missing otherYaml.nested.type")?;
    assert_eq!(nested_ty.as_str(), Some("string"));

    Ok(())
}
