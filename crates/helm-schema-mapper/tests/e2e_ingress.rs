use color_eyre::eyre;
use helm_schema_mapper::{Role, analyze_template_file};
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn maps_scalar_values_to_yaml_paths() -> eyre::Result<()> {
    Builder::default().build();
    
    let root = VfsPath::new(vfs::MemoryFS::new());
    let tpl = indoc! {r#"
        {{- if .Values.ingress.enabled }}
        kind: Ingress
        spec:
          rules:
            - host: {{ tpl .Values.ingress.hostname . }}
              http:
                paths:
                  - path: {{ .Values.ingress.path }}
                    pathType: {{ .Values.ingress.pathType }}
        {{- end }}
    "#};
    write(&root.join("templates/ing.yaml")?, tpl)?;

    let uses = analyze_template_file(&root.join("templates/ing.yaml")?)?;
    dbg!(&uses);

    // Filter only scalar value roles
    let scalar: Vec<_> = uses
        .into_iter()
        .filter(|u| matches!(u.role, Role::ScalarValue))
        .collect();

    // Collect a map value_path -> yaml_path string
    use std::collections::BTreeMap;
    let mut mp = BTreeMap::new();
    for u in scalar {
        mp.insert(
            u.value_path.clone(),
            u.yaml_path
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_default(),
        );
    }

    dbg!(&mp);
    let mp: Vec<_> = mp.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    assert_that!(&mp, unordered_elements_are![
      eq(&("ingress.hostname", "spec.rules[0].host")),
      eq(&("ingress.path", "spec.rules[0].http.paths[0].path")),
      eq(&("ingress.pathType", "spec.rules[0].http.paths[0].pathType")),
    ]);

    // Spot-check core bindings
    // assert_that!(
    //     mp.get("ingress.hostname").cloned(),
    //     some(eq("spec.rules[0].host"))
    // );
    // assert_that!(
    //     mp.get("ingress.path").cloned(),
    //     some(eq("spec.rules[0].http.paths[0].path"))
    // );
    // assert_that!(
    //     mp.get("ingress.pathType").cloned(),
    //     some(eq("spec.rules[0].http.paths[0].pathType"))
    // );
    //
    // // Enabled appears only as a guard (no scalar insertion)
    // assert_that!(mp.contains_key("ingress.enabled"), eq(false));
    Ok(())
}
