use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbstractValue {
    ValuesPath(String),
    RootContext,
    PathSet(BTreeSet<String>),
    Dict(BTreeMap<String, AbstractValue>),
    List(Vec<AbstractValue>),
    Choice(BTreeSet<AbstractValue>),
}

impl AbstractValue {
    pub(crate) fn values_root() -> Self {
        Self::ValuesPath(String::new())
    }

    pub(crate) fn paths(&self) -> BTreeSet<String> {
        match self {
            Self::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Self::RootContext => BTreeSet::new(),
            Self::PathSet(paths) => paths.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::paths).collect(),
            Self::Dict(map) => map.values().flat_map(Self::paths).collect(),
            Self::List(items) => items.iter().flat_map(Self::paths).collect(),
        }
    }

    pub(crate) fn choice(values: Vec<Self>) -> Option<Self> {
        let mut flat = BTreeSet::new();
        for value in values {
            match value {
                Self::Choice(inner) => flat.extend(inner),
                other => {
                    flat.insert(other);
                }
            }
        }
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(Self::Choice(flat)),
        }
    }

    pub(crate) fn apply_to_path(&self, rest: &[String]) -> Option<Self> {
        match self {
            Self::ValuesPath(prefix) => {
                if rest.is_empty() {
                    Some(Self::ValuesPath(prefix.clone()))
                } else if prefix.is_empty() {
                    Some(Self::ValuesPath(rest.join(".")))
                } else {
                    Some(Self::ValuesPath(format!("{prefix}.{}", rest.join("."))))
                }
            }
            Self::RootContext => {
                if rest.first().is_some_and(|segment| segment == "Values") {
                    if rest.len() == 1 {
                        Some(Self::values_root())
                    } else {
                        Some(Self::ValuesPath(rest[1..].join(".")))
                    }
                } else {
                    None
                }
            }
            Self::PathSet(paths) => {
                let appended = paths
                    .iter()
                    .map(|path| {
                        if rest.is_empty() {
                            path.clone()
                        } else if path.is_empty() {
                            rest.join(".")
                        } else {
                            format!("{path}.{}", rest.join("."))
                        }
                    })
                    .collect();
                Some(Self::PathSet(appended))
            }
            Self::Choice(choices) => {
                let mut out = Vec::new();
                for value in choices {
                    if let Some(bound) = value.apply_to_path(rest) {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            Self::Dict(map) if rest.len() == 1 => map.get(&rest[0]).cloned(),
            Self::List(items) if rest.len() == 1 => {
                let index = rest[0].parse::<usize>().ok()?;
                items.get(index).cloned()
            }
            Self::Dict(_) | Self::List(_) => None,
        }
    }

    pub(crate) fn item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => {
                if path.is_empty() {
                    Some(Self::ValuesPath("*".to_string()))
                } else {
                    Some(Self::ValuesPath(format!("{path}.*")))
                }
            }
            Self::RootContext => None,
            Self::PathSet(paths) => Some(Self::PathSet(
                paths
                    .iter()
                    .map(|path| {
                        if path.is_empty() {
                            "*".to_string()
                        } else {
                            format!("{path}.*")
                        }
                    })
                    .collect(),
            )),
            Self::Choice(choices) => {
                let mut out = Vec::new();
                for choice_value in choices {
                    if let Some(bound) = choice_value.item() {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            Self::List(items) => Self::choice(items.clone()),
            Self::Dict(map) => Self::choice(map.values().cloned().collect()),
        }
    }
}
