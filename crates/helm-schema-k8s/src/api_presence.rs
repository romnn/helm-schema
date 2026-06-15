use helm_schema_ir::ResourceRef;

/// A typed `.Capabilities.APIVersions.Has ...` query.
///
/// Helm accepts both api-version-only literals (`policy/v1`, `v1`) and
/// resource-qualified literals (`policy/v1/PodDisruptionBudget`, `v1/Secret`).
/// Keeping that distinction explicit lets the resolver probe exact resources
/// directly and confines the canonical-kind table to the api-version-only arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApiPresenceQuery {
    Resource(ResourceRef),
    GroupVersion { api_version: String },
}

impl ApiPresenceQuery {
    #[must_use]
    pub fn parse_helm_literal(api: &str) -> Option<Self> {
        let parts: Vec<&str> = api.split('/').collect();
        match parts.as_slice() {
            [group, version, kind]
                if !group.is_empty() && !version.is_empty() && !kind.is_empty() =>
            {
                Some(Self::Resource(ResourceRef {
                    api_version: format!("{group}/{version}"),
                    kind: (*kind).to_string(),
                    api_version_candidates: Vec::new(),
                    api_version_branches: Vec::new(),
                }))
            }
            [version, kind] if is_k8s_api_version_segment(version) && !kind.is_empty() => {
                Some(Self::Resource(ResourceRef {
                    api_version: (*version).to_string(),
                    kind: (*kind).to_string(),
                    api_version_candidates: Vec::new(),
                    api_version_branches: Vec::new(),
                }))
            }
            [api_version] if !api_version.is_empty() => Some(Self::GroupVersion {
                api_version: (*api_version).to_string(),
            }),
            [group, version] if !group.is_empty() && !version.is_empty() => {
                Some(Self::GroupVersion {
                    api_version: format!("{group}/{version}"),
                })
            }
            _ => None,
        }
    }

    /// Canonical Helm literal for this query.
    ///
    /// Resource queries intentionally use only apiVersion and kind. Other
    /// [`ResourceRef`] fields describe schema-resolution candidates and are not
    /// part of capability-presence identity.
    #[must_use]
    pub fn canonical_helm_literal(&self) -> String {
        match self {
            ApiPresenceQuery::Resource(resource) => {
                format!("{}/{}", resource.api_version, resource.kind)
            }
            ApiPresenceQuery::GroupVersion { api_version } => api_version.clone(),
        }
    }
}

pub(crate) fn is_k8s_api_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let suffix = &rest[digit_count..];
    if suffix.is_empty() {
        return true;
    }
    for qualifier in ["alpha", "beta"] {
        if let Some(number) = suffix.strip_prefix(qualifier) {
            return !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(api: &str) -> Option<ApiPresenceQuery> {
        ApiPresenceQuery::parse_helm_literal(api)
    }

    #[test]
    fn parses_group_version_query() {
        assert_eq!(
            parse("policy/v1"),
            Some(ApiPresenceQuery::GroupVersion {
                api_version: "policy/v1".to_string(),
            })
        );
    }

    #[test]
    fn parses_core_version_query() {
        assert_eq!(
            parse("v1"),
            Some(ApiPresenceQuery::GroupVersion {
                api_version: "v1".to_string(),
            })
        );
    }

    #[test]
    fn parses_resource_qualified_group_version_query() {
        assert_eq!(
            parse("policy/v1/PodSecurityPolicy"),
            Some(ApiPresenceQuery::Resource(ResourceRef {
                api_version: "policy/v1".to_string(),
                kind: "PodSecurityPolicy".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }))
        );
    }

    #[test]
    fn parses_resource_qualified_core_version_query() {
        assert_eq!(
            parse("v1/Secret"),
            Some(ApiPresenceQuery::Resource(ResourceRef {
                api_version: "v1".to_string(),
                kind: "Secret".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }))
        );
    }

    #[test]
    fn rejects_malformed_resource_queries() {
        assert!(parse("policy/v1/").is_none());
        assert!(parse("v1/").is_none());
        assert!(parse("policy/v1/Pod/extra").is_none());
    }

    #[test]
    fn api_version_segment_parser_accepts_stable_and_prerelease_versions() {
        assert!(is_k8s_api_version_segment("v1"));
        assert!(is_k8s_api_version_segment("v2beta1"));
        assert!(is_k8s_api_version_segment("v3alpha2"));
    }

    #[test]
    fn api_version_segment_parser_rejects_group_names_and_incomplete_versions() {
        assert!(!is_k8s_api_version_segment("policy"));
        assert!(!is_k8s_api_version_segment("v"));
        assert!(!is_k8s_api_version_segment("v1gamma1"));
    }
}
