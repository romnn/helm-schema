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
    /// Input-type hints from a literal `default`/`coalesce` fallback. The
    /// selection call itself never consumes the raw value — every Helm-empty
    /// input takes the fallback and renders — so these type only the TRUTHY
    /// arm of the path and must never close the base against the Helm-falsy
    /// set.
    pub(crate) fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Types observed by a predicate expression. These become input
    /// alternatives only when an expression such as `ternary` consumes the
    /// predicate; control-flow lowering owns ordinary `if`/`with` guards.
    pub(crate) tested_type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) parsed_yaml_input_paths: BTreeSet<String>,
    pub(crate) yaml_serialized_paths: BTreeSet<String>,
    pub(crate) json_serialized_paths: BTreeSet<String>,
    pub(crate) encoded_paths: BTreeSet<String>,
    pub(crate) shape_erased_paths: BTreeSet<String>,
    /// Paths whose value in this expression IS the exact Go `%v` rendering
    /// of the path (`toString .Values.x` over a single identity operand).
    /// Unlike `shape_erased_paths` — which also covers `quote`, `join`,
    /// `len`, and the numeric casts, whose output is NOT that text — an
    /// equality on such a value projects its literal back through the
    /// `toString` preimage.
    pub(crate) stringified_paths: BTreeSet<String>,
    /// Paths whose value was replaced by derived text in this expression
    /// (`printf`, `quote`, `trunc`, `b64enc`, …): later transform stages
    /// operate on that text, so they claim nothing about the raw path.
    pub(crate) derived_text_paths: BTreeSet<String>,
    /// Paths consumed as a DIRECT operand of a Sprig `merge` family call in
    /// this expression. The operand's strict map contract rides its own fail
    /// implication (keyed on the call's live gate), so the operand's splice
    /// row cannot itself reject a Helm-falsy value and the base falsy escape
    /// survives it. Only operands that ARE a path identity are recorded;
    /// constructed containers referencing a path abstain.
    pub(crate) merge_operand_paths: BTreeSet<String>,
    /// Literal keys an `omit` in this expression removed from the map at
    /// each path: whole-map sink typing must not bind those members
    /// (external-secrets' OpenShift `adaptSecurityContext` omit).
    pub(crate) omitted_map_keys: BTreeMap<String, BTreeSet<String>>,
    /// Range keys converted to text by an earlier pipeline stage.
    pub(crate) derived_range_key_paths: BTreeSet<String>,
    /// Paths on which a string-consuming transform (`trunc`, `b64enc`, …)
    /// bound a real runtime string contract: rendering fails for non-string
    /// values, so a later total stringification must not erase their shape.
    pub(crate) string_contract_paths: BTreeSet<String>,
    /// Range identities exported by called helper bodies.
    pub(crate) range_modes: crate::range_modes::RangeModes,
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
    /// Literal root-context fields replaced by structural `set` calls.
    pub(crate) root_set_mutations: BTreeMap<String, AbstractValue>,
    /// Root-field truth predicates already decoded inside called helpers.
    pub(crate) root_set_predicates: BTreeMap<String, helm_schema_core::Predicate>,
    /// Root-field value dispatches already joined inside called helpers.
    pub(crate) root_set_value_dispatches: BTreeMap<String, RootValueDispatch>,
    /// Chart value subtrees that supply defaults to a replaced effective `.Values` tree.
    pub(crate) values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    /// Helper names through which the values ROOT was replaced
    /// (`set . "Values" (get (include NAME …) …)`); the symbolic context
    /// decides whether a name is a program-wrapper engine.
    pub(crate) values_root_helper_includes: BTreeSet<String>,
    /// Pathless reads observed inside called helper bodies (guard reads and
    /// dependency-lane rows), carrying helper-internal guards only; the
    /// absorbing site adds its ambient guards and provenance.
    pub(crate) helper_reads: Vec<ValueRead>,
    /// Rendered claims of called helpers, for no-render demotion and
    /// per-path meta restoration (see [`RenderedRow`]).
    pub(crate) helper_rendered: Vec<RenderedRow>,
    /// Rendered claims produced while eagerly evaluating a call argument.
    /// They executed, but are not the enclosing helper's returned value, so
    /// every later absorption keeps them on the dependency lane.
    pub(crate) helper_dependency_rendered: Vec<RenderedRow>,
    /// Predicate paths severed by index-call narrowing inside called
    /// helpers; ancestor guard reads absorb against them.
    pub(crate) helper_suppressed_paths: BTreeSet<String>,
    /// `fail` captures of called helpers, carrying helper-internal
    /// predicates only; the absorbing site prepends its ambient state.
    pub(crate) helper_fails: Vec<FailCapture>,
    /// Object-producing mutations that have executed before later member
    /// reads. Their outer predicates remain attached so only accesses that
    /// imply the mutation's execution may accept the converted input kind.
    pub(crate) member_host_conversions: BTreeSet<MemberHostConversion>,
}

/// The exhaustive value alternatives of one root-context field assigned
/// across the arms of an if/else chain (`$_ := set . "mode" "…"` in every
/// arm). The arm conditions are mutually exclusive and total by
/// construction — each carries the negations of every earlier arm — so an
/// equality on the field decodes as the exact disjunction of the arms
/// assigning the compared literal (vault's `ne .mode "external"` gates).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RootValueDispatch {
    pub(crate) arms: Vec<(helm_schema_core::Predicate, helm_schema_core::GuardValue)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MemberHostConversion {
    pub(crate) path: String,
    pub(crate) input_kind: String,
    pub(crate) outer_predicates: Vec<helm_schema_core::Predicate>,
}

/// One captured `fail` call: the predicate conjunction reaching it. Raw
/// predicates, not [`helm_schema_core::GuardDnf`]: the DNF conversion drops
/// conjuncts it cannot represent, which is safe for row conditions (wider
/// arms) but unsound for fail NEGATION. Enclosing conditions whose lowering
/// was APPROXIMATE (truthy fallbacks, dropped conjuncts) appear in the
/// conjunction as [`helm_schema_core::Predicate::Approximate`] conjuncts,
/// so the negation can abstain instead of manufacturing requirements the
/// chart never stated.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FailCapture {
    pub(crate) conjunction: Vec<helm_schema_core::Predicate>,
    /// The range facts the capture rides: `direct` marks paths the capture
    /// site was actively ranging (only these have member identities, and
    /// helper-scope ranges never reach the document-lane directness
    /// channel); JSON-decoded and destructured flavors carry every observed
    /// occurrence.
    pub(crate) ranged: crate::range_modes::RangeModes,
    pub(crate) kind: CaptureKind,
}

/// How a [`FailCapture`]'s conjunction lowers into schema requirements.
/// The variants select mutually exclusive lowering paths in the signal
/// builder; the payloads exist only for their variant's lane.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CaptureKind {
    /// A direct `fail`-style capture: the failing TEST conjunct is negated
    /// wherever the outer guards hold.
    #[default]
    Fail,
    /// Collections whose range key reaches a strict string consumer.
    RangeKeyStrings { paths: BTreeSet<String> },
    /// Every member of the named collection paths reaches a strict runtime
    /// consumer with the given JSON kind; a pattern additionally binds each
    /// member to a parser's lexical domain (genSignedCert's ip list).
    CollectionItems {
        paths: BTreeSet<String>,
        schema_type: String,
        pattern: Option<String>,
    },
    /// A literal zero-based `index` executes on this source path.
    IndexAccess { path: String, index: usize },
    /// A literal index executes on a list produced by splitting source text.
    SplitIndexAccess {
        paths: BTreeSet<String>,
        separator: String,
        index: usize,
        total_text_preimage: bool,
    },
    /// A scalar path must have the named JSON Schema type whenever the
    /// capture's execution predicates hold.
    ValueType { path: String, schema_type: String },
    /// A `dig` SUBJECT step: whenever the capture's execution predicates
    /// hold, the path must be an object even when explicitly null — Sprig
    /// type-asserts the dict before any nil handling, so a null aborts
    /// while absence stays open (the conjunction carries the strict
    /// presence guard).
    DigSubject { path: String },
    /// A comparison operand must have the named JSON Schema type when
    /// PRESENT and non-null; `eq`/`ne` compare `nil` against anything.
    ComparableKind { path: String, schema_type: String },
    /// A string path must match the pattern whenever the capture's execution
    /// predicates hold.
    ValuePattern {
        path: String,
        pattern: String,
        templated: bool,
    },
    /// A raw splice inside a manually quoted scalar: whenever the capture's
    /// execution predicates hold, every string the path's value contributes
    /// to the rendered token must be valid content for the quoting style.
    QuotedSerialization {
        path: String,
        style: helm_schema_core::QuotedScalarStyle,
    },
    /// A member-access capture (`[outer…, ¬object(P)]` from a field access
    /// through `P`): the signal builder folds these per path into one
    /// bypass-proof arm instead of lowering each as its own implication.
    MemberAccess {
        /// Raw input kinds converted to an object by a proven earlier
        /// mutation on every execution path reaching this member access.
        handled_kinds: BTreeSet<String>,
    },
}

impl FailCapture {
    /// Whether any enclosing condition's lowering was approximate: the
    /// negation-based lowering must abstain for the whole capture.
    pub(crate) fn contains_approximation(&self) -> bool {
        self.conjunction
            .iter()
            .any(helm_schema_core::Predicate::contains_approximation)
    }
}

impl CaptureKind {
    /// Rewrite every values path the kind payload carries (dependency
    /// namespacing rebases captures under the subchart's key exactly like
    /// the conjunction's predicate paths).
    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Self::Fail | Self::MemberAccess { .. } => {}
            Self::RangeKeyStrings { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                *paths = paths.iter().map(|path| map(path)).collect();
            }
            Self::IndexAccess { path, .. }
            | Self::ValueType { path, .. }
            | Self::DigSubject { path }
            | Self::ComparableKind { path, .. }
            | Self::ValuePattern { path, .. }
            | Self::QuotedSerialization { path, .. } => {
                *path = map(path);
            }
        }
    }
}

impl Effects {
    pub(crate) fn from_value(value: &AbstractValue) -> Self {
        Self {
            output_paths: value.paths(),
            ..Self::default()
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        // Exhaustive destructuring: a new channel refuses to compile until
        // this merge decides how to combine it, instead of being silently
        // dropped across expression boundaries.
        let Self {
            output_paths,
            bound_output_paths,
            defaults,
            type_hints,
            guarded_type_hints,
            fallback_type_hints,
            tested_type_hints,
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
            stringified_paths,
            derived_text_paths,
            merge_operand_paths,
            omitted_map_keys,
            derived_range_key_paths,
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            chart_default_paths,
            local_default_paths,
            local_output_meta,
            local_source_paths,
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            values_default_sources,
            values_root_helper_includes,
            helper_reads,
            helper_rendered,
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            member_host_conversions,
        } = other;
        self.output_paths.extend(output_paths);
        self.bound_output_paths.extend(bound_output_paths);
        self.defaults.extend(defaults);
        self.parsed_yaml_input_paths.extend(parsed_yaml_input_paths);
        self.yaml_serialized_paths.extend(yaml_serialized_paths);
        self.json_serialized_paths.extend(json_serialized_paths);
        self.encoded_paths.extend(encoded_paths);
        self.shape_erased_paths.extend(shape_erased_paths);
        self.stringified_paths.extend(stringified_paths);
        self.derived_text_paths.extend(derived_text_paths);
        self.merge_operand_paths.extend(merge_operand_paths);
        for (path, keys) in omitted_map_keys {
            self.omitted_map_keys.entry(path).or_default().extend(keys);
        }
        self.derived_range_key_paths.extend(derived_range_key_paths);
        self.string_contract_paths.extend(string_contract_paths);
        self.range_modes.merge(&range_modes);
        self.direct_string_consumer_paths
            .extend(direct_string_consumer_paths);
        self.chart_default_paths.extend(chart_default_paths);
        self.local_default_paths.extend(local_default_paths);
        self.local_source_paths.extend(local_source_paths);
        for (path, meta) in local_output_meta {
            self.local_output_meta.entry(path).or_default().merge(&meta);
        }
        for (name, entries) in local_set_mutations {
            self.local_set_mutations
                .entry(name)
                .or_default()
                .extend(entries);
        }
        for key in root_set_mutations.keys() {
            self.root_set_predicates.remove(key);
            self.root_set_value_dispatches.remove(key);
        }
        self.root_set_mutations.extend(root_set_mutations);
        self.root_set_predicates.extend(root_set_predicates);
        self.root_set_value_dispatches
            .extend(root_set_value_dispatches);
        self.values_default_sources.extend(values_default_sources);
        self.values_root_helper_includes
            .extend(values_root_helper_includes);
        for read in helper_reads {
            if !self.helper_reads.contains(&read) {
                self.helper_reads.push(read);
            }
        }
        for row in helper_rendered {
            if !self.helper_rendered.contains(&row) {
                self.helper_rendered.push(row);
            }
        }
        for row in helper_dependency_rendered {
            if !self.helper_dependency_rendered.contains(&row) {
                self.helper_dependency_rendered.push(row);
            }
        }
        self.helper_suppressed_paths.extend(helper_suppressed_paths);
        for condition in helper_fails {
            if !self.helper_fails.contains(&condition) {
                self.helper_fails.push(condition);
            }
        }
        self.member_host_conversions.extend(member_host_conversions);
        for (path, hints) in type_hints {
            for hint in hints {
                insert_type_hint(&mut self.type_hints, path.clone(), &hint);
            }
        }
        for (path, hints) in guarded_type_hints {
            for hint in hints {
                insert_type_hint(&mut self.guarded_type_hints, path.clone(), &hint);
            }
        }
        for (path, hints) in fallback_type_hints {
            for hint in hints {
                insert_type_hint(&mut self.fallback_type_hints, path.clone(), &hint);
            }
        }
        for (path, hints) in tested_type_hints {
            for hint in hints {
                insert_type_hint(&mut self.tested_type_hints, path.clone(), &hint);
            }
        }
    }

    /// Keep effects caused by evaluating a value while discarding facts that
    /// merely describe the value returned by that expression.
    ///
    /// Helper arguments are eager, so failures, strict consumers, nested
    /// helper reads, and mutations still execute even when the callee ignores
    /// its context. The argument value itself does not render at the call
    /// site; its output identity and selection metadata must not leak there.
    pub(crate) fn execution_only(self) -> Self {
        // Exhaustive rebuild: a new channel refuses to compile until this
        // boundary decides whether it describes the value (discard) or its
        // evaluation (keep).
        let Self {
            output_paths: _,
            bound_output_paths: _,
            defaults: _,
            type_hints: _,
            guarded_type_hints: _,
            fallback_type_hints: _,
            tested_type_hints: _,
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
            // Describes the value returned by the expression, not its
            // evaluation: the argument value does not render at the call
            // site.
            stringified_paths: _,
            derived_text_paths,
            // Describes the merged VALUE's operands, which do not render at
            // the call site: keeping it would grant falsy tolerance to
            // unrelated splices of the same path in the caller.
            merge_operand_paths: _,
            omitted_map_keys: _,
            derived_range_key_paths,
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            chart_default_paths,
            local_default_paths: _,
            local_output_meta: _,
            local_source_paths: _,
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            values_default_sources,
            values_root_helper_includes,
            helper_reads,
            helper_rendered,
            mut helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            member_host_conversions,
        } = self;
        for row in helper_rendered {
            if !helper_dependency_rendered.contains(&row) {
                helper_dependency_rendered.push(row);
            }
        }
        Self {
            output_paths: BTreeSet::new(),
            bound_output_paths: BTreeSet::new(),
            defaults: BTreeSet::new(),
            type_hints: BTreeMap::new(),
            guarded_type_hints: BTreeMap::new(),
            fallback_type_hints: BTreeMap::new(),
            tested_type_hints: BTreeMap::new(),
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
            stringified_paths: BTreeSet::new(),
            derived_text_paths,
            merge_operand_paths: BTreeSet::new(),
            omitted_map_keys: BTreeMap::new(),
            derived_range_key_paths,
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            chart_default_paths,
            local_default_paths: BTreeSet::new(),
            local_output_meta: BTreeMap::new(),
            local_source_paths: BTreeSet::new(),
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            values_default_sources,
            values_root_helper_includes,
            helper_reads,
            helper_rendered: Vec::new(),
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            member_host_conversions,
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

    pub(crate) fn add_fallback_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            if !path.trim().is_empty() {
                insert_type_hint(&mut self.fallback_type_hints, path, schema_type);
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
