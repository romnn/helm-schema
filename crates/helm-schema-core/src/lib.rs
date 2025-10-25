use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

// pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Other: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartIdentity {
    pub name: String,
    pub version: String,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    pub name: String,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub alias: Option<String>,
    pub condition: Option<String>,
}

/// Simple helper to ensure normalized UTF-8 paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathBufUtf8(#[serde(with = "serde_path")] pub Utf8PathBuf);

mod serde_path {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(p: &Utf8PathBuf, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(p.as_str())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Utf8PathBuf, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Utf8PathBuf::from(s))
    }
}

pub trait Utf8PathExt {
    fn join_utf8(&self, p: &str) -> Utf8PathBuf;
}
impl Utf8PathExt for Utf8Path {
    fn join_utf8(&self, p: &str) -> Utf8PathBuf {
        self.join(p)
    }
}

