//! Projection: value uses read off the fragment tree.
//!
//! A value use is one `(values_path, yaml_path, guards)` claim: splices and
//! taint attribute at their tree position with the root-to-leaf conditions
//! lowered to contract guards; pathless reads (conditions, assignment
//! right-hand sides, helper-internal guard reads) carry the guards recorded
//! at their read site. This is the projection the differential harness
//! compares against the current pipeline's `ContractUse` rows, and the
//! shape that will feed the existing emission terminal at cutover.

use crate::{Guard, ValueKind, YamlPath};
use helm_schema_core::{Predicate, sequence_item_path};

use super::domain::{AbstractFragment, AbstractString, EntryKey, Guarded, Splice, StringPart};
use super::eval::EvaluatedDocument;

/// One projected value use.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FragmentValueUse {
    /// The dotted `.Values` path.
    pub values_path: String,
    /// The rendered YAML path this use lands on (empty for pathless reads).
    pub yaml_path: YamlPath,
    /// Root-to-leaf guards, lowered from the path conditions (plus a
    /// `Guard::Default` for defaulted splices), in push order and not yet
    /// canonicalized.
    pub guards: Vec<Guard>,
    /// Whether the use renders a whole scalar, part of a scalar, or a YAML
    /// fragment.
    pub kind: ValueKind,
}

/// Project every value use out of an evaluated document.
#[must_use]
pub fn document_value_uses(document: &EvaluatedDocument) -> Vec<FragmentValueUse> {
    let mut out = Vec::new();
    let mut conditions = Vec::new();
    walk_guarded(
        &document.root,
        &YamlPath(Vec::new()),
        &mut conditions,
        &mut out,
    );
    for read in &document.reads {
        out.push(FragmentValueUse {
            values_path: read.values_path.clone(),
            yaml_path: YamlPath(Vec::new()),
            guards: read.guards.clone(),
            kind: ValueKind::Scalar,
        });
    }
    out
}

fn walk_guarded(
    guarded: &Guarded<AbstractFragment>,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    out: &mut Vec<FragmentValueUse>,
) {
    for (condition, node) in &guarded.arms {
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        walk_node(node, path, conditions, out);
        if pushed {
            conditions.pop();
        }
    }
}

fn walk_node(
    node: &AbstractFragment,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    out: &mut Vec<FragmentValueUse>,
) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mapping.entries {
                match &entry.key {
                    EntryKey::Literal(key) if !key.is_empty() => {
                        let mut child = path.clone();
                        child.0.push(key.clone());
                        walk_guarded(&entry.value, &child, conditions, out);
                    }
                    EntryKey::Literal(_) => {
                        walk_guarded(&entry.value, path, conditions, out);
                    }
                    EntryKey::Dynamic(_) => {
                        // Templated keys: the key's reads were recorded at
                        // the eval site (where range/branch predicates were
                        // still ambient); the value attributes at the parent
                        // path without an invented segment.
                        walk_guarded(&entry.value, path, conditions, out);
                    }
                }
            }
        }
        AbstractFragment::Sequence(sequence) => {
            let item_path = sequence_item_path(path);
            for item in &sequence.items {
                walk_guarded(item, &item_path, conditions, out);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            // Render-suppressed blobs (block scalar bodies) influence their
            // text without sink-typing the document position.
            let effective_path = if scalar.suppressed {
                YamlPath(Vec::new())
            } else {
                path.clone()
            };
            project_parts(scalar, &effective_path, conditions, out);
        }
        AbstractFragment::Splice(splice) => {
            out.push(splice_use(splice, path, conditions));
        }
        AbstractFragment::Opaque(opaque) => {
            for taint_path in &opaque.taint {
                if taint_path.is_empty() {
                    continue;
                }
                out.push(FragmentValueUse {
                    values_path: taint_path.clone(),
                    yaml_path: path.clone(),
                    guards: Predicate::contract_guard_stack(conditions),
                    kind: ValueKind::Scalar,
                });
            }
        }
    }
}

fn project_parts(
    scalar: &AbstractString,
    path: &YamlPath,
    conditions: &[Predicate],
    out: &mut Vec<FragmentValueUse>,
) {
    for part in &scalar.parts {
        match part {
            StringPart::Text(_) => {}
            StringPart::Splice(splice) => out.push(splice_use(splice, path, conditions)),
            StringPart::Taint(paths) => {
                for taint_path in paths {
                    if taint_path.is_empty() {
                        continue;
                    }
                    out.push(FragmentValueUse {
                        values_path: taint_path.clone(),
                        yaml_path: path.clone(),
                        guards: Predicate::contract_guard_stack(conditions),
                        kind: ValueKind::Scalar,
                    });
                }
            }
        }
    }
}

fn splice_use(splice: &Splice, path: &YamlPath, conditions: &[Predicate]) -> FragmentValueUse {
    let mut guards = Predicate::contract_guard_stack(conditions);
    if splice.meta.defaulted {
        let default_guard = Guard::Default {
            path: splice.values_path.clone(),
        };
        if !guards.contains(&default_guard) {
            guards.push(default_guard);
        }
    }
    FragmentValueUse {
        values_path: splice.values_path.clone(),
        yaml_path: path.clone(),
        guards,
        kind: splice.kind,
    }
}
