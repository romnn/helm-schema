use helm_schema_core::{ChartIdentity, DependencySpec};
use serde::{Deserialize, Serialize};
use vfs::VfsPath;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartYaml {
    pub api_version: Option<String>,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<DependencySpec>,
}

#[derive(Debug, Clone)]
pub struct ChartSummary {
    pub identity: ChartIdentity,
    pub root: String,
    pub has_helpers: bool,
    pub templates: Vec<VfsPath>,
    pub crds: Vec<VfsPath>,
    pub values_files: Vec<VfsPath>,
    pub dependencies: Vec<DependencySpec>,
    pub subcharts: Vec<SubchartSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubchartLocation {
    /// Unpacked directory inside the parent chart (relative path).
    Directory { path: VfsPath },
    /// Chart is inside a tarball under `charts/`; we do not extract to disk.
    /// `tgz_path` is the relative path to the .tgz from the parent chart root.
    /// `inner_root` is the top-level directory inside the tarball that contains Chart.yaml.
    Archive {
        tgz_path: VfsPath,
        // tgz_path: String,
        inner_root: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubchartSummary {
    pub name: String,
    pub location: SubchartLocation,
}
