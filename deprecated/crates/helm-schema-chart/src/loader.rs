// use std::io::Read;
use crate::util::VfsPathExt;
use vfs::{FileSystem, VfsPath};

// use camino::{Utf8Path, Utf8PathBuf};
// use ignore::WalkBuilder;

use crate::model::*;
use crate::util::{is_template_like, is_yaml_like};
use helm_schema_core::ChartIdentity;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("Missing {0:?}")]
    MissingChartManifest(VfsPath),
    // MissingChartManifest(Utf8PathBuf),
    #[error("failed to parse")]
    Parse(#[from] serde_yaml::Error),
    #[error("failed to walk directory")]
    Walk(#[from] helm_schema_vfs_walk::WalkError),

    // #[error("ignore error: {0}")]
    // Ignore(#[from] ignore::Error),
    #[error("non utf8 path: {path}")]
    Path {
        #[source]
        source: camino::FromPathBufError,
        path: std::path::PathBuf,
    },
    #[error("invalid path")]
    Vfs(#[from] vfs::VfsError),
    // Vfs {
    //     #[source]
    //     source: vfs::VfsError,
    //     path: std::path::PathBuf,
    // },
    // #[error("YAML: {0}")]
    // Yaml(#[from] serde_yaml::Error),
    // #[error("Other: {0}")]
    // Other(String),
}

#[derive(Debug, Clone)]
pub struct LoadOptions {
    // pub follow_ignored: bool,
    // pub include_tests: bool,
    pub include_tests: bool,
    /// When feature `extract_tgz` is enabled,
    /// extract charts/*.tgz into charts/__extracted/<name>/
    pub auto_extract_tgz: bool,
    /// Recurse into subcharts found in charts/ (both unpacked and extracted)
    pub recurse_subcharts: bool,
    /// Respect .gitignore files during scan
    pub respect_gitignore: bool,
    /// Include hidden files/dirs (dotfiles)
    pub include_hidden: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            include_tests: false,
            // auto_extract_tgz: cfg!(feature = "extract_tgz"),
            auto_extract_tgz: false,
            recurse_subcharts: true,
            respect_gitignore: true,
            include_hidden: false,
        }
    }
}

impl LoadOptions {
    pub fn with_include_tests(mut self, enabled: bool) -> Self {
        self.include_tests = enabled;
        self
    }

    pub fn with_recurse_subcharts(mut self, enabled: bool) -> Self {
        self.recurse_subcharts = enabled;
        self
    }

    pub fn with_respect_gitignore(mut self, enabled: bool) -> Self {
        self.respect_gitignore = enabled;
        self
    }

    pub fn with_include_hidden(mut self, enabled: bool) -> Self {
        self.include_hidden = enabled;
        self
    }
}

pub fn load_chart(root: &VfsPath, opts: &LoadOptions) -> Result<ChartSummary, Error> {
    let chart_yaml = root.join("Chart.yaml")?;
    if !chart_yaml.exists()? {
        return Err(Error::MissingChartManifest(chart_yaml.into()));
    }

    let raw_chart = chart_yaml.read_to_string()?;
    let chart: ChartYaml = serde_yaml::from_str(&raw_chart)?;

    let identity = ChartIdentity {
        name: chart.name.clone(),
        version: chart.version.clone(),
        app_version: chart.app_version.clone(),
    };

    let mut templates = vec![];
    let mut crds = vec![];
    let mut values_files = vec![];
    let mut has_helpers = false;

    let walker = helm_schema_vfs_walk::VfsWalkBuilder::new(root.clone())
        .hidden(opts.include_hidden)
        .standard_filters(true)
        .git_ignore(opts.respect_gitignore)
        .build()?;

    for path in walker {
        let path = path?.path;
        if !path.is_file()? {
            continue;
        }

        let rel = path.relative_to(root);

        // Tests directory optional
        if !opts.include_tests && rel.starts_with("tests/") {
            continue;
        }

        if rel == "templates/_helpers.tpl" {
            has_helpers = true;
        }

        // TODO: remove tests
        if rel.starts_with("tests/") && is_template_like(&path) {
            templates.push(path);
            continue;
        }
        if rel.starts_with("templates/") && is_template_like(&path) {
            templates.push(path);
            continue;
        }
        if rel.starts_with("crds/") && is_yaml_like(&path) {
            crds.push(path);
            continue;
        }
        if rel == "values.yaml" || rel.starts_with("values.") && is_yaml_like(&path) {
            values_files.push(path);
            continue;
        }
    }

    // Subcharts (unpacked)
    let mut subcharts = vec![];
    let charts_dir = root.join("charts")?;
    if charts_dir.exists()? {
        // Discover unpacked subcharts
        for path in charts_dir.read_dir()? {
            if path.is_dir()? && path.join("Chart.yaml")?.exists()? {
                subcharts.push(SubchartSummary {
                    name: path.filename(),
                    location: SubchartLocation::Directory { path },
                });
            }
        }

        #[cfg(feature = "extract_tgz")]
        {
            // Discover packed subcharts IN MEMORY (no writes)
            let mut packed = crate::extract::discover_packed_subcharts_in_memory(&charts_dir)?;
            subcharts.append(&mut packed);
        }
    }

    Ok(ChartSummary {
        identity,
        root: root.as_str().to_string(),
        has_helpers,
        templates,
        crds,
        values_files,
        dependencies: chart.dependencies.clone(),
        subcharts,
    })
}
