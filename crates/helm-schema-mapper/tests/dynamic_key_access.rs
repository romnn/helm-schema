use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn dynamic_index_get_pluck_generate_map_like_schema() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());
    let _ = write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: dyn
            version: 0.1.0
        "#},
    )?;

    // NOTE: no values.yaml on purpose; this should be inferred purely from template usage.
    let tpl = indoc! {r#"
        {{- $k := "dynamic" }}
        data:
          chosen: {{ index .Values.extra $k }}
          chosen2: {{ index .Values.extra "static" }}
        imagePullPolicy: {{ default "Always" (get .Values.image "pullPolicy") }}
        picked: {{ (pluck "x" .Values.labels .Values.annotations) | first }}
    "#};
    let _ = write(&root.join("templates/cm.yaml")?, tpl)?;

    let chart = load_chart(&root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // index .Values.extra $k -> extra.__any__ -> map schema
    let extra_ty = schema
        .pointer("/properties/extra/type")
        .ok_or_eyre("missing extra.type")?;
    assert_eq!(extra_ty.as_str(), Some("object"));

    let extra_ap_ty = schema
        .pointer("/properties/extra/additionalProperties/type")
        .ok_or_eyre("missing extra.additionalProperties.type")?;
    assert_eq!(extra_ap_ty.as_str(), Some("string"));

    // index .Values.extra "static" -> extra.static
    let static_ty = schema
        .pointer("/properties/extra/properties/static/type")
        .ok_or_eyre("missing extra.static.type")?;
    assert_eq!(static_ty.as_str(), Some("string"));

    // get .Values.image "pullPolicy" -> image.pullPolicy
    let pull_policy = schema
        .pointer("/properties/image/properties/pullPolicy/type")
        .ok_or_eyre("missing image.pullPolicy.type")?;
    assert_eq!(pull_policy.as_str(), Some("string"));

    // pluck "x" .Values.labels ... should at least create labels.x and annotations.x as potential sources.
    // (We don't infer array-ness here; just ensure we surfaced the key access.)
    let labels_x = schema
        .pointer("/properties/labels/properties/x/type")
        .ok_or_eyre("missing labels.x.type")?;
    assert_eq!(labels_x.as_str(), Some("string"));

    let annotations_x = schema
        .pointer("/properties/annotations/properties/x/type")
        .ok_or_eyre("missing annotations.x.type")?;
    assert_eq!(annotations_x.as_str(), Some("string"));

    Ok(())
}
