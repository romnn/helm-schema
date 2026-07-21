use vfs::VfsPath;

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
    pub values_prefix: Vec<String>,
    pub is_library: bool,
    /// Ancestor-first activation levels: one entry per dependency edge on
    /// the path from the root chart that carries a condition or tags. Helm
    /// renders a nested chart only while EVERY level activates (each
    /// Chart.yaml condition is evaluated against the top-level values), so
    /// a doubly-nested chart like signoz's clickhouse→zookeeper is gated on
    /// `clickhouse.enabled` AND `clickhouse.zookeeper.enabled`.
    pub dependency_activation_chain: Vec<ChartDependencyActivation>,
}

#[derive(Debug, Clone, Default)]
pub struct ChartDependencyActivation {
    pub condition_paths: Vec<String>,
    pub tag_paths: Vec<String>,
}
