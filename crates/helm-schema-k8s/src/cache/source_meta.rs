use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write a `<path>.meta` sidecar recording the source URL and a
/// fetch timestamp. Best-effort: silently swallows errors so a sidecar
/// failure never blocks the actual cache write.
pub fn write_meta_sidecar(file_path: &Path, source_url: &str) {
    let meta_path = file_path.with_extension(
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| format!("{ext}.meta"))
            .unwrap_or_else(|| "meta".to_string()),
    );
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!("source_url: {source_url}\nfetched_at: {timestamp}\n");
    if let Ok(mut f) = fs::File::create(&meta_path) {
        let _ = f.write_all(body.as_bytes());
    }
}
