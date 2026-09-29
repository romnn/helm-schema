use std::collections::BTreeMap;
use vfs::VfsPath;

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
    pub values_prefix: Vec<String>,
    /// Helm's template namespace for this chart: the root chart's name
    /// followed by one `charts/<dependency key>` segment per dependency
    /// edge, with the alias when a dependency is aliased. Helm registers
    /// every template under `<namespace>/<chart-relative path>` and reports
    /// `<namespace>/templates` as `.Template.BasePath`.
    pub template_namespace: String,
    pub is_library: bool,
    /// Exact scalar fields Helm exposes through this chart's `.Chart` root.
    pub static_root_strings: BTreeMap<Vec<String>, String>,
    /// Ancestor-first activation levels: one entry per dependency edge on
    /// the path from the root chart that carries a condition or tags. Helm
    /// renders a nested chart only while EVERY level activates (each
    /// Chart.yaml condition is evaluated against the top-level values), so
    /// a doubly-nested chart like signoz's clickhouse→zookeeper is gated on
    /// `clickhouse.enabled` AND `clickhouse.zookeeper.enabled`.
    pub dependency_activation_chain: Vec<ChartDependencyActivation>,
    /// The parent's `Chart.yaml` (or `requirements.yaml`) lists this chart as
    /// a dependency; false for the root and for a vendored chart no entry
    /// names.
    pub listed_dependency: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ChartDependencyActivation {
    pub condition_paths: Vec<String>,
    pub tag_paths: Vec<String>,
}
