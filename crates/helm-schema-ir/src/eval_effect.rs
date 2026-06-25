use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::helper_summary::{
    HelperFragmentOutputUse, HelperOutputMeta, HelperSummary, insert_type_hint,
};
use crate::{ValueKind, YamlPath};
use helm_schema_core::Predicate;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    pub(crate) reads: BTreeSet<String>,
    pub(crate) output_paths: BTreeSet<String>,
    pub(crate) bound_output_paths: BTreeSet<String>,
    pub(crate) defaults: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) string_hints: BTreeSet<String>,
    pub(crate) encoded_paths: BTreeSet<String>,
    pub(crate) chart_default_paths: BTreeSet<String>,
    pub(crate) local_default_paths: BTreeSet<String>,
    pub(crate) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) local_output_values: Vec<AbstractValue>,
    pub(crate) local_set_mutations: BTreeMap<String, BTreeMap<String, AbstractValue>>,
    pub(crate) helper_summary: HelperSummary,
}

impl Effects {
    pub(crate) fn from_value(value: &AbstractValue) -> Self {
        Self {
            reads: value.paths(),
            output_paths: value.paths(),
            ..Self::default()
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.reads.extend(other.reads);
        self.output_paths.extend(other.output_paths);
        self.bound_output_paths.extend(other.bound_output_paths);
        self.defaults.extend(other.defaults);
        self.string_hints.extend(other.string_hints);
        self.encoded_paths.extend(other.encoded_paths);
        self.chart_default_paths.extend(other.chart_default_paths);
        self.local_default_paths.extend(other.local_default_paths);
        self.local_output_values.extend(other.local_output_values);
        for (path, meta) in other.local_output_meta {
            self.local_output_meta.entry(path).or_default().merge(meta);
        }
        for (name, entries) in other.local_set_mutations {
            self.local_set_mutations
                .entry(name)
                .or_default()
                .extend(entries);
        }
        self.helper_summary.extend(other.helper_summary);
        for (path, hints) in other.type_hints {
            for hint in hints {
                insert_type_hint(&mut self.type_hints, path.clone(), &hint);
            }
        }
    }

    pub(crate) fn add_default_paths(&mut self, paths: BTreeSet<String>) {
        self.defaults
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn add_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            if !path.trim().is_empty() {
                insert_type_hint(&mut self.type_hints, path, schema_type);
            }
        }
    }

    pub(crate) fn add_string_hints(&mut self, paths: BTreeSet<String>) {
        self.string_hints
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn schema_type_hints(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut hints = self.type_hints.clone();
        for path in &self.string_hints {
            insert_type_hint(&mut hints, path.clone(), "string");
        }
        hints
    }

    pub(crate) fn add_encoded_paths(&mut self, paths: BTreeSet<String>) {
        self.encoded_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn output_value_paths(&self) -> BTreeSet<String> {
        let mut paths = self.output_paths.clone();
        paths.extend(self.local_source_paths());
        paths.extend(self.local_output_meta.keys().cloned());
        paths.retain(|path| !path.trim().is_empty());
        paths
    }

    pub(crate) fn default_paths_with_local(&self) -> BTreeSet<String> {
        let mut paths = self.defaults.clone();
        paths.extend(self.local_default_paths.iter().cloned());
        paths.retain(|path| !path.trim().is_empty());
        paths
    }

    pub(crate) fn local_output_sources(&self) -> BTreeSet<String> {
        let mut paths = self.local_rendered_paths();
        paths.extend(self.local_output_meta.keys().cloned());
        paths.retain(|path| !path.trim().is_empty());
        paths
    }

    pub(crate) fn local_source_paths(&self) -> BTreeSet<String> {
        self.local_output_values
            .iter()
            .flat_map(AbstractValue::fragment_source_paths)
            .collect()
    }

    pub(crate) fn local_rendered_paths(&self) -> BTreeSet<String> {
        self.local_output_values
            .iter()
            .flat_map(AbstractValue::fragment_rendered_paths)
            .collect()
    }

    pub(crate) fn merge_local_output_meta<'a>(
        &mut self,
        meta: impl IntoIterator<Item = (&'a String, &'a HelperOutputMeta)>,
    ) {
        for (path, meta) in meta {
            self.local_output_meta
                .entry(path.clone())
                .or_default()
                .merge_ref(meta);
        }
    }

    pub(crate) fn add_local_set_mutation(
        &mut self,
        name: String,
        keys: BTreeSet<String>,
        value: AbstractValue,
    ) {
        if name.trim().is_empty() || keys.is_empty() {
            return;
        }
        let entries = keys
            .into_iter()
            .map(|key| (key, value.clone()))
            .collect::<BTreeMap<_, _>>();
        self.local_set_mutations
            .entry(name)
            .or_default()
            .extend(entries);
    }

    pub(crate) fn local_output_uses(
        &self,
        output_path: &YamlPath,
        kind: ValueKind,
        active_output_predicates: &BTreeSet<Predicate>,
    ) -> Vec<HelperFragmentOutputUse> {
        let mut outputs = output_uses_from_values(
            &self.local_output_values,
            output_path,
            kind,
            &BTreeSet::new(),
            active_output_predicates,
            &self.local_default_paths,
            true,
        );
        for output in &mut outputs {
            if let Some(meta) = self.local_output_meta.get(&output.source_expr) {
                output.meta.merge_ref(meta);
            }
        }
        outputs
    }
}

fn output_uses_from_values(
    values: &[AbstractValue],
    output_path: &YamlPath,
    kind: ValueKind,
    encoded_paths: &BTreeSet<String>,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
    suppress_values_root: bool,
) -> Vec<HelperFragmentOutputUse> {
    let mut outputs = Vec::new();
    for value in values {
        value.collect_output_uses_with_encoding(
            &mut outputs,
            output_path,
            kind,
            encoded_paths,
            active_output_predicates,
            defaulted_paths,
            suppress_values_root,
        );
    }
    outputs
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalResult {
    pub(crate) value: Option<AbstractValue>,
    pub(crate) effects: Effects,
}

impl EvalResult {
    pub(crate) fn none() -> Self {
        Self::default()
    }

    pub(crate) fn from_value(value: AbstractValue) -> Self {
        Self {
            effects: Effects::from_value(&value),
            value: Some(value),
        }
    }

    pub(crate) fn with_effects(value: Option<AbstractValue>, mut effects: Effects) -> Self {
        if let Some(value) = &value {
            effects.output_paths.extend(value.paths());
        }
        Self { value, effects }
    }
}
