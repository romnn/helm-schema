use vfs::VfsPath;

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
    pub values_prefix: Vec<String>,
    pub is_library: bool,
    pub dependency_activation: ChartDependencyActivation,
}

#[derive(Debug, Clone, Default)]
pub struct ChartDependencyActivation {
    pub condition_paths: Vec<String>,
    pub tag_paths: Vec<String>,
}

#[derive(Debug)]
pub struct ChartDiscovery {
    pub charts: Vec<ChartContext>,
}
