use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_eval::ValueRead;
use crate::helper_meta::{HelperOutputMeta, RenderedRow, insert_type_hint};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};

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
    /// Paths serialized to YAML and then evaluated by `tpl`. Their
    /// collection shape survives, but template-bearing string leaves are
    /// programs whose rendered values reach the sink.
    pub(crate) templated_yaml_paths: BTreeSet<String>,
    pub(crate) json_serialized_paths: BTreeSet<String>,
    pub(crate) encoded_paths: BTreeSet<String>,
    pub(crate) shape_erased_paths: BTreeSet<String>,
    /// Total stringifications observed somewhere inside called helper
    /// bodies. They are execution facts for the caller's aggregate contract,
    /// not transformations of every returned occurrence of the same path.
    pub(crate) helper_observed_shape_erased_paths: BTreeSet<String>,
    /// Paths rendered through Sprig `quote`/`squote` in this expression:
    /// unlike every other total stringification, those SKIP nil operands
    /// entirely, so a missing or null source renders an explicit YAML
    /// null into the sink (traefik's `mountPath: {{ … | quote }}`).
    pub(crate) nil_omitting_paths: BTreeSet<String>,
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
    /// (external-secrets' `OpenShift` `adaptSecurityContext` omit).
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
    /// Paths a nil-strict string consumer in THIS expression read as their
    /// whole operand — the operand IS that values path, not a derivation
    /// carrying it (`printf … | trimSuffix`, an `include`'s text, a
    /// `default` chain). Only these may claim abort-grade PRESENCE: a
    /// derived operand renders whatever its derivation produced, so the
    /// path's own absence is not what the consumer reads.
    pub(crate) nil_strict_identity_paths: BTreeSet<String>,
    /// Paths whose rendered text in THIS expression is `tpl`'s render of the
    /// raw value. `tpl` is the identity on template-ACTION-free input, so the
    /// sink's LEXICAL language still projects back onto the raw value (modulo
    /// values carrying `{{`) even though the semantic constraints observed on
    /// the render do not — those belong to the program's output, which is why
    /// the same paths are `derived_text_paths`.
    pub(crate) templated_text_identity_paths: BTreeSet<String>,
    /// Paths transformed only by ASCII case mapping in THIS expression.
    /// Case mapping preserves every character that can structurally end a
    /// plain YAML token, so a plain-slot sink still projects that lexical
    /// language even though the transform independently requires a string.
    pub(crate) plain_text_preserving_paths: BTreeSet<String>,
    /// Paths substituted by a `%s` that opens a complete literal `printf`
    /// result. A plain-slot sink requires the selected raw arm to be a
    /// present, structurally safe string; other placements remain total.
    pub(crate) plain_slot_string_format_paths: BTreeSet<String>,
    /// Range-key paths whose rendered text in THIS expression still carries
    /// the raw key's token-ending characters: a `replace` whose token and
    /// replacement cannot introduce or remove one leaves the unquoted-slot
    /// language projectable back onto the collection's keys (crossplane's
    /// `replace "." "_"` over ranged env var keys).
    pub(crate) plain_text_range_key_paths: BTreeSet<String>,
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
    pub(crate) root_set_value_dispatches: BTreeMap<String, ScalarValueDispatch>,
    /// Chart value subtrees that supply defaults to a replaced effective `.Values` tree.
    pub(crate) values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    /// Values subtrees merged IN PLACE over the values root
    /// (`mustMergeOverwrite $.Values .Values.pilot`): members written under
    /// the prefix overwrite their effective-root twins for the rest of the
    /// render, so root contracts project back onto the prefixed spellings.
    pub(crate) values_root_overlay_prefixes: BTreeSet<String>,
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
    /// Captures of called helpers that hold only where the called body's
    /// rendered TEXT is consumed as YAML. Ordinary absorption ignores them:
    /// only a site that certified its own sink records them.
    pub(crate) helper_text_fails: Vec<FailCapture>,
    /// Object-producing mutations that have executed before later member
    /// reads. Their outer predicates remain attached so only accesses that
    /// imply the mutation's execution may accept the converted input kind.
    pub(crate) member_host_conversions: BTreeSet<MemberHostConversion>,
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
    /// The range facts active at the capture site. Input identity says which
    /// path the header actually iterates; member identity says which path
    /// supplies the values-backed members of a possibly derived iterable.
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
    /// capture's execution predicates hold. `null_aborts` distinguishes a
    /// strict Go parameter from a selected input whose null arm never reaches
    /// that parameter.
    ValueType {
        path: String,
        schema_type: String,
        null_aborts: bool,
    },
    /// A range header iterates this values path itself, or a wildcard path
    /// identifies the values-backed member alternative supplied to a
    /// derived iterable. The header establishes that input's iterable
    /// domain independently of whether its body renders any rows.
    RangeInput {
        path: String,
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
        path: String,
        chain: Vec<String>,
        allow_integer: bool,
    },
    /// A `dig` SUBJECT step: whenever the capture's execution predicates
    /// hold, the path must be an object even when explicitly null — Sprig
    /// type-asserts the dict before any nil handling, so a null aborts
    /// while absence stays open (the conjunction carries the strict
    /// presence guard).
    DigSubject { path: String },
    /// A `dig` SUBJECT must additionally be PRESENT: the type assertion
    /// runs before any missing-key handling, so an absent subject reads as
    /// nil and aborts exactly like an explicit null (loki's null-deleted
    /// `storage_config`). Lowers to a `HasMember` presence requirement on
    /// the parent path.
    RequiredPresence { path: String },
    /// Rendering ABORTS wherever the capture's execution predicates hold and
    /// this path is absent: a nil-strict string consumer reads it (helm
    /// answers `wrong type for value; expected string; got interface {}`
    /// for `tpl`, `b64enc`, `trim`, `nindent`, … on a nil operand).
    /// Lowers to a document-level terminal clause, which — unlike a parent
    /// member requirement — reaches a top-level path and carries the
    /// `Absent` guard's ownership semantics.
    AbsenceAborts { path: String },
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
        templated: bool,
    },
    /// A `%s` substitution opens a plain token, so the raw operand must
    /// format as either string text or a structurally safe mapping.
    PrintfStringOperand { path: String },
    /// A raw splice inside an UNQUOTED scalar: the path's own text must keep
    /// the plain token intact (`: `, ` #`, and line breaks end it).
    PlainSlotText {
        path: String,
        token_initial: bool,
        templated: bool,
    },
    /// Collections whose range KEY renders raw into an unquoted slot — the
    /// mapping key it becomes (`{{ $key }}:`) or a plain value slot it fills
    /// (`name: {{ $key }}`). The claim binds the collection's KEYS, so it
    /// lowers onto `propertyNames` rather than a member.
    RangeKeyPlainSlot { paths: BTreeSet<String> },
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
    pub(crate) fn sole_value_path(&self) -> Option<&str> {
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
        F: FnMut(&str) -> String,
    {
        match self {
            Self::Fail | Self::MemberAccess { .. } => {}
            Self::RangeKeyStrings { paths }
            | Self::RangeKeyPlainSlot { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                *paths = paths.iter().map(|path| map(path)).collect();
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
                *path = map(path);
            }
            Self::RangeSelection { path, chain, .. } => {
                *path = map(path);
                *chain = chain.iter().map(|path| map(path)).collect();
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

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
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
            templated_yaml_paths,
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
            helper_observed_shape_erased_paths,
            nil_omitting_paths,
            stringified_paths,
            derived_text_paths,
            merge_operand_paths,
            omitted_map_keys,
            derived_range_key_paths,
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            nil_strict_identity_paths,
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
            values_default_sources,
            values_root_overlay_prefixes,
            values_root_helper_includes,
            helper_reads,
            helper_rendered,
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            helper_text_fails,
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
        self.shape_erased_paths.extend(shape_erased_paths);
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
        self.string_contract_paths.extend(string_contract_paths);
        self.range_modes.merge(&range_modes);
        self.direct_string_consumer_paths
            .extend(direct_string_consumer_paths);
        self.nil_strict_identity_paths
            .extend(nil_strict_identity_paths);
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
        self.values_default_sources.extend(values_default_sources);
        self.values_root_overlay_prefixes
            .extend(values_root_overlay_prefixes);
        self.values_root_helper_includes
            .extend(values_root_helper_includes);
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
        for condition in helper_fails {
            if !self.helper_fails.contains(&condition) {
                self.helper_fails.push(condition);
            }
        }
        for condition in helper_text_fails {
            if !self.helper_text_fails.contains(&condition) {
                self.helper_text_fails.push(condition);
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
            // Describes returned YAML text, not evaluation of an ignored argument.
            templated_yaml_paths: _,
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
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
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            nil_strict_identity_paths,
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
            values_default_sources,
            values_root_overlay_prefixes,
            values_root_helper_includes,
            helper_reads,
            helper_rendered,
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            // Describes the text the argument RENDERS, which never reaches a
            // sink of its own: whatever the callee does with it decides the
            // language, and only a certifying sink may record the claim.
            helper_text_fails: _,
            member_host_conversions,
        } = self;
        let mut helper_dependency_rendered = helper_dependency_rendered;
        append_unique_rendered_rows(&mut helper_dependency_rendered, helper_rendered);
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
            templated_yaml_paths: BTreeSet::new(),
            json_serialized_paths,
            encoded_paths,
            shape_erased_paths,
            helper_observed_shape_erased_paths,
            nil_omitting_paths,
            stringified_paths: BTreeSet::new(),
            derived_text_paths,
            merge_operand_paths: BTreeSet::new(),
            omitted_map_keys: BTreeMap::new(),
            derived_range_key_paths,
            string_contract_paths,
            range_modes,
            direct_string_consumer_paths,
            nil_strict_identity_paths,
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
            values_default_sources,
            values_root_overlay_prefixes,
            values_root_helper_includes,
            helper_reads,
            helper_rendered: Vec::new(),
            helper_dependency_rendered,
            helper_suppressed_paths,
            helper_fails,
            helper_text_fails: Vec::new(),
            member_host_conversions,
        }
    }

    /// Keep contracts learned while consuming an expression as a predicate,
    /// without treating the predicate's returned value as rendered output.
    pub(crate) fn consumed_as_predicate(self) -> Self {
        let type_hints = self.type_hints.clone();
        let guarded_type_hints = self.guarded_type_hints.clone();
        let fallback_type_hints = self.fallback_type_hints.clone();
        let tested_type_hints = self.tested_type_hints.clone();
        let mut effects = self.execution_only();
        effects.type_hints = type_hints;
        effects.guarded_type_hints = guarded_type_hints;
        effects.fallback_type_hints = fallback_type_hints;
        effects.tested_type_hints = tested_type_hints;
        effects
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
                insert_type_hint(&mut self.guarded_type_hints, path.clone(), &hint);
            }
        }
    }

    pub(crate) fn add_encoded_paths(&mut self, paths: BTreeSet<String>) {
        self.clear_plain_slot_string_format_paths(&paths);
        self.encoded_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn clear_plain_slot_string_format_paths(&mut self, paths: &BTreeSet<String>) {
        self.plain_slot_string_format_paths
            .retain(|path| !paths.contains(path));
        for path in paths {
            if let Some(meta) = self.local_output_meta.get_mut(path) {
                meta.plain_slot_string_format = false;
            }
        }
        for row in &mut self.helper_rendered {
            if paths.contains(&row.path) {
                row.meta.plain_slot_string_format = false;
            }
        }
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalResult {
    pub(crate) value: Option<AbstractValue>,
    pub(crate) effects: Effects,
    pub(crate) truth: TruthCondition,
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
            AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
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
        Self {
            effects: Effects::from_value(&value),
            value: Some(value),
            truth,
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
            json_payload_truth: TruthCondition::Unknown,
            scalar_dispatch: None,
            field_scalar_dispatches: BTreeMap::new(),
        }
    }

    pub(crate) fn with_truth(mut self, predicate: helm_schema_core::Predicate) -> Self {
        self.truth = TruthCondition::exact(predicate);
        self
    }

    pub(crate) fn with_scalar_dispatch(mut self, dispatch: ScalarValueDispatch) -> Self {
        self.truth = dispatch.truth_condition();
        self.scalar_dispatch = Some(dispatch);
        self
    }
}

fn truth_for_value(value: Option<&AbstractValue>) -> TruthCondition {
    match value {
        Some(AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path)) => {
            TruthCondition::exact(helm_schema_core::Predicate::truthy_path(path.clone()))
        }
        _ => TruthCondition::Unknown,
    }
}
