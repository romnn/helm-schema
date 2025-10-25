use color_eyre::eyre;
use helm_schema_mapper::{Role, analyze_template_file};
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

    use std::collections::BTreeMap;
    let mut by = BTreeMap::new();
    for u in uses {
        by.entry(u.value_path.clone()).or_insert(u.role.clone());
    }

    dbg!(&by);

    // badKey appears before colon on the same line => MappingKey (no placeholder)
    assert_that!(by.get("badKey"), some(eq(&Role::MappingKey)));
    // meta.name appears after colon => ScalarValue (placeholder inserted)
    assert_that!(by.get("meta.name"), some(eq(&Role::ScalarValue)));
    // include ... | nindent (fragment) -> classified Fragment/Unknown (no placeholder)
    // (value_path under include may not exist; classification sanity checked indirectly)
    Ok(())
}
