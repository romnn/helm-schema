use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_eval::ValueRead;
use crate::helper_meta::{HelperOutputMeta, RenderedRow, insert_type_hint};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    pub(crate) output_paths: BTreeSet<String>,
    pub(crate) bound_output_paths: BTreeSet<String>,
    pub(crate) defaults: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Input-type hints that arose under branch predicates inside a called
    /// helper body: they hold only where those branches render, so they may
    /// type conditional overlays but never the unconditional base.
    pub(crate) guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Types observed by a predicate expression. These become input
    /// alternatives only when an expression such as `ternary` consumes the
    /// predicate; control-flow lowering owns ordinary `if`/`with` guards.
    pub(crate) tested_type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) parsed_yaml_input_paths: BTreeSet<String>,
    pub(crate) yaml_serialized_paths: BTreeSet<String>,
    pub(crate) encoded_paths: BTreeSet<String>,
    pub(crate) shape_erased_paths: BTreeSet<String>,
    /// Paths whose value was replaced by derived text in this expression
    /// (`printf`, `quote`, `trunc`, `b64enc`, …): later transform stages
    /// operate on that text, so they claim nothing about the raw path.
    pub(crate) derived_text_paths: BTreeSet<String>,
    /// Paths on which a string-consuming transform (`trunc`, `b64enc`, …)
    /// bound a real runtime string contract: rendering fails for non-string
    /// values, so a later total stringification must not erase their shape.
    pub(crate) string_contract_paths: BTreeSet<String>,
    /// The subset of string contracts recorded by consumers evaluated in
    /// THIS expression (never copied across a helper-summary boundary):
    /// only these may become ambient-scoped truthy⇒string fail captures —
    /// a called helper's path-level contract flags lost their body-internal
    /// guards and stay row evidence.
    pub(crate) direct_string_consumer_paths: BTreeSet<String>,
    pub(crate) chart_default_paths: BTreeSet<String>,
    pub(crate) local_default_paths: BTreeSet<String>,
    pub(crate) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    /// Shallow (non-descending) `.Values` source paths of locals that were
    /// read by the expression. Guard-path seeding and expression path
    /// resolution consume this; output rows ride the value itself.
    pub(crate) local_source_paths: BTreeSet<String>,
    pub(crate) local_set_mutations: BTreeMap<String, BTreeMap<String, AbstractValue>>,
    /// Pathless reads observed inside called helper bodies (guard reads and
    /// dependency-lane rows), carrying helper-internal guards only; the
    /// absorbing site adds its ambient guards and provenance.
    pub(crate) helper_reads: Vec<ValueRead>,
    /// Rendered claims of called helpers, for no-render demotion and
    /// per-path meta restoration (see [`RenderedRow`]).
    pub(crate) helper_rendered: Vec<RenderedRow>,
    /// Predicate paths severed by index-call narrowing inside called
    /// helpers; ancestor guard reads absorb against them.
    pub(crate) helper_suppressed_paths: BTreeSet<String>,
    /// `fail` captures of called helpers, carrying helper-internal
    /// predicates only; the absorbing site prepends its ambient state.
    pub(crate) helper_fails: Vec<FailCapture>,
}

/// One captured `fail` call: the predicate conjunction reaching it, plus
/// the values paths of enclosing conditions whose lowering was APPROXIMATE
/// (truthy fallbacks, dropped conjuncts). Raw predicates, not
/// [`helm_schema_core::GuardDnf`]: the DNF conversion drops conjuncts it
/// cannot represent, which is safe for row conditions (wider arms) but
/// unsound for fail NEGATION. The approximate paths let the negation
/// abstain when the imprecision touches the tested path itself.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FailCapture {
    pub(crate) conjunction: Vec<helm_schema_core::Predicate>,
    pub(crate) approximate_condition_paths: BTreeSet<String>,
    /// Ranged paths in the conjunction that iterate the path DIRECTLY
    /// (`range .Values.x`): only these have member identities, and
    /// helper-scope ranges never reach the document-lane directness
    /// channel.
    pub(crate) direct_ranged_paths: BTreeSet<String>,
}

impl Effects {
    pub(crate) fn from_value(value: &AbstractValue) -> Self {
        Self {
            output_paths: value.paths(),
            ..Self::default()
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.output_paths.extend(other.output_paths);
        self.bound_output_paths.extend(other.bound_output_paths);
        self.defaults.extend(other.defaults);
        self.parsed_yaml_input_paths
            .extend(other.parsed_yaml_input_paths);
        self.yaml_serialized_paths
            .extend(other.yaml_serialized_paths);
        self.encoded_paths.extend(other.encoded_paths);
        self.shape_erased_paths.extend(other.shape_erased_paths);
        self.derived_text_paths.extend(other.derived_text_paths);
        self.string_contract_paths
            .extend(other.string_contract_paths);
        self.direct_string_consumer_paths
            .extend(other.direct_string_consumer_paths);
        self.chart_default_paths.extend(other.chart_default_paths);
        self.local_default_paths.extend(other.local_default_paths);
        self.local_source_paths.extend(other.local_source_paths);
        for (path, meta) in other.local_output_meta {
            self.local_output_meta.entry(path).or_default().merge(&meta);
        }
        for (name, entries) in other.local_set_mutations {
            self.local_set_mutations
                .entry(name)
                .or_default()
                .extend(entries);
        }
        for read in other.helper_reads {
            if !self.helper_reads.contains(&read) {
                self.helper_reads.push(read);
            }
        }
        for row in other.helper_rendered {
            if !self.helper_rendered.contains(&row) {
                self.helper_rendered.push(row);
            }
        }
        self.helper_suppressed_paths
            .extend(other.helper_suppressed_paths);
        for condition in other.helper_fails {
            if !self.helper_fails.contains(&condition) {
                self.helper_fails.push(condition);
            }
        }
        for (path, hints) in other.type_hints {
            for hint in hints {
                insert_type_hint(&mut self.type_hints, path.clone(), &hint);
            }
        }
        for (path, hints) in other.guarded_type_hints {
            for hint in hints {
                insert_type_hint(&mut self.guarded_type_hints, path.clone(), &hint);
            }
        }
        for (path, hints) in other.tested_type_hints {
            for hint in hints {
                insert_type_hint(&mut self.tested_type_hints, path.clone(), &hint);
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

    pub(crate) fn add_tested_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            if !path.trim().is_empty() {
                insert_type_hint(&mut self.tested_type_hints, path, schema_type);
            }
        }
    }

    pub(crate) fn promote_tested_type_hints(&mut self) {
        for (path, hints) in std::mem::take(&mut self.tested_type_hints) {
            for hint in hints {
                insert_type_hint(&mut self.type_hints, path.clone(), &hint);
            }
        }
    }

    pub(crate) fn add_encoded_paths(&mut self, paths: BTreeSet<String>) {
        self.encoded_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn add_shape_erased_paths(&mut self, paths: BTreeSet<String>) {
        self.shape_erased_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn output_value_paths(&self) -> BTreeSet<String> {
        let mut paths = self.output_paths.clone();
        paths.extend(self.local_source_paths.iter().cloned());
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

    pub(crate) fn merge_local_output_meta<'a>(
        &mut self,
        meta: impl IntoIterator<Item = (&'a String, &'a HelperOutputMeta)>,
    ) {
        for (path, meta) in meta {
            self.local_output_meta
                .entry(path.clone())
                .or_default()
                .merge(meta);
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
