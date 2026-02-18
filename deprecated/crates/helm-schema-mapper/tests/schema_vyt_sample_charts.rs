use color_eyre::eyre;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn generates_values_schema_for_local_sample_charts_vyt() -> eyre::Result<()> {
    Builder::default().build();

    // Optional local fixtures: users can drop real charts into testdata/charts/<chart>/
    // This test is a no-op if the directory is empty.
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let dir = root.join("crates/helm-schema-mapper/testdata/charts")?;
    if !dir.exists()? {
        return Ok(());
    }

    let mut found_any = false;
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        if d.join("Chart.yaml")?.exists()? {
            found_any = true;
            let chart = load_chart(&d, &LoadOptions::default())?;
            let schema = generate_values_schema_for_chart_vyt(&chart)?;

            // Minimal sanity: must be Draft-07 root object.
            assert_eq!(
                schema.get("$schema").and_then(|v| v.as_str()),
                Some("http://json-schema.org/draft-07/schema#")
            );
            assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
            assert!(schema.get("properties").is_some());
            continue;
        }

        for entry in d.read_dir()? {
            if entry.is_dir()? {
                stack.push(entry);
            }
        }
    }

    if !found_any {
        return Ok(());
    }

    Ok(())
}
