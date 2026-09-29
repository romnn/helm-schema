//! Pinned Helm execution and offline adjudication shared by the corpus battery
//! (`crates/helm-schema/tests`) and the `cell_matrix` tool: the render runner and
//! its replay store, render cacheability, chart preparation, the Kubernetes
//! version a chart renders under, and the values and resource validators.

pub mod adjudication;
pub mod cache_policy;
pub mod invocation;
pub mod kubernetes_version;
