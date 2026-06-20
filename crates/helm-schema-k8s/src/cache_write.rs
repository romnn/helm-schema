use std::fs;
use std::io::Write;
use std::path::Path;

use serde_json::Value;

use crate::cache::write_meta_sidecar;
use crate::schema_doc::SchemaDoc;

pub(crate) fn write_atomic_file(local: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = local.parent() {
        fs::create_dir_all(parent)?;
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = local.with_extension(format!("json.tmp.{}.{}", std::process::id(), unique));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
    }
    match fs::rename(&tmp, local) {
        Ok(()) => Ok(()),
        Err(err) => {
            if local.exists() {
                let _ = fs::remove_file(&tmp);
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

pub(crate) fn write_fetched_schema_doc(
    local: &Path,
    url: &str,
    bytes: &[u8],
    record_source: bool,
) -> Option<SchemaDoc> {
    write_atomic_file(local, bytes).ok()?;
    if record_source {
        write_meta_sidecar(local, url);
    }
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .map(SchemaDoc::new)
}
