//! Static evaluation of Helm helpers to literal output candidates.
//!
//! Targeted at apiVersion-shaped helpers that vendored charts use to
//! emit a single literal apiVersion (or a finite if/else set of them)
//! based on `Capabilities.APIVersions.Has` checks. The detector calls
//! this when it sees `apiVersion: {{ template "X" . }}` or
//! `apiVersion: {{ include "X" . }}` in a document header.
//!
//! This is intentionally NOT a general Helm renderer:
//! - only handles `{{ print … }}`, `{{ printf "%s" "X" }}`, bare string
//!   literals, and nested `{{ template/include "Y" . }}` calls;
//! - dives into `if` / `with` branches to collect alternatives;
//! - skips Field / Variable references (returns nothing for those —
//!   the literal-only output set is the contract).
//!
//! Output is typed: `HelperOutput` preserves branch structure for the
//! common `if Capabilities.APIVersions.Has … else …` shape that
//! vendored apiVersion helpers use. Callers that want a flat literal
//! list can still get one via `HelperOutput::all_literals()` or the
//! back-compat wrapper `helper_literal_outputs()`.

use std::collections::HashSet;

use helm_schema_ast::{DefineIndex, HelmAst, TemplateExpr, parse_action_expressions};
use serde::{Deserialize, Serialize};

const MAX_RECURSION_DEPTH: usize = 6;

/// Typed output of helper evaluation.
///
/// Preserves branch structure (guard + literals) for if/elif/else
/// chains so callers downstream — specifically the `Chain` lookup layer
/// — can evaluate `Capabilities.APIVersions.Has` guards against the
/// actual K8s version cache and pick the live branch instead of
/// guessing between mutually-exclusive alternatives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HelperOutput {
    /// Helper body is linear (no top-level branching). The vector
    /// holds the deduplicated literal outputs in first-seen order.
    /// Empty = could not be resolved statically.
    Literals(Vec<String>),
    /// Helper body has a single top-level if/elif/else chain. Each
    /// branch carries its guard (when decodable) and the literals it
    /// can produce.
    Branched { branches: Vec<HelperBranch> },
}

/// One branch of an if/elif/else chain in a helper body.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HelperBranch {
    /// `None` = unguarded (the trailing `else` branch, or a top-level
    /// unguarded section). `Some(g)` = the guard expression decoded
    /// from the `{{ if g }}` / `{{ else if g }}` action.
    pub guard: Option<CapabilityGuard>,
    /// What this branch resolves to when its guard is live: either a
    /// flat list of literal alternatives, or a nested
    /// `if`/`else` chain when the branch body delegates to another
    /// typed-branched helper (or is itself a typed `if`). Nested
    /// bodies preserve guard structure across delegation depth, so
    /// the chain layer composes capability evaluation through
    /// arbitrary nesting instead of flattening at the first
    /// boundary.
    pub body: HelperBranchBody,
}

/// What a `HelperBranch` produces when its guard is live.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelperBranchBody {
    /// Flat list of literal alternatives. Deduplicated in
    /// first-seen order. Empty = the branch resolves to no literal
    /// (e.g. it references a Values field with no statically-known
    /// value).
    Literals { values: Vec<String> },
    /// Branch body is itself a typed `if`/`else` chain. Reached when
    /// the branch body is exactly one nested `HelmAst::If` or a lone
    /// `{{ include "X" . }}` whose callee is itself branched.
    /// Recursive: nested branches can themselves contain
    /// `Nested` bodies.
    Nested { branches: Vec<HelperBranch> },
}

impl HelperBranchBody {
    /// Helper: build a literal-bodied branch.
    #[must_use]
    pub fn literals(values: Vec<String>) -> Self {
        Self::Literals { values }
    }

    /// Convenience: empty literal-bodied branch. Used as the
    /// per-branch accumulator while the IR walker is still
    /// collecting literal values.
    #[must_use]
    pub fn empty_literals() -> Self {
        Self::Literals { values: Vec::new() }
    }

    /// True when the body carries no statically-reachable literal,
    /// either directly or through any nested branch — i.e. selecting
    /// this branch would give the chain nothing to try.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            HelperBranchBody::Literals { values } => values.is_empty(),
            HelperBranchBody::Nested { branches } => branches.iter().all(|b| b.body.is_empty()),
        }
    }

    /// Flatten to all reachable literals in first-seen depth-first
    /// order. Used by the back-compat `all_literals` accessor and by
    /// the chain's filename iteration when there's no oracle answer.
    #[must_use]
    pub fn all_literals(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.append_all_literals(&mut out, &mut seen);
        out
    }

    pub(crate) fn append_all_literals(&self, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match self {
            HelperBranchBody::Literals { values } => {
                for v in values {
                    if seen.insert(v.clone()) {
                        out.push(v.clone());
                    }
                }
            }
            HelperBranchBody::Nested { branches } => {
                for b in branches {
                    b.body.append_all_literals(out, seen);
                }
            }
        }
    }
}

impl HelperBranch {
    /// Convenience constructor for the common literal-bodied case.
    #[must_use]
    pub fn with_literals(guard: Option<CapabilityGuard>, values: Vec<String>) -> Self {
        Self {
            guard,
            body: HelperBranchBody::Literals { values },
        }
    }

    /// Convenience constructor for a nested-bodied branch.
    #[must_use]
    pub fn with_nested(guard: Option<CapabilityGuard>, branches: Vec<HelperBranch>) -> Self {
        Self {
            guard,
            body: HelperBranchBody::Nested { branches },
        }
    }
}

/// Structurally-decoded guard for an `if` action. Only the family of
/// guards that the chain can actually evaluate against its K8s cache
/// is decoded structurally; everything else is `Opaque`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CapabilityGuard {
    /// `.Capabilities.APIVersions.Has "X"` where X is the literal
    /// argument (typically `"group/version"` or
    /// `"group/version/Kind"`).
    Has { api: String },
    /// `not .Capabilities.APIVersions.Has "X"` — the negated form.
    NotHas { api: String },
    /// Any guard the static decoder couldn't break apart. The chain
    /// treats this as "potentially live" — it never proves the branch
    /// dead, so the literals participate as candidates.
    Opaque { text: String },
}

impl HelperOutput {
    /// Flatten branches into a single deduplicated literal list (in
    /// first-seen order, depth-first through nested branches).
    /// Back-compat for callers that don't need the branch structure.
    #[must_use]
    pub fn all_literals(&self) -> Vec<String> {
        match self {
            HelperOutput::Literals(l) => l.clone(),
            HelperOutput::Branched { branches } => {
                let mut out: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for branch in branches {
                    branch.body.append_all_literals(&mut out, &mut seen);
                }
                out
            }
        }
    }

    /// `true` when the helper body has any decodable branch
    /// structure — i.e. the typed model is more precise than the
    /// flat literal list.
    #[must_use]
    pub fn is_branched(&self) -> bool {
        matches!(self, HelperOutput::Branched { .. })
    }
}

/// Resolve a helper name to its typed output.
#[must_use]
pub fn helper_evaluate(name: &str, helpers: &DefineIndex) -> HelperOutput {
    let mut seen: HashSet<String> = HashSet::new();
    let body = helpers.get(name).unwrap_or(&[]);
    if let Some(branches) = extract_top_level_branches(body, helpers, &mut seen, 0) {
        return HelperOutput::Branched { branches };
    }
    let flat = collect_literals(body, helpers, &mut seen, 0);
    HelperOutput::Literals(dedup_preserve_order(flat))
}

/// Back-compat: resolve a helper name to the flat set of literal
/// outputs it could produce. Empty list = "could not be resolved
/// statically".
///
/// Caller is responsible for trimming / validating the returned
/// strings as apiVersions (e.g. `apps/v1`, `policy/v1beta1`).
#[must_use]
pub fn helper_literal_outputs(name: &str, helpers: &DefineIndex) -> Vec<String> {
    helper_evaluate(name, helpers).all_literals()
}

/// Try to project the helper body as a top-level if/elif/else chain.
///
/// Returns `Some(branches)` when the body is one of:
///   - exactly one If node (optionally surrounded by whitespace-only
///     Scalars and HelmComments), with at least one branch yielding
///     literals and at least one branch carrying a decoded
///     `CapabilityGuard::Has` / `NotHas` guard; or
///   - a lone `{{ template "X" . }}` / `{{ include "X" . }}` call
///     (optionally surrounded by whitespace-only Scalars and
///     HelmComments) whose callee `X` itself resolves to typed
///     branches — i.e. branch structure is preserved transitively
///     through wrapper helpers, so an `outer` helper that just
///     `include`s a branched `inner` helper inherits `inner`'s
///     branches.
///
/// Returns `None` when the body has mixed content (literal prefixes,
/// multiple Ifs at the same level, a helper call mixed with other
/// content, …) — those cases fall through to the flat `Literals`
/// representation via `collect_literals`.
fn extract_top_level_branches(
    body: &[HelmAst],
    helpers: &DefineIndex,
    seen: &mut HashSet<String>,
    depth: usize,
) -> Option<Vec<HelperBranch>> {
    if depth >= MAX_RECURSION_DEPTH {
        return None;
    }
    let mut if_node: Option<&HelmAst> = None;
    let mut lone_helper_call: Option<String> = None;
    for node in body {
        match node {
            HelmAst::Scalar { text } if text.trim().is_empty() => continue,
            HelmAst::HelmComment { .. } => continue,
            HelmAst::If { .. } => {
                if if_node.is_some() || lone_helper_call.is_some() {
                    // Two top-level Ifs at the same level — the helper
                    // output is the concatenation, which doesn't fit
                    // the "pick one branch" model. Or an If mixed with
                    // a helper call. Fall through to flat literals.
                    return None;
                }
                if_node = Some(node);
            }
            HelmAst::HelmExpr { text } => {
                if if_node.is_some() || lone_helper_call.is_some() {
                    return None;
                }
                let Some(callee) = lone_helper_call_callee(text) else {
                    // The HelmExpr is something other than a bare
                    // helper call — leave for the flat path.
                    return None;
                };
                lone_helper_call = Some(callee);
            }
            _ => return None,
        }
    }
    // Lone helper-call wrapper: recurse into the callee. Preserves
    // typed branch structure across `include` / `template` indirection
    // so the chain still gets to see CapabilityHas guards that live
    // one level down.
    if let Some(callee) = lone_helper_call {
        // Cycle guard — fall through to flat literals if recursion
        // would loop. `collect_literals` will hit the same guard and
        // return empty rather than overflow.
        if !seen.insert(callee.clone()) {
            return None;
        }
        let callee_body = helpers.get(&callee);
        let result =
            callee_body.and_then(|body| extract_top_level_branches(body, helpers, seen, depth + 1));
        seen.remove(&callee);
        return result;
    }

    let if_node = if_node?;
    let HelmAst::If {
        cond,
        then_branch,
        else_branch,
    } = if_node
    else {
        unreachable!("if_node is non-None only when matched as If above");
    };
    let mut branches: Vec<HelperBranch> = Vec::new();
    collect_if_branches(
        cond,
        then_branch,
        else_branch,
        helpers,
        seen,
        depth,
        &mut branches,
    );
    // Require at least one structurally-decoded guard (Has / NotHas)
    // AND at least one branch whose body resolves to literals
    // (directly or recursively through Nested). If every branch's
    // guard is Opaque, the typed structure adds nothing the chain
    // can act on — fall through to the flat `Literals`
    // representation, which preserves the literals as candidates
    // without misleading the chain into thinking it can
    // structurally evaluate the guards.
    let has_decoded_guard = branches.iter().any(|b| {
        matches!(
            b.guard,
            Some(CapabilityGuard::Has { .. }) | Some(CapabilityGuard::NotHas { .. })
        )
    });
    let has_lits = branches.iter().any(|b| !b.body.is_empty());
    if !has_decoded_guard || !has_lits {
        return None;
    }
    Some(branches)
}

/// If `text` is exactly a `template "X" …` or `include "X" …` action
/// (possibly with extra args), return `"X"`. Otherwise `None`. The
/// arg-after-name doesn't matter for our purposes — what matters is
/// that the body of THIS helper is just a delegation, so the callee's
/// typed output can be passed through.
fn lone_helper_call_callee(text: &str) -> Option<String> {
    let wrapped = format!("{{{{ {text} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    if exprs.len() != 1 {
        return None;
    }
    match &exprs[0] {
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "template" | "include") =>
        {
            args.first().and_then(|a| match a {
                TemplateExpr::Literal(lit) => lit.as_string().map(str::to_string),
                _ => None,
            })
        }
        _ => None,
    }
}

fn collect_if_branches(
    cond: &str,
    then_branch: &[HelmAst],
    else_branch: &[HelmAst],
    helpers: &DefineIndex,
    seen: &mut HashSet<String>,
    depth: usize,
    out: &mut Vec<HelperBranch>,
) {
    let guard = decode_guard(cond);
    out.push(HelperBranch {
        guard: Some(guard),
        body: collect_branch_body(then_branch, helpers, seen, depth + 1),
    });
    // Detect elif-chains: an else-branch consisting solely of an If
    // (plus optional whitespace / comments) is the Helm lowering of
    // `{{ else if ... }}`.
    if let Some(nested_if) = lone_if_in(else_branch) {
        let HelmAst::If {
            cond: c,
            then_branch: t,
            else_branch: e,
        } = nested_if
        else {
            unreachable!("lone_if_in returns only If nodes");
        };
        collect_if_branches(c, t, e, helpers, seen, depth, out);
    } else if !else_branch.is_empty() {
        let body = collect_branch_body(else_branch, helpers, seen, depth + 1);
        if !body.is_empty() {
            out.push(HelperBranch { guard: None, body });
        }
    }
}

/// Build a branch payload from a sub-AST. Tries the typed-branched
/// shape first (the branch body is itself a typed `if`/`else` chain
/// or a delegation to a branched helper) so guard structure
/// composes through nested boundaries — round-12 Finding 1. Falls
/// through to flat literal collection when the body has mixed
/// content or no decodable structure.
fn collect_branch_body(
    nodes: &[HelmAst],
    helpers: &DefineIndex,
    seen: &mut HashSet<String>,
    depth: usize,
) -> HelperBranchBody {
    if let Some(nested) = extract_top_level_branches(nodes, helpers, seen, depth) {
        return HelperBranchBody::Nested { branches: nested };
    }
    let literals = dedup_preserve_order(collect_literals(nodes, helpers, seen, depth));
    HelperBranchBody::Literals { values: literals }
}

/// Returns the single `If` node nested inside a slice of HelmAst nodes
/// (ignoring whitespace-only Scalars and HelmComments). Used to
/// recognise `{{ else if ... }}` which the parser lowers into a
/// nested If inside the parent's else branch.
fn lone_if_in(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let mut found: Option<&HelmAst> = None;
    for n in nodes {
        match n {
            HelmAst::Scalar { text } if text.trim().is_empty() => continue,
            HelmAst::HelmComment { .. } => continue,
            HelmAst::If { .. } => {
                if found.is_some() {
                    return None;
                }
                found = Some(n);
            }
            _ => return None,
        }
    }
    found
}

/// Decode an if-condition string into a typed `CapabilityGuard`.
/// Recognises:
///   - `.Capabilities.APIVersions.Has "X"` (any leading dot prefix)
///   - `not .Capabilities.APIVersions.Has "X"`
///
/// Falls back to `Opaque` for anything else.
///
/// Implementation is fully structural: the condition is wrapped as a
/// plain template action (`{{ <cond> }}`) so it parses as an
/// expression rather than as a control directive, then walked
/// recursively over the resulting `TemplateExpr` tree. The
/// tree-sitter grammar lowers the dotted method-call form
/// `.Capabilities.APIVersions.Has "X"` into a single
/// `Call { function: ".Capabilities.APIVersions.Has", args: [Literal] }`,
/// and lowers `not .X.Y.Has "Z"` into a `Call { function: "not",
/// args: [Field([X, Y, Has]), Literal] }` (the dotted chain is split
/// into a Field receiver with `Has` as the last segment, and the
/// string arg becomes a peer of `not`). Both shapes are recognised
/// without re-doing any text parsing on `cond`.
pub fn decode_guard(cond: &str) -> CapabilityGuard {
    let trimmed = cond.trim();
    // Wrap as a plain expression action — NOT an `{{ if ... }}` block,
    // which the grammar treats as a control directive and surfaces as
    // an opaque `Unknown` node rather than parsing the condition.
    let wrapped = format!("{{{{ {trimmed} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    for expr in &exprs {
        if let Some((negated, api)) = find_capability_has(expr, false) {
            return if negated {
                CapabilityGuard::NotHas { api }
            } else {
                CapabilityGuard::Has { api }
            };
        }
    }
    CapabilityGuard::Opaque {
        text: cond.trim().to_string(),
    }
}

/// Does this function-name (from `Call.function`) refer to the Helm
/// builtin `.Capabilities.APIVersions.Has`? The grammar carries the
/// receiver path as part of the `function` string, so the check is a
/// suffix match on that field — not a regex on the raw template
/// source. Matches `.Capabilities.APIVersions.Has`,
/// `$.Capabilities.APIVersions.Has`, and any other variable/field
/// prefix that ends in `.Capabilities.APIVersions.Has`.
fn is_capabilities_has(function: &str) -> bool {
    function == ".Capabilities.APIVersions.Has"
        || function == "$.Capabilities.APIVersions.Has"
        || function.ends_with(".Capabilities.APIVersions.Has")
}

/// Recursive structural walker: find a `Capabilities.APIVersions.Has
/// "X"` call anywhere in `expr`, tracking whether the walk is
/// currently under a `not` Call (flips `negated`). Returns the
/// (negated, api) pair on first match.
///
/// Recognises two parser-produced shapes for the call site:
///   - dotted method-call form: `Call { function: ".X.Y.Has", args:
///     [Literal] }` — emitted for `.X.Y.Has "Z"` without negation.
///   - negation-quirk form: `Call { function: "not", args:
///     [Field([X, Y, Has]), Literal] }` — emitted for `not .X.Y.Has
///     "Z"`, where the parser pulls `Has` off the dotted chain into
///     the Field's last segment and hoists the literal arg up to be
///     a peer of `not`.
fn find_capability_has(expr: &TemplateExpr, negated: bool) -> Option<(bool, String)> {
    match expr {
        // Negation: walk args looking either for a Has call or for
        // the parser-quirk Field+Literal pattern.
        TemplateExpr::Call { function, args } if function == "not" => {
            // First: nested Has call.
            for a in args {
                if let Some((n, api)) = find_capability_has(a, !negated) {
                    return Some((n, api));
                }
            }
            // Then: the (Field-ending-in-Has, Literal) quirk pattern.
            let field_ends_in_has = args.iter().find_map(|a| match a {
                TemplateExpr::Field(path)
                    if path.last().map(String::as_str) == Some("Has")
                        && path.iter().rev().nth(1).map(String::as_str) == Some("APIVersions")
                        && path.iter().rev().nth(2).map(String::as_str) == Some("Capabilities") =>
                {
                    Some(())
                }
                _ => None,
            });
            if field_ends_in_has.is_some() {
                let literal = args.iter().find_map(|a| match a {
                    TemplateExpr::Literal(lit) => lit.as_string().map(str::to_string),
                    _ => None,
                });
                if let Some(api) = literal {
                    return Some((!negated, api));
                }
            }
            None
        }
        // Dotted method-call form: the function name is the entire
        // receiver chain ending in `.Capabilities.APIVersions.Has`.
        TemplateExpr::Call { function, args } if is_capabilities_has(function) => {
            args.iter().find_map(|a| match a {
                TemplateExpr::Literal(lit) => lit.as_string().map(|s| (negated, s.to_string())),
                _ => None,
            })
        }
        // Other Calls: recurse into args.
        TemplateExpr::Call { args, .. } => {
            args.iter().find_map(|a| find_capability_has(a, negated))
        }
        TemplateExpr::Pipeline(stages) => {
            stages.iter().find_map(|s| find_capability_has(s, negated))
        }
        TemplateExpr::Parenthesized(inner) => find_capability_has(inner, negated),
        TemplateExpr::Selector { operand, .. } => find_capability_has(operand, negated),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            find_capability_has(value, negated)
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => None,
    }
}

fn dedup_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for s in items {
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

fn collect_literals(
    nodes: &[HelmAst],
    helpers: &DefineIndex,
    seen: &mut HashSet<String>,
    depth: usize,
) -> Vec<String> {
    if depth >= MAX_RECURSION_DEPTH {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for node in nodes {
        match node {
            HelmAst::Scalar { text } => {
                let t = text.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
            HelmAst::HelmExpr { text } => {
                for s in extract_expr_outputs(text, helpers, seen, depth) {
                    out.push(s);
                }
            }
            HelmAst::HelmComment { .. } => {}
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                out.extend(collect_literals(then_branch, helpers, seen, depth + 1));
                out.extend(collect_literals(else_branch, helpers, seen, depth + 1));
            }
            HelmAst::With {
                body, else_branch, ..
            } => {
                out.extend(collect_literals(body, helpers, seen, depth + 1));
                out.extend(collect_literals(else_branch, helpers, seen, depth + 1));
            }
            HelmAst::Range {
                body, else_branch, ..
            } => {
                out.extend(collect_literals(body, helpers, seen, depth + 1));
                out.extend(collect_literals(else_branch, helpers, seen, depth + 1));
            }
            HelmAst::Define { body, .. } => {
                // A nested define inside a define is unusual; recurse
                // for completeness but it won't show up in practice.
                out.extend(collect_literals(body, helpers, seen, depth + 1));
            }
            HelmAst::Block { body, .. } => {
                out.extend(collect_literals(body, helpers, seen, depth + 1));
            }
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items } => {
                out.extend(collect_literals(items, helpers, seen, depth + 1));
            }
            HelmAst::Pair { value, .. } => {
                if let Some(v) = value.as_deref() {
                    out.extend(collect_literals(
                        std::slice::from_ref(v),
                        helpers,
                        seen,
                        depth + 1,
                    ));
                }
            }
        }
    }
    out
}

/// Extract the unique string-literal argument from a call's args.
/// Returns `None` unless `args` has exactly one entry that is a
/// `TemplateExpr::Literal` carrying a string. This is the precise
/// model for `print "X"` and `quote "X"` — anything else (extra
/// args, non-literal arg, non-string literal) is rejected rather
/// than producing a partial-literal result.
fn single_string_literal_arg(args: &[TemplateExpr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let TemplateExpr::Literal(lit) = &args[0] else {
        return None;
    };
    lit.as_string().map(str::to_string)
}

/// Statically evaluate `printf` for the small set of shapes we can
/// model exactly:
///   - `printf "X"` with no extra args (no `%` directives) → `"X"`
///   - `printf "%s" "Y"` (exactly one `%s`, exactly one string-literal
///     arg) → `"Y"`
///
/// Anything else (compositional formats like `printf "%s/%s" "X" "Y"`,
/// non-`%s` directives like `printf "%d" .x`, non-literal args, format
/// width/precision modifiers, …) returns `None`. Emitting a partial
/// literal would mislead downstream callers into treating intermediate
/// pieces as valid apiVersion candidates.
fn evaluate_printf(args: &[TemplateExpr]) -> Option<String> {
    let format = match args.first()? {
        TemplateExpr::Literal(lit) => lit.as_string()?,
        _ => return None,
    };
    // Zero-substitution case: format is the output, no other args.
    if !format.contains('%') {
        if args.len() != 1 {
            // Trailing args with a no-substitution format is a coding
            // error in the chart; refuse to model it.
            return None;
        }
        return Some(format.to_string());
    }
    // Single `%s` substitution: exactly one extra string-literal arg.
    if format == "%s" && args.len() == 2 {
        let TemplateExpr::Literal(lit) = &args[1] else {
            return None;
        };
        return lit.as_string().map(str::to_string);
    }
    // Any other format directive: reject.
    None
}

fn extract_expr_outputs(
    text: &str,
    helpers: &DefineIndex,
    seen: &mut HashSet<String>,
    depth: usize,
) -> Vec<String> {
    // HelmAst::HelmExpr.text is the unwrapped interior of an action
    // (the tree-sitter parser strips the `{{`, `-`, `}}` markers).
    // `parse_action_expressions` expects a full body string containing
    // wrapped actions, so re-wrap here before parsing.
    let wrapped = format!("{{{{ {text} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    let mut out: Vec<String> = Vec::new();
    for expr in &exprs {
        // `apiVersion: ("v1")` and `apiVersion: (printf "%s/%s" "apps" "v1")`
        // — parens around the whole emitted expression are syntactic
        // grouping, not a new sub-expression. Skip them so the literal /
        // call patterns below fire on the actual payload.
        match expr.deparen() {
            TemplateExpr::Literal(lit) => {
                if let Some(s) = lit.as_string() {
                    out.push(s.to_string());
                }
            }
            TemplateExpr::Call { function, args } => match function.as_str() {
                // `print "X"` / `quote "X"`: exactly one string-literal
                // arg whose value is the output. `quote` adds YAML
                // double-quotes at render time, but for apiVersion
                // resolution we want the inner literal.
                "print" | "quote" => {
                    if let Some(s) = single_string_literal_arg(args) {
                        out.push(s);
                    }
                }
                // `printf` — only model the forms we can statically
                // evaluate correctly: `printf "X"` (no substitutions)
                // or `printf "%s" "X"` (single substitution). Anything
                // more compositional (e.g. `printf "%s/%s" "apps" "v1"`,
                // `printf "%d" .Values.x`, `printf "%-10s" "X"`) is
                // rejected — emitting bogus partial literals (the
                // format string, the args separately) would mislead
                // downstream callers into treating `"apps"`, `"v1"`,
                // and `"%s/%s"` as candidate apiVersions.
                "printf" => {
                    if let Some(s) = evaluate_printf(args) {
                        out.push(s);
                    }
                }
                // Nested helper call — recurse one level deeper.
                "template" | "include" => {
                    let Some(first) = args.first() else { continue };
                    let TemplateExpr::Literal(lit) = first else {
                        continue;
                    };
                    let Some(name) = lit.as_string() else {
                        continue;
                    };
                    if !seen.insert(name.to_string()) {
                        // Cycle guard.
                        continue;
                    }
                    if let Some(body) = helpers.get(name) {
                        out.extend(collect_literals(body, helpers, seen, depth + 1));
                    }
                    seen.remove(name);
                }
                _ => {}
            },
            TemplateExpr::Pipeline(stages) => {
                // A pipeline's final output is what the helper emits.
                // The only shapes we statically evaluate are:
                //   - `"X" | quote` / `"X" | print`: pass-through of
                //     the seed literal.
                //   - `printf "X" | quote`: pass-through of the printf
                //     result (which itself must be one of the
                //     statically-modellable shapes above).
                // We collect from the LAST stage (the pipeline's final
                // output); upstream stages that aren't statically
                // evaluable in this restricted model are silently
                // skipped so we don't emit partial intermediate
                // literals as candidate apiVersions.
                if let Some(last) = stages.last() {
                    match last {
                        TemplateExpr::Literal(lit) => {
                            if let Some(s) = lit.as_string() {
                                out.push(s.to_string());
                            }
                        }
                        TemplateExpr::Call { function, args } => match function.as_str() {
                            "print" | "quote" => {
                                if let Some(s) = single_string_literal_arg(args) {
                                    out.push(s);
                                }
                            }
                            "printf" => {
                                if let Some(s) = evaluate_printf(args) {
                                    out.push(s);
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                // Also: a bare seed literal threaded into a known
                // identity-shaped pipeline like `"X" | quote` is
                // equivalent to emitting "X". Walk stages to find an
                // initial literal followed only by identity-shaped
                // calls (`quote`, `print` with no other args).
                if let Some(seed) = stages.first().and_then(|s| match s {
                    TemplateExpr::Literal(lit) => lit.as_string().map(str::to_string),
                    _ => None,
                }) && stages.iter().skip(1).all(|s| {
                    matches!(
                        s,
                        TemplateExpr::Call { function, args }
                            if matches!(function.as_str(), "print" | "quote") && args.is_empty()
                    )
                }) {
                    out.push(seed);
                }
            }
            // Field / Variable / Selector / etc. → can't resolve to a
            // literal statically.
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_ast::TreeSitterParser;
    use indoc::indoc;

    /// Test helper: extract literals from a Literals-bodied
    /// `HelperBranch`. Panics if the branch is unexpectedly Nested
    /// (tests that need to walk nested structure should match the
    /// `body` field directly instead of using this helper).
    fn literals_of(b: &HelperBranch) -> &[String] {
        match &b.body {
            HelperBranchBody::Literals { values } => values.as_slice(),
            HelperBranchBody::Nested { .. } => panic!("expected Literals-bodied branch; got {b:?}"),
        }
    }

    fn index_with(src: &str) -> DefineIndex {
        let mut idx = DefineIndex::new();
        idx.add_source(&TreeSitterParser, src)
            .expect("parse helpers");
        idx
    }

    #[test]
    fn single_literal_helper_resolves() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_literal_outputs("x.apiVersion", &helpers),
            vec!["apps/v1"]
        );
    }

    #[test]
    fn if_else_helper_returns_both_branches() {
        let helpers = index_with(indoc! {r#"
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let outs = helper_literal_outputs("rbac.apiVersion", &helpers);
        assert!(
            outs.contains(&"rbac.authorization.k8s.io/v1".to_string()),
            "must include modern; got {outs:?}"
        );
        assert!(
            outs.contains(&"rbac.authorization.k8s.io/v1beta1".to_string()),
            "must include legacy; got {outs:?}"
        );
    }

    #[test]
    fn helper_with_values_reference_is_silent_about_dynamic_branch() {
        // grafana podDisruptionBudget shape: first branch is
        // Values-driven (unresolvable), other branches are literal.
        // We collect the literal branches and skip the dynamic one.
        let helpers = index_with(indoc! {r#"
            {{- define "grafana.podDisruptionBudget.apiVersion" -}}
            {{- if $.Values.podDisruptionBudget.apiVersion }}
            {{- print $.Values.podDisruptionBudget.apiVersion }}
            {{- else if $.Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
            {{- print "policy/v1" }}
            {{- else }}
            {{- print "policy/v1beta1" }}
            {{- end }}
            {{- end }}
        "#});
        let outs = helper_literal_outputs("grafana.podDisruptionBudget.apiVersion", &helpers);
        assert!(
            outs.contains(&"policy/v1".to_string()),
            "must include policy/v1 literal branch; got {outs:?}"
        );
        assert!(
            outs.contains(&"policy/v1beta1".to_string()),
            "must include policy/v1beta1 literal branch; got {outs:?}"
        );
    }

    #[test]
    fn unknown_helper_returns_empty() {
        let helpers = DefineIndex::new();
        assert_eq!(
            helper_literal_outputs("nope", &helpers),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nested_helper_recurses_one_level() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- template "inner" . -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(helper_literal_outputs("outer", &helpers), vec!["apps/v1"]);
    }

    #[test]
    fn cyclic_helper_does_not_stack_overflow() {
        let helpers = index_with(indoc! {r#"
            {{- define "a" -}}
            {{- template "b" . -}}
            {{- end -}}
            {{- define "b" -}}
            {{- template "a" . -}}
            {{- end -}}
        "#});
        // Either empty (cycle suppressed) — must NOT panic / overflow.
        let outs = helper_literal_outputs("a", &helpers);
        assert!(
            outs.is_empty(),
            "cyclic helper must return empty, not infinite recursion; got {outs:?}"
        );
    }

    #[test]
    fn typed_output_preserves_guard_and_branch_literals() {
        // The vendored RBAC-shaped helper: stable variant gated by
        // Capabilities.APIVersions.Has, legacy as else. The typed
        // output must split into two branches; one carrying the guard,
        // one unguarded (the else fallback).
        let helpers = index_with(indoc! {r#"
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("rbac.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("expected Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2, "expected 2 branches; got {branches:?}");
        // First branch carries the CapabilityHas guard for the v1 API
        // and yields the modern literal.
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "rbac.authorization.k8s.io/v1".to_string(),
            }),
            "branch[0] guard mismatch"
        );
        assert_eq!(
            literals_of(&branches[0]),
            vec!["rbac.authorization.k8s.io/v1".to_string()]
        );
        // Second branch is the unguarded fallback yielding the legacy
        // literal.
        assert_eq!(branches[1].guard, None, "branch[1] should be unguarded");
        assert_eq!(
            literals_of(&branches[1]),
            vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
        );
    }

    /// Round-11 Finding 1: typed branch structure must survive
    /// through a wrapper helper that just delegates via
    /// `{{ include "branched_inner" . }}`. Before the fix, this
    /// shape silently degraded to `Literals(["v1", "v1beta1"])` —
    /// losing the CapabilityHas guards — because
    /// `extract_top_level_branches` only recognised a top-level If
    /// node, not a top-level helper-call delegation.
    #[test]
    fn typed_output_preserves_branches_through_wrapper_include() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer.apiVersion" -}}
            {{- include "rbac.apiVersion" . -}}
            {{- end -}}
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!(
                "wrapper helper must preserve branched typed output from delegated callee; got {out:?}"
            );
        };
        assert_eq!(branches.len(), 2, "expected 2 branches; got {branches:?}");
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "rbac.authorization.k8s.io/v1".to_string(),
            }),
            "branch[0] guard must carry the CapabilityHas decoded from the inner helper"
        );
        assert_eq!(
            literals_of(&branches[0]),
            vec!["rbac.authorization.k8s.io/v1".to_string()]
        );
        assert_eq!(branches[1].guard, None);
        assert_eq!(
            literals_of(&branches[1]),
            vec!["rbac.authorization.k8s.io/v1beta1".to_string()]
        );
    }

    /// Same shape, but with `template` (the bare-string variant of
    /// the delegation keyword) instead of `include`. Both must
    /// preserve branches identically.
    #[test]
    fn typed_output_preserves_branches_through_wrapper_template() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer.apiVersion" -}}
            {{- template "rbac.apiVersion" . -}}
            {{- end -}}
            {{- define "rbac.apiVersion" -}}
            {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
            {{- print "rbac.authorization.k8s.io/v1" -}}
            {{- else -}}
            {{- print "rbac.authorization.k8s.io/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer.apiVersion", &helpers);
        assert!(
            matches!(out, HelperOutput::Branched { .. }),
            "wrapper via template must preserve Branched output; got {out:?}"
        );
    }

    /// Multi-level delegation chain: outer → middle → branched inner.
    /// Branches must propagate through arbitrary wrapper depth (up to
    /// the recursion guard).
    #[test]
    fn typed_output_preserves_branches_through_multi_level_wrapper() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- include "middle" . -}}
            {{- end -}}
            {{- define "middle" -}}
            {{- include "inner" . -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "policy/v1" }}
            {{- print "policy/v1" -}}
            {{- else -}}
            {{- print "policy/v1beta1" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("multi-level wrapper must preserve branched output; got {out:?}");
        };
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1".to_string()
            })
        );
    }

    /// Round-12 Finding 1: typed branch structure must compose
    /// THROUGH branch bodies, not just at the top level. The shape:
    ///
    /// ```text
    /// {{- define "outer" -}}
    /// {{- if .Capabilities.APIVersions.Has "A" -}}
    /// {{- include "branched_inner" . -}}    {{- /* nested-branched delegation */ -}}
    /// {{- else -}}
    /// fallback
    /// {{- end -}}
    /// {{- end -}}
    /// ```
    ///
    /// must yield `HelperOutput::Branched` whose first branch carries
    /// a `Nested` body holding the inner helper's typed branches, NOT
    /// flatten the inner branches to a `Literals` body. This
    /// preserves the inner Has-B guard so the chain can recurse
    /// through both A and B at evaluation time.
    #[test]
    fn typed_output_preserves_nested_branches_through_branch_body_delegation() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- if .Capabilities.APIVersions.Has "A" -}}
            {{- include "inner" . -}}
            {{- else -}}
            {{- print "fallback" -}}
            {{- end -}}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "B" -}}
            {{- print "b" -}}
            {{- else -}}
            {{- print "b_legacy" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("outer must be Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2, "expected 2 outer branches");

        // First branch: Has A guard + Nested body (the inner helper's branches).
        assert_eq!(
            branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "A".to_string()
            }),
        );
        let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
            panic!(
                "branch[0].body must be Nested to preserve inner Has-B guard; got {:?}",
                branches[0].body
            );
        };
        assert_eq!(nested.len(), 2, "inner helper should contribute 2 branches");
        assert_eq!(
            nested[0].guard,
            Some(CapabilityGuard::Has {
                api: "B".to_string()
            }),
            "nested branch[0] must preserve the inner Has-B guard"
        );
        assert_eq!(literals_of(&nested[0]), vec!["b".to_string()]);
        assert_eq!(nested[1].guard, None);
        assert_eq!(literals_of(&nested[1]), vec!["b_legacy".to_string()]);

        // Second branch: unguarded else + flat literal payload.
        assert_eq!(branches[1].guard, None);
        assert_eq!(literals_of(&branches[1]), vec!["fallback".to_string()]);
    }

    /// Round-12 Finding 1: the same shape but with an INLINE nested
    /// if inside the branch body (no delegation through include).
    /// Demonstrates that the structural recursion comes from the AST
    /// shape, not from helper-call lookup specifically.
    #[test]
    fn typed_output_preserves_nested_branches_through_inline_nested_if() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            {{- if .Capabilities.APIVersions.Has "A" -}}
            {{- if .Capabilities.APIVersions.Has "B" -}}
            {{- print "b" -}}
            {{- else -}}
            {{- print "b_legacy" -}}
            {{- end -}}
            {{- else -}}
            {{- print "fallback" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("outer must be Branched; got {out:?}");
        };
        assert_eq!(branches.len(), 2);
        let HelperBranchBody::Nested { branches: nested } = &branches[0].body else {
            panic!(
                "inline nested if must produce Nested body; got {:?}",
                branches[0].body
            );
        };
        assert_eq!(nested.len(), 2);
        assert_eq!(
            nested[0].guard,
            Some(CapabilityGuard::Has {
                api: "B".to_string()
            })
        );
    }

    /// Wrapper helper that mixes a delegation with other content must
    /// NOT promote the callee's branches — the wrapper's output isn't
    /// equivalent to the callee's output any more (the prefix changes
    /// the rendered string). Fall through to the flat path, which
    /// already conservatively collects literals as candidates.
    #[test]
    fn wrapper_with_mixed_content_does_not_promote_branches() {
        let helpers = index_with(indoc! {r#"
            {{- define "outer" -}}
            prefix-{{ include "inner" . }}
            {{- end -}}
            {{- define "inner" -}}
            {{- if .Capabilities.APIVersions.Has "X" }}
            {{- print "X" -}}
            {{- else -}}
            {{- print "Y" -}}
            {{- end -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("outer", &helpers);
        // The mixed-content wrapper is NOT a pure delegation — the
        // typed `Branched` form doesn't fit. Fall back to flat.
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "mixed-content wrapper must fall through to flat literals; got {out:?}"
        );
    }

    /// Wrapper indirection must respect the same cycle guard the
    /// flat-literal recursion uses — a cyclic helper graph must not
    /// stack-overflow the typed-branch extractor.
    #[test]
    fn wrapper_cycle_falls_through_safely() {
        let helpers = index_with(indoc! {r#"
            {{- define "a" -}}
            {{- include "b" . -}}
            {{- end -}}
            {{- define "b" -}}
            {{- include "a" . -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("a", &helpers);
        // Cycle → no branches discoverable → fall through to
        // Literals (which will also be empty per the existing
        // cycle guard in collect_literals).
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "cyclic wrapper chain must fall through cleanly; got {out:?}"
        );
    }

    #[test]
    fn typed_output_preserves_elif_chain() {
        // Three-way chain: Values guard (opaque), CapabilityHas guard
        // (decoded), unguarded fallback.
        let helpers = index_with(indoc! {r#"
            {{- define "grafana.pdb.apiVersion" -}}
            {{- if $.Values.podDisruptionBudget.apiVersion }}
            {{- print $.Values.podDisruptionBudget.apiVersion }}
            {{- else if $.Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
            {{- print "policy/v1" }}
            {{- else }}
            {{- print "policy/v1beta1" }}
            {{- end }}
            {{- end }}
        "#});
        let out = helper_evaluate("grafana.pdb.apiVersion", &helpers);
        let HelperOutput::Branched { branches } = out else {
            panic!("expected Branched; got {out:?}");
        };
        // The Values-guard branch has no literal output, so the
        // extractor drops it (per `has_lits` guard? no — wait, we
        // _keep_ the branch even with empty literals because callers
        // still want to see the guard). Actually we DO keep empty
        // branches with guards so the chain has a complete picture.
        // Check that the CapabilityHas branch is present with the
        // correct literal.
        let has_branch = branches.iter().find(|b| {
            matches!(
                &b.guard,
                Some(CapabilityGuard::Has { api }) if api == "policy/v1/PodDisruptionBudget"
            )
        });
        assert!(
            has_branch.is_some(),
            "expected CapabilityHas branch; got {branches:?}"
        );
        assert_eq!(
            literals_of(has_branch.unwrap()),
            vec!["policy/v1".to_string()]
        );
        // Final unguarded branch carries the legacy fallback.
        let else_branch = branches.iter().find(|b| b.guard.is_none());
        assert!(
            else_branch.is_some(),
            "expected unguarded else branch; got {branches:?}"
        );
        assert_eq!(
            literals_of(else_branch.unwrap()),
            vec!["policy/v1beta1".to_string()]
        );
    }

    #[test]
    fn decode_guard_recognises_capability_has() {
        assert_eq!(
            decode_guard(".Capabilities.APIVersions.Has \"policy/v1\""),
            CapabilityGuard::Has {
                api: "policy/v1".to_string()
            }
        );
        assert_eq!(
            decode_guard("$.Capabilities.APIVersions.Has \"networking.k8s.io/v1/Ingress\""),
            CapabilityGuard::Has {
                api: "networking.k8s.io/v1/Ingress".to_string()
            }
        );
    }

    #[test]
    fn decode_guard_recognises_negated_capability_has() {
        assert_eq!(
            decode_guard("not .Capabilities.APIVersions.Has \"extensions/v1beta1\""),
            CapabilityGuard::NotHas {
                api: "extensions/v1beta1".to_string()
            }
        );
    }

    #[test]
    fn decode_guard_falls_back_to_opaque_for_values_refs() {
        let g = decode_guard("$.Values.podDisruptionBudget.apiVersion");
        assert!(
            matches!(g, CapabilityGuard::Opaque { .. }),
            "expected Opaque; got {g:?}"
        );
    }

    /// Round-8 Finding 2: `printf "%s/%s" "apps" "v1"` is compositional
    /// formatting we don't statically model. The old code emitted ALL
    /// three string literals (`"%s/%s"`, `"apps"`, `"v1"`) as
    /// candidate apiVersions — none of which would actually appear at
    /// runtime. The tightened code rejects this shape (returns no
    /// literal) rather than emitting bogus candidates.
    #[test]
    fn printf_compositional_format_emits_no_bogus_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%s/%s" "apps" "v1" -}}
            {{- end -}}
        "#});
        let outs = helper_literal_outputs("x.apiVersion", &helpers);
        assert!(
            outs.is_empty(),
            "compositional printf must emit no literal candidates; got {outs:?}"
        );
    }

    /// `printf "%s" "X"` is the one substitution shape we DO model
    /// exactly: a single `%s` placeholder + a single string-literal
    /// arg evaluates to the arg.
    #[test]
    fn printf_single_substitution_resolves_to_arg() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%s" "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_literal_outputs("x.apiVersion", &helpers),
            vec!["apps/v1"]
        );
    }

    /// `printf "X"` with no substitutions evaluates to the literal.
    #[test]
    fn printf_no_substitution_resolves_to_format() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_literal_outputs("x.apiVersion", &helpers),
            vec!["apps/v1"]
        );
    }

    /// `printf "%d" .x` uses a non-`%s` directive — refuse to model.
    #[test]
    fn printf_non_string_directive_emits_no_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- printf "%d" 1 -}}
            {{- end -}}
        "#});
        let outs = helper_literal_outputs("x.apiVersion", &helpers);
        assert!(
            outs.is_empty(),
            "non-%s printf directive must emit no candidates; got {outs:?}"
        );
    }

    /// `quote "X"` should produce the inner literal "X" (without the
    /// added quote wrapping — for apiVersion resolution we want the
    /// raw value).
    #[test]
    fn quote_with_single_string_arg_resolves() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- quote "apps/v1" -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_literal_outputs("x.apiVersion", &helpers),
            vec!["apps/v1"]
        );
    }

    /// `print` with multiple args is unusual and not the apiVersion
    /// shape we model; refuse rather than emit partial candidates.
    #[test]
    fn print_with_multiple_args_emits_no_candidates() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps" "v1" -}}
            {{- end -}}
        "#});
        let outs = helper_literal_outputs("x.apiVersion", &helpers);
        assert!(
            outs.is_empty(),
            "print with multiple args must emit no candidates; got {outs:?}"
        );
    }

    /// `"X" | quote` pipeline: the seed literal is passed through the
    /// identity-shaped `quote` stage; result is the seed.
    #[test]
    fn pipeline_seed_literal_then_quote_resolves_to_seed() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- "apps/v1" | quote -}}
            {{- end -}}
        "#});
        assert_eq!(
            helper_literal_outputs("x.apiVersion", &helpers),
            vec!["apps/v1"]
        );
    }

    #[test]
    fn single_literal_helper_is_not_branched() {
        let helpers = index_with(indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#});
        let out = helper_evaluate("x.apiVersion", &helpers);
        assert!(
            matches!(out, HelperOutput::Literals(_)),
            "single-literal helper must not be Branched; got {out:?}"
        );
    }
}
