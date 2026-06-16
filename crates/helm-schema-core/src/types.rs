use serde::{Deserialize, Serialize};

use crate::HelperBranch;

/// YAML path in the rendered manifest, e.g. `["metadata", "name"]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YamlPath(pub Vec<String>);

/// Whether a value use produces a full scalar, part of a scalar, or a YAML fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    Scalar = 0,
    PartialScalar = 1,
    Fragment = 2,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub api_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_branches: Vec<HelperBranch>,
}

/// Order resource apiVersions by stability and version preference.
#[must_use]
pub fn ordered_api_versions_for_resource(resource: &ResourceRef) -> Vec<&str> {
    let mut versions: Vec<&str> = Vec::new();
    if !resource.api_version.trim().is_empty() {
        versions.push(resource.api_version.as_str());
    }
    for version in &resource.api_version_candidates {
        if !version.trim().is_empty() {
            versions.push(version.as_str());
        }
    }
    if versions.is_empty() {
        versions.push(resource.api_version.as_str());
    }

    versions.sort_by_key(|version| api_version_rank(version));
    versions.dedup();
    versions
}

fn api_version_rank(api_version: &str) -> (u8, u8, i32, i32) {
    let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
    let is_extensions = u8::from(group == "extensions");

    let (stability, prerelease_iteration) = if version.contains("alpha") {
        let iteration = version
            .split("alpha")
            .nth(1)
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0);
        (2_u8, iteration)
    } else if version.contains("beta") {
        let iteration = version
            .split("beta")
            .nth(1)
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0);
        (1_u8, iteration)
    } else if version.starts_with('v')
        && version
            .get(1..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|character| character.is_ascii_digit())
        && version
            .get(1..)
            .is_some_and(|rest| rest.chars().all(|character| character.is_ascii_digit()))
    {
        (0_u8, 0)
    } else {
        (3_u8, 0)
    };

    let major = version
        .strip_prefix('v')
        .and_then(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<i32>()
                .ok()
        })
        .unwrap_or(0);

    (stability, is_extensions, -major, -prerelease_iteration)
}
