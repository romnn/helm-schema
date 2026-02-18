use vfs::VfsPath;

pub fn is_yaml_like(p: &VfsPath) -> bool {
    match p.extension().as_deref() {
        Some("yaml") | Some("yml") => true,
        _ => false,
    }
}

pub fn is_template_like(p: &VfsPath) -> bool {
    if let Some(ext) = p.extension().as_deref() {
        return matches!(ext, "yaml" | "yml" | "tpl" | "txt");
    }
    false
}

fn relative_to<'a>(base: &'a VfsPath, path: &'a VfsPath) -> &'a str {
    path.as_str()
        .strip_prefix(base.as_str())
        .unwrap_or(path.as_str())
        .trim_start_matches(|c| c == '/')
}

pub trait VfsPathExt {
    fn relative_to<'a>(&'a self, base: &'a Self) -> &'a str;
}

impl VfsPathExt for VfsPath {
    fn relative_to<'a>(&'a self, base: &'a Self) -> &'a str {
        relative_to(base, self)
    }
}

// use camino::{Utf8Path, Utf8PathBuf};
//
// impl VfsPathExt for &VfsPath {
//     fn relative_to(&self, base: Self) -> &str {
//         relative_to(&base, self)
//     }
// }
