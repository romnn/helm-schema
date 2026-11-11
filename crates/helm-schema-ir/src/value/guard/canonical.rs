//! The canonical form of runtime guards: one hash-consed, reduced, ordered decision diagram per
//! decision context over its input decisions (seams P2).
//!
//! A runtime guard may test another runtime guard (a join test, a deferred construction, an abort
//! continuation). Its canonical form composes every such test into the diagram of its holder, so
//! the form depends only on which input decisions the guard admits. Two guards are then
//! compatible exactly when the conjunction of their forms reaches true, which is a walk over the
//! two forms instead of a search through the nested tests.
//!
//! Every form and query runs under its own fixed allowance and a memo of its own, so whether it
//! completes depends only on its operands, never on what earlier queries left in the tables.

use ahash::AHashMap;

use super::RuntimeGuard;

/// Steps one form construction or one compatibility walk may take.
const MAX_CANONICAL_WORK: usize = 16_384;
/// Diagram nodes a context keeps before it starts over (answers do not depend on the tables).
const MAX_CANONICAL_NODES: usize = 1 << 22;

const FALSE: u32 = 0;
const TRUE: u32 = 1;

/// One internal node: the input decision it tests (larger decisions nearer the root, the order
/// the decision store builds in) and its two cofactors.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Node {
    decision: u64,
    no: u32,
    yes: u32,
}

#[derive(Default)]
pub(super) struct CanonicalGuards {
    nodes: Vec<Node>,
    unique: AHashMap<Node, u32>,
    /// The form of each runtime guard node by its id; `None` when its allowance ran out.
    forms: AHashMap<u64, Option<u32>>,
}

/// A form read at one outcome: the documents where the guard evaluates to `value`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Polar {
    form: u32,
    value: bool,
}

impl CanonicalGuards {
    fn node(&self, form: u32) -> Option<Node> {
        let index = usize::try_from(form.checked_sub(2)?).ok()?;
        self.nodes.get(index).copied()
    }

    fn top(&self, form: u32) -> Option<u64> {
        self.node(form).map(|node| node.decision)
    }

    fn cofactors(&self, form: u32, decision: u64) -> (u32, u32) {
        match self.node(form) {
            Some(node) if node.decision == decision => (node.no, node.yes),
            Some(_) | None => (form, form),
        }
    }

    fn make(&mut self, decision: u64, no: u32, yes: u32) -> u32 {
        if no == yes {
            return no;
        }
        let node = Node { decision, no, yes };
        if let Some(form) = self.unique.get(&node) {
            return *form;
        }
        let form = u32::try_from(self.nodes.len() + 2).unwrap_or(u32::MAX);
        self.nodes.push(node);
        self.unique.insert(node, form);
        form
    }

    fn ite(
        &mut self,
        [test, yes, no]: [u32; 3],
        memo: &mut AHashMap<[u32; 3], u32>,
        work: &mut usize,
    ) -> Option<u32> {
        match (test, yes, no) {
            (TRUE, _, _) => return Some(yes),
            (FALSE, _, _) => return Some(no),
            _ if yes == no => return Some(yes),
            (_, TRUE, FALSE) => return Some(test),
            _ => {}
        }
        if let Some(form) = memo.get(&[test, yes, no]) {
            return Some(*form);
        }
        *work = work.checked_sub(1)?;
        let decision = [test, yes, no]
            .into_iter()
            .filter_map(|form| self.top(form))
            .max()?;
        let (test_no, test_yes) = self.cofactors(test, decision);
        let (yes_no, yes_yes) = self.cofactors(yes, decision);
        let (no_no, no_yes) = self.cofactors(no, decision);
        let high = self.ite([test_yes, yes_yes, no_yes], memo, work)?;
        let low = self.ite([test_no, yes_no, no_no], memo, work)?;
        let form = self.make(decision, low, high);
        memo.insert([test, yes, no], form);
        Some(form)
    }

    /// Whether some assignment of the input decisions gives `left` and `right` their values.
    fn joint(
        &self,
        left: Polar,
        right: Polar,
        refuted: &mut ahash::AHashSet<(Polar, Polar)>,
        work: &mut usize,
    ) -> Option<bool> {
        // A reduced internal node takes both values.
        for (constant, other) in [(left, right), (right, left)] {
            if constant.form <= TRUE {
                if (constant.form == TRUE) != constant.value {
                    return Some(false);
                }
                return Some(other.form > TRUE || (other.form == TRUE) == other.value);
            }
        }
        if left.form == right.form {
            return Some(left.value == right.value);
        }
        if refuted.contains(&(left, right)) {
            return Some(false);
        }
        *work = work.checked_sub(1)?;
        let decision = self.top(left.form)?.max(self.top(right.form)?);
        let (left_no, left_yes) = self.cofactors(left.form, decision);
        let (right_no, right_yes) = self.cofactors(right.form, decision);
        for (left_form, right_form) in [(left_yes, right_yes), (left_no, right_no)] {
            let found = self.joint(
                Polar {
                    form: left_form,
                    value: left.value,
                },
                Polar {
                    form: right_form,
                    value: right.value,
                },
                refuted,
                work,
            )?;
            if found {
                return Some(true);
            }
        }
        refuted.insert((left, right));
        Some(false)
    }

    /// The canonical form of `guard`, composing every runtime test it holds; `None` when a
    /// node belongs to another context or a construction exhausts its allowance.
    fn form(&mut self, guard: &RuntimeGuard, context: *const super::GuardQueries) -> Option<u32> {
        if self.nodes.len() >= MAX_CANONICAL_NODES {
            self.nodes.clear();
            self.unique.clear();
            self.forms.clear();
        }
        let mut pending = vec![(guard, false)];
        while let Some((guard, expanded)) = pending.pop() {
            let RuntimeGuard::Branch(node) = guard else {
                continue;
            };
            // Node ids repeat across contexts, so the context is checked before the memo.
            if !std::ptr::eq(std::rc::Rc::as_ptr(&node.queries), context) {
                return None;
            }
            if self.forms.contains_key(&node.id) {
                continue;
            }
            let parts = [node.test.runtime.as_ref(), Some(&node.yes), Some(&node.no)];
            if !expanded {
                pending.push((guard, true));
                pending.extend(parts.into_iter().flatten().map(|part| (part, false)));
                continue;
            }
            let form = self.compose(node.test.id, parts);
            self.forms.insert(node.id, form);
        }
        self.known(guard)
    }

    /// The form of a node from its parts' forms: its test (an input decision is its own
    /// variable), then its two arms.
    fn compose(&mut self, decision: u64, parts: [Option<&RuntimeGuard>; 3]) -> Option<u32> {
        let mut forms = [FALSE; 3];
        for (slot, part) in forms.iter_mut().zip(parts) {
            *slot = if let Some(part) = part {
                self.known(part)?
            } else {
                self.make(decision, FALSE, TRUE)
            };
        }
        self.ite(forms, &mut AHashMap::new(), &mut { MAX_CANONICAL_WORK })
    }

    fn known(&self, guard: &RuntimeGuard) -> Option<u32> {
        match guard {
            RuntimeGuard::False => Some(FALSE),
            RuntimeGuard::True => Some(TRUE),
            RuntimeGuard::Branch(node) => self.forms.get(&node.id).copied().flatten(),
        }
    }

    /// Whether `left` can evaluate to `left_value` while `right` evaluates to `right_value`;
    /// `None` when either form or the walk exhausts its allowance.
    pub(super) fn compatible(
        &mut self,
        [(left, left_value), (right, right_value)]: [(&RuntimeGuard, bool); 2],
        context: *const super::GuardQueries,
    ) -> Option<bool> {
        let left = self.form(left, context)?;
        let right = self.form(right, context)?;
        self.joint(
            Polar {
                form: left,
                value: left_value,
            },
            Polar {
                form: right,
                value: right_value,
            },
            &mut ahash::AHashSet::new(),
            &mut { MAX_CANONICAL_WORK },
        )
    }

    /// Whether `guard` can evaluate to `value` while the input decision `atom` takes its arm.
    pub(super) fn possible_atom(
        &mut self,
        guard: &RuntimeGuard,
        atom: &super::DecisionEdgeKey,
        value: bool,
        context: *const super::GuardQueries,
    ) -> Option<bool> {
        let form = self.form(guard, context)?;
        let decision = self.make(atom.decision, FALSE, TRUE);
        self.joint(
            Polar { form, value },
            Polar {
                form: decision,
                value: atom.taken,
            },
            &mut ahash::AHashSet::new(),
            &mut { MAX_CANONICAL_WORK },
        )
    }
}
