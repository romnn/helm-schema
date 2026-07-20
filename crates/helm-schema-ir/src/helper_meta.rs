//! Per-path helper render facts shared between the value lattice and the
//! fragment domain: `OutputPath` values, local bindings, and fragment
//! summaries all carry a [`HelperOutputMeta`] per rendered `.Values` path.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ContractProvenance, ValueKind};
use helm_schema_core::{GuardValue, Predicate};

/// A `coalesce` substituted a constant string fallback for a stringified
/// binding's Helm-empty rendering.
///
/// `spellings` are the exact raw values whose final stringification is
/// empty: always the empty string itself, plus any values a preceding
/// `"<nil>" → ""` normalization arm diverted. An equality against exactly
/// the `fallback` literal admits every spelling beside the literal's own
/// `toString` preimage.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EmptyRescue {
    pub(crate) fallback: String,
    pub(crate) spellings: BTreeSet<GuardValue>,
}

/// One lexical divergence between a raw string input and the transformed
/// value a capture observes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LexicalEscape {
    /// The transform is the identity on raw strings NOT containing the
    /// token (`replace TOKEN …`, `(split TOKEN …)._0`): captures exempt
    /// any raw string holding it.
    Contains(String),
    /// `trimPrefix TOKEN` — at most one leading occurrence is stripped
    /// before the capture's check runs, so the accepted raw language is
    /// the capture language optionally prefixed by the token.
    TrimPrefix(String),
    /// `trimSuffix TOKEN` — the suffix mirror of [`Self::TrimPrefix`].
    TrimSuffix(String),
}

/// The facts one rendered path carries out of a helper body: the branch
/// conditions under which it renders (one set per branch), whether the
/// render site substitutes a fallback, and the body sites it was derived
/// through.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    /// The binding's value is a total stringification (`quote`, `toString`,
    /// `join`) of this path, so splices rendering it expose no input shape.
    pub(crate) shape_erased: bool,
    /// The binding's value is the exact Go `%v` rendering of this path
    /// (`toString` over the path identity): an equality on the binding
    /// projects its literal back through the `toString` preimage. A join
    /// with a raw-identity branch keeps the flag — the raw branch's
    /// type-mismatched comparisons abort Helm, so the projected preimage
    /// only widens there.
    pub(crate) stringified: bool,
    /// The binding's value is YAML serialization of this path. Serialization
    /// accepts any input kind, while a sequence placement remains structural.
    pub(crate) yaml_serialized: bool,
    /// The binding's value is derived text of this path (an `include`'s
    /// rendered output, a `printf` result): a consuming transform applied to
    /// the local operates on that text and claims nothing about the path.
    pub(crate) derived_text: bool,
    /// The path rendered as one part of a COMPOSED scalar (literal text
    /// around the splice) in a projected helper value: a structural
    /// re-lowering must keep the partial-text discipline so provider
    /// typing and full-value lexical preimages abstain (traefik's
    /// `--…={{ $value }}` flag items through the pod-template roundtrip).
    /// Unlike [`Self::derived_text`], this never rides ordinary
    /// include-bound locals, whose splices render the picked value's exact
    /// text and stay provider-typable.
    pub(crate) partial_text: bool,
    /// A string-consuming transform bound a runtime string contract on this
    /// path while producing the binding's value: splices rendering it carry
    /// the contract under their own render conditions.
    pub(crate) string_contract: bool,
    /// The path's value was serialized as JSON at this render boundary.
    pub(crate) json_serialized: bool,
    /// The path's runtime identity came from JSON decoding.
    pub(crate) json_decoded: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
    /// Predicate paths this row's derivation explicitly severed (index-call
    /// narrowing): guard reads of their strict ancestors are dropped.
    pub(crate) suppress_predicate_paths: BTreeSet<String>,
    /// Conditions under which this path's RAW value is the consumed operand:
    /// a sibling `if` arm reassigned the binding away (datadog's `latest` →
    /// `1.20.0` sentinel), so strict-operand captures conjoin these
    /// before rejecting raw inputs. Only the capture lanes read them —
    /// guard decoding and row lowering see the joined value choice itself.
    pub(crate) capture_exclusions: BTreeSet<Predicate>,
    /// Lexical divergences between the raw string and this value (see
    /// [`LexicalEscape`]): a `Contains` token exempts raw strings holding
    /// it (traefik's `replace "latest-" ""` sentinel stripping), while a
    /// trim affix projects the capture language through the exact
    /// stripped-affix preimage (datadog's `trimSuffix "-jmx"` tag).
    pub(crate) lexical_escapes: BTreeSet<LexicalEscape>,
    /// Literal member keys an `omit` removed from this map value on some
    /// path to the render, mapped to the sound RETAIN guards under which
    /// the key certainly survives (external-secrets' OpenShift
    /// `adaptSecurityContext` omit). Empty guards mean survival is
    /// undecidable: the key's sink typing abstains.
    pub(crate) omitted_keys: std::collections::BTreeMap<String, Vec<crate::Guard>>,
    /// Raw spellings a sibling branch arm diverted to the EMPTY string
    /// before any fallback selection (the `if eq $x "<nil>" { $x = "" }`
    /// normalization idiom): the diverting arm's exact header equality
    /// literals. `None` means no divert was recorded (or an undecodable
    /// one), so a downstream `coalesce` rescue seeing an empty-literal
    /// alternative must abstain.
    pub(crate) empty_fold_spellings: Option<BTreeSet<GuardValue>>,
    /// A downstream `coalesce` substituted a constant fallback while this
    /// stringified binding rendered Helm-empty (cilium's
    /// `coalesce $stringValueKPR "false"`). Consumed by equality decoding;
    /// see [`EmptyRescue`].
    pub(crate) empty_rescue: Option<EmptyRescue>,
}

impl HelperOutputMeta {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        self.shape_erased |= other.shape_erased;
        self.stringified |= other.stringified;
        self.yaml_serialized |= other.yaml_serialized;
        self.derived_text |= other.derived_text;
        self.partial_text |= other.partial_text;
        self.string_contract |= other.string_contract;
        self.json_serialized |= other.json_serialized;
        self.json_decoded |= other.json_decoded;
        merge_provenance_sites(&mut self.provenance, &other.provenance);
        self.suppress_predicate_paths
            .extend(other.suppress_predicate_paths.iter().cloned());
        self.capture_exclusions
            .extend(other.capture_exclusions.iter().cloned());
        self.lexical_escapes
            .extend(other.lexical_escapes.iter().cloned());
        for (key, retain_guards) in &other.omitted_keys {
            // Conflicting retain guards for one key collapse to abstention:
            // typing may only bind where the key CERTAINLY survives.
            self.omitted_keys
                .entry(key.clone())
                .and_modify(|existing| {
                    if existing != retain_guards {
                        existing.clear();
                    }
                })
                .or_insert_with(|| retain_guards.clone());
        }
        // The empty-rescue facts must stay EXACT: a one-sided fact survives
        // (the fresh `or_default()` entry every meta transfer merges into
        // carries none), but disagreeing recorded facts drop to abstention
        // instead of unioning into a claim neither side made.
        self.empty_fold_spellings = merge_exact_fact(
            self.empty_fold_spellings.take(),
            other.empty_fold_spellings.clone(),
        );
        self.empty_rescue = merge_exact_fact(self.empty_rescue.take(), other.empty_rescue.clone());
    }

    pub(crate) fn suppress_predicate_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !path.is_empty() {
            self.suppress_predicate_paths.insert(path);
        }
    }

    /// Conjoin `predicates` onto every recorded branch (one fresh branch when
    /// none are recorded yet).
    pub(crate) fn conjoin_branches(&mut self, predicates: &BTreeSet<Predicate>) {
        if predicates.is_empty() {
            return;
        }
        if self.predicates.is_empty() {
            self.predicates.insert(predicates.clone());
            return;
        }
        self.predicates = std::mem::take(&mut self.predicates)
            .into_iter()
            .map(|mut branch| {
                branch.extend(predicates.iter().cloned());
                branch
            })
            .collect();
    }
}

/// Merges one optional exact fact: agreement (or one-sidedness) keeps the
/// fact, disagreement drops it.
fn merge_exact_fact<T: PartialEq>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (Some(left), Some(right)) => (left == right).then_some(left),
        (left, right) => left.or(right),
    }
}

/// One rendered claim of a helper call flattened from its summary fragment:
/// the path, its render kind, encoding, and per-path meta. Call sites use
/// these for no-render demotion (assignments and conditions read the paths
/// without rendering them) and for restoring per-path branch meta when
/// transfer functions collapse the value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderedRow {
    pub(crate) path: String,
    pub(crate) kind: ValueKind,
    pub(crate) encoded: bool,
    pub(crate) meta: HelperOutputMeta,
}

/// Merges the meta of every rendered row into a per-source meta map (the
/// shape local bindings carry).
pub(crate) fn merge_rendered_row_meta(
    output_meta: &mut BTreeMap<String, HelperOutputMeta>,
    rows: &[RenderedRow],
) {
    for row in rows {
        output_meta
            .entry(row.path.clone())
            .or_default()
            .merge(&row.meta);
    }
}

/// Appends `extra` provenance sites onto `target`, preserving first-seen
/// order and skipping sites already present. Every provenance merge in the
/// contract pipeline uses this discipline so emitted site lists stay
/// deterministic.
pub(crate) fn merge_provenance_sites(
    target: &mut Vec<ContractProvenance>,
    extra: &[ContractProvenance],
) {
    for site in extra {
        if !target.contains(site) {
            target.push(site.clone());
        }
    }
}

/// Whether two values paths describe related data: same top-level root, or
/// one is an ancestor of the other.
pub(crate) fn values_paths_are_related(left: &str, right: &str) -> bool {
    let left_root = helm_schema_core::split_value_path(left).into_iter().next();
    let right_root = helm_schema_core::split_value_path(right).into_iter().next();
    left_root == right_root
        || helm_schema_core::values_path_is_descendant(left, right)
        || helm_schema_core::values_path_is_descendant(right, left)
}

pub(crate) fn insert_type_hint(
    hints: &mut BTreeMap<String, BTreeSet<String>>,
    path: String,
    schema_type: &str,
) {
    if path.trim().is_empty() {
        return;
    }
    hints
        .entry(path)
        .or_default()
        .insert(schema_type.to_string());
}

/// Projects a lexical capture pattern through its escapes.
///
/// A single trim-affix escape composes EXACTLY: the runtime strips at most
/// one affix occurrence before the check runs, so the raw language is the
/// capture language optionally wearing the affix (`^P$` becomes
/// `^(?:P)(?:-jmx)?$` for `trimSuffix "-jmx"`; a capture-valid string that
/// itself ends in the affix is trimmed first, so admitting it is a bounded
/// widening, never a rejection). `Contains` tokens — and any escape MIX the
/// affix composition cannot order — weaken to exemption alternatives: a raw
/// string containing the token diverged from the observed value before the
/// check ran, so the capture accepts it unconditionally (JSON Schema
/// `pattern` is an unanchored search, so a bare token alternative matches
/// any string containing it).
pub(crate) fn pattern_with_lexical_escapes(
    pattern: &str,
    escapes: &BTreeSet<LexicalEscape>,
) -> String {
    if escapes.is_empty() {
        return pattern.to_string();
    }
    if let [escape] = escapes.iter().collect::<Vec<_>>().as_slice()
        && let Some(anchored) = pattern.strip_prefix('^').and_then(|p| p.strip_suffix('$'))
    {
        match escape {
            LexicalEscape::TrimPrefix(token) => {
                return format!("^(?:{})?(?:{anchored})$", regex_literal(token));
            }
            LexicalEscape::TrimSuffix(token) => {
                return format!("^(?:{anchored})(?:{})?$", regex_literal(token));
            }
            LexicalEscape::Contains(_) => {}
        }
    }
    let mut alternatives: Vec<String> = escapes
        .iter()
        .map(|escape| match escape {
            LexicalEscape::Contains(token)
            | LexicalEscape::TrimPrefix(token)
            | LexicalEscape::TrimSuffix(token) => regex_literal(token),
        })
        .collect();
    alternatives.push(format!("(?:{pattern})"));
    alternatives.join("|")
}

fn regex_literal(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(
            character,
            '.' | '+'
                | '*'
                | '?'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '{'
                | '}'
                | '^'
                | '$'
                | '\\'
                | '/'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
