//! Resource headers and List envelopes recovered from executed YAML ownership.

use std::collections::BTreeMap;

use helm_schema_core::{
    CapabilityGuard, Guard, GuardValue, KindBranch, Predicate, PredicateKind, PredicateMemo,
    ResourceRef, ValuesPath,
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
    capabilities: Vec<CapabilityGuard>,
}

#[derive(Default)]
struct HeaderNodes {
    kind: Vec<NodeIx>,
    api_version: Vec<NodeIx>,
    items: Vec<NodeIx>,
    has_computed_key: bool,
}

#[tracing::instrument(skip_all, level = "debug")]
pub(crate) fn placed_resources(
    document: &PlacedDocument<'_>,
    execution: &DocumentExecution,
) -> Vec<PlacedResource> {
    let projection = ResourceProjection::new(document, execution);
    let mut resources = Vec::new();
    for index in projection.roots() {
        projection.mapping(index, &document.condition, &[], &[], &mut resources);
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
            let Some(key) = self.document.mapping_key(*pair) else {
                continue;
            };
            let Some(key) = self.document.literal_key(key) else {
                headers.has_computed_key = true;
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
        inherited_capabilities: &[CapabilityGuard],
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
            has_computed_key,
        } = self.headers(index);
        let Some(mut capabilities) = self.document.header_capabilities(&kind, &api_version) else {
            return;
        };
        capabilities.extend_from_slice(inherited_capabilities);
        let overwrites = has_computed_key.then(|| self.mapping_survivors(index));
        let kinds = self.selected_names(&kind, overwrites.as_ref());
        let mut versions = self.selected_names(&api_version, overwrites.as_ref());
        if versions.is_empty() {
            if has_computed_key {
                return;
            }
            versions.push(NamedValue {
                condition: Predicate::True,
                text: String::new(),
                capabilities: Vec::new(),
            });
        }
        for NamedValue {
            condition: kind_condition,
            text: kind,
            capabilities: kind_capabilities,
        } in kinds
        {
            if kind.is_empty() {
                continue;
            }
            for NamedValue {
                condition: version_condition,
                text: version,
                capabilities: version_capabilities,
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
                let Some(capabilities) =
                    join_capabilities(&capabilities, &kind_capabilities, version_capabilities)
                else {
                    continue;
                };
                if kind == "List" && version == "v1" {
                    for items in &items {
                        let selected = self.document.placement_guard(*items);
                        self.list_items(
                            *items,
                            &self
                                .memo
                                .normalize(Predicate::all(vec![condition.clone(), selected])),
                            &capabilities,
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
                        capability_guards: capabilities,
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
        capabilities: &[CapabilityGuard],
        path_prefix: &[String],
        resources: &mut Vec<PlacedResource>,
    ) {
        let mut prefix = path_prefix.to_vec();
        prefix.push("items[*]".to_owned());
        for item in self.document.children(items) {
            if self.mapping_node(*item) {
                self.mapping(*item, condition, capabilities, &prefix, resources);
                continue;
            }
            for child in self.document.children(*item) {
                if self.mapping_node(*child) {
                    self.mapping(*child, condition, capabilities, &prefix, resources);
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

    fn mapping_survivors(&self, index: NodeIx) -> BTreeMap<NodeIx, Predicate> {
        self.document.mapping_survivors(
            self.document.children(index),
            &BTreeMap::new(),
            &self.memo,
            |_, _| Predicate::False,
        )
    }

    fn selected_names(
        &self,
        indices: &[NodeIx],
        overwrites: Option<&BTreeMap<NodeIx, Predicate>>,
    ) -> Vec<NamedValue> {
        let mut names = Vec::new();
        let mut remaining = Predicate::True;
        for index in indices.iter().rev().take(128) {
            if self.document.kind(*index).is_none() {
                continue;
            }
            let active = self.document.selection_guard(*index);
            let survives = overwrites.map_or_else(
                || remaining.clone(),
                |survivors| {
                    self.document
                        .parent(*index)
                        .and_then(|pair| survivors.get(&pair).cloned())
                        .unwrap_or(Predicate::False)
                },
            );
            for mut name in self.names(*index).unwrap_or_default() {
                name.condition = self.memo.normalize(Predicate::all(vec![
                    name.condition,
                    survives.clone(),
                    active.clone(),
                    self.document.placement_guard(*index),
                ]));
                if name.condition != Predicate::False {
                    names.push(name);
                }
            }
            if overwrites.is_none() {
                remaining = self
                    .memo
                    .normalize(Predicate::all(vec![remaining, active.negated()]));
            }
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

fn join_capabilities(
    inherited: &[CapabilityGuard],
    kind: &[CapabilityGuard],
    version: &[CapabilityGuard],
) -> Option<Vec<CapabilityGuard>> {
    let mut guards = [inherited, kind, version].concat();
    guards.sort_unstable();
    guards.dedup();
    for guard in &guards {
        if let CapabilityGuard::Has { api } = guard
            && guards
                .binary_search(&CapabilityGuard::NotHas { api: api.clone() })
                .is_ok()
        {
            return None;
        }
    }
    Some(guards)
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
                capabilities: Vec::new(),
            }]
        });
    };
    let mut names = Vec::new();
    for (guard, value) in value {
        let ambient = memo.normalize(Predicate::all(vec![document.condition.clone(), guard]));
        let selected = value.selected_names(|leaf| {
            if let Some(exact) = leaf.exact_text() {
                decoded_name(&format!("{before}{exact}{after}"))
                    .map(|text| (text, Predicate::True))
                    .into_iter()
                    .collect()
            } else {
                relationship_names(leaf, execution, &before, &after, memo)
            }
        });
        names.extend(
            selected
                .into_iter()
                .filter_map(|(text, condition, capabilities)| {
                    let condition =
                        memo.normalize(Predicate::all(vec![ambient.clone(), condition]));
                    (condition != Predicate::False).then_some(NamedValue {
                        condition,
                        text,
                        capabilities,
                    })
                }),
        );
    }
    Some(names)
}

fn relationship_names(
    value: &crate::value::FrozenValue,
    execution: &DocumentExecution,
    before: &str,
    after: &str,
    memo: &PredicateMemo,
) -> Vec<(String, Predicate)> {
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
            .literal_conditions()
            .chain(execution.sites().map(crate::value::ExecutedSite::condition))
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
            result.push((
                text,
                memo.normalize(Predicate::all(vec![
                    policy.condition.clone(),
                    Predicate::from(Guard::Eq {
                        path: policy.source.as_ref().clone(),
                        value: GuardValue::String(candidate),
                    }),
                ])),
            ));
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
