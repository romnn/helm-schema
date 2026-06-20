use std::fs;
use std::io::Write;
use std::path::Path;

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
