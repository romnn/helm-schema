use color_eyre::eyre;
use helm_schema_mapper::{
    Role, ValueUse,
    analyze::{Occurrence, group_uses},
    analyze_template_file,
};
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

    // --- Use-level assertions (exact scalar placements)
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"ingress.hostname"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.rules[0].host"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"ingress.path"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.rules[0].http.paths[0].path"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"ingress.pathType"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.rules[0].http.paths[0].pathType"))),
            ..
        }))
    );

    // Guards are recorded and have no YAML path
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"ingress"),
            role: eq(&Role::Guard),
            yaml_path: none(),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"ingress.enabled"),
            role: eq(&Role::Guard),
            yaml_path: none(),
            ..
        }))
    );

    // --- Group-level assertions
    let groups = group_uses(&uses);
    dbg!(&groups);

    // Focus the grouped map to just the three scalar insertions under the guard
    let by: Vec<_> = groups
        .clone()
        .into_iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "ingress.hostname" | "ingress.path" | "ingress.pathType"
            )
        })
        .collect();

    assert_that!(
        &by,
        unordered_elements_are![
            (
                eq(&"ingress.hostname"),
                unordered_elements_are![matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].host"))),
                    ..
                })]
            ),
            (
                eq(&"ingress.path"),
                unordered_elements_are![matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].http.paths[0].path"))),
                    ..
                })]
            ),
            (
                eq(&"ingress.pathType"),
                unordered_elements_are![matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].http.paths[0].pathType"))),
                    ..
                })]
            )
        ]
    );

    // And the guard keys exist in groups with at least one Guard occurrence and no path.
    assert_that!(
        groups.get("ingress"),
        some(contains(matches_pattern!(Occurrence {
            role: eq(&Role::Guard),
            path: none(),
            ..
        })))
    );
    assert_that!(
        groups.get("ingress.enabled"),
        some(contains(matches_pattern!(Occurrence {
            role: eq(&Role::Guard),
            path: none(),
            ..
        })))
    );

    Ok(())
}
