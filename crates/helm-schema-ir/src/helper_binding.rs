use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::HelperOutputMeta;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HelperBinding {
    ValuesPath(String),
    RootContext,
    Unknown,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    StringSet(BTreeSet<String>),
    PathSet(BTreeSet<String>),
    Dict(BTreeMap<String, HelperBinding>),
    List(Vec<HelperBinding>),
    Overlay {
        entries: BTreeMap<String, HelperBinding>,
        fallback: Box<HelperBinding>,
    },
    Choice(BTreeSet<HelperBinding>),
}

impl HelperBinding {
    pub(crate) fn to_fragment_binding(&self) -> FragmentBinding {
        AbstractValue::from_helper_binding(self)
            .to_fragment_binding()
            .unwrap_or(FragmentBinding::Unknown)
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        AbstractValue::from_helper_binding(self).strings()
    }

    pub(crate) fn item_binding(&self) -> Option<Self> {
        AbstractValue::from_helper_binding(self)
            .helper_range_item()
            .and_then(|value| value.to_helper_binding())
    }

    pub(crate) fn definitely_nonempty_iterable(&self) -> bool {
        AbstractValue::from_helper_binding(self).definitely_nonempty_iterable()
    }

    pub(crate) fn choice(bindings: Vec<Self>) -> Option<Self> {
        let mut choices = BTreeSet::new();
        for binding in bindings {
            match binding {
                Self::Choice(inner) => choices.extend(inner),
                Self::Unknown => {}
                other => {
                    choices.insert(other);
                }
            }
        }
        match choices.len() {
            0 => None,
            1 => choices.into_iter().next(),
            _ => Some(Self::Choice(choices)),
        }
    }
}
