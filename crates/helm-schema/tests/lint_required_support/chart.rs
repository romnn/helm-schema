use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use serde_json::{Value, json};

use super::{Cell, DefaultValue, Feature, Override, Transport, write_json};

pub(super) fn copy_public(source: &Path, destination: &Path) -> eyre::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        // Shipped schemas are outputs, so neither the control nor generation may read them.
        if entry.file_name() == "values.schema.json" {
            continue;
        }
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_public(&entry.path(), &target)?;
        } else {
            eyre::ensure!(
                kind.is_file(),
                "non-file chart entry {}",
                entry.path().display()
            );
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub(super) fn write(directory: &Path, cell: Cell) -> eyre::Result<PathBuf> {
    let chart = directory.join("chart");
    let scope = scope(cell);
    let mut current = chart.clone();
    for level in 0..=cell.depth {
        fs::create_dir_all(current.join("templates"))?;
        let mut metadata = json!({
            "apiVersion": "v2", "name": if level == 0 { "root" } else { "kid" },
            "version": "0.1.0", "description": "Helm dependency presence matrix"
        });
        if level < cell.depth {
            let dependency = dependency(cell, level)?;
            object(&mut metadata)?.insert("dependencies".into(), json!([dependency]));
        }
        let mut values = if level == cell.depth {
            child_values(cell)?
        } else {
            json!({})
        };
        if level == 0 {
            values = parent_values(cell, &scope)?;
        }
        if level == cell.depth && matches!(cell.feature, Feature::Library) {
            object(&mut metadata)?.insert("type".into(), json!("library"));
        }
        write_json(&current.join("Chart.yaml"), &metadata)?;
        write_json(&current.join("values.yaml"), &values)?;
        if level == cell.depth {
            let filename = if matches!(cell.feature, Feature::Library) {
                "_pod.tpl"
            } else {
                "pod.yaml"
            };
            fs::write(current.join("templates").join(filename), template(cell))?;
        }
        current = current.join("charts/kid");
    }
    write_root_template(&chart, cell)?;
    let overlay = overlay(cell, &scope)?;
    match cell.transport {
        Transport::File => write_json(&directory.join("override.yaml"), &overlay)?,
        Transport::Chart => {
            let mut values = parent_values(cell, &scope)?;
            merge(&mut values, &overlay)?;
            write_json(&chart.join("values.yaml"), &values)?;
            write_json(&directory.join("override.yaml"), &json!({}))?;
        }
    }
    Ok(chart)
}

fn scope(cell: Cell) -> Vec<&'static str> {
    let mut scope = vec!["kid"; cell.depth];
    if matches!(cell.feature, Feature::Alias) {
        scope = vec!["alias"];
    }
    scope
}

fn dependency(cell: Cell, level: usize) -> eyre::Result<Value> {
    let mut dependency = json!({"name": "kid", "version": "0.1.0"});
    let fields = object(&mut dependency)?;
    if level == 0 {
        match cell.feature {
            Feature::Alias => {
                fields.insert("alias".into(), json!("alias"));
            }
            Feature::Condition => {
                fields.insert("condition".into(), json!("kid.enabled"));
            }
            Feature::Tags => {
                fields.insert("tags".into(), json!(["optional"]));
            }
            Feature::ImportChild => {
                fields.insert("import-values".into(), json!(["settings"]));
            }
            Feature::ImportParent => {
                fields.insert(
                    "import-values".into(),
                    json!([{
                        "child": "exports.settings", "parent": "imported"
                    }]),
                );
            }
            Feature::Plain
            | Feature::Global
            | Feature::Library
            | Feature::Required
            | Feature::Fail => {}
        }
    }
    Ok(dependency)
}

fn child_values(cell: Cell) -> eyre::Result<Value> {
    let mut values = json!({"flag": true});
    let fields = object(&mut values)?;
    if let Some(value) = cell.child.value() {
        fields.insert("enableServiceLinks".into(), value);
    }
    match cell.feature {
        Feature::Global => {
            fields.insert("global".into(), json!({"enableServiceLinks": false}));
        }
        Feature::ImportChild | Feature::ImportParent => {
            fields.insert(
                "exports".into(),
                json!({"settings": {"enableServiceLinks": false}}),
            );
        }
        Feature::Plain
        | Feature::Alias
        | Feature::Condition
        | Feature::Tags
        | Feature::Library
        | Feature::Required
        | Feature::Fail => {}
    }
    Ok(values)
}

fn parent_values(cell: Cell, scope: &[&str]) -> eyre::Result<Value> {
    let mut values = match cell.parent {
        DefaultValue::Absent => json!({}),
        DefaultValue::Null => nested(scope, json!({"flag": true, "enableServiceLinks": null})),
        DefaultValue::Set => nested(scope, json!({"flag": true, "enableServiceLinks": false})),
    };
    let fields = object(&mut values)?;
    match cell.feature {
        Feature::Condition => {
            fields.insert("kid".into(), json!({"enabled": false}));
        }
        Feature::Tags => {
            fields.insert("tags".into(), json!({"optional": false}));
        }
        Feature::Global => {
            fields.insert("global".into(), json!({"enableServiceLinks": true}));
        }
        Feature::Plain
        | Feature::Alias
        | Feature::ImportChild
        | Feature::ImportParent
        | Feature::Library
        | Feature::Required
        | Feature::Fail => {}
    }
    Ok(values)
}

fn overlay(cell: Cell, scope: &[&str]) -> eyre::Result<Value> {
    let mut leaf = match cell.overlay {
        Override::Absent => return Ok(json!({})),
        Override::Partial => json!({"flag": true}),
        Override::Delete => json!({"flag": true, "enableServiceLinks": null}),
        Override::Set => json!({"flag": true, "enableServiceLinks": false}),
    };
    if matches!(cell.feature, Feature::Required | Feature::Fail) {
        object(&mut leaf)?.insert("token".into(), json!("present"));
    }
    let mut values = nested(scope, leaf);
    if matches!(cell.feature, Feature::Global) {
        match cell.overlay {
            Override::Delete => {
                object(&mut values)?.insert("global".into(), json!({"enableServiceLinks": null}));
            }
            Override::Set => {
                object(&mut values)?.insert("global".into(), json!({"enableServiceLinks": false}));
            }
            Override::Absent | Override::Partial => {}
        }
    }
    Ok(values)
}

fn nested(scope: &[&str], mut value: Value) -> Value {
    for segment in scope.iter().rev() {
        value = json!({*segment: value});
    }
    value
}

fn template(cell: Cell) -> String {
    let mut template = indoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: kid
        spec:
          enableServiceLinks: {{ .Values.enableServiceLinks }}
          containers:
            - name: kid
              image: busybox:1.37
    "}
    .to_string();
    if cell.guarded {
        template = template.replace(
            "  enableServiceLinks: {{ .Values.enableServiceLinks }}",
            indoc! {"
                {{ if ne .Values.enableServiceLinks nil }}
                  enableServiceLinks: {{ .Values.enableServiceLinks }}
                {{ end }}
            "}
            .trim_end(),
        );
    }
    match cell.feature {
        Feature::Global => {
            template = template.replace(
                ".Values.enableServiceLinks",
                ".Values.global.enableServiceLinks",
            );
        }
        Feature::Library => {
            template = indoc::formatdoc!("{{{{ define \"kid.pod\" }}}}\n{template}{{{{ end }}}}\n");
        }
        Feature::Required => {
            template = indoc::formatdoc!(
                "{{{{ $_ := required \"token must be set\" .Values.token }}}}\n{template}"
            );
        }
        Feature::Fail => {
            template = indoc::formatdoc!(
                "{{{{ if not .Values.token }}}}{{{{ fail \"token must be set\" }}}}{{{{ end }}}}\n{template}"
            );
        }
        Feature::Condition | Feature::Tags => {
            template =
                indoc::formatdoc!("{{{{ fail \"disabled dependency rendered\" }}}}\n{template}");
        }
        Feature::Plain | Feature::Alias | Feature::ImportChild | Feature::ImportParent => {}
    }
    template
}

fn write_root_template(chart: &Path, cell: Cell) -> eyre::Result<()> {
    let template = match cell.feature {
        Feature::Library => "{{ include \"kid.pod\" (dict \"Values\" .Values.kid) }}\n".into(),
        Feature::ImportChild | Feature::ImportParent => {
            let field = if matches!(cell.feature, Feature::ImportChild) {
                ".Values.enableServiceLinks"
            } else {
                ".enableServiceLinks"
            };
            let body = indoc::formatdoc! {"
                apiVersion: v1
                kind: Pod
                metadata:
                  name: imported
                spec:
                  enableServiceLinks: {{{{ {field} }}}}
                  containers:
                    - name: imported
                      image: busybox:1.37
            "};
            if matches!(cell.feature, Feature::ImportParent) {
                indoc::formatdoc!("{{{{ with .Values.imported }}}}\n{body}{{{{ end }}}}\n")
            } else {
                body
            }
        }
        Feature::Plain
        | Feature::Alias
        | Feature::Condition
        | Feature::Tags
        | Feature::Global
        | Feature::Required
        | Feature::Fail => return Ok(()),
    };
    fs::write(chart.join("templates/root.yaml"), template)?;
    Ok(())
}

fn object(value: &mut Value) -> eyre::Result<&mut serde_json::Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_eyre("chart value is not an object")
}

fn merge(target: &mut Value, overlay: &Value) -> eyre::Result<()> {
    for (key, value) in overlay.as_object().ok_or_eyre("overlay is not an object")? {
        let target = object(target)?.entry(key).or_insert(Value::Null);
        if target.is_object() && value.is_object() {
            merge(target, value)?;
        } else {
            *target = value.clone();
        }
    }
    Ok(())
}
