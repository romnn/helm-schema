#!/usr/bin/env python3
"""Fail-closed declaration inventory of helm-schema-ir semantic owners (value-algebra plan, R0).

Candidates (enumerated with ast-grep over crates/helm-schema-ir/src, tests excluded):
  field:   struct fields whose type names a *semantic type* or is a String-keyed map or set. Semantic types are the closure of the base
           carriers (BASE) under "a struct, enum or alias whose body names a semantic type", so new
           wrappers, aliases and variants are enumerated, never silently skipped.
  variant: enum variants whose payload names a semantic type, plus every variant of AbstractValue and
           ScalarValue.
  alias:   type aliases whose target names a semantic type.
  fn:      functions that evaluate syntax or helper bodies (eval_expr*, parsed_helper_body,
           Interpreter::for_source/with_body_facts).
  needle:  listed re-derivation sites that cannot be enumerated structurally (exactly one occurrence).

Inputs (this directory):
  classification.tsv   one reviewed row per declaration: key, class, fact, destination, delete, seal, review, replaces.
  baseline-65a6d6c7.tsv  the frozen R0 classification. Never edited.
  landed.txt           stages that have landed (one per line), e.g. R1a.
  producers.tsv        reviewed old->new producer mapping for every deleted baseline F/S/E row.

Checks (exit 1 on any failure):
  1. every candidate has a classification row (no defaults);
  2. every row is consumed: present unless its delete stage has landed, then absent;
  3. a row whose seal stage has landed is no longer `pub`;
  4. classes are frozen: a key in the baseline keeps its class;
  5. a row not in the baseline with class F, S or E names in `replaces` baseline rows deleted by a landed stage;
  6. every deleted baseline F/S/E row has a producers.tsv mapping to a key that still exists.

Usage: python3 -I plan/value-algebra-r0-inventory/inventory.py [--self-test]
"""
import collections, csv, json, pathlib, re, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SRC = "crates/helm-schema-ir/src"
BASE = {"Predicate", "TruthCondition", "ScalarValueDispatch", "ScalarValue", "AbstractValue", "ValuesPath",
        "HelperOutputMeta"}
ALL_VARIANTS = {"AbstractValue", "ScalarValue"}
REEXEC = (r"^(crate::expr_eval::)?(eval_expr|eval_expr_with_helper_calls|eval_expr_with_[a-z_]+)\(|"
          r"parsed_helper_body\(|Interpreter::(for_source|with_body_facts)\(")
NAME_KEYED = re.compile(r"(HashMap|BTreeMap|HashSet|BTreeSet)<String")
COLUMNS = ["key", "class", "fact", "destination", "delete", "seal", "review", "replaces"]
CLASSES = set("FSDKGORVEX")


def scan(rule):
    inline = f"id: inventory\nlanguage: Rust\nrule:\n{rule}"
    out = subprocess.run(["ast-grep", "scan", "--inline-rules", inline, "--json=compact", SRC],
                         cwd=ROOT, capture_output=True, text=True, check=True).stdout
    return [r for r in json.loads(out) if "/tests/" not in r["file"]]


def rel(r):
    return r["file"].removeprefix(SRC + "/")


def enclosing(r, pattern):
    lines = (ROOT / r["file"]).read_text().splitlines()
    for i in range(r["range"]["start"]["line"], -1, -1):
        m = re.search(pattern, lines[i])
        if m:
            return m.group(1)
    return "?"


def body(text):
    for sep in ("{", "=", "("):
        if sep in text:
            return text.split(sep, 1)[1]
    return ""


def candidates():
    items = scan("  any:\n    - kind: struct_item\n    - kind: enum_item\n    - kind: type_item")
    named = {re.search(r"\b(?:struct|enum|type)\s+(\w+)", r["text"]).group(1): r["text"] for r in items}
    semantic = set(BASE)
    while True:
        rx = re.compile(r"\b(" + "|".join(sorted(semantic)) + r")\b")
        grown = {n for n, t in named.items() if n not in semantic and rx.search(body(t))}
        if not grown:
            break
        semantic |= grown
    rx = re.compile(r"\b(" + "|".join(sorted(semantic)) + r")\b")
    found = {}
    for r in scan("  kind: field_declaration\n  inside:\n    kind: struct_item\n    stopBy: end"):
        if rx.search(r["text"].split(":", 1)[1]) or NAME_KEYED.search(r["text"]):
            field = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:", r["text"]).group(1)
            struct = enclosing(r, r"\bstruct\s+(\w+)")
            found[f"field:{rel(r)}:{struct}.{field}"] = r["text"]
    for r in scan("  kind: enum_variant"):
        enum = enclosing(r, r"\benum\s+(\w+)")
        name = re.match(r"\s*(\w+)", r["text"]).group(1)
        if enum in ALL_VARIANTS or rx.search(body(r["text"])):
            found[f"variant:{rel(r)}:{enum}::{name}"] = r["text"]
    for r in scan("  kind: type_item"):
        if rx.search(r["text"].split("=", 1)[1]):
            found[f"alias:{rel(r)}:{re.search(r'type\s+(\w+)', r['text']).group(1)}"] = r["text"]
    rule = (f"  kind: function_item\n  has:\n    stopBy: end\n    kind: call_expression\n"
            f"    regex: {json.dumps(REEXEC)}")
    for r in scan(rule):
        found[f"fn:{rel(r)}:{re.search(r'fn\s+(\w+)', r['text']).group(1)}"] = r["text"]
    return found


def read_tsv(name):
    path = HERE / name
    if not path.exists():
        return []
    rows = list(csv.DictReader(path.open(), delimiter="\t", quoting=csv.QUOTE_NONE, quotechar=None))
    return rows


def needle_count(key):
    _, path, needle = key.split(":", 2)
    return (ROOT / SRC / path).read_text().count(needle)


def check(found, rows, baseline, landed, producers):
    errors = []
    by_key = {}
    for row in rows:
        if row["key"] in by_key:
            errors.append(f"duplicate row {row['key']}")
        if row["class"] not in CLASSES:
            errors.append(f"bad class {row['class']} for {row['key']}")
        by_key[row["key"]] = row
    for key in sorted(found):
        if key not in by_key:
            errors.append(f"unclassified candidate {key}: {' '.join(found[key].split())[:100]}")
    for key, row in by_key.items():
        deleted = row["delete"] in landed
        present = needle_count(key) == 1 if key.startswith("needle:") else key in found
        if deleted and present:
            errors.append(f"{key} should be deleted by landed stage {row['delete']}")
        if not deleted and not present:
            errors.append(f"stale row (declaration not found): {key}")
        if row["seal"] in landed and present and not key.startswith("needle:"):
            if re.match(r"\s*pub\b", found[key]):
                errors.append(f"{key} should be sealed by landed stage {row['seal']}")
    base = {row["key"]: row for row in baseline}
    for key, row in by_key.items():
        if key in base and base[key]["class"] != row["class"]:
            errors.append(f"frozen class changed for {key}: {base[key]['class']} -> {row['class']}")
        if key not in base and row["class"] in "FSE":
            replaced = [k for k in row["replaces"].split(",") if k]
            if not replaced or any(k not in base or base[k]["delete"] not in landed for k in replaced):
                errors.append(f"new {row['class']} row {key} must replace baseline rows deleted by a landed stage")
    mapped = {p["old"]: p["new"] for p in producers}
    for key, row in base.items():
        if row["class"] in "FSE" and row["delete"] in landed:
            new = mapped.get(key)
            if new is None:
                errors.append(f"deleted baseline row {key} has no producers.tsv mapping")
            elif new != "-" and new not in by_key:
                errors.append(f"producers.tsv maps {key} to unknown {new}")
    return errors


def self_test(found, rows, baseline):
    fake = dict(found)
    fake["field:eval_effect.rs:Effects.parallel_truths"] = "pub(crate) parallel_truths: BTreeMap<String, Predicate>"
    assert any("unclassified" in e for e in check(fake, rows, baseline, set(), [])), "new field passed"
    ghost = rows + [dict(rows[0], key="field:eval_env.rs:EvalEnv.ghost")]
    assert any("stale row" in e for e in check(found, ghost, baseline, set(), [])), "unused row passed"
    relabel = [dict(r, **{"class": "V"}) if r["key"] == "field:eval_env.rs:EvalEnv.root_truthy_predicates" else r
               for r in rows]
    assert any("frozen class" in e for e in check(found, relabel, baseline, set(), [])), "relabel passed"
    print("self-test: 3 fail-closed cases rejected", file=sys.stderr)


def main():
    found = candidates()
    rows, baseline = read_tsv("classification.tsv"), read_tsv("baseline-65a6d6c7.tsv")
    landed = {l.strip() for l in (HERE / "landed.txt").read_text().splitlines() if l.strip()}
    if "--self-test" in sys.argv:
        self_test(found, rows, baseline)
    errors = check(found, rows, baseline, landed, read_tsv("producers.tsv"))
    for e in errors:
        print(e, file=sys.stderr)
    counts = collections.Counter(r["class"] for r in rows)
    pending = collections.Counter((r["class"], r["delete"]) for r in rows if r["delete"] not in ("", "keep"))
    seals = collections.Counter(r["seal"] for r in rows if r["seal"])
    print(f"rows {len(rows)} candidates {len(found)} landed {sorted(landed)} classes {dict(sorted(counts.items()))}",
          file=sys.stderr)
    print(f"deletions by (class, stage) {dict(sorted(pending.items()))}; seals {dict(seals)}", file=sys.stderr)
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
