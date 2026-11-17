//! Resource headers and List envelopes recovered from executed YAML ownership.

use helm_schema_core::{
    Guard, GuardValue, KindBranch, Predicate, PredicateKind, PredicateMemo, ResourceRef, ValuesPath,
};
use helm_schema_syntax::YamlNodeKind;

use crate::value::DocumentExecution;
use crate::value::yaml::{NodeIx, Piece, PlacedDocument};

pub(crate) struct PlacedResource {
    pub(crate) node: NodeIx,
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
    kind: Vec<NodeIx>,
    api_version: Vec<NodeIx>,
    items: Vec<NodeIx>,
}

#[tracing::instrument(skip_all, level = "debug")]
pub(crate) fn placed_resources(
    document: &PlacedDocument<'_>,
    execution: &DocumentExecution,
) -> Vec<PlacedResource> {
    let projection = ResourceProjection::new(document, execution);
    let mut resources = Vec::new();
    for index in projection.roots() {
        projection.mapping(index, &document.condition, &[], &mut resources);
    }
    resources
}

struct ResourceProjection<'a, 'b> {
    document: &'a PlacedDocument<'b>,
    execution: &'a DocumentExecution,
    memo: PredicateMemo,
}

impl<'a, 'b> ResourceProjection<'a, 'b> {
    fn new(document: &'a PlacedDocument<'b>, execution: &'a DocumentExecution) -> Self {
        Self {
            document,
            execution,
            memo: PredicateMemo::new(),
        }
    }

    fn roots(&self) -> impl Iterator<Item = NodeIx> + '_ {
        self.document.nodes().filter(|index| {
            self.mapping_node(*index)
                && self
                    .document
                    .parent(*index)
                    .and_then(|parent| self.document.kind(parent))
                    == Some(YamlNodeKind::Document)
        })
    }

    fn headers(&self, mapping: NodeIx) -> HeaderNodes {
        let mut headers = HeaderNodes::default();
        for pair in self.document.children(mapping) {
            let children = self.document.children(*pair);
            let Some(key) = children
                .iter()
                .copied()
                .find(|index| self.document.is_key(*index))
            else {
                continue;
            };
            let Some(key) = self.document.literal_key(key) else {
                continue;
            };
            let value = children.iter().copied().find(|index| {
                self.document.kind(*index).is_some() && !self.document.is_key(*index)
            });
            match key {
                "kind" => headers.kind.extend(value),
                "apiVersion" => headers.api_version.extend(value),
                "items" => headers.items.extend(value),
                _ => {}
            }
        }
        headers
    }

    fn mapping(
        &self,
        index: NodeIx,
        ambient: &Predicate,
        path_prefix: &[String],
        resources: &mut Vec<PlacedResource>,
    ) {
        if self.document.kind(index).is_none() {
            return;
        }
        let HeaderNodes {
            kind,
            api_version,
            items,
        } = self.headers(index);
        let kinds = self.selected_names(&kind);
        let mut versions = self.selected_names(&api_version);
        if versions.is_empty() {
            versions.push(NamedValue {
                condition: Predicate::True,
                text: String::new(),
            });
        }
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
                    kind_condition.clone(),
                    version_condition.clone(),
                ]));
                if condition == Predicate::False {
                    continue;
                }
                if kind == "List" && version == "v1" {
                    for items in &items {
                        let selected = self.document.placement_guard(*items);
                        self.list_items(
                            *items,
                            &self
                                .memo
                                .normalize(Predicate::all(vec![condition.clone(), selected])),
                            path_prefix,
                            resources,
                        );
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
                    node: index,
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
        items: NodeIx,
        condition: &Predicate,
        path_prefix: &[String],
        resources: &mut Vec<PlacedResource>,
    ) {
        let mut prefix = path_prefix.to_vec();
        prefix.push("items[*]".to_owned());
        for item in self.document.children(items) {
            if self.mapping_node(*item) {
                self.mapping(*item, condition, &prefix, resources);
                continue;
            }
            for child in self.document.children(*item) {
                if self.mapping_node(*child) {
                    self.mapping(*child, condition, &prefix, resources);
                }
            }
        }
    }

    fn mapping_node(&self, index: NodeIx) -> bool {
        matches!(
            self.document.kind(index),
            Some(YamlNodeKind::Mapping | YamlNodeKind::FlowMapping)
        )
    }

    fn selected_names(&self, indices: &[NodeIx]) -> Vec<NamedValue> {
        let mut names = Vec::new();
        let mut remaining = Predicate::True;
        for index in indices.iter().rev().take(128) {
            if self.document.kind(*index).is_none() {
                continue;
            }
            let active = self.document.selection_guard(*index);
            for mut name in self.names(*index).unwrap_or_default() {
                name.condition = self.memo.normalize(Predicate::all(vec![
                    name.condition,
                    remaining.clone(),
                    active.clone(),
                    self.document.placement_guard(*index),
                ]));
                if name.condition != Predicate::False {
                    names.push(name);
                }
            }
            remaining = self
                .memo
                .normalize(Predicate::all(vec![remaining, active.negated()]));
        }
        names.reverse();
        names
    }

    fn names(&self, index: NodeIx) -> Option<Vec<NamedValue>> {
        if !matches!(self.document.kind(index)?, YamlNodeKind::Scalar(_)) {
            return None;
        }
        scalar_names(self.document, self.execution, index, &self.memo)
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
    execution: &DocumentExecution,
    node: NodeIx,
    memo: &PredicateMemo,
) -> Option<Vec<NamedValue>> {
    let mut before = String::new();
    let mut after = String::new();
    let mut hole = None;
    for piece in document.pieces(node)? {
        match piece {
            Piece::Hole(term) => {
                if hole.is_some() {
                    return None;
                }
                hole = Some(term.frozen_alternatives()?);
            }
            Piece::Literal(text) if hole.is_some() => after.push_str(text),
            Piece::Literal(text) => before.push_str(text),
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
    for (guard, value) in value {
        for alternative in value.alternatives() {
            let condition = memo.normalize(Predicate::all(vec![
                document.condition.clone(),
                guard.clone(),
                alternative.condition().clone(),
            ]));
            if condition == Predicate::False {
                continue;
            }
            if !alternative.coverage_exact() {
                return None;
            }
            let Some(text) = alternative.value().exact_text() else {
                names.extend(relationship_names(
                    alternative.value(),
                    execution,
                    &before,
                    &after,
                    &condition,
                    memo,
                ));
                continue;
            };
            let text = decoded_name(&format!("{before}{text}{after}"))?;
            names.push(NamedValue { condition, text });
        }
    }
    Some(names)
}

fn relationship_names(
    value: &crate::value::FrozenValue,
    execution: &DocumentExecution,
    before: &str,
    after: &str,
    ambient: &Predicate,
    memo: &PredicateMemo,
) -> Vec<NamedValue> {
    let mut result = Vec::new();
    let mut remaining = 1024_usize;
    for relationship in execution.relationships(value, &[], remaining).known {
        let Some(policy) = relationship.destination() else {
            continue;
        };
        if policy.kind != crate::ValueKind::Scalar
            || policy.stringified
            || policy.digest
            || policy.nil_omitting
            || policy.capture.is_some()
            || !policy.output_path.is_empty()
        {
            continue;
        }
        let mut candidates = std::collections::BTreeSet::new();
        let mut pending = execution
            .sites()
            .map(crate::value::ExecutedSite::condition)
            .collect::<Vec<_>>();
        while let Some(predicate) = pending.pop() {
            let Some(rest) = remaining.checked_sub(1) else {
                break;
            };
            remaining = rest;
            match predicate.kind() {
                PredicateKind::Guard(Guard::Eq {
                    path,
                    value: GuardValue::String(text),
                }) if path == policy.source.as_ref() => {
                    candidates.insert(text.clone());
                }
                PredicateKind::And(parts) | PredicateKind::Or(parts) => {
                    pending.extend(parts.iter().cloned());
                }
                PredicateKind::Not(inner) => pending.push(inner.clone()),
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
        for candidate in candidates {
            let Some(text) = decoded_name(&format!("{before}{candidate}{after}")) else {
                continue;
            };
            result.push(NamedValue {
                condition: memo.normalize(Predicate::all(vec![
                    ambient.clone(),
                    policy.condition.clone(),
                    Predicate::from(Guard::Eq {
                        path: policy.source.as_ref().clone(),
                        value: GuardValue::String(candidate),
                    }),
                ])),
                text,
            });
        }
    }
    result
}

#[derive(Default)]
struct LiteralNames {
    values: std::collections::BTreeMap<String, Option<String>>,
    bytes: usize,
}

thread_local! {
    static LITERAL_NAMES: std::cell::RefCell<LiteralNames> = std::cell::RefCell::default();
}

fn decoded_name(text: &str) -> Option<String> {
    LITERAL_NAMES.with_borrow_mut(|cache| {
        if let Some(name) = cache.values.get(text) {
            return name.clone();
        }
        let name = if text.trim().is_empty() {
            Some(String::new())
        } else {
            serde_yaml::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
        };
        let cost = text
            .len()
            .saturating_add(name.as_ref().map_or(0, String::len));
        if cache.values.len() < 1024 && cache.bytes.saturating_add(cost) <= 1024 * 1024 {
            cache.bytes += cost;
            cache.values.insert(text.to_owned(), name.clone());
        }
        name
    })
}

#[cfg(test)]
#[path = "tests/resource_identity_compat.rs"]
pub(crate) mod test_support;
