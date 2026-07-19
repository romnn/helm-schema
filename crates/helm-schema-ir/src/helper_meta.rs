//! Per-path helper render facts shared between the value lattice and the
//! fragment domain: `OutputPath` values, local bindings, and fragment
//! summaries all carry a [`HelperOutputMeta`] per rendered `.Values` path.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ContractProvenance, ValueKind};
use helm_schema_core::Predicate;

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
    /// Literal tokens whose PRESENCE in the raw string diverts it from this
    /// value: the value equals the raw string exactly when the raw contains
    /// none of them (traefik's `replace "latest-" ""` sentinel stripping and
    /// `(split "@" …)._0` digest trimming). Lexical captures must
    /// exempt raw strings containing any token instead of projecting the
    /// final language onto them.
    pub(crate) lexical_escapes: BTreeSet<String>,
    /// Literal member keys an `omit` removed from this map value on some
    /// path to the render, mapped to the sound RETAIN guards under which
    /// the key certainly survives (external-secrets' OpenShift
    /// `adaptSecurityContext` omit). Empty guards mean survival is
    /// undecidable: the key's sink typing abstains.
    pub(crate) omitted_keys: std::collections::BTreeMap<String, Vec<crate::Guard>>,
}

impl HelperOutputMeta {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        self.shape_erased |= other.shape_erased;
        self.stringified |= other.stringified;
        self.yaml_serialized |= other.yaml_serialized;
        self.derived_text |= other.derived_text;
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

/// Weakens a lexical capture pattern by escape tokens: a raw string
/// containing any token diverged from the observed value before the check
/// ran, so the capture accepts it unconditionally. JSON Schema
/// `pattern` is an unanchored search, so a bare escaped token alternative
/// matches any string containing it.
pub(crate) fn pattern_with_lexical_escapes(pattern: &str, escapes: &BTreeSet<String>) -> String {
    if escapes.is_empty() {
        return pattern.to_string();
    }
    let mut alternatives: Vec<String> = escapes.iter().map(|token| regex_literal(token)).collect();
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
