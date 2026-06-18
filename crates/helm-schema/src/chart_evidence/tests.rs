use super::*;
use vfs::VfsPath;

#[test]
fn reachable_helper_defaults_are_scoped_as_template_evidence() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("charts/child/values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/templates/_helpers.tpl")?,
        r#"{{- define "child.name" -}}
{{ default "demo" .Values.name }}
{{- end -}}
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "child.name" . }}
"#,
    )?;

    let discovery = crate::chart::discover_chart_contexts(&chart_dir)?;
    let evidence = collect_chart_template_evidence(&discovery.charts, false)?;

    assert!(
        evidence.type_hints.contains_key("kid.name"),
        "reachable helper default should produce a scoped type hint: {:?}",
        evidence.type_hints
    );
    Ok(())
}
