use std::collections::HashMap;

use yaml_rust::fused::{FusedNode, FusedParseError, parse_fused_yaml_helm};

use crate::vyt::{ResourceRef, VYKind, VYUse, YPath};

/// Index of named template definitions parsed from helper files.
#[derive(Default, Debug)]
pub struct FusedDefineIndex {
    defines: HashMap<String, Vec<FusedNode>>,
}

impl FusedDefineIndex {
    pub fn add_source(&mut self, src: &str) -> Result<(), FusedParseError> {
        let tree = parse_fused_yaml_helm(src)?;
        self.collect_defines(&tree);
        Ok(())
    }

    fn collect_defines(&mut self, node: &FusedNode) {
        match node {
            FusedNode::Stream { items } | FusedNode::Document { items } => {
                for item in items {
                    self.collect_defines(item);
                }
            }
            FusedNode::Define { header, body } => {
                let name = header.trim().trim_matches('"').to_string();
                self.defines.insert(name, body.clone());
            }
            FusedNode::If {
                then_branch,
                else_branch,
                ..
            } => {
                for item in then_branch {
                    self.collect_defines(item);
                }
                for item in else_branch {
                    self.collect_defines(item);
                }
            }
            _ => {}
        }
    }
}

/// Walk a `FusedNode` AST and produce IR (`Vec<VYUse>`).
pub fn generate_fused_ir(node: &FusedNode, defines: &FusedDefineIndex) -> Vec<VYUse> {
    let mut w = Walker {
        uses: Vec::new(),
        guards: Vec::new(),
        resource: None,
        defines,
        inline_depth: 0,
    };
    w.walk(node, &[]);
    w.finish()
}

// ---------------------------------------------------------------------------

struct Walker<'a> {
    uses: Vec<VYUse>,
    guards: Vec<String>,
    resource: Option<ResourceRef>,
    defines: &'a FusedDefineIndex,
    inline_depth: usize,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node: &FusedNode, yaml_path: &[String]) {
        match node {
            FusedNode::Stream { items } | FusedNode::Document { items } => {
                for item in items {
                    self.walk(item, yaml_path);
                }
            }

            FusedNode::Mapping { items } => {
                for item in items {
                    self.walk(item, yaml_path);
                }
            }

            FusedNode::Pair { key, value } => {
                let key_text = scalar_text(key);

                // Detect apiVersion / kind for resource tracking.
                if let Some(k) = &key_text {
                    if let Some(v) = value.as_deref().and_then(scalar_text_node) {
                        if k == "apiVersion" {
                            let prev_kind = self
                                .resource
                                .as_ref()
                                .map(|r| r.kind.clone())
                                .unwrap_or_default();
                            self.resource = Some(ResourceRef {
                                api_version: v.to_string(),
                                kind: prev_kind,
                            });
                        } else if k == "kind" {
                            let prev_api = self
                                .resource
                                .as_ref()
                                .map(|r| r.api_version.clone())
                                .unwrap_or_default();
                            self.resource = Some(ResourceRef {
                                api_version: prev_api,
                                kind: v.to_string(),
                            });
                        }
                    }
                }

                let mut child_path = yaml_path.to_vec();
                if let Some(k) = &key_text {
                    child_path.push(k.clone());
                }

                if let Some(v) = value {
                    self.walk(v, &child_path);
                }
            }

            FusedNode::Sequence { items } => {
                let mut seq_path = yaml_path.to_vec();
                if let Some(last) = seq_path.last_mut() {
                    *last = format!("{}[*]", last);
                }
                for item in items {
                    self.walk(item, &seq_path);
                }
            }

            FusedNode::Item { value } => {
                if let Some(v) = value {
                    self.walk(v, yaml_path);
                }
            }

            FusedNode::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_values = extract_values_paths(cond);
                let guard_save = self.guards.len();

                for g in &cond_values {
                    // Record with current guards *before* pushing this guard.
                    self.uses.push(VYUse {
                        source_expr: g.clone(),
                        path: YPath(vec![]),
                        kind: VYKind::Scalar,
                        guards: self.guards.clone(),
                        resource: self.resource.clone(),
                    });
                    self.guards.push(g.clone());
                }

                for item in then_branch {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            FusedNode::With {
                header,
                body,
                else_branch,
            } => {
                let values = extract_values_paths(header);
                let guard_save = self.guards.len();

                for g in &values {
                    self.uses.push(VYUse {
                        source_expr: g.clone(),
                        path: YPath(vec![]),
                        kind: VYKind::Scalar,
                        guards: self.guards.clone(),
                        resource: self.resource.clone(),
                    });
                    self.guards.push(g.clone());
                }

                for item in body {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            FusedNode::Range {
                header,
                body,
                else_branch,
            } => {
                let values = extract_values_paths(header);
                let guard_save = self.guards.len();

                for g in &values {
                    self.uses.push(VYUse {
                        source_expr: g.clone(),
                        path: YPath(yaml_path.to_vec()),
                        kind: VYKind::Scalar,
                        guards: self.guards.clone(),
                        resource: self.resource.clone(),
                    });
                    self.guards.push(g.clone());
                }

                for item in body {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            FusedNode::HelmExpr { text } => {
                self.handle_helm_expr(text, yaml_path);
            }

            FusedNode::Define { .. } | FusedNode::Block { .. } => {
                // Definitions are collected into the index; not walked inline.
            }

            FusedNode::HelmComment { .. } | FusedNode::Scalar { .. } => {}

            FusedNode::Unknown { children, .. } => {
                for item in children {
                    self.walk(item, yaml_path);
                }
            }
        }
    }

    fn handle_helm_expr(&mut self, text: &str, yaml_path: &[String]) {
        let is_assignment = text.contains(":=");

        let is_fragment = is_fragment_expr(text);
        let kind = if is_fragment {
            VYKind::Fragment
        } else {
            VYKind::Scalar
        };

        // Direct .Values.* references in the expression text.
        let values = extract_values_paths(text);
        for v in &values {
            self.uses.push(VYUse {
                source_expr: v.clone(),
                path: YPath(yaml_path.to_vec()),
                kind,
                guards: self.guards.clone(),
                resource: self.resource.clone(),
            });
        }

        // Inline included/template'd defines (but not from assignments).
        if !is_assignment && self.inline_depth < 10 {
            if let Some(name) = parse_include_name(text) {
                if let Some(define_body) = self.defines.defines.get(&name).cloned() {
                    self.inline_depth += 1;
                    for item in &define_body {
                        self.walk(item, yaml_path);
                    }
                    self.inline_depth -= 1;
                }
            }
        }
    }

    fn finish(mut self) -> Vec<VYUse> {
        self.uses.sort_by(|a, b| {
            a.source_expr
                .cmp(&b.source_expr)
                .then_with(|| a.path.0.cmp(&b.path.0))
                .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
                .then_with(|| a.resource.cmp(&b.resource))
                .then_with(|| a.guards.cmp(&b.guards))
        });
        self.uses.dedup();
        self.uses
    }
}

// ---------------------------------------------------------------------------
// helpers

fn scalar_text(node: &FusedNode) -> Option<String> {
    match node {
        FusedNode::Scalar { text, .. } => Some(text.clone()),
        _ => None,
    }
}

fn scalar_text_node(node: &FusedNode) -> Option<&str> {
    match node {
        FusedNode::Scalar { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

/// Extract `.Values.foo.bar` references → `["foo.bar"]`.
fn extract_values_paths(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\.Values\.([\w]+(?:\.[\w]+)*)").unwrap();
    let mut result: Vec<String> = re.captures_iter(text).map(|c| c[1].to_string()).collect();
    result.sort();
    result.dedup();
    result
}

/// True when the expression likely produces a YAML fragment rather than a single scalar.
fn is_fragment_expr(text: &str) -> bool {
    text.contains("toYaml")
        || text.contains("nindent")
        || text.contains("indent")
        || text.contains("tpl")
        || {
            // `include` / `template` piped through formatting → fragment
            (text.contains("include") || text.contains("template"))
                && (text.contains("nindent") || text.contains("toYaml"))
        }
}

/// Extract the template name from `include "name" ctx` or `template "name" ctx`.
fn parse_include_name(text: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?:include|template)\s+"([^"]+)""#).unwrap();
    re.captures(text).map(|c| c[1].to_string())
}
