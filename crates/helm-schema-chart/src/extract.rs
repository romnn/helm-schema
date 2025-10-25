use crate::{
    loader::Error,
    model::{SubchartLocation, SubchartSummary},
};
use flate2::read::GzDecoder;
use std::io::Read;
use tar::Archive;
use vfs::VfsPath;

pub fn probe_chart_tgz(bytes: &[u8]) -> Result<Option<(String, String)>, Error> {
    let gz = GzDecoder::new(bytes);
    let mut ar = Archive::new(gz);

    // We’ll search for "*/Chart.yaml", capture inner root and parse the chart name.
    let mut found_root: Option<String> = None;
    let mut chart_yaml_bytes: Option<Vec<u8>> = None;

    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?; // tar path
        let s = path.to_string_lossy();

        // Normalize: we expect "<root>/Chart.yaml"
        if s.ends_with("/Chart.yaml") || s == "Chart.yaml" {
            // inner root is the first component (or "" if none)
            let inner_root = s.split('/').next().unwrap_or("").to_string();
            let mut buf = Vec::new();
            e.read_to_end(&mut buf)?;
            found_root = Some(inner_root);
            chart_yaml_bytes = Some(buf);
            break;
        }
    }

    let (inner_root, yaml) = match (found_root, chart_yaml_bytes) {
        (Some(r), Some(b)) => (r, b),
        _ => return Ok(None),
    };

    // parse Chart.yaml just to get the chart name
    #[derive(serde::Deserialize)]
    struct ChartYaml {
        name: String,
        version: String,
    }
    let ch: ChartYaml = serde_yaml::from_slice(&yaml)?;
    Ok(Some((inner_root, ch.name)))
}

pub fn discover_packed_subcharts_in_memory(
    charts_dir: &VfsPath,
) -> Result<Vec<SubchartSummary>, Error> {
    let mut out = vec![];
    if !charts_dir.exists()? {
        return Ok(out);
    }

    for entry in charts_dir.read_dir()? {
        if entry.is_file()? && entry.filename().ends_with(".tgz") {
            // Read bytes from VFS
            let mut bytes = Vec::new();
            entry.open_file()?.read_to_end(&mut bytes)?;

            if let Some((inner_root, name)) = probe_chart_tgz(&bytes)? {
                out.push(SubchartSummary {
                    name,
                    location: SubchartLocation::Archive {
                        tgz_path: entry,
                        inner_root,
                    },
                });
            }
        }
    }
    Ok(out)
}

fn restore_tgz_into_memory_fs(bytes: &[u8]) -> Result<VfsPath, Error> {
    let root = VfsPath::new(vfs::MemoryFS::new());

    let gz = GzDecoder::new(bytes);
    let mut ar = Archive::new(gz);

    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?;
        let path_str = path.to_string_lossy();
        let out = root.join(path_str.as_ref())?;
        if e.header().entry_type().is_dir() {
            out.create_dir_all()?;
        } else {
            out.parent().create_dir_all()?;
            let mut f = out.create_file()?;
            std::io::copy(&mut e, &mut f)?;
        }
    }
    Ok(root)
}
