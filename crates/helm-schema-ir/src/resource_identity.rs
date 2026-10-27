//! Resource headers and List envelopes recovered from executed YAML ownership.

use helm_schema_core::{
    Guard, GuardValue, KindBranch, Predicate, PredicateKind, PredicateMemo, ResourceRef, ValuesPath,
};
use helm_schema_syntax::{Span, YamlNodeKind};

use crate::value::DocumentExecution;
use crate::value::yaml::PlacedDocument;

pub(crate) struct PlacedResource {
    pub(crate) span: Span,
    pub(crate) condition: Predicate,
    pub(crate) resource: ResourceRef,
    pub(crate) path_prefix: Vec<String>,
}

struct NamedValue {
    condition: Predicate,
    text: String,
}

#[derive(Default)]
struct HeaderNodes {
    kind: Option<usize>,
    api_version: Option<usize>,
    items: Option<usize>,
}

pub(crate) fn placed_resources(
    document: &PlacedDocument<'_>,
    _execution: &DocumentExecution,
) -> Vec<PlacedResource> {
    let projection = ResourceProjection::new(document);
    let mut resources = Vec::new();
    for index in projection.roots() {
        projection.mapping(index, &document.condition, &[], &mut resources);
    }
    resources
}

struct ResourceProjection<'a, 'b> {
    document: &'a PlacedDocument<'b>,
    children: Vec<Vec<usize>>,
    memo: PredicateMemo,
}

impl<'a, 'b> ResourceProjection<'a, 'b> {
    fn new(document: &'a PlacedDocument<'b>) -> Self {
        let mut children = vec![Vec::new(); document.ownership.nodes.len()];
        for (index, node) in document.ownership.nodes.iter().enumerate() {
            if let Some(parent) = node.parent.and_then(|parent| children.get_mut(parent)) {
                parent.push(index);
            }
        }
        Self {
            document,
            children,
            memo: PredicateMemo::new(),
        }
    }

    fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        self.document
            .ownership
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                (matches!(node.kind, YamlNodeKind::Mapping | YamlNodeKind::FlowMapping)
                    && node
                        .parent
                        .and_then(|parent| self.document.ownership.nodes.get(parent))
                        .is_some_and(|parent| parent.kind == YamlNodeKind::Document))
                .then_some(index)
            })
    }

    fn headers(&self, mapping: usize) -> HeaderNodes {
        let mut headers = HeaderNodes::default();
        for pair in self.children.get(mapping).into_iter().flatten() {
            let Some(children) = self.children.get(*pair) else {
                continue;
            };
            let Some(key) = children
                .iter()
                .filter_map(|index| self.document.ownership.nodes.get(*index))
                .find(|node| node.key)
            else {
                continue;
            };
            if self.document.terms(key.span).any(|term| term.placeholder) {
                continue;
            }
            let Some(key) = literal_key(self.document, key.span) else {
                continue;
            };
            let value = children.iter().copied().find(|index| {
                self.document
                    .ownership
                    .nodes
                    .get(*index)
                    .is_some_and(|node| !node.key)
            });
            match key.as_str() {
                "kind" if headers.kind.is_none() => headers.kind = value,
                "apiVersion" if headers.api_version.is_none() => headers.api_version = value,
                "items" if headers.items.is_none() => headers.items = value,
                _ => {}
            }
        }
        headers
    }

    fn mapping(
        &self,
        index: usize,
        ambient: &Predicate,
        path_prefix: &[String],
        resources: &mut Vec<PlacedResource>,
    ) {
        let Some(mapping) = self.document.ownership.nodes.get(index) else {
            return;
        };
        let HeaderNodes {
            kind,
            api_version,
            items,
        } = self.headers(index);
        let Some(kind) = kind else {
            return;
        };
        let Some(kinds) = self.names(kind) else {
            return;
        };
        let versions = api_version
            .and_then(|index| self.names(index))
            .unwrap_or_else(|| {
                vec![NamedValue {
                    condition: Predicate::True,
                    text: String::new(),
                }]
            });
        let placement = self.document.placement_guard(
            self.document
                .ownership
                .nodes
                .get(kind)
                .map_or(mapping.span, |node| node.span),
        );
        for NamedValue {
            condition: kind_condition,
            text: kind,
        } in kinds
        {
            if kind.is_empty() {
                continue;
            }
            for NamedValue {
                condition: version_condition,
                text: version,
            } in &versions
            {
                let condition = self.memo.normalize(Predicate::all(vec![
                    ambient.clone(),
                    placement.clone(),
                    kind_condition.clone(),
                    version_condition.clone(),
                ]));
                if condition == Predicate::False {
                    continue;
                }
                if kind == "List" && version == "v1" {
                    if let Some(items) = items {
                        self.list_items(items, &condition, path_prefix, resources);
                    }
                    continue;
                }
                let kind_selector = selector_path(&condition, &kind, &self.memo);
                let kind_branches = kind_selector
                    .as_ref()
                    .map(|_| KindBranch {
                        predicate: condition.clone(),
                        kind: kind.clone(),
                    })
                    .into_iter()
                    .collect();
                resources.push(PlacedResource {
                    span: mapping.span,
                    condition,
                    resource: ResourceRef {
                        api_version: version.clone(),
                        kind: kind.clone(),
                        kind_candidates: Vec::new(),
                        api_version_candidates: Vec::new(),
                        api_version_branches: Vec::new(),
                        kind_branches,
                        kind_selector,
                    },
                    path_prefix: path_prefix.to_vec(),
                });
            }
        }
    }

    fn list_items(
        &self,
        items: usize,
        condition: &Predicate,
        path_prefix: &[String],
        resources: &mut Vec<PlacedResource>,
    ) {
        let mut prefix = path_prefix.to_vec();
        prefix.push("items[*]".to_owned());
        for item in self.children.get(items).into_iter().flatten() {
            if self.mapping_node(*item) {
                self.mapping(*item, condition, &prefix, resources);
                continue;
            }
            for child in self.children.get(*item).into_iter().flatten() {
                if self.mapping_node(*child) {
                    self.mapping(*child, condition, &prefix, resources);
                }
            }
        }
    }

    fn mapping_node(&self, index: usize) -> bool {
        self.document
            .ownership
            .nodes
            .get(index)
            .is_some_and(|node| {
                matches!(node.kind, YamlNodeKind::Mapping | YamlNodeKind::FlowMapping)
            })
    }

    fn names(&self, index: usize) -> Option<Vec<NamedValue>> {
        let node = self.document.ownership.nodes.get(index)?;
        if !matches!(node.kind, YamlNodeKind::Scalar(_)) {
            return None;
        }
        scalar_names(self.document, node.span, &self.memo)
    }
}

fn selector_path(predicate: &Predicate, kind: &str, memo: &PredicateMemo) -> Option<ValuesPath> {
    let mut pending = vec![predicate];
    let mut candidates = std::collections::BTreeSet::new();
    while let Some(part) = pending.pop() {
        match part.kind() {
            PredicateKind::Guard(Guard::Eq {
                path,
                value: GuardValue::String(value),
            }) if value == kind => {
                candidates.insert(path.clone());
            }
            PredicateKind::And(parts) | PredicateKind::Or(parts) => pending.extend(parts),
            PredicateKind::Not(inner) => pending.push(inner),
            PredicateKind::True
            | PredicateKind::False
            | PredicateKind::Guard(_)
            | PredicateKind::Approximate {
                marker: _,
                paths: _,
                role: _,
                sound_subset: _,
            } => {}
        }
    }
    candidates.into_iter().find(|path| {
        memo.exactly_implies(
            predicate,
            &Predicate::Guard(Guard::Eq {
                path: path.clone(),
                value: GuardValue::String(kind.to_owned()),
            }),
        )
    })
}

fn scalar_names(
    document: &PlacedDocument<'_>,
    span: Span,
    memo: &PredicateMemo,
) -> Option<Vec<NamedValue>> {
    let mut before = String::new();
    let mut after = String::new();
    let mut hole = None;
    for term in document.terms(span) {
        let start = term.range.start.max(span.start);
        let end = term.range.end.min(span.end);
        if term.placeholder {
            if hole.is_some() {
                return None;
            }
            hole = Some(term.value?);
        } else if hole.is_some() {
            after.push_str(document.ownership.skeleton.text.get(start..end)?);
        } else {
            before.push_str(document.ownership.skeleton.text.get(start..end)?);
        }
    }
    let Some(value) = hole else {
        return decoded_name(&before).map(|text| {
            vec![NamedValue {
                condition: Predicate::True,
                text,
            }]
        });
    };
    let mut names = Vec::new();
    for alternative in value.alternatives() {
        let condition = memo.normalize(Predicate::all(vec![
            document.condition.clone(),
            alternative.condition().clone(),
        ]));
        if condition == Predicate::False {
            continue;
        }
        if !alternative.coverage_exact() {
            return None;
        }
        let text = alternative.value().exact_text()?;
        let text = decoded_name(&format!("{before}{text}{after}"))?;
        names.push(NamedValue { condition, text });
    }
    Some(names)
}

fn decoded_name(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return Some(String::new());
    }
    serde_yaml::from_str::<serde_json::Value>(text)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "tests/resource_identity_compat.rs"]
mod compatibility;
#[cfg(test)]
pub(crate) use compatibility::{collect_resource_spans, executed_scalar_body};

pub(crate) fn literal_key(document: &PlacedDocument<'_>, span: Span) -> Option<String> {
    let text = document.ownership.skeleton.text.get(span.start..span.end)?;
    helm_schema_syntax::parse_yaml_key(&format!("{text}:"))
}
