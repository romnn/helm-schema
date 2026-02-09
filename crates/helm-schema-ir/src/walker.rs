use regex::Regex;

use helm_schema_ast::{DefineIndex, HelmAst};

use crate::{Guard, IrGenerator, ResourceRef, ValueKind, ValueUse, YamlPath};

/// Default IR generator: walks a `HelmAst` and extracts `.Values.*` uses
/// with their YAML paths, guards, and resource context.
///
/// This is a clean implementation without a Shape tracker.
pub struct DefaultIrGenerator;

impl IrGenerator for DefaultIrGenerator {
    fn generate(&self, ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        let mut w = Walker {
            uses: Vec::new(),
            guards: Vec::new(),
            resource: None,
            defines,
            inline_depth: 0,
        };
        w.walk(ast, &[]);
        w.finish()
    }
}

// ---------------------------------------------------------------------------

struct Walker<'a> {
    uses: Vec<ValueUse>,
    guards: Vec<Guard>,
    resource: Option<ResourceRef>,
    defines: &'a DefineIndex,
    inline_depth: usize,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node: &HelmAst, yaml_path: &[String]) {
        match node {
            HelmAst::Document { items } => {
                for item in items {
                    self.walk(item, yaml_path);
                }
            }

            HelmAst::Mapping { items } => {
                for item in items {
                    self.walk(item, yaml_path);
                }
            }

            HelmAst::Pair { key, value } => {
                let key_text = scalar_text(key);

                // Detect apiVersion / kind for resource tracking.
                if let Some(k) = &key_text {
                    if let Some(v) = value.as_deref().and_then(scalar_text_ref) {
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

            HelmAst::Sequence { items } => {
                let mut seq_path = yaml_path.to_vec();
                if let Some(last) = seq_path.last_mut() {
                    *last = format!("{}[*]", last);
                }
                for item in items {
                    self.walk(item, &seq_path);
                }
            }

            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_guards = parse_condition(cond);
                let guard_save = self.guards.len();

                // Emit uses for each value path referenced in the condition.
                for g in &cond_guards {
                    for path in g.value_paths() {
                        self.uses.push(ValueUse {
                            source_expr: path.to_string(),
                            path: YamlPath(vec![]),
                            kind: ValueKind::Scalar,
                            guards: self.guards.clone(),
                            resource: self.resource.clone(),
                        });
                    }
                    // Push guard only if not already present (dedup).
                    if !self.guards.contains(g) {
                        self.guards.push(g.clone());
                    }
                }

                for item in then_branch {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let cond_guards = parse_condition(header);
                let guard_save = self.guards.len();

                for g in &cond_guards {
                    for path in g.value_paths() {
                        self.uses.push(ValueUse {
                            source_expr: path.to_string(),
                            path: YamlPath(vec![]),
                            kind: ValueKind::Scalar,
                            guards: self.guards.clone(),
                            resource: self.resource.clone(),
                        });
                    }
                    if !self.guards.contains(g) {
                        self.guards.push(g.clone());
                    }
                }

                for item in body {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let values = extract_values_paths(header);
                let guard_save = self.guards.len();

                for v in &values {
                    self.uses.push(ValueUse {
                        source_expr: v.clone(),
                        path: YamlPath(yaml_path.to_vec()),
                        kind: ValueKind::Scalar,
                        guards: self.guards.clone(),
                        resource: self.resource.clone(),
                    });
                    // Push as truthy guard, but deduplicate.
                    let g = Guard::Truthy { path: v.clone() };
                    if !self.guards.contains(&g) {
                        self.guards.push(g);
                    }
                }

                for item in body {
                    self.walk(item, yaml_path);
                }

                self.guards.truncate(guard_save);

                for item in else_branch {
                    self.walk(item, yaml_path);
                }
            }

            HelmAst::HelmExpr { text } => {
                self.handle_helm_expr(text, yaml_path);
            }

            HelmAst::Define { .. } | HelmAst::Block { .. } => {
                // Definitions are collected into the DefineIndex; not walked inline.
            }

            HelmAst::HelmComment { .. } | HelmAst::Scalar { .. } => {}
        }
    }

    fn handle_helm_expr(&mut self, text: &str, yaml_path: &[String]) {
        let is_assignment = text.contains(":=");

        let is_fragment = is_fragment_expr(text);
        let kind = if is_fragment {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };

        let values = extract_values_paths(text);
        for v in &values {
            self.uses.push(ValueUse {
                source_expr: v.clone(),
                path: YamlPath(yaml_path.to_vec()),
                kind,
                guards: self.guards.clone(),
                resource: self.resource.clone(),
            });
        }

        // Inline included/template'd defines (but not from assignments).
        if !is_assignment && self.inline_depth < 10 {
            if let Some(name) = parse_include_name(text) {
                if let Some(define_body) = self.defines.get(&name) {
                    let body = define_body.to_vec();
                    self.inline_depth += 1;
                    for item in &body {
                        self.walk(item, yaml_path);
                    }
                    self.inline_depth -= 1;
                }
            }
        }
    }

    fn finish(mut self) -> Vec<ValueUse> {
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
// Helpers
// ---------------------------------------------------------------------------

fn scalar_text(node: &HelmAst) -> Option<String> {
    match node {
        HelmAst::Scalar { text, .. } => Some(text.clone()),
        _ => None,
    }
}

fn scalar_text_ref(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

/// Extract `.Values.foo.bar` references → `["foo.bar"]`.
pub fn extract_values_paths(text: &str) -> Vec<String> {
    let re = Regex::new(r"\.Values\.([\w]+(?:\.[\w]+)*)").unwrap();
    let mut result: Vec<String> = re.captures_iter(text).map(|c| c[1].to_string()).collect();
    result.sort();
    result.dedup();
    result
}

/// Parse a Go template condition string into structured `Guard`(s).
///
/// Supports patterns like:
/// - `.Values.X`                       → `[Truthy("X")]`
/// - `not .Values.X`                   → `[Not("X")]`
/// - `or .Values.A .Values.B`          → `[Or(["A", "B"])]`
/// - `eq .Values.X "value"`            → `[Eq("X", "value")]`
/// - `and (.Values.A) (.Values.B)`     → `[Truthy("A"), Truthy("B")]`
///
/// Returns an empty vec if no `.Values.*` references are found.
pub fn parse_condition(text: &str) -> Vec<Guard> {
    let trimmed = text.trim();

    // `not .Values.X` → Guard::Not
    if let Some(rest) = trimmed
        .strip_prefix("not ")
        .or_else(|| trimmed.strip_prefix("not\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            return vec![Guard::Not {
                path: paths.into_iter().next().unwrap(),
            }];
        }
    }

    // `or .Values.A .Values.B` → Guard::Or
    if let Some(rest) = trimmed
        .strip_prefix("or ")
        .or_else(|| trimmed.strip_prefix("or\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() >= 2 {
            return vec![Guard::Or { paths }];
        }
    }

    // `eq .Values.X "value"` → Guard::Eq
    if let Some(rest) = trimmed
        .strip_prefix("eq ")
        .or_else(|| trimmed.strip_prefix("eq\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            let eq_re = Regex::new(r#""([^"]*)""#).unwrap();
            if let Some(caps) = eq_re.captures(rest) {
                return vec![Guard::Eq {
                    path: paths.into_iter().next().unwrap(),
                    value: caps[1].to_string(),
                }];
            }
        }
    }

    // `ne .Values.X "value"` → treat as a truthy guard on the referenced path
    if let Some(rest) = trimmed
        .strip_prefix("ne ")
        .or_else(|| trimmed.strip_prefix("ne\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            return vec![Guard::Truthy {
                path: paths.into_iter().next().unwrap(),
            }];
        }
    }

    // Default: simple truthy check(s)
    // `and (.Values.A) (.Values.B)` or bare multiple .Values refs
    // each become a separate Truthy guard.
    let paths = extract_values_paths(trimmed);
    paths
        .into_iter()
        .map(|p| Guard::Truthy { path: p })
        .collect()
}

/// True when the expression likely produces a YAML fragment rather than a single scalar.
pub fn is_fragment_expr(text: &str) -> bool {
    text.contains("toYaml")
        || text.contains("nindent")
        || text.contains("indent")
        || text.contains("tpl")
        || {
            (text.contains("include") || text.contains("template"))
                && (text.contains("nindent") || text.contains("toYaml"))
        }
}

/// Extract the template name from `include "name" ctx` or `template "name" ctx`.
pub fn parse_include_name(text: &str) -> Option<String> {
    let re = Regex::new(r#"(?:include|template)\s+"([^"]+)""#).unwrap();
    re.captures(text).map(|c| c[1].to_string())
}
