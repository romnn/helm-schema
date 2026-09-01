use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_eval::ValueRead;
use crate::helper_meta::{HelperOutputMeta, RenderedRow};
use crate::observed_facts::{HintGrade, ObservedFacts};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use helm_schema_core::ValuesPath;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    pub(crate) output_paths: BTreeSet<ValuesPath>,
    pub(crate) bound_output_paths: BTreeSet<ValuesPath>,
    pub(crate) defaults: BTreeSet<ValuesPath>,
    pub(crate) observed_facts: ObservedFacts,
    pub(crate) parsed_yaml_input_paths: BTreeSet<ValuesPath>,
    pub(crate) yaml_serialized_paths: BTreeSet<ValuesPath>,
    /// Paths serialized to YAML and then evaluated by `tpl`. Their
    /// collection shape survives, but template-bearing string leaves are
    /// programs whose rendered values reach the sink.
    pub(crate) templated_yaml_paths: BTreeSet<ValuesPath>,
    pub(crate) json_serialized_paths: BTreeSet<ValuesPath>,
    pub(crate) encoded_paths: BTreeSet<ValuesPath>,
    /// Total stringifications observed somewhere inside called helper
    /// bodies. They are execution facts for the caller's aggregate contract,
    /// not transformations of every returned occurrence of the same path.
    pub(crate) helper_observed_shape_erased_paths: BTreeSet<ValuesPath>,
    /// Paths rendered through Sprig `quote`/`squote` in this expression:
    /// unlike every other total stringification, those SKIP nil operands
    /// entirely, so a missing or null source renders an explicit YAML
    /// null into the sink (traefik's `mountPath: {{ … | quote }}`).
    pub(crate) nil_omitting_paths: BTreeSet<ValuesPath>,
    /// Paths whose value in this expression IS the exact Go `%v` rendering
    /// of the path (`toString .Values.x` over a single identity operand).
    /// Unlike `shape_erased_paths` — which also covers `quote`, `join`,
    /// `len`, and the numeric casts, whose output is NOT that text — an
    /// equality on such a value projects its literal back through the
    /// `toString` preimage.
    pub(crate) stringified_paths: BTreeSet<ValuesPath>,
    /// Paths whose value was replaced by derived text in this expression
    /// (`printf`, `quote`, `trunc`, `b64enc`, …): later transform stages
    /// operate on that text, so they claim nothing about the raw path.
    pub(crate) derived_text_paths: BTreeSet<ValuesPath>,
    /// Paths consumed as a DIRECT operand of a Sprig `merge` family call in
    /// this expression. The operand's strict map contract rides its own fail
    /// implication (keyed on the call's live gate), so the operand's splice
    /// row cannot itself reject a Helm-falsy value and the base falsy escape
    /// survives it. Only operands that ARE a path identity are recorded;
    /// constructed containers referencing a path abstain.
    pub(crate) merge_operand_paths: BTreeSet<ValuesPath>,
    /// Literal keys an `omit` in this expression removed from the map at
    /// each path: whole-map sink typing must not bind those members
    /// (external-secrets' `OpenShift` `adaptSecurityContext` omit).
    pub(crate) omitted_map_keys: BTreeMap<ValuesPath, BTreeSet<String>>,
    /// Range keys converted to text by an earlier pipeline stage.
    pub(crate) derived_range_key_paths: BTreeSet<ValuesPath>,
    /// Paths whose rendered text in THIS expression is `tpl`'s render of the
    /// raw value. `tpl` is the identity on template-ACTION-free input, so the
    /// sink's LEXICAL language still projects back onto the raw value (modulo
    /// values carrying `{{`) even though the semantic constraints observed on
    /// the render do not — those belong to the program's output, which is why
    /// the same paths are `derived_text_paths`.
    pub(crate) templated_text_identity_paths: BTreeSet<ValuesPath>,
    /// Paths transformed only by ASCII case mapping in THIS expression.
    /// Case mapping preserves every character that can structurally end a
    /// plain YAML token, so a plain-slot sink still projects that lexical
    /// language even though the transform independently requires a string.
    pub(crate) plain_text_preserving_paths: BTreeSet<ValuesPath>,
    /// Paths substituted by a `%s` that opens a complete literal `printf`
    /// result. A plain-slot sink requires the selected raw arm to be a
    /// present, structurally safe string; other placements remain total.
    pub(crate) plain_slot_string_format_paths: BTreeSet<ValuesPath>,
    /// Range-key paths whose rendered text in THIS expression still carries
    /// the raw key's token-ending characters: a `replace` whose token and
    /// replacement cannot introduce or remove one leaves the unquoted-slot
    /// language projectable back onto the collection's keys (crossplane's
    /// `replace "." "_"` over ranged env var keys).
    pub(crate) plain_text_range_key_paths: BTreeSet<ValuesPath>,
    pub(crate) chart_default_paths: BTreeSet<ValuesPath>,
    pub(crate) local_default_paths: BTreeSet<ValuesPath>,
    pub(crate) local_output_meta: BTreeMap<ValuesPath, HelperOutputMeta>,
    /// Shallow (non-descending) `.Values` source paths of locals that were
    /// read by the expression. Guard-path seeding and expression path
    /// resolution consume this; output rows ride the value itself.
    pub(crate) local_source_paths: BTreeSet<ValuesPath>,
    pub(crate) local_set_mutations: BTreeMap<String, BTreeMap<String, AbstractValue>>,
    /// Literal root-context fields replaced by structural `set` calls.
    pub(crate) root_set_mutations: BTreeMap<String, AbstractValue>,
    /// Root-field truth predicates already decoded inside called helpers.
    pub(crate) root_set_predicates: BTreeMap<String, helm_schema_core::Predicate>,
    /// Root-field value dispatches already joined inside called helpers.
    pub(crate) root_set_value_dispatches: BTreeMap<String, ScalarValueDispatch>,
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
    pub(crate) helper_suppressed_paths: BTreeSet<ValuesPath>,
    /// Captures of called helpers that hold only where the called body's
    /// rendered TEXT is consumed as YAML. Ordinary absorption ignores them:
    /// only a site that certified its own sink records them.
    pub(crate) helper_text_captures: BTreeSet<FailCapture>,
    /// Object-producing mutations that have executed before later member
    /// reads. Their outer predicates remain attached so only accesses that
    /// imply the mutation's execution may accept the converted input kind.
    pub(crate) member_host_conversions: BTreeSet<MemberHostConversion>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MemberHostConversion {
    pub(crate) path: ValuesPath,
    pub(crate) input_kind: String,
    pub(crate) outer_predicates: helm_schema_core::Conjunction,
}

/// One captured `fail` call: the predicate conjunction reaching it. Raw
/// predicates, not [`helm_schema_core::GuardDnf`]: the DNF conversion drops
/// conjuncts it cannot represent, which is safe for row conditions (wider
/// arms) but unsound for fail NEGATION. Enclosing conditions whose lowering
/// was APPROXIMATE (truthy fallbacks, dropped conjuncts) appear in the
/// conjunction as [`helm_schema_core::Predicate::Approximate`] conjuncts,
/// so the negation can abstain instead of manufacturing requirements the
/// chart never stated.
#[derive(Clone, Debug)]
pub(crate) struct FailCapture {
    pub(crate) conjunction: helm_schema_core::Conjunction,
    /// The range facts active at the capture site. Input identity says which
    /// path the header actually iterates; member identity says which path
    /// supplies the values-backed members of a possibly derived iterable.
    pub(crate) ranged: crate::range_modes::RangeModes,
    pub(crate) kind: CaptureKind,
}

impl PartialEq for FailCapture {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for FailCapture {}

impl PartialOrd for FailCapture {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FailCapture {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.conjunction
            .cmp(&other.conjunction)
            .then_with(|| self.ranged.cmp(&other.ranged))
            .then_with(|| match (&self.kind, &other.kind) {
                (
                    CaptureKind::StringRequirement {
                        path: left_path,
                        route: left_route,
                        selection: left_selection,
                    },
                    CaptureKind::StringRequirement {
                        path: right_path,
                        route: right_route,
                        selection: right_selection,
                    },
                ) => left_path
                    .cmp(right_path)
                    .then_with(|| left_route.cmp(right_route))
                    .then_with(|| left_selection.cmp(right_selection)),
                _ => self.kind.cmp(&other.kind),
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StringRequirementRoute {
    /// A direct consumer whose row already owns its raw string contract.
    Direct,
    /// A direct consumer lowered under ambient execution predicates.
    Scoped,
    /// One selected raw input arm of a composed expression.
    Selected,
    /// A certified serializer rendered the raw value before this consumer.
    /// The consumer constrains that text, not the original value's kind.
    Serialized,
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
    RangeKeyStrings { paths: BTreeSet<ValuesPath> },
    /// Every member of the named collection paths reaches a strict runtime
    /// consumer with the given JSON kind; a pattern additionally binds each
    /// member to a parser's lexical domain (genSignedCert's ip list).
    CollectionItems {
        paths: BTreeSet<ValuesPath>,
        schema_type: String,
        pattern: Option<String>,
    },
    /// A literal zero-based `index` executes on this source path.
    IndexAccess { path: ValuesPath, index: usize },
    /// A literal index executes on a list produced by splitting source text.
    SplitIndexAccess {
        paths: BTreeSet<ValuesPath>,
        separator: String,
        index: usize,
        total_text_preimage: bool,
    },
    /// A scalar path must have the named JSON Schema type whenever the
    /// capture's execution predicates hold. `null_aborts` distinguishes a
    /// strict Go parameter from a selected input whose null arm never reaches
    /// that parameter.
    ValueType {
        path: ValuesPath,
        schema_type: String,
        null_aborts: bool,
    },
    /// A strict string consumer with its exact selection conjunction.
    /// `route` distinguishes direct raw consumption, selected raw
    /// consumption, and consumption after a certified serializer.
    StringRequirement {
        path: ValuesPath,
        route: StringRequirementRoute,
        selection: helm_schema_core::Conjunction,
    },
    /// A range header iterates this values path itself, or a wildcard path
    /// identifies the values-backed member alternative supplied to a
    /// derived iterable. The header establishes that input's iterable
    /// domain independently of whether its body renders any rows.
    RangeInput {
        path: ValuesPath,
        destructured: bool,
        json_decoded: bool,
    },
    /// A range header iterates the first-truthy selection of the ordered
    /// identity candidates in `chain`, and `path` is the candidate this
    /// capture claims: it must be iterable exactly where the conjunction's
    /// selection conjuncts pick it. `chain` lets the lowering strip the
    /// caller's conjunctive with-marker stamp for those paths — the
    /// header's disjunctive condition was approximated by per-path `With`
    /// markers, which the exact selection conjuncts refine (a `¬truthy`
    /// prior would otherwise contradict its own marker and the claim could
    /// never fire).
    RangeSelection {
        path: ValuesPath,
        chain: Vec<ValuesPath>,
        allow_integer: bool,
    },
    /// A `dig` SUBJECT step: whenever the capture's execution predicates
    /// hold, the path must be an object even when explicitly null — Sprig
    /// type-asserts the dict before any nil handling, so a null aborts
    /// while absence stays open (the conjunction carries the strict
    /// presence guard).
    DigSubject { path: ValuesPath },
    /// A `dig` SUBJECT must additionally be PRESENT: the type assertion
    /// runs before any missing-key handling, so an absent subject reads as
    /// nil and aborts exactly like an explicit null (loki's null-deleted
    /// `storage_config`). Lowers to a `HasMember` presence requirement on
    /// the parent path.
    RequiredPresence { path: ValuesPath },
    /// Rendering ABORTS wherever the capture's execution predicates hold and
    /// this path is absent: a nil-strict string consumer reads it (helm
    /// answers `wrong type for value; expected string; got interface {}`
    /// for `tpl`, `b64enc`, `trim`, `nindent`, … on a nil operand).
    /// Lowers to a document-level terminal clause, which — unlike a parent
    /// member requirement — reaches a top-level path and carries the
    /// `Absent` guard's ownership semantics.
    AbsenceAborts { path: ValuesPath },
    /// A comparison operand must have the named JSON Schema type when
    /// PRESENT and non-null; `eq`/`ne` compare `nil` against anything.
    ComparableKind {
        path: ValuesPath,
        schema_type: String,
    },
    /// A string path must match the pattern whenever the capture's execution
    /// predicates hold.
    ValuePattern {
        path: ValuesPath,
        pattern: String,
        templated: bool,
    },
    /// A raw splice inside a manually quoted scalar: whenever the capture's
    /// execution predicates hold, every string the path's value contributes
    /// to the rendered token must be valid content for the quoting style.
    QuotedSerialization {
        path: ValuesPath,
        style: helm_schema_core::QuotedScalarStyle,
        templated: bool,
    },
    /// A `%s` substitution opens a plain token, so the raw operand must
    /// format as either string text or a structurally safe mapping.
    PrintfStringOperand { path: ValuesPath },
    /// A raw splice inside an UNQUOTED scalar: the path's own text must keep
    /// the plain token intact (`: `, ` #`, and line breaks end it).
    PlainSlotText {
        path: ValuesPath,
        token_initial: bool,
        templated: bool,
    },
    /// Collections whose range KEY renders raw into an unquoted slot — the
    /// mapping key it becomes (`{{ $key }}:`) or a plain value slot it fills
    /// (`name: {{ $key }}`). The claim binds the collection's KEYS, so it
    /// lowers onto `propertyNames` rather than a member.
    RangeKeyPlainSlot { paths: BTreeSet<ValuesPath> },
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
    pub(crate) fn sole_value_path(&self) -> Option<&ValuesPath> {
        match self {
            Self::Fail | Self::MemberAccess { .. } => None,
            Self::RangeKeyStrings { paths }
            | Self::RangeKeyPlainSlot { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                let mut paths = paths.iter();
                match (paths.next(), paths.next()) {
                    (Some(path), None) => Some(path),
                    _ => None,
                }
            }
            Self::IndexAccess { path, .. }
            | Self::ValueType { path, .. }
            | Self::StringRequirement { path, .. }
            | Self::RangeInput { path, .. }
            | Self::RangeSelection { path, .. }
            | Self::DigSubject { path }
            | Self::RequiredPresence { path }
            | Self::AbsenceAborts { path }
            | Self::ComparableKind { path, .. }
            | Self::ValuePattern { path, .. }
            | Self::QuotedSerialization { path, .. }
            | Self::PrintfStringOperand { path }
            | Self::PlainSlotText { path, .. } => Some(path),
        }
    }

    /// Rewrite every values path the kind payload carries (dependency
    /// namespacing rebases captures under the subchart's key exactly like
    /// the conjunction's predicate paths).
    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        match self {
            Self::Fail | Self::MemberAccess { .. } => {}
            Self::RangeKeyStrings { paths }
            | Self::RangeKeyPlainSlot { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                *paths = paths.iter().cloned().map(&mut *map).collect();
            }
            Self::IndexAccess { path, .. }
            | Self::ValueType { path, .. }
            | Self::RangeInput { path, .. }
            | Self::DigSubject { path }
            | Self::RequiredPresence { path }
            | Self::AbsenceAborts { path }
            | Self::ComparableKind { path, .. }
            | Self::ValuePattern { path, .. }
            | Self::QuotedSerialization { path, .. }
            | Self::PrintfStringOperand { path }
            | Self::PlainSlotText { path, .. } => {
                *path = map(path.clone());
            }
            Self::StringRequirement {
                path, selection, ..
            } => {
                *path = map(path.clone());
                *selection = selection
                    .iter()
                    .cloned()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect();
            }
            Self::RangeSelection { path, chain, .. } => {
                *path = map(path.clone());
                *chain = chain.iter().cloned().map(&mut *map).collect();
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
            observed_facts,
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            templated_yaml_paths,
            json_serialized_paths,
            encoded_paths,
            helper_observed_shape_erased_paths,
            nil_omitting_paths,
            stringified_paths,
            derived_text_paths,
            merge_operand_paths,
            omitted_map_keys,
            derived_range_key_paths,
            templated_text_identity_paths,
            plain_text_preserving_paths,
            plain_slot_string_format_paths,
            plain_text_range_key_paths,
            chart_default_paths,
            local_default_paths,
            local_output_meta,
            local_source_paths,
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            helper_reads,
            helper_rendered,
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_text_captures,
            member_host_conversions,
        } = other;
        self.output_paths.extend(output_paths);
        self.bound_output_paths.extend(bound_output_paths);
        self.defaults.extend(defaults);
        self.parsed_yaml_input_paths.extend(parsed_yaml_input_paths);
        self.yaml_serialized_paths.extend(yaml_serialized_paths);
        self.templated_yaml_paths.extend(templated_yaml_paths);
        self.json_serialized_paths.extend(json_serialized_paths);
        self.encoded_paths.extend(encoded_paths);
        self.helper_observed_shape_erased_paths
            .extend(helper_observed_shape_erased_paths);
        self.nil_omitting_paths.extend(nil_omitting_paths);
        self.stringified_paths.extend(stringified_paths);
        self.derived_text_paths.extend(derived_text_paths);
        self.merge_operand_paths.extend(merge_operand_paths);
        for (path, keys) in omitted_map_keys {
            self.omitted_map_keys.entry(path).or_default().extend(keys);
        }
        self.derived_range_key_paths.extend(derived_range_key_paths);
        self.templated_text_identity_paths
            .extend(templated_text_identity_paths);
        self.plain_text_preserving_paths
            .extend(plain_text_preserving_paths);
        self.plain_slot_string_format_paths
            .extend(plain_slot_string_format_paths);
        self.plain_text_range_key_paths
            .extend(plain_text_range_key_paths);
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
        for read in helper_reads {
            if !self.helper_reads.contains(&read) {
                self.helper_reads.push(read);
            }
        }
        append_unique_rendered_rows(&mut self.helper_rendered, helper_rendered);
        append_unique_rendered_rows(
            &mut self.helper_dependency_rendered,
            helper_dependency_rendered,
        );
        self.helper_suppressed_paths.extend(helper_suppressed_paths);
        self.helper_text_captures.extend(helper_text_captures);
        self.member_host_conversions.extend(member_host_conversions);
        self.observed_facts.absorb(&observed_facts);
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
            observed_facts,
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            // Describes returned YAML text, not evaluation of an ignored argument.
            templated_yaml_paths: _,
            json_serialized_paths,
            encoded_paths,
            helper_observed_shape_erased_paths,
            nil_omitting_paths,
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
            templated_text_identity_paths,
            plain_text_preserving_paths: _,
            plain_slot_string_format_paths: _,
            plain_text_range_key_paths,
            chart_default_paths,
            local_default_paths: _,
            local_output_meta: _,
            local_source_paths: _,
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            helper_reads,
            helper_rendered,
            helper_dependency_rendered,
            helper_suppressed_paths,
            // Describes the text the argument RENDERS, which never reaches a
            // sink of its own: whatever the callee does with it decides the
            // language, and only a certifying sink may record the claim.
            helper_text_captures: _,
            member_host_conversions,
        } = self;
        let mut helper_dependency_rendered = helper_dependency_rendered;
        append_unique_rendered_rows(&mut helper_dependency_rendered, helper_rendered);
        Self {
            output_paths: BTreeSet::new(),
            bound_output_paths: BTreeSet::new(),
            defaults: BTreeSet::new(),
            observed_facts: observed_facts.execution_only(),
            parsed_yaml_input_paths,
            yaml_serialized_paths,
            templated_yaml_paths: BTreeSet::new(),
            json_serialized_paths,
            encoded_paths,
            helper_observed_shape_erased_paths,
            nil_omitting_paths,
            stringified_paths: BTreeSet::new(),
            derived_text_paths,
            merge_operand_paths: BTreeSet::new(),
            omitted_map_keys: BTreeMap::new(),
            derived_range_key_paths,
            templated_text_identity_paths,
            plain_text_preserving_paths: BTreeSet::new(),
            plain_slot_string_format_paths: BTreeSet::new(),
            plain_text_range_key_paths,
            chart_default_paths,
            local_default_paths: BTreeSet::new(),
            local_output_meta: BTreeMap::new(),
            local_source_paths: BTreeSet::new(),
            local_set_mutations,
            root_set_mutations,
            root_set_predicates,
            root_set_value_dispatches,
            helper_reads,
            helper_rendered: Vec::new(),
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_text_captures: BTreeSet::new(),
            member_host_conversions,
        }
    }

    /// Keep contracts learned while consuming an expression as a predicate,
    /// without treating the predicate's returned value as rendered output.
    pub(crate) fn consumed_as_predicate(self) -> Self {
        let observed_facts = self.observed_facts.clone();
        let mut effects = self.execution_only();
        effects.observed_facts = observed_facts;
        effects
    }

    pub(crate) fn add_default_paths(&mut self, paths: BTreeSet<String>) {
        self.defaults.extend(
            paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .map(|path| ValuesPath::parse(&path)),
        );
    }

    pub(crate) fn add_fallback_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            if !path.trim().is_empty() {
                self.observed_facts.insert_type_hint(
                    HintGrade::FALLBACK,
                    ValuesPath::parse(&path),
                    schema_type,
                );
            }
        }
    }

    pub(crate) fn add_tested_type_hints(&mut self, paths: BTreeSet<String>, schema_type: &str) {
        for path in paths {
            if !path.trim().is_empty() {
                self.observed_facts.insert_type_hint(
                    HintGrade::TESTED,
                    ValuesPath::parse(&path),
                    schema_type,
                );
            }
        }
    }

    pub(crate) fn promote_tested_type_hints(&mut self) {
        self.observed_facts.promote_tested_type_hints();
    }

    pub(crate) fn add_encoded_paths(&mut self, paths: BTreeSet<String>) {
        self.clear_plain_slot_string_format_paths(&paths);
        self.encoded_paths.extend(
            paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .map(|path| ValuesPath::parse(&path)),
        );
    }

    pub(crate) fn clear_plain_slot_string_format_paths(&mut self, paths: &BTreeSet<String>) {
        self.plain_slot_string_format_paths
            .retain(|path| !paths.contains(&path.encode()));
        for path in paths {
            if let Some(meta) = self.local_output_meta.get_mut(&ValuesPath::parse(path)) {
                meta.plain_slot_string_format = false;
            }
        }
        for row in &mut self.helper_rendered {
            if paths.contains(&row.path.encode()) {
                row.meta.plain_slot_string_format = false;
            }
        }
    }

    pub(crate) fn add_shape_erased_paths(&mut self, paths: BTreeSet<String>) {
        self.observed_facts.shape_erased_paths.extend(
            paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .map(|path| ValuesPath::parse(&path)),
        );
    }

    pub(crate) fn output_value_paths(&self) -> BTreeSet<String> {
        let mut paths = self
            .output_paths
            .iter()
            .map(ValuesPath::encode)
            .collect::<BTreeSet<_>>();
        paths.extend(self.local_source_paths.iter().map(ValuesPath::encode));
        paths.extend(self.local_output_meta.keys().map(ValuesPath::encode));
        paths.retain(|path| !path.trim().is_empty());
        paths
    }

    pub(crate) fn default_paths_with_local(&self) -> BTreeSet<ValuesPath> {
        self.defaults
            .iter()
            .chain(&self.local_default_paths)
            .cloned()
            .collect()
    }

    pub(crate) fn merge_local_output_meta<'a>(
        &mut self,
        meta: impl IntoIterator<Item = (&'a ValuesPath, &'a HelperOutputMeta)>,
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
        value: &AbstractValue,
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

fn append_unique_rendered_rows(target: &mut Vec<RenderedRow>, rows: Vec<RenderedRow>) {
    for row in rows {
        if !target.contains(&row) {
            target.push(row);
        }
    }
}

/// Which runtime representation supplies the truthiness used to select an
/// expression result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SelectionTruthSource {
    /// Helm truthiness applies to the raw input value.
    #[default]
    RawInput,
    /// Helm truthiness applies after scalar rendering or formatting.
    RenderedScalar,
}

/// Which polarity of a truth condition selects an expression result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionPolarity {
    Truthy,
    Falsy,
}

/// Static knowledge about whether one evaluated value supplies an
/// expression's output.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectionState {
    Always,
    Never,
    Exact(helm_schema_core::Predicate),
    /// The exact selection condition is unknown. A sound subset, when
    /// present, may prove selected states but cannot be negated.
    Approximate {
        sound_subset: Option<helm_schema_core::Predicate>,
    },
}

/// Output reachability carried independently from eager evaluation effects.
///
/// [`EvalResult::effects`] remains populated even for [`SelectionState::Never`]:
/// Go-template arguments are evaluated before their result is discarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectionReachability {
    state: SelectionState,
    truth_source: SelectionTruthSource,
}

/// Reachability for both outcomes of one decoded truth condition.
///
/// Partial truth analysis may prove disjoint subsets for the truthy and falsy
/// outcomes. Keeping two instances of the same carrier preserves both proofs
/// without making an approximate subset invertible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectionTruthReachability {
    when_true: SelectionReachability,
    when_false: SelectionReachability,
}

impl SelectionTruthReachability {
    pub(crate) fn from_condition(
        condition: &TruthCondition,
        truth_source: SelectionTruthSource,
    ) -> Self {
        Self {
            when_true: SelectionReachability::from((
                condition,
                SelectionPolarity::Truthy,
                truth_source,
            )),
            when_false: SelectionReachability::from((
                condition,
                SelectionPolarity::Falsy,
                truth_source,
            )),
        }
    }

    pub(crate) fn exact(
        predicate: helm_schema_core::Predicate,
        truth_source: SelectionTruthSource,
    ) -> Self {
        let when_true = SelectionReachability::exact(predicate, truth_source);
        let when_false = when_true.complement();
        Self {
            when_true,
            when_false,
        }
    }

    pub(crate) fn unknown(truth_source: SelectionTruthSource) -> Self {
        Self {
            when_true: SelectionReachability::approximate(None, truth_source),
            when_false: SelectionReachability::approximate(None, truth_source),
        }
    }

    pub(crate) fn when_true(&self) -> &SelectionReachability {
        &self.when_true
    }

    pub(crate) fn truth_condition(&self) -> TruthCondition {
        if let Some(predicate) = self.when_true.exact_predicate() {
            return TruthCondition::exact(predicate);
        }
        TruthCondition::from_subsets(
            self.when_true.proven_selected_subset(),
            self.when_false.proven_selected_subset(),
            false,
        )
    }
}

impl SelectionReachability {
    pub(crate) const fn always(truth_source: SelectionTruthSource) -> Self {
        Self {
            state: SelectionState::Always,
            truth_source,
        }
    }

    pub(crate) const fn never(truth_source: SelectionTruthSource) -> Self {
        Self {
            state: SelectionState::Never,
            truth_source,
        }
    }

    pub(crate) fn exact(
        predicate: helm_schema_core::Predicate,
        truth_source: SelectionTruthSource,
    ) -> Self {
        if predicate.contains_approximation() {
            let condition = TruthCondition::exact(predicate);
            return Self::from((&condition, SelectionPolarity::Truthy, truth_source));
        }
        let state = match predicate.kind() {
            helm_schema_core::PredicateKind::True => SelectionState::Always,
            helm_schema_core::PredicateKind::False => SelectionState::Never,
            _ => SelectionState::Exact(predicate),
        };
        Self {
            state,
            truth_source,
        }
    }

    pub(crate) fn approximate(
        sound_subset: Option<helm_schema_core::Predicate>,
        truth_source: SelectionTruthSource,
    ) -> Self {
        if sound_subset.as_ref() == Some(&helm_schema_core::Predicate::True) {
            return Self::always(truth_source);
        }
        let sound_subset = sound_subset.filter(|predicate| {
            !matches!(predicate.kind(), helm_schema_core::PredicateKind::False)
                && !predicate.contains_approximation()
        });
        Self {
            state: SelectionState::Approximate { sound_subset },
            truth_source,
        }
    }

    pub(crate) fn complement(&self) -> Self {
        match &self.state {
            SelectionState::Always => Self::never(self.truth_source),
            SelectionState::Never => Self::always(self.truth_source),
            SelectionState::Exact(predicate) => Self::exact(
                TruthCondition::exact(predicate.clone()).when_false(),
                self.truth_source,
            ),
            SelectionState::Approximate { .. } => Self::approximate(None, self.truth_source),
        }
    }

    pub(crate) const fn is_always(&self) -> bool {
        matches!(self.state, SelectionState::Always)
    }

    pub(crate) const fn is_never(&self) -> bool {
        matches!(self.state, SelectionState::Never)
    }

    pub(crate) const fn has_proven_selection_condition(&self) -> bool {
        matches!(
            self.state,
            SelectionState::Always | SelectionState::Exact(_)
        )
    }

    pub(crate) fn exact_predicate(&self) -> Option<helm_schema_core::Predicate> {
        match &self.state {
            SelectionState::Always => Some(helm_schema_core::Predicate::True),
            SelectionState::Never => Some(helm_schema_core::Predicate::False),
            SelectionState::Exact(predicate) => Some(predicate.clone()),
            SelectionState::Approximate { .. } => None,
        }
    }

    pub(crate) fn proven_selected_subset(&self) -> helm_schema_core::Predicate {
        match &self.state {
            SelectionState::Always => helm_schema_core::Predicate::True,
            SelectionState::Never => helm_schema_core::Predicate::False,
            SelectionState::Exact(predicate) => predicate.clone(),
            SelectionState::Approximate { sound_subset } => sound_subset
                .clone()
                .unwrap_or(helm_schema_core::Predicate::False),
        }
    }

    pub(crate) fn conjoin_predicates(
        &self,
        predicates: impl IntoIterator<Item = helm_schema_core::Predicate>,
    ) -> Self {
        let predicates = predicates.into_iter().collect::<Vec<_>>();
        if predicates.is_empty() {
            return self.clone();
        }
        match &self.state {
            SelectionState::Always => Self::exact(
                helm_schema_core::Predicate::all(predicates),
                self.truth_source,
            ),
            SelectionState::Never => Self::never(self.truth_source),
            SelectionState::Exact(selection) => Self::exact(
                helm_schema_core::Predicate::all(
                    std::iter::once(selection.clone())
                        .chain(predicates)
                        .collect(),
                ),
                self.truth_source,
            ),
            SelectionState::Approximate { sound_subset } => {
                let sound_subset = sound_subset.as_ref().map(|selection| {
                    helm_schema_core::Predicate::all(
                        std::iter::once(selection.clone())
                            .chain(predicates)
                            .collect(),
                    )
                });
                Self::approximate(sound_subset, self.truth_source)
            }
        }
    }

    pub(crate) fn output_selection_predicate(
        &self,
        marker: impl Into<String>,
        mut involved_paths: BTreeSet<String>,
    ) -> helm_schema_core::Predicate {
        match &self.state {
            SelectionState::Always => helm_schema_core::Predicate::True,
            SelectionState::Never => helm_schema_core::Predicate::False,
            SelectionState::Exact(predicate) => predicate.clone(),
            SelectionState::Approximate { sound_subset } => {
                if let Some(sound_subset) = sound_subset {
                    involved_paths.extend(
                        sound_subset
                            .value_paths()
                            .into_iter()
                            .map(|path| path.encode()),
                    );
                }
                helm_schema_core::Predicate::approximate_output_selection(
                    marker,
                    involved_paths,
                    sound_subset
                        .clone()
                        .unwrap_or(helm_schema_core::Predicate::False),
                )
            }
        }
    }

    pub(crate) fn output_selection_conjunction(
        &self,
        marker: impl Into<String>,
        involved_paths: BTreeSet<String>,
    ) -> BTreeSet<helm_schema_core::Predicate> {
        let predicate = self.output_selection_predicate(marker, involved_paths);
        match predicate.kind() {
            helm_schema_core::PredicateKind::True => BTreeSet::new(),
            helm_schema_core::PredicateKind::And(predicates) => {
                predicates.iter().cloned().collect()
            }
            _ => BTreeSet::from([predicate]),
        }
    }

    pub(crate) fn execution_predicate(
        &self,
        marker: impl Into<String>,
        mut involved_paths: BTreeSet<String>,
    ) -> helm_schema_core::Predicate {
        match &self.state {
            SelectionState::Always => helm_schema_core::Predicate::True,
            SelectionState::Never => helm_schema_core::Predicate::False,
            SelectionState::Exact(predicate) => predicate.clone(),
            SelectionState::Approximate { sound_subset } => {
                if let Some(sound_subset) = sound_subset {
                    involved_paths.extend(
                        sound_subset
                            .value_paths()
                            .into_iter()
                            .map(|path| path.encode()),
                    );
                }
                helm_schema_core::Predicate::approximate_with_sound_predicate(
                    marker,
                    involved_paths,
                    sound_subset
                        .clone()
                        .unwrap_or(helm_schema_core::Predicate::False),
                )
            }
        }
    }

    pub(crate) const fn truth_source(&self) -> SelectionTruthSource {
        self.truth_source
    }
}

impl From<(&TruthCondition, SelectionPolarity, SelectionTruthSource)> for SelectionReachability {
    fn from(
        (condition, polarity, truth_source): (
            &TruthCondition,
            SelectionPolarity,
            SelectionTruthSource,
        ),
    ) -> Self {
        if let Some(predicate) = condition.predicate() {
            let selection = Self::exact(predicate.clone(), truth_source);
            return match polarity {
                SelectionPolarity::Truthy => selection,
                SelectionPolarity::Falsy => selection.complement(),
            };
        }
        let sound_subset = match polarity {
            SelectionPolarity::Truthy => condition.when_true(),
            SelectionPolarity::Falsy => condition.when_false(),
        };
        Self::approximate(Some(sound_subset), truth_source)
    }
}

impl From<(&ScalarValueDispatch, SelectionPolarity)> for SelectionReachability {
    fn from((dispatch, polarity): (&ScalarValueDispatch, SelectionPolarity)) -> Self {
        Self::from((
            &dispatch.truth_condition(),
            polarity,
            SelectionTruthSource::RenderedScalar,
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalResult {
    pub(crate) value: Option<AbstractValue>,
    pub(crate) effects: Effects,
    pub(crate) truth: TruthCondition,
    pub(crate) selection_reachability: Option<SelectionReachability>,
    /// Truthiness of the typed payload retained across a JSON encode/decode
    /// round trip. The serialized text has different Helm truthiness.
    pub(crate) json_payload_truth: TruthCondition,
    pub(crate) scalar_dispatch: Option<ScalarValueDispatch>,
    /// Exact scalar values of fields in a statically constructed mapping.
    ///
    /// The mapping's fragment value carries provenance and shape; these
    /// dispatches carry the runtime values that a helper receiving the
    /// mapping observes through its dot-relative fields.
    pub(crate) field_scalar_dispatches: BTreeMap<String, ScalarValueDispatch>,
}

impl EvalResult {
    pub(crate) fn none() -> Self {
        Self::default()
    }

    pub(crate) fn from_value(value: AbstractValue) -> Self {
        let scalar_dispatch = match &value {
            AbstractValue::ValuesPath(path) => Some(ScalarValueDispatch::identity(path.clone())),
            AbstractValue::JsonDecodedPath(path) => {
                Some(ScalarValueDispatch::identity(path.clone()))
            }
            AbstractValue::StringSet(values) if values.len() == 1 => values.first().map(|value| {
                ScalarValueDispatch::constant(helm_schema_core::GuardValue::string(value))
            }),
            _ => None,
        };
        let truth = scalar_dispatch
            .as_ref()
            .map(ScalarValueDispatch::truth_condition)
            .unwrap_or_else(|| truth_for_value(Some(&value)));
        let selection_reachability = scalar_dispatch.as_ref().map(|dispatch| {
            if matches!(
                &value,
                AbstractValue::ValuesPath(_) | AbstractValue::JsonDecodedPath(_)
            ) {
                SelectionReachability::from((
                    &truth,
                    SelectionPolarity::Truthy,
                    SelectionTruthSource::RawInput,
                ))
            } else {
                SelectionReachability::from((dispatch, SelectionPolarity::Truthy))
            }
        });
        Self {
            effects: Effects::from_value(&value),
            value: Some(value),
            truth,
            selection_reachability,
            json_payload_truth: TruthCondition::Unknown,
            scalar_dispatch,
            field_scalar_dispatches: BTreeMap::new(),
        }
    }

    pub(crate) fn with_effects(value: Option<AbstractValue>, mut effects: Effects) -> Self {
        if let Some(value) = &value {
            effects.output_paths.extend(value.paths());
        }
        Self {
            value,
            effects,
            truth: TruthCondition::Unknown,
            selection_reachability: None,
            json_payload_truth: TruthCondition::Unknown,
            scalar_dispatch: None,
            field_scalar_dispatches: BTreeMap::new(),
        }
    }

    pub(crate) fn with_truth(mut self, predicate: helm_schema_core::Predicate) -> Self {
        self.set_truth_condition(
            TruthCondition::exact(predicate),
            SelectionTruthSource::RawInput,
        );
        self
    }

    pub(crate) fn with_scalar_dispatch(mut self, dispatch: ScalarValueDispatch) -> Self {
        self.set_scalar_dispatch(dispatch);
        self
    }

    pub(crate) fn set_truth_condition(
        &mut self,
        truth: TruthCondition,
        truth_source: SelectionTruthSource,
    ) {
        self.selection_reachability = Some(SelectionReachability::from((
            &truth,
            SelectionPolarity::Truthy,
            truth_source,
        )));
        self.truth = truth;
    }

    pub(crate) fn set_scalar_dispatch(&mut self, dispatch: ScalarValueDispatch) {
        self.selection_reachability = Some(SelectionReachability::from((
            &dispatch,
            SelectionPolarity::Truthy,
        )));
        self.truth = dispatch.truth_condition();
        self.scalar_dispatch = Some(dispatch);
    }

    pub(crate) fn output_reachability(&self, polarity: SelectionPolarity) -> SelectionReachability {
        let selected = self.selection_reachability.clone().unwrap_or_else(|| {
            SelectionReachability::approximate(None, SelectionTruthSource::RawInput)
        });
        match polarity {
            SelectionPolarity::Truthy => selected,
            SelectionPolarity::Falsy => selected.complement(),
        }
    }

    pub(crate) fn exact_input_identity(&self) -> Option<String> {
        let path = match self.value.as_ref()? {
            AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => path.encode(),
            AbstractValue::OutputPath(path, meta)
                if meta.is_input_identity() && meta.predicates.is_empty() =>
            {
                path.encode()
            }
            _ => return None,
        };
        let typed_path = ValuesPath::parse(&path);
        (!self.effects.defaults.contains(&typed_path)
            && !self.effects.local_default_paths.contains(&typed_path)
            && !self.effects.derived_text_paths.contains(&typed_path)
            && self
                .effects
                .local_output_meta
                .get(&typed_path)
                .is_none_or(|meta| meta.predicates.is_empty()))
        .then_some(path)
    }
}

fn truth_for_value(value: Option<&AbstractValue>) -> TruthCondition {
    match value {
        Some(AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path)) => {
            TruthCondition::exact(helm_schema_core::Predicate::truthy_path(path.encode()))
        }
        _ => TruthCondition::Unknown,
    }
}
