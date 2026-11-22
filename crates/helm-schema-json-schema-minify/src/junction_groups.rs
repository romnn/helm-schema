use std::collections::{BTreeSet, HashMap, hash_map};

use helm_schema_json_schema_walk::{
    GENERATED_DEFINITIONS, ReferenceSiblings, parse_definition_reference, visit_subschemas,
    visit_subschemas_mut,
};
use serde_json::{Value, json};

use crate::{has_reference_scope, protected_reference_targets, value_address};

#[cfg(test)]
#[path = "tests/junction_groups.rs"]
mod tests;

const MIN_GROUPED_REFERENCES: usize = 8;

fn draft_07(schema: &Value) -> bool {
    matches!(
        schema.get("$schema").and_then(Value::as_str),
        Some(
            "http://json-schema.org/draft-07/schema#" | "https://json-schema.org/draft-07/schema#"
        )
    )
}

#[tracing::instrument(skip_all)]
pub(crate) fn flatten_junctions(schema: &mut Value) {
    if !draft_07(schema) {
        return;
    }
    let Some(protected) = protected_reference_targets(schema) else {
        return;
    };
    let container = GENERATED_DEFINITIONS.as_str();
    let Some(definitions) = schema.get(container).and_then(Value::as_object) else {
        return;
    };
    let mut candidates = definitions
        .iter()
        .filter(|(_, body)| !protected.contains(&value_address(body)))
        .filter_map(|(name, body)| Some((name.clone(), (junction_keyword(body)?, 0))))
        .collect::<HashMap<_, _>>();
    count_definition_uses(schema, &mut candidates);
    let mut selected = BTreeSet::new();
    find_splices(schema, &protected, &candidates, &mut selected);

    // Keep map entries until the splices finish so other protected positions retain their addresses.
    let mut removed = HashMap::new();
    if let Some(definitions) = schema.get_mut(container).and_then(Value::as_object_mut) {
        for (name, body) in definitions {
            if selected.contains(name) {
                removed.insert(name.clone(), std::mem::take(body));
            }
        }
    }
    splice_junctions(schema, &protected, &mut removed);
    if let Some(definitions) = schema.get_mut(container).and_then(Value::as_object_mut) {
        for name in selected {
            if let Some(body) = removed.remove(&name) {
                definitions.insert(name, body);
            } else {
                definitions.remove(&name);
            }
        }
    }
}

fn junction_keyword(node: &Value) -> Option<&'static str> {
    if node.as_object()?.len() != 1 {
        return None;
    }
    ["allOf", "anyOf"]
        .into_iter()
        .find(|keyword| node.get(keyword).is_some_and(Value::is_array))
}

fn whole_definition_reference(node: &Value) -> Option<String> {
    let reference = parse_definition_reference(node.get("$ref")?.as_str()?)?;
    (reference.keyword == GENERATED_DEFINITIONS && reference.suffix.is_empty())
        .then_some(reference.name)
}

fn count_definition_uses(node: &Value, candidates: &mut HashMap<String, (&'static str, usize)>) {
    if let Some(name) = whole_definition_reference(node)
        && let Some((_, count)) = candidates.get_mut(&name)
    {
        *count += 1;
    }
    visit_subschemas(node, ReferenceSiblings::Visit, &mut |child| {
        count_definition_uses(child, candidates);
    });
}

fn find_splices(
    node: &Value,
    protected: &BTreeSet<usize>,
    candidates: &HashMap<String, (&'static str, usize)>,
    selected: &mut BTreeSet<String>,
) {
    for keyword in ["allOf", "anyOf"] {
        if let Some(array) = node.get(keyword)
            && !protected.contains(&value_address(array))
            && let Some(items) = array.as_array()
        {
            for item in items {
                if item.as_object().is_some_and(|object| object.len() == 1)
                    && let Some(name) = whole_definition_reference(item)
                    && candidates.get(&name) == Some(&(keyword, 1))
                {
                    selected.insert(name);
                }
            }
        }
    }
    visit_subschemas(node, ReferenceSiblings::Visit, &mut |child| {
        if !has_reference_scope(child) {
            find_splices(child, protected, candidates, selected);
        }
    });
}

fn splice_junctions(
    node: &mut Value,
    protected: &BTreeSet<usize>,
    removed: &mut HashMap<String, Value>,
) {
    for keyword in ["allOf", "anyOf"] {
        if let Some(array) = node.get_mut(keyword)
            && !protected.contains(&value_address(array))
            && let Some(items) = array.as_array_mut()
        {
            let mut pending = std::mem::take(items);
            pending.reverse();
            while let Some(mut arm) = pending.pop() {
                if arm.as_object().is_some_and(|object| object.len() == 1) {
                    if let Some(name) = whole_definition_reference(&arm)
                        && let Some(body) = removed.remove(&name)
                    {
                        arm = body;
                    }
                    if let Some(children) = arm.get_mut(keyword).and_then(Value::as_array_mut) {
                        pending.extend(std::mem::take(children).into_iter().rev());
                        continue;
                    }
                }
                items.push(arm);
            }
        }
    }
    visit_subschemas_mut(node, ReferenceSiblings::Visit, &mut |child| {
        if !has_reference_scope(child) {
            splice_junctions(child, protected, removed);
        }
    });
}

#[tracing::instrument(skip_all)]
pub(crate) fn share_reference_groups(schema: &mut Value) {
    if !draft_07(schema) {
        return;
    }
    let keyword = GENERATED_DEFINITIONS.as_str();
    let Some(definitions) = schema.get(keyword).and_then(Value::as_object) else {
        return;
    };
    let prefix = GENERATED_DEFINITIONS.reference("");
    let mut demand = HashMap::<String, usize>::new();
    visit_junctions(schema, &mut |_, _, items| {
        for reference in items
            .iter()
            .filter_map(|item| pure_reference(item, &prefix))
        {
            if let Some(count) = demand.get_mut(reference) {
                *count += 1;
            } else {
                demand.insert(reference.to_owned(), 1);
            }
        }
    });
    if !demand.values().any(|count| *count > 1) {
        return;
    }
    let Some(protected) = protected_reference_targets(schema) else {
        return;
    };
    let mut demand = demand.into_iter().collect::<Vec<_>>();
    // Adjacent ranks put the most widely reused operands in the same intervals.
    demand.sort_unstable_by(|(left, left_uses), (right, right_uses)| {
        right_uses.cmp(left_uses).then_with(|| left.cmp(right))
    });
    let references = demand
        .into_iter()
        .map(|(reference, _)| reference)
        .collect::<Vec<_>>();
    let ranks = references
        .iter()
        .enumerate()
        .map(|(rank, reference)| (reference.as_str(), rank))
        .collect::<HashMap<_, _>>();
    let mut groups = ReferenceGroups::default();
    let mut roots = HashMap::new();
    visit_junctions(schema, &mut |node, keyword, items| {
        let address = value_address(node);
        if protected.contains(&address) {
            return;
        }
        let mut operands = items
            .iter()
            .filter_map(|item| ranks.get(pure_reference(item, &prefix)?).copied())
            .collect::<Vec<_>>();
        if operands.len() < MIN_GROUPED_REFERENCES {
            return;
        }
        operands.sort_unstable();
        operands.dedup();
        if let Some(root) = groups.balance(keyword, &operands) {
            roots.insert(address, (keyword, root));
        }
    });
    let reserved = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let names = groups.profitable_names(
        roots.values().map(|&(_, root)| root),
        &references,
        &reserved,
    );
    if names.iter().all(Option::is_none) {
        return;
    }

    groups.rewrite_junctions(schema, &roots, &references, &names, &prefix);
    let Some(definitions) = schema.get_mut(keyword).and_then(Value::as_object_mut) else {
        return;
    };
    for (group, name) in groups.groups.iter().zip(&names) {
        let Some(name) = name else {
            continue;
        };
        let &Group { keyword, children } = group;
        let mut items = Vec::new();
        for child in children {
            groups.append(child, &references, &names, &mut items);
        }
        definitions.insert(name.clone(), json!({(keyword): items}));
    }
}

fn visit_junctions(node: &Value, visit: &mut impl FnMut(&Value, &'static str, &[Value])) {
    if let Some(keyword) = junction_keyword(node)
        && let Some(items) = node.get(keyword).and_then(Value::as_array)
        && items.len() >= MIN_GROUPED_REFERENCES
    {
        visit(node, keyword, items);
    }
    visit_subschemas(node, ReferenceSiblings::Visit, &mut |child| {
        if !has_reference_scope(child) {
            visit_junctions(child, visit);
        }
    });
}

fn pure_reference<'a>(value: &'a Value, prefix: &str) -> Option<&'a str> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    value
        .get("$ref")
        .and_then(Value::as_str)
        .filter(|reference| reference.starts_with(prefix))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Operand {
    Reference(usize),
    Group(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Group {
    keyword: &'static str,
    children: [Operand; 2],
}

#[derive(Default)]
struct ReferenceGroups {
    groups: Vec<Group>,
    interned: HashMap<Group, usize>,
}

impl ReferenceGroups {
    fn balance(&mut self, keyword: &'static str, ranks: &[usize]) -> Option<Operand> {
        let (first, last) = (*ranks.first()?, *ranks.last()?);
        if first == last {
            return Some(Operand::Reference(first));
        }
        // The highest differing bit fixes the same split for a subset in any junction.
        let bit = 1 << (usize::BITS - 1 - (first ^ last).leading_zeros());
        let pivot = last & !(bit - 1);
        let (left, right) = ranks.split_at(ranks.partition_point(|rank| *rank < pivot));
        let children = [self.balance(keyword, left)?, self.balance(keyword, right)?];
        let group = Group { keyword, children };
        let id = match self.interned.entry(group) {
            hash_map::Entry::Occupied(entry) => *entry.get(),
            hash_map::Entry::Vacant(entry) => {
                let id = self.groups.len();
                entry.insert(id);
                self.groups.push(group);
                id
            }
        };
        Some(Operand::Group(id))
    }

    fn profitable_names(
        &self,
        roots: impl Iterator<Item = Operand>,
        references: &[String],
        reserved: &BTreeSet<String>,
    ) -> Vec<Option<String>> {
        let costs = self.expanded_costs(references);
        let mut uses = vec![0; self.groups.len()];
        for root in roots {
            add_use(&mut uses, root, 1);
        }
        let mut names = vec![None; self.groups.len()];
        let short_width = (reserved.len() + self.groups.len()).max(1).ilog(62) + 1;
        let short_name = "x".repeat(usize::try_from(short_width).unwrap_or_default());
        let mut available = (0..)
            .map(|ordinal| format!("constrains/group-{ordinal}"))
            .filter(|name| !reserved.contains(name))
            .peekable();

        // Parents have larger IDs than their children. An emitted parent carries just
        // one copy of its children, regardless of how many times it is referenced.
        for (id, (&Group { keyword, children }, slot)) in
            self.groups.iter().zip(&mut names).enumerate().rev()
        {
            let Some((&count, cost)) = uses.get(id).zip(costs.get(id)) else {
                continue;
            };
            if count > 1
                && let Some(name) = available.peek()
                && cost.saves_bytes(count, keyword, name, &short_name)
            {
                *slot = available.next();
            }
            for child in children {
                add_use(&mut uses, child, if slot.is_some() { 1 } else { count });
            }
        }
        names
    }

    fn expanded_costs(&self, references: &[String]) -> Vec<ArrayCost> {
        let references = references
            .iter()
            .map(|reference| ArrayCost::reference(reference))
            .collect::<Vec<_>>();
        let mut expanded = Vec::with_capacity(self.groups.len());
        for &Group {
            keyword: _,
            children,
        } in &self.groups
        {
            let [left, right] = children.map(|operand| {
                match operand {
                    Operand::Reference(rank) => references.get(rank),
                    Operand::Group(id) => expanded.get(id),
                }
                .copied()
                .unwrap_or_default()
            });
            expanded.push(ArrayCost {
                readable: left.readable + right.readable,
                shortened: left.shortened + right.shortened,
            });
        }
        expanded
    }

    fn rewrite_junctions(
        &self,
        node: &mut Value,
        roots: &HashMap<usize, (&'static str, Operand)>,
        references: &[String],
        names: &[Option<String>],
        prefix: &str,
    ) {
        let address = value_address(node);
        // Descendant positions must be processed before a parent moves its arms.
        visit_subschemas_mut(node, ReferenceSiblings::Visit, &mut |child| {
            if !has_reference_scope(child) {
                self.rewrite_junctions(child, roots, references, names, prefix);
            }
        });
        let Some(&(keyword, root)) = roots.get(&address) else {
            return;
        };
        let Some(items) = node.get_mut(keyword).and_then(Value::as_array_mut) else {
            return;
        };
        let mut replacements = Vec::new();
        self.append(root, references, names, &mut replacements);
        if replacements.len()
            < items
                .iter()
                .filter_map(|item| pure_reference(item, prefix))
                .count()
        {
            items.retain(|item| pure_reference(item, prefix).is_none());
            items.extend(replacements);
        }
    }

    fn append(
        &self,
        operand: Operand,
        references: &[String],
        names: &[Option<String>],
        items: &mut Vec<Value>,
    ) {
        match operand {
            Operand::Reference(rank) => {
                if let Some(reference) = references.get(rank) {
                    items.push(json!({"$ref": reference}));
                }
            }
            Operand::Group(id) => {
                if let Some(Some(name)) = names.get(id) {
                    items.push(json!({"$ref": GENERATED_DEFINITIONS.reference(name)}));
                } else if let Some(group) = self.groups.get(id) {
                    for child in group.children {
                        self.append(child, references, names, items);
                    }
                }
            }
        }
    }
}

fn add_use(uses: &mut [usize], operand: Operand, count: usize) {
    if let Operand::Group(id) = operand
        && let Some(uses) = uses.get_mut(id)
    {
        *uses += count;
    }
}

#[derive(Clone, Copy, Default)]
struct ArrayCost {
    readable: usize,
    shortened: usize,
}

impl ArrayCost {
    fn reference(reference: &str) -> Self {
        let readable = reference_item_bytes(reference);
        // Price the removed operand at its shortest spelling and the new group at
        // the widest available short key. Using the existing keys and appending
        // the new group would fit, even before ranking keys again by reference count.
        let shortened = parse_definition_reference(reference).map_or(readable, |mut parsed| {
            parsed.name = "x".to_owned();
            reference_item_bytes(&parsed.to_reference()).min(readable)
        });
        Self {
            readable,
            shortened,
        }
    }

    fn saves_bytes(self, uses: usize, keyword: &str, name: &str, short_name: &str) -> bool {
        let saves = |expanded: usize, name: &str| {
            let reference = reference_item_bytes(&GENERATED_DEFINITIONS.reference(name));
            // Strip the outer braces. The definition separator pays for the final
            // operand separator already included in `expanded`.
            let definition = json!({(name): {(keyword): []}}).to_string().len() - 2;
            (uses - 1) * expanded > uses * reference + definition
        };
        saves(self.readable, name) && saves(self.shortened, short_name)
    }
}

fn reference_item_bytes(reference: &str) -> usize {
    json!({"$ref": reference}).to_string().len() + 1
}
