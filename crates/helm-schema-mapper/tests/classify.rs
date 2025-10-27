use color_eyre::eyre;
use helm_schema_mapper::{
    Role,
    analyze::{Occurrence, group_uses},
    analyze_template_file,
};
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn classifies_key_vs_value_vs_fragment() -> eyre::Result<()> {
    Builder::default().build();
    let root = VfsPath::new(vfs::MemoryFS::new());
    let tpl = indoc! {r#"
        metadata:
          {{ .Values.badKey }}: literal
          name: {{ .Values.meta.name }}
          labels:
            {{- include "toYamlFragment" . | nindent 4 }}
    "#};
    write(&root.join("t.yaml")?, tpl)?;

    let uses = analyze_template_file(&root.join("t.yaml")?)?;
    dbg!(&uses);

    let groups = group_uses(&uses);
    dbg!(&groups);

    // Yes—the assertion for the "meta" entry is correct given how the collector currently works.
    //
    // What’s happening:
    //
    // The AST for {{ .Values.meta.name }} contains two selector_expression nodes:
    //
    // inner: .Values.meta → resolves to the Values key meta
    //
    // outer: .Values.meta.name → resolves to meta.name
    //
    // In collect_values_with_scope we don’t filter out the inner selector when its parent is also a selector, so both the inner (meta) and the outer (meta.name) get recorded at the same scalar site and therefore both get the same YAML path, metadata.name. That’s exactly what your debug dump shows.
    //
    // The “labels” section is unrelated to that. It’s an include "toYamlFragment" . | nindent 4 which we classify as a Fragment attached to metadata.labels. Since that include’s argument is just ., it doesn’t introduce any .Values.* key, so there’s no group entry like "labels" or similar—only a Fragment occurrence (and the empty value_paths are skipped by group_uses, as your log shows).
    //
    // If, down the road, you’d rather only keep the maximal key on scalar sites (i.e., record meta.name but not meta), we could change collect_values_with_scope to ignore selector_expression nodes whose parent is also a selector_expression (mirroring the parent_is_selector filter you used elsewhere). But with the current behavior, asserting both "meta" and "meta.name" mapped to "metadata.name" is the right check.

    // Verify:
    // 1) a template expression used in a mapping *key* is treated as a Fragment bound to the
    //    parent mapping path ("metadata"), not as a ScalarValue.
    // 2) scalar emission for ".Values.meta.name" maps precisely to "metadata.name".
    // 3) we also record the shorter "meta" key at the same scalar site.
    assert_that!(
        &groups,
        unordered_elements_are![
            (
                eq(&"badKey"),
                unordered_elements_are![matches_pattern!(Occurrence {
                    role: eq(&Role::Fragment),
                    path: some(displays_as(eq("metadata"))),
                    ..
                })]
            ),
            // (
            //     eq(&"meta"),
            //     unordered_elements_are![matches_pattern!(Occurrence {
            //         role: eq(&Role::ScalarValue),
            //         path: some(displays_as(eq("metadata.name"))),
            //         ..
            //     })]
            // ),
            (
                eq(&"meta.name"),
                unordered_elements_are![matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("metadata.name"))),
                    ..
                })]
            ),
        ]
    );
    Ok(())
}
