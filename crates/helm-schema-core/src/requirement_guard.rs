//! Values-decidable decisions exported from executed requirements.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{ConditionalGuard, Predicate, ValuesPath};

/// One reduced Boolean decision; indices zero and one denote false and true.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RequirementDecision {
    /// Values-decidable test selecting the outgoing edge.
    pub test: ConditionalGuard,
    /// Successor when the test holds.
    pub when_true: usize,
    /// Successor when the test does not hold.
    pub when_false: usize,
}

/// A shared acyclic guard whose children precede their parents.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RequirementGuard {
    pub(crate) decisions: Vec<RequirementDecision>,
    pub(crate) root: usize,
    pub(crate) projection_exact: bool,
}

impl RequirementGuard {
    /// Whether universal projection retained the complete execution guard.
    #[must_use]
    pub fn projection_is_exact(&self) -> bool {
        self.projection_exact
    }

    pub(crate) fn check_export_budget(&self, remaining: &mut usize) -> Option<()> {
        *remaining = remaining.checked_sub(self.decisions.len())?;
        let mut depths = vec![0_usize, 0];
        let mut bytes = 0_usize;
        for RequirementDecision {
            test,
            when_true,
            when_false,
        } in &self.decisions
        {
            let depth = depths
                .get(*when_true)?
                .max(depths.get(*when_false)?)
                .checked_add(1)?;
            if depth > 128 {
                return None;
            }
            let cost = test.exact_requirement_encoding_cost()?;
            bytes = bytes.checked_add(cost)?;
            if bytes > 65_536 {
                return None;
            }
            *remaining = remaining.checked_sub(cost / 16 + 1)?;
            depths.push(depth);
        }
        Some(())
    }

    pub(crate) fn excludes_atom_value(&self, atom: &ConditionalGuard, value: bool) -> bool {
        let mut possible = vec![false, true];
        for RequirementDecision {
            test,
            when_true,
            when_false,
        } in &self.decisions
        {
            let Some(yes) = possible.get(*when_true).copied() else {
                return false;
            };
            let Some(no) = possible.get(*when_false).copied() else {
                return false;
            };
            possible.push(if test == atom {
                if value { yes } else { no }
            } else {
                yes || no
            });
        }
        possible.get(self.root).is_some_and(|possible| !possible)
    }

    /// Decisions in child-before-parent order, numbered starting at two.
    #[must_use]
    pub fn decisions(&self) -> &[RequirementDecision] {
        &self.decisions
    }

    /// Entry decision, or the false/true terminal zero/one.
    #[must_use]
    pub fn root(&self) -> usize {
        self.root
    }

    /// All values paths tested by this guard.
    #[must_use]
    pub fn value_paths(&self) -> BTreeSet<ValuesPath> {
        self.decisions
            .iter()
            .flat_map(|node| node.test.value_paths())
            .collect()
    }

    /// Maps every tested path while preserving the decision graph.
    #[must_use]
    pub fn map_value_paths(mut self, map: &mut impl FnMut(ValuesPath) -> ValuesPath) -> Self {
        for node in &mut self.decisions {
            node.test = node.test.clone().map_value_paths(map);
        }
        self
    }

    /// Reconstructs the shared Boolean formula without expanding paths.
    #[must_use]
    pub fn predicate(&self) -> Predicate {
        let mut predicates = vec![Predicate::False, Predicate::True];
        for RequirementDecision {
            test,
            when_true,
            when_false,
        } in &self.decisions
        {
            let test = test.predicate();
            let yes = predicates
                .get(*when_true)
                .cloned()
                .unwrap_or(Predicate::False);
            let no = predicates
                .get(*when_false)
                .cloned()
                .unwrap_or(Predicate::False);
            predicates.push(Predicate::Or(vec![
                Predicate::And(vec![test.clone(), yes]),
                Predicate::And(vec![test.negated(), no]),
            ]));
        }
        predicates
            .get(self.root)
            .cloned()
            .unwrap_or(Predicate::False)
    }
}

/// Identity of one unresolved choice in one interpreter scope.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequirementChoice {
    /// Canonical interpreter scope assigned before BDD ordering.
    pub scope: String,
    /// Canonical ordinal preserving the original runtime decision identity.
    pub decision: u64,
}

/// One input atom of a runtime requirement's Boolean cone.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequirementAtom {
    /// A predicate over the values document.
    Value(crate::Guard),
    /// An unresolved runtime choice, universally eliminated on export.
    Choice(RequirementChoice),
}

impl RequirementAtom {
    /// Charges borrowed atom traversal and payload comparisons before cloning or ordering.
    pub fn charge_inventory(&self, remaining: &mut usize, comparisons: usize) -> Option<()> {
        match self {
            Self::Value(guard) => Self::charge_value_inventory(guard, remaining, comparisons),
            Self::Choice(choice) => AtomInventory {
                remaining,
                comparisons: comparisons.max(1),
            }
            .text(&choice.scope),
        }
    }

    /// Charges a borrowed values atom before its first owned inventory copy.
    pub fn charge_value_inventory(
        guard: &crate::Guard,
        remaining: &mut usize,
        comparisons: usize,
    ) -> Option<()> {
        AtomInventory {
            remaining,
            comparisons: comparisons.max(1),
        }
        .guard(guard, 64)
    }

    /// Charges a borrowed path before cloning or comparing its grouping key.
    pub fn charge_path_inventory(
        path: &ValuesPath,
        remaining: &mut usize,
        comparisons: usize,
    ) -> Option<()> {
        AtomInventory {
            remaining,
            comparisons: comparisons.max(1),
        }
        .path(path)
    }

    /// Charges borrowed text before cloning or comparing its grouping key.
    pub fn charge_text_inventory(
        text: &str,
        remaining: &mut usize,
        comparisons: usize,
    ) -> Option<()> {
        AtomInventory {
            remaining,
            comparisons: comparisons.max(1),
        }
        .text(text)
    }
}

struct AtomInventory<'a> {
    remaining: &'a mut usize,
    comparisons: usize,
}

impl AtomInventory<'_> {
    fn charge(&mut self, units: usize) -> Option<()> {
        *self.remaining = self
            .remaining
            .checked_sub(units.checked_mul(self.comparisons)?)?;
        Some(())
    }

    fn text(&mut self, text: &str) -> Option<()> {
        self.charge(1)?;
        if text.len() > 8192 {
            return None;
        }
        self.charge(text.len().div_ceil(16))
    }

    fn path(&mut self, path: &ValuesPath) -> Option<()> {
        self.charge(1)?;
        for (index, segment) in path.segments().enumerate() {
            self.charge(1)?;
            if index >= 64 {
                return None;
            }
            if let Some(literal) = segment.literal() {
                self.text(literal)?;
            }
        }
        Some(())
    }

    fn value(&mut self, value: &crate::GuardValue) -> Option<()> {
        self.charge(1)?;
        match value {
            crate::GuardValue::String(text) | crate::GuardValue::Float(text) => self.text(text),
            crate::GuardValue::Bool(_) | crate::GuardValue::Int(_) | crate::GuardValue::Null => {
                Some(())
            }
        }
    }

    fn guard(&mut self, guard: &crate::Guard, depth: usize) -> Option<()> {
        use crate::Guard;
        self.charge(1)?;
        let depth = depth.checked_sub(1)?;
        match guard {
            Guard::Truthy { path }
            | Guard::Not { path }
            | Guard::Absent { path }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path }
            | Guard::AtMostOneMember { path }
            | Guard::IntGt { path, bound: _ }
            | Guard::IntLt { path, bound: _ }
            | Guard::MinMembers { path, bound: _ } => self.path(path),
            Guard::Eq { path, value }
            | Guard::NotEq { path, value }
            | Guard::ContainsEquals { path, value } => {
                self.path(path)?;
                self.value(value)
            }
            Guard::MatchesPattern {
                path,
                pattern,
                templated: _,
            }
            | Guard::NotMatchesPattern { path, pattern }
            | Guard::RangeKeyMatches { path, pattern }
            | Guard::CollectionPattern {
                path,
                pattern,
                domain: _,
                quantifier: _,
            } => {
                self.path(path)?;
                self.text(pattern)
            }
            Guard::RangeKeyPrefix { path, prefix: text }
            | Guard::RangeKeyEquals { path, key: text }
            | Guard::TypeIs {
                path,
                schema_type: text,
            }
            | Guard::NotTypeIs {
                path,
                schema_type: text,
            }
            | Guard::HasKey { path, key: text }
            | Guard::NotHasKey { path, key: text }
            | Guard::ContainsTruthyMember { path, member: text } => {
                self.path(path)?;
                self.text(text)
            }
            Guard::ContainsMemberEquals {
                path,
                member,
                value,
            } => {
                self.path(path)?;
                self.text(member)?;
                self.value(value)
            }
            Guard::Or { paths } => paths.iter().try_for_each(|path| self.path(path)),
            Guard::AnyOf { alternatives } => alternatives.iter().try_for_each(|guards| {
                self.charge(1)?;
                guards.iter().try_for_each(|guard| self.guard(guard, depth))
            }),
            Guard::MatchesValue { path, pattern } => {
                self.path(path)?;
                self.pattern(pattern, depth)
            }
        }
    }

    fn pattern(&mut self, pattern: &crate::ValuePattern, depth: usize) -> Option<()> {
        use crate::ValuePattern;
        self.charge(1)?;
        let depth = depth.checked_sub(1)?;
        match pattern {
            ValuePattern::Any
            | ValuePattern::Scalar
            | ValuePattern::TemplateFree
            | ValuePattern::TemplateActionFree
            | ValuePattern::MarkerFree
            | ValuePattern::YamlFlowStable
            | ValuePattern::StringMap => Some(()),
            ValuePattern::String { pattern } => self.text(pattern),
            ValuePattern::Array { prefix, items } => {
                prefix
                    .iter()
                    .try_for_each(|pattern| self.pattern(pattern, depth))?;
                if let Some(pattern) = items {
                    self.pattern(pattern, depth)?;
                }
                Some(())
            }
            ValuePattern::Object { keys, values } => {
                self.pattern(keys, depth)?;
                self.pattern(values, depth)
            }
            ValuePattern::Not(pattern) | ValuePattern::Contains(pattern) => {
                self.pattern(pattern, depth)
            }
            ValuePattern::AllOf(patterns) | ValuePattern::AnyOf(patterns) => patterns
                .iter()
                .try_for_each(|pattern| self.pattern(pattern, depth)),
        }
    }
}

/// Opaque node handle in a requirement's ordered BDD.
#[derive(Clone, Copy, Debug)]
pub struct RequirementNode(pub(crate) usize);
