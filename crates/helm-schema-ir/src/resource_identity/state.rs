use crate::{HelperBranch, HelperBranchBody, ResourceRef};

#[derive(Default)]
pub(super) struct ResourceState {
    kind: Option<String>,
    api_versions: Vec<String>,
    multi_branch: bool,
    api_version_branches: Vec<HelperBranch>,
}

impl ResourceState {
    pub(super) fn set_kind_if_empty(&mut self, kind: &str) {
        if self.kind.is_none() && !kind.is_empty() {
            self.kind = Some(kind.to_string());
        }
    }

    pub(super) fn record_api_version_output(&mut self, output: HelperBranchBody) {
        match output {
            HelperBranchBody::Literals { values } => {
                if values.len() > 1 {
                    self.multi_branch = true;
                }
                for literal in values {
                    self.insert_api_version(literal);
                }
            }
            HelperBranchBody::Nested { branches } => self.record_api_version_branches(branches),
        }
    }

    pub(super) fn record_api_version_branches(&mut self, branches: Vec<HelperBranch>) {
        if branches.is_empty() {
            return;
        }
        self.multi_branch = true;
        for branch in &branches {
            for literal in branch.body.all_literals() {
                self.insert_api_version(literal);
            }
        }
        self.api_version_branches.extend(branches);
    }

    fn insert_api_version(&mut self, value: String) {
        if !value.is_empty() && !self.api_versions.contains(&value) {
            self.api_versions.push(value);
        }
    }

    pub(super) fn into_resource(self) -> Option<ResourceRef> {
        let kind = self.kind?;
        let (api_version, api_version_candidates) = if self.multi_branch {
            (String::new(), self.api_versions)
        } else {
            let mut versions = self.api_versions;
            let primary = versions.first().cloned().unwrap_or_default();
            versions.retain(|version| version != &primary);
            (primary, versions)
        };
        Some(ResourceRef {
            api_version,
            kind,
            api_version_candidates,
            api_version_branches: self.api_version_branches,
        })
    }
}
