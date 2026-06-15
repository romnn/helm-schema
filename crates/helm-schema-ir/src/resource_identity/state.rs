use crate::ResourceRef;
use crate::capability_branch::HelperBranch;

use super::helper_output::HelperOutput;

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

    pub(super) fn record_api_version_output(&mut self, output: HelperOutput) {
        match output {
            HelperOutput::Literals(literals) => {
                if literals.len() > 1 {
                    self.multi_branch = true;
                }
                for literal in literals {
                    self.insert_api_version(literal);
                }
            }
            HelperOutput::Branched { branches } => self.record_api_version_branches(branches),
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

    pub(super) fn into_helper_output(self) -> Option<HelperOutput> {
        if self.api_versions.is_empty() && self.api_version_branches.is_empty() {
            None
        } else if self.api_version_branches.is_empty() {
            Some(HelperOutput::Literals(self.api_versions))
        } else {
            Some(HelperOutput::Branched {
                branches: self.api_version_branches,
            })
        }
    }
}
