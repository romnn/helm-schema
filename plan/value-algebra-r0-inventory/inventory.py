#!/usr/bin/env python3
"""Declaration-level inventory of helm-schema-ir semantic owners (R0, value-algebra plan).

Enumerates candidate declarations with ast-grep (struct fields whose type names a semantic carrier, or
name-keyed maps/sets), adds the listed enum variants and re-execution entry points, and classifies every
candidate. Fails (exit 1) on an unclassified candidate or a stale classification, so the inventory stays
reproducible: rerun after any IR change.

Usage: python3 -I plan/value-algebra-r0-inventory/inventory.py > plan/value-algebra-r0-inventory/inventory.tsv
"""
import collections, json, pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = "crates/helm-schema-ir/src"
SEMANTIC = ("Predicate|TruthCondition|ScalarValueDispatch|ScalarValue|AbstractValue|LocalBinding|BindingValue|"
            "SelectionReachability|ProvenOperands|HelperOutputMeta|RangeSubject|GetBinding|IntCastSource|DotFrame|"
            "EvalResult|Effects|FragmentSummary|ValuesPath|BoundValueContext|MemberHostConversion")
RULES = [
    ("semantic-field", f'kind: field_declaration\n  regex: "{SEMANTIC}"'),
    ("name-keyed-field", 'kind: field_declaration\n  regex: "(HashMap|BTreeMap|HashSet|BTreeSet)<String"'),
]

# Classes. F: independently maintained fact store (must reach one owner). S: snapshot or copy of another
# owner's fact. D: derived projection, borrowed view or per-call plan. K: cache or cache key. G: guard/ambient
# context. O: observation exported to ObservedFacts. R: representation (type/variant). E: re-execution or
# second-interpreter entry point. V: the surviving owner itself. X: outside the interpreter (parse caches,
# downstream contract builders, policy).
# Destination stages: R1a, R1b, R2, R3a, R3b, keep. Format: class, fact, destination, stage.
STRUCT_DEFAULT = {
    "Effects": ("O", "path observation", "ObservedFacts", "keep"),
    "LowerScope": ("D", "borrowed lowering view", "follows its owner", "keep"),
    "ArmClaims": ("O", "quote-context claims", "ObservedFacts", "keep"),
    "ContractIr": ("X", "contract", "-", "keep"), "DependencyValuesRoot": ("X", "contract", "-", "keep"),
    "ObservedFacts": ("X", "phase artifact", "-", "keep"), "ActivatedValuesRootOverlay": ("X", "phase artifact", "-", "keep"),
    "ValuesRootOverlay": ("X", "phase artifact", "-", "keep"), "ContractPathAccumulator": ("X", "contract builder", "-", "keep"),
    "SymbolicPolicy": ("X", "policy", "-", "keep"), "ParsedDefinesInner": ("X", "parse cache", "-", "keep"),
    "TaintPart": ("O", "output fragment", "O buffer", "keep"), "Splice": ("O", "output fragment", "O buffer", "keep"),
    "Opaque": ("O", "output fragment", "O buffer", "keep"), "SpliceMeta": ("O", "output fragment", "O buffer", "keep"),
    "RenderedRow": ("O", "rendered row", "O buffer", "keep"), "ValueRead": ("O", "read observation", "ObservedFacts", "keep"),
    "EvaluatedDocument": ("O", "document observation", "ObservedFacts", "keep"),
    "HelperOutputMeta": ("V", "transform program (provenance)", "V provenance", "keep"),
    "RangeModes": ("G", "range modes", "frozen guard context", "keep"),
    "DomainConstraints": ("D", "per-evaluation constraint", "-", "keep"), "ValueDomainConstraint": ("D", "per-evaluation constraint", "-", "keep"),
    "BoundHelperValueResolverParams": ("D", "borrowed params", "-", "keep"),
    "ResolveBoundHelperCallParams": ("D", "borrowed params", "-", "keep"),
    "ValuePathContext": ("D", "borrowed view of local state", "follows R1 slot", "keep"),
    "RangeKeyConcretization": ("D", "range-key rewrite plan", "-", "keep"),
    "StrictLayerWalk": ("D", "walk scratch", "-", "keep"), "StaticTemplateProgram": ("D", "template request", "-", "keep"),
    "FunctionSemantics": ("R", "catalog", "catalog facets", "keep"),
    "MemberHostConversion": ("R", "object conversion", "H write", "R3b"),
    "IntCastSource": ("R", "int-cast provenance", "V provenance", "R3b"),
    "GetBinding": ("R", "get-plan provenance", "V provenance", "R3b"), "GetBindingPlan": ("D", "get plan", "-", "R3b"),
    "RangeSubjectIdentity": ("R", "range identity", "V provenance", "keep"),
    "VariableLocalState": ("S", "scope-undo copy of one local's 13 facts", "becomes LocalSlot", "R1a"),
    "LocalScopeFrame": ("S", "scope-undo frame", "whole-slot save", "R1a"),
    "ProvenOperand": ("R", "second selection tree node", "guarded alternatives", "R3b"),
    "HoleEval": ("S", "copy of one EvalResult", "hole holds EvalResult", "R3a"),
    "RootSetState": ("S", "save/restore of root and observed triples", "one RootSlot map", "R1a"),
    "BoundHelperCallCacheKey": ("K", "helper summary key", "key by slot map, then snapshot id + seen", "R1a/R3b"),
    "BoundHelperCallResolution": ("S", "caller root snapshot for helper", "slot map, then heap snapshot", "R1a/R3b"),
    "DotFrame": ("F", "dot in two flavors (value, fragment)", "one dot value + fragment consumer", "R3b"),
    "BoundValueContext": ("S", "cloned range/get maps", "view of LocalSlot", "R1a"),
    "HelperArgBindings": ("F", "helper argument bindings beside a dispatch map", "frame slots", "R1a/R3b"),
    "PipedOperand": ("V", "owned operand", "Operand", "keep"),
    "SelectionTruthReachability": ("F", "two-polarity reachability", "query over alternatives", "R3b"),
    "RangeIterationAlternative": ("F", "per-iteration truth", "frozen guard", "R3b"),
    "RangeIterationBinding": ("D", "per-iteration frame", "frame", "keep"),
    "RangeSubject": ("F", "range alternatives carrier", "V iterable + member identity", "R3b"),
    "RangeValueAlternative": ("F", "range alternative", "guarded alternative", "R3b"),
    "ControlOutcome": ("S", "control exit state", "fork/join under frozen guard", "R3b"),
    "ResolvedTemplateProgram": ("G", "program selection predicate", "frozen guard", "keep"),
    "BindingLeaf": ("D", "leaf view", "-", "keep"), "BindingDecision": ("V", "decision structure", "single decision structure", "keep"),
    "BindingValue": ("V", "binding leaf", "V leaf", "keep"), "BindingValueMetadata": ("V", "leaf provenance", "V provenance", "keep"),
    "DotBinding": ("V", "dot stack frame", "frame", "keep"),
    "BoundHelperCallSummary": ("K", "cached summary", "cache entry", "keep"),
    "ResolvedBoundHelperCall": ("O", "argument effects", "-", "keep"),
    "ScalarValueDispatch": ("R", "dispatch arms", "sealed view (R2), query (R3b)", "R2"),
    "DeferredScalarProjection": ("E", "deferred second interpreter", "in-walk output", "R3b"),
    "HelperOutputEvaluator": ("E", "apiVersion helper-output evaluator", "consume evaluator output", "R3b"),
}
FIELD = {
    "IrAnalysisDb.bound_helper_calls": ("K", "helper summary cache", "rekey; read-only+deterministic only", "R3b"),
    "IrAnalysisDb.predicate_memo": ("K", "predicate memo", "-", "keep"),
    "IrAnalysisDb.static_root_fields": ("S", "initial root fields", "heap initialization", "R3b"),
    "IrAnalysisDb.chart_default_strings": ("X", "policy", "-", "keep"),
    "IrAnalysisDb.body_eval_facts": ("K", "per-helper body facts", "-", "keep"),
    "IrAnalysisDb.helper_seen_footprints": ("K", "recursion footprints", "-", "keep"),
    "IrAnalysisDb.custom_merge_helpers": ("K", "helper-shape recognizer result", "executed body (if reproduced)", "R3b"),
    "IrAnalysisDb.nil_scrub_helpers": ("K", "helper-shape recognizer result", "executed body (if reproduced)", "R3b"),
    "BoundHelperCallCacheKey.seen": ("K", "recursion cut context", "retained in key", "keep"),
    "Effects.local_default_paths": ("S", "binding metadata exported via effects", "read V provenance", "R3b"),
    "Effects.local_output_meta": ("S", "binding metadata exported via effects", "read V provenance", "R3b"),
    "Effects.local_source_paths": ("S", "binding metadata exported via effects", "read V provenance", "R3b"),
    "Effects.local_set_mutations": ("F", "local map writes", "H write", "R3b"),
    "Effects.root_set_mutations": ("F", "root writes: value", "RootSlot map, then H", "R1a/R3b"),
    "Effects.root_set_predicates": ("F", "root writes: truth", "RootSlot field, then Q", "R1a/R3b"),
    "Effects.root_set_value_dispatches": ("F", "root writes: dispatch", "RootSlot field, then V", "R1a/R3b"),
    "Effects.member_host_conversions": ("F", "object conversions", "H write", "R3b"),
    "EvalResult.value": ("V", "expression value", "V", "keep"),
    "EvalResult.effects": ("O", "observations", "ObservedFacts", "keep"),
    "EvalResult.truth": ("F", "truth beside value", "query", "R3b"),
    "EvalResult.selection_reachability": ("F", "selection beside value (ADR 0022)", "query", "R3b"),
    "EvalResult.json_payload_truth": ("F", "payload truth beside text", "serialization term snapshot + query", "R3b"),
    "EvalResult.scalar_dispatch": ("F", "content/kind arms beside value", "sealed view (R2), query (R3b)", "R2/R3b"),
    "EvalResult.field_scalar_dispatches": ("F", "per-field arms of a constructed map", "H fields", "R3b"),
    "EvalResult.proven_operands": ("F", "second selection tree", "guarded alternatives", "R3b"),
    "EvalEnv.dot": ("V", "dot", "frame", "keep"), "EvalEnv.locals": ("V", "local bindings", "frame (LocalSlot map)", "keep"),
    "EvalEnv.active_predicates": ("G", "ambient guards", "frozen guards", "keep"),
    "EvalEnv.predicate_memo": ("K", "predicate memo", "-", "keep"),
    "EvalEnv.root_fields": ("F", "root: value", "RootSlot map, then H", "R1a/R3b"),
    "EvalEnv.root_truthy_predicates": ("F", "root: truth", "RootSlot field, then Q", "R1a/R3b"),
    "EvalEnv.root_value_dispatches": ("F", "root: dispatch", "RootSlot field, then V", "R1a/R3b"),
    "EvalEnv.local_scalar_dispatches": ("F", "local: dispatch", "LocalSlot field (R1a), leaf (R1b)", "R1a/R1b"),
    "EvalEnv.local_default_paths": ("F", "local: default paths (dup of leaf metadata)", "leaf metadata", "R1a/R1b"),
    "EvalEnv.local_output_meta": ("F", "local: transform program (dup of leaf metadata)", "leaf metadata", "R1a/R1b"),
    "EvalEnv.local_truthy_reductions": ("F", "local: truth", "LocalSlot field (R1a), leaf or residual (R1b)", "R1a/R1b"),
    "EvalEnv.member_host_conversions": ("F", "object conversions", "H write", "R3b"),
    "EvalEnv.bound_values": ("S", "BoundValueContext clone", "view of LocalSlot", "R1a"),
    "Interpreter.json_payload_truth_outputs": ("F", "payload truth at hole outputs", "query over O", "R3b"),
    "Interpreter.root_value_dot": ("F", "value-flavor helper dot", "frame", "R3b"),
    "Interpreter.root_bindings": ("F", "root: value", "RootSlot map, then H", "R1a/R3b"),
    "Interpreter.root_truthy_predicates": ("F", "root: truth", "RootSlot field, then Q", "R1a/R3b"),
    "Interpreter.root_value_dispatches": ("F", "root: dispatch", "RootSlot field, then V", "R1a/R3b"),
    "Interpreter.root_set_mutations_observed": ("S", "root writes exported", "RootSlot map, then H", "R1a/R3b"),
    "Interpreter.root_set_predicates_observed": ("S", "root writes exported", "RootSlot field", "R1a/R3b"),
    "Interpreter.root_value_dispatches_observed": ("S", "root writes exported", "RootSlot field", "R1a/R3b"),
    "Interpreter.chart_defaults_observed": ("F", "set-default writes", "H write", "R3b"),
    "Interpreter.active_predicates": ("G", "ambient guards", "frozen guards", "keep"),
    "Interpreter.active_range_modes": ("G", "range modes", "frozen guards", "keep"),
    "Interpreter.alternative_capture_approximates": ("G", "approximation guards", "frozen guards", "keep"),
    "Interpreter.helper_seen": ("K", "recursion cut", "-", "keep"),
    "FragmentSummary.root_set_mutations": ("S", "helper root writes", "RootSlot map, then H", "R1a/R3b"),
    "FragmentSummary.root_set_predicates": ("S", "helper root writes", "RootSlot field", "R1a/R3b"),
    "FragmentSummary.root_set_value_dispatches": ("S", "helper root writes", "RootSlot field", "R1a/R3b"),
    "FragmentSummary.chart_defaults": ("F", "helper set-default writes", "H write", "R3b"),
    "FragmentSummary.value": ("S", "helper result value", "V returned by helper", "R3b"),
    "FragmentSummary.json_payload_truth": ("S", "helper payload truth", "query", "R3b"),
    "FragmentSummary.scalar_dispatch": ("D", "lazy helper output arms", "sealed view (R2), O buffer (R3b)", "R2/R3b"),
    "SymbolicLocalState.chart_value_defaults": ("F", "set-default writes", "H write", "R3b"),
    "SymbolicLocalState.fragment_values": ("V", "local bindings", "LocalSlot.binding", "R1a"),
}
for owner in ("Interpreter", "FragmentSummary"):
    for f in ("pre_rewrite_strict_paths", "parsed_yaml_input_paths", "yaml_serialized_paths", "run_templated_text_paths",
              "suppress_predicate_paths"):
        FIELD[f"{owner}.{f}"] = ("O", "path observation", "ObservedFacts", "keep")
    FIELD[f"{owner}.member_host_conversions"] = ("F", "object conversions", "H write", "R3b")
for f in ("range_domains", "get_bindings", "default_paths", "output_meta", "scalar_dispatches", "truthy_reductions",
          "truthiness_abstentions", "truthiness_clears", "typeof_sources", "int_cast_sources", "range_member_values",
          "definite_range_member_values"):
    FIELD[f"SymbolicLocalState.{f}"] = ("F", f"local: {f}", "LocalSlot field", "R1a")
for f in ("value", "effects", "truth", "truth_reachability", "json_payload_truth", "scalar_dispatch", "proven_operands"):
    FIELD.setdefault(f"HoleEval.{f}", ("S", "copy of EvalResult." + f, "hole holds one EvalResult", "R3a"))

VARIANTS = [  # (file, enum, variant regex, class, fact, destination, stage)
    ("abstract_value.rs", "AbstractValue", r"(Dict|List|Overlay|Choice|FirstTruthy|MergedLayers|SplitList|SplitSegment|Widened)\b",
     "R", "runtime-shaped value variant", "H object / V sequence / guarded alternatives", "R3b"),
    ("abstract_value.rs", "AbstractValue", r"(Top|Unknown|ValuesPath|JsonDecodedPath|RangeKey|KeysList|OutputPath|RootContext|StringSet|DerivedBoolean)\b",
     "R", "path/scalar value variant", "V leaf + provenance", "R3b"),
]
ENTRIES = [  # (file, needle, class, fact, destination, stage)
    ("fragment_eval/summary.rs", "fn evaluate(&self, db: &IrAnalysisDb)", "E", "DeferredScalarProjection second run", "in-walk output", "R3b"),
    ("fragment_eval/summary.rs", "fn bound_helper_interpreter<'a>(", "E", "fresh helper interpreter without caller state", "helper frame with State", "R3b"),
    ("fragment_eval/summary.rs", "fn scalar_dispatch_from_fragment(", "E", "fragment-to-output fallback", "in-walk output", "R3b"),
    ("fragment_eval/files.rs", "let mut nested = Interpreter::for_source(", "E", "nested file/tpl interpreter", "tpl with cloned namespace + State", "R3b"),
    ("fragment_expr_eval/mod.rs", "pub(crate) fn document_result_from_expr(", "E", "EvalResult rebuilt via fragments", "X", "R3b"),
    ("expr_eval.rs", "fn apply_local_set_mutations_expr(", "E", "mutation re-evaluated", "H write", "R3b"),
    ("eval_env.rs", "pub(crate) fn apply_local_set_mutations(", "E", "mutation replay", "H write", "R3b"),
    ("fragment_eval/assignments.rs", "fn apply_helper_scope_set_mutations(", "E", "helper-scope mutation replay", "H write", "R3b"),
    ("fragment_eval/assignments.rs", "fn apply_context_copy_value_mutations(", "E", "context-copy re-evaluation (:434)", "H write", "R3b"),
    ("fragment_eval/hole_effects.rs", "fn apply_root_set_mutations(", "E", "root mutation replay", "H write", "R3b"),
    ("fragment_eval/hole_effects.rs", "fn eval_condition_header(", "E", "header evaluated twice", "one evaluation", "R3a"),
    ("value_path_context/path_resolution.rs", "pub(crate) fn paths_for_expr(", "E", "paths re-derived by evaluation", "V provenance", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_set_truthy_predicate(", "E", "AST truth re-derivation", "query over written V", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_set_stringified_comparison(", "E", "AST re-derivation", "query", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_set_string_literal(", "E", "AST re-derivation", "query", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_set_literal_is_truthy(", "E", "AST re-derivation", "query", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_set_predicate_any(", "E", "AST re-derivation", "query", "R3b"),
    ("expr_call_eval/root_mutation.rs", "fn root_values_default_source(", "E", "merge operands evaluated again (:316, :320)", "one evaluation", "R3a"),
    ("fragment_eval/hole_effects.rs", "pub(super) fn required_call_subjects(", "E", "AST required lane", "evaluator required consumer", "R2"),
    ("fragment_eval/hole_effects.rs", "pub(super) fn subject_is_statically_helm_empty(", "E", "AST required lane", "evaluator required consumer", "R2"),
    ("fragment_eval/hole_effects.rs", "pub(super) fn collect_required_subjects<'e>(", "E", "AST required lane", "evaluator required consumer", "R2"),
    ("fragment_eval/hole_effects.rs", "pub(super) fn record_required_subjects(", "E", "AST required lane", "evaluator required consumer", "R2"),
    ("fragment_eval/eval.rs", "pub(super) fn record_required_condition(", "E", "AST required lane", "evaluator required consumer", "R2"),
    ("expr_call_eval/mod.rs", '"required" if matches!(args, [_, _]) => {', "E", "negated-truth required arm", "required_subject over kinds", "R2"),
    ("expr_call_eval/comparisons.rs", "// Literal and intrinsic return types survive preceding argument mutations.", "E", "syntactic ternary kind list", "require_kind(Bool)", "R2"),
    ("expr_call_eval/mod.rs", "fn split_length_dispatch(", "E", "AST-only argument fact", "V fact", "R3b"),
    ("expr_call_eval/mod.rs", "fn scalar_pattern_condition(", "E", "AST-only argument fact", "V fact", "R3b"),
    ("expr_call_eval/mod.rs", "fn direct_stringified_dispatch(", "E", "AST-only argument fact", "V fact", "R3b"),
    ("expr_call_eval/mod.rs", "fn direct_quoted_falsy_pattern_condition(", "E", "AST-only argument fact", "V fact", "R3b"),
    ("helper_literal_dispatch.rs", "pub(crate) fn helper_literal_dispatch(", "E", "syntactic helper-output interpreter", "O buffer of executed helper", "R3b"),
    ("resource_identity.rs", "pub(crate) struct HelperOutputEvaluator {", "E", "apiVersion helper-output evaluator", "O buffer of executed helper", "R3b"),
    ("analysis_db.rs", "pub(crate) fn program_wrapper_sentinels(", "E", "helper-shape recognizer", "executed body (if reproduced)", "R3b"),
    ("analysis_db.rs", "fn classify_nil_scrub_helper(", "E", "helper-shape recognizer", "executed body (if reproduced)", "R3b"),
    ("analysis_db.rs", "fn classify_pair_merge_helper(", "E", "helper-shape recognizer", "executed body (if reproduced)", "R3b"),
    ("analysis_db.rs", "fn classify_parsed_map_list_merge_helper(", "E", "helper-shape recognizer", "executed body (if reproduced)", "R3b"),
    ("analysis_db.rs", "fn classify_typed_yaml_renderer(", "E", "helper-shape recognizer", "executed body (if reproduced)", "R3b"),
    ("static_file_template.rs", "pub(crate) fn collect_template_requests_from_helper(", "E", "syntactic helper walk", "evaluator", "R3b"),
]
COND_REEVAL = ("value_path_context/condition_predicate.rs", "eval_expr(")


def enclosing_struct(lines, ln):
    for i in range(ln, -1, -1):
        m = re.match(r"\s*(pub(\([^)]*\))?\s+)?struct\s+(\w+)", lines[i])
        if m:
            return m.group(3)
    return "?"


def main():
    candidates = {}
    for rid, rule in RULES:
        inline = f"id: {rid}\nlanguage: Rust\nrule:\n  {rule}\n  inside:\n    kind: struct_item\n    stopBy: end"
        out = subprocess.run(["ast-grep", "scan", "--inline-rules", inline, "--json=compact", SRC],
                             cwd=ROOT, capture_output=True, text=True, check=True).stdout
        for r in json.loads(out):
            if "/tests/" in r["file"]:
                continue
            path = r["file"].removeprefix(SRC + "/")
            lines = (ROOT / r["file"]).read_text().splitlines()
            ln = r["range"]["start"]["line"]
            field = re.match(r"\s*(pub(\([^)]*\))?\s+)?(\w+)\s*:", r["text"]).group(3)
            struct = enclosing_struct(lines, ln)
            candidates[(path, ln + 1)] = (f"{struct}.{field}", " ".join(r["text"].split()))
    rows, missing = [], []
    for (path, line), (decl, text) in sorted(candidates.items()):
        struct = decl.split(".")[0]
        cls = FIELD.get(decl) or STRUCT_DEFAULT.get(struct)
        if cls is None:
            missing.append(f"{path}:{line}\t{decl}\t{text}")
            continue
        rows.append((cls[0], decl, f"{path}:{line}", *cls[1:]))
    for path, enum, rx, *cls in VARIANTS:
        lines = (ROOT / SRC / path).read_text().splitlines()
        start = next(i for i, l in enumerate(lines) if re.match(rf"\s*pub(\(crate\))? enum {enum}\b", l))
        for i in range(start + 1, len(lines)):
            if lines[i].startswith("}"):
                break
            m = re.match(rf"    {rx}", lines[i])
            if m:
                rows.append((cls[0], f"{enum}::{m.group(1)}", f"{path}:{i + 1}", *cls[1:]))
    for path, needle, *cls in ENTRIES:
        lines = (ROOT / SRC / path).read_text().splitlines()
        hits = [i for i, l in enumerate(lines) if needle in l]
        if len(hits) != 1:
            missing.append(f"entry {path}: {needle!r} found {len(hits)} times")
            continue
        name = re.search(r"(fn|struct) (\w+)", needle)
        rows.append((cls[0], name.group(2) if name else needle.strip()[:40], f"{path}:{hits[0] + 1}", *cls[1:]))
    path, needle = COND_REEVAL
    lines = (ROOT / SRC / path).read_text().splitlines()
    for i, l in enumerate(lines):
        if needle in l and "self.expression_eval_env()" in l:
            rows.append(("E", "condition re-evaluation", f"{path}:{i + 1}", "condition evaluated again", "consume hole EvalResult", "R3a"))
    if missing:
        print("UNCLASSIFIED OR STALE:", *missing, sep="\n", file=sys.stderr)
        sys.exit(1)
    print("class\tdeclaration\tlocation\tfact\tdestination\tstage")
    for row in rows:
        print("\t".join(row))
    counts = collections.Counter(r[0] for r in rows)
    stages = collections.Counter((r[0], r[5]) for r in rows if r[0] in "FSERK" and r[5] != "keep")
    print("# rows", len(rows), dict(sorted(counts.items())), file=sys.stderr)
    print("# to delete or merge, by (class, stage):", dict(sorted(stages.items())), file=sys.stderr)


main()
