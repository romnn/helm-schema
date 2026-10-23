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
  landed.txt           stages that have landed (one per line, `#` comments), e.g. R1a.
  producers.tsv        reviewed old->new producer mapping for every deleted baseline F/S/E row.

Checks (exit 1 on any failure):
  1. every candidate has a classification row (no defaults);
  2. every row is consumed: present unless its delete stage has landed, then absent;
  3. a row whose seal stage has landed is no longer `pub`;
  4. the baseline is frozen: every baseline key stays in classification.tsv with its class (rows are never
     removed; a declaration disappears only through a landed delete stage, see check 2);
  5. a row not in the baseline with class F, S or E names in `replaces` baseline rows deleted by a landed stage;
  6. every deleted baseline F/S/E row has a producers.tsv mapping to a surviving producer: a key that is
     present in the scanned candidates (or a present needle) and whose delete stage has not landed.

Usage: python3 -I plan/value-algebra-r0-inventory/inventory.py [--self-test]
"""
import collections, csv, json, pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
HERE = ROOT / "plan/value-algebra-r0-inventory"
SRC = "crates/helm-schema-ir/src"
BASE = {"Predicate", "TruthCondition", "ScalarValueDispatch", "ScalarValue", "AbstractValue", "ValuesPath",
        "HelperOutputMeta"}
ALL_VARIANTS = {"AbstractValue", "ScalarValue"}
REEXEC = (r"^(crate::expr_eval::)?(eval_expr|eval_expr_with_helper_calls|eval_expr_with_[a-z_]+)\(|"
          r"parsed_helper_body\(|Interpreter::(for_source|with_body_facts)\(")
NAME_KEYED = re.compile(r"(HashMap|BTreeMap|HashSet|BTreeSet)<String")
COLUMNS = ["key", "class", "fact", "destination", "delete", "seal", "review", "replaces"]
CLASSES = set("FSDKGORVEX")


def scan(rule, source=SRC):
    inline = f"id: inventory\nlanguage: Rust\nrule:\n{rule}"
    out = subprocess.run(["ast-grep", "scan", "--inline-rules", inline, "--json=compact", source],
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


def candidates(source=SRC):
    def parsed(rule):
        return scan(rule, source)

    items = parsed("  any:\n    - kind: struct_item\n    - kind: enum_item\n    - kind: type_item")
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
    for r in parsed("  kind: field_declaration\n  inside:\n    kind: struct_item\n    stopBy: end"):
        struct = enclosing(r, r"\bstruct\s+(\w+)")
        if struct == "LocalSlot" or rx.search(r["text"].split(":", 1)[1]) or NAME_KEYED.search(r["text"]):
            field = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:", r["text"]).group(1)
            found[f"field:{rel(r)}:{struct}.{field}"] = r["text"]
    for r in parsed("  kind: enum_variant"):
        enum = enclosing(r, r"\benum\s+(\w+)")
        name = re.match(r"\s*(\w+)", r["text"]).group(1)
        if enum in ALL_VARIANTS or rx.search(body(r["text"])):
            found[f"variant:{rel(r)}:{enum}::{name}"] = r["text"]
    for r in parsed("  kind: type_item"):
        if rx.search(r["text"].split("=", 1)[1]):
            found[f"alias:{rel(r)}:{re.search(r'type\s+(\w+)', r['text']).group(1)}"] = r["text"]
    rule = (f"  kind: function_item\n  has:\n    stopBy: end\n    kind: call_expression\n"
            f"    regex: {json.dumps(REEXEC)}")
    for r in parsed(rule):
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
    for key in base:
        if key not in by_key:
            errors.append(f"baseline row removed from classification: {key}")
    for key, row in by_key.items():
        if key in base and base[key]["class"] != row["class"]:
            errors.append(f"frozen class changed for {key}: {base[key]['class']} -> {row['class']}")
        if key in base:
            for column in ("delete", "seal"):
                previous, current = base[key][column], row[column]
                stages = ["R1a", "R1b", "R2", "R3a", "R3b", "R4", "keep", ""]
                if previous in stages and current in stages and stages.index(current) < stages.index(previous):
                    errors.append(f"{column} stage moved earlier for {key}: {previous} -> {current}")
        if key not in base and row["class"] in "FSE":
            replaced = [k for k in row["replaces"].split(",") if k]
            if not replaced or any(k not in base or k not in by_key or by_key[k]["delete"] not in landed
                                   for k in replaced):
                errors.append(f"new {row['class']} row {key} must replace baseline rows deleted by a landed stage")
    def surviving(key):
        row = by_key.get(key)
        if row is None or row["delete"] in landed:
            return False
        return needle_count(key) == 1 if key.startswith("needle:") else key in found

    mapped = {p["old"]: p["new"] for p in producers}
    if len(mapped) != len(producers):
        errors.append("duplicate old key in producers.tsv")
    if any(not p.get("reviewer", "").strip() for p in producers):
        errors.append("producer mapping has no reviewer")
    for key, row in by_key.items():
        if key in base and row["class"] in "FSE" and row["delete"] in landed:
            new = mapped.get(key)
            if new is None:
                errors.append(f"deleted baseline row {key} has no producers.tsv mapping")
            elif not surviving(new):
                errors.append(f"producers.tsv maps {key} to {new}, which is not a surviving producer")
    for stage in landed:
        deleted = [key for key, row in by_key.items()
                   if key in base and row["class"] in "FSE" and row["delete"] == stage]
        targets = {mapped[key] for key in deleted if key in mapped}
        if deleted and len(targets) >= len(deleted):
            errors.append(f"{stage} has {len(targets)} surviving producers for {len(deleted)} deleted rows")
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
    # A declaration and its row both removed while the delete stage has not landed.
    victim = next(r["key"] for r in rows if r["class"] == "F" and r["delete"] == "R1a")
    gone = {k: v for k, v in found.items() if k != victim}
    unrowed = [r for r in rows if r["key"] != victim]
    assert any("baseline row removed" in e for e in check(gone, unrowed, baseline, set(), [])), "silent removal passed"
    # A landed deletion whose producer mapping points at the deleted declaration itself.
    errors = check(gone, rows, baseline, {"R1a"}, [{"old": victim, "new": victim}])
    assert any(f"maps {victim} to {victim}" in e for e in errors), "self-mapping passed"
    print("self-test: 5 fail-closed cases rejected", file=sys.stderr)
    fixture = "tools/value-algebra-inventory/fixtures/classified.rs"
    parsed = candidates(fixture)
    expected = {f"alias:{fixture}:SemanticAlias", f"field:{fixture}:BorrowedWrapper.value",
                f"variant:{fixture}:BorrowedValue::Known"}
    assert set(parsed) == expected, f"semantic closure fixture differs: {sorted(parsed)}"
    classified = [dict.fromkeys(COLUMNS, "") | {"key": key, "class": "D"} for key in expected]
    assert not check(parsed, classified, classified, set(), []), "classified fixture failed"
    unclassified = candidates("tools/value-algebra-inventory/fixtures/unclassified.rs")
    assert any("unclassified" in e for e in check(unclassified, [], [], set(), [])), "parser missed new owner"
    postponed = [dict(r, delete="R3b") if r["key"] == victim else r for r in rows]
    assert not any(f"deleted baseline row {victim}" in e for e in check(gone, postponed, baseline, {"R1a"}, []))
    earlier = [dict(r, delete="R1a") for r in baseline if r["delete"] == "R3b"]
    assert any("stage moved earlier" in e for e in check(found, earlier, baseline, set(), []))
    print("self-test: parser closure, unclassified owner and stage monotonicity passed", file=sys.stderr)


def main():
    found = candidates()
    rows, baseline = read_tsv("classification.tsv"), read_tsv("baseline-65a6d6c7.tsv")
    landed = {l.strip() for l in (HERE / "landed.txt").read_text().splitlines() if l.strip() and not l.startswith("#")}
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
