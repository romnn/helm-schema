#!/usr/bin/env python3
"""Validate each chart's shipped ci/*-values.yaml against the pinned schema.

Deep-merges the ci values over the chart defaults (helm coalesce semantics:
user maps merge, scalars/lists replace, explicit null deletes), drops nulls,
validates. Rejections are candidate narrowings on non-default branches; each
needs adjudication against `helm template`.
"""
import glob
import json
import os
import sys
import yaml
import jsonschema

ROOT = "/home/roman/dev/helm-schema"
SCHEMAS = os.path.join(ROOT, "testdata/chart-corpus-schemas")


def merge(base, over):
    if isinstance(base, dict) and isinstance(over, dict):
        out = dict(base)
        for k, v in over.items():
            if v is None:
                out.pop(k, None)
            elif k in out:
                out[k] = merge(out[k], v)
            else:
                out[k] = v
        return out
    return over


def drop_nulls(v):
    if isinstance(v, dict):
        return {k: drop_nulls(x) for k, x in v.items() if x is not None}
    if isinstance(v, list):
        return [drop_nulls(x) for x in v if x is not None]
    return v


total = rejected = 0
for chart in sorted(os.listdir(SCHEMAS)):
    if not chart.endswith(".schema.json"):
        continue
    name = chart[: -len(".schema.json")]
    chart_dir = os.path.join(ROOT, "testdata/charts", name)
    ci_files = sorted(glob.glob(os.path.join(chart_dir, "ci", "*values.yaml")))
    if not ci_files:
        continue
    try:
        defaults = yaml.safe_load(open(os.path.join(chart_dir, "values.yaml"))) or {}
    except yaml.YAMLError:
        continue
    schema = json.load(open(os.path.join(SCHEMAS, chart)))
    validator = jsonschema.Draft7Validator(schema)
    for ci in ci_files:
        try:
            over = yaml.safe_load(open(ci)) or {}
        except yaml.YAMLError as e:
            print(f"{name}/{os.path.basename(ci)}: ci yaml parse error")
            continue
        if not isinstance(over, dict):
            continue
        total += 1
        merged = drop_nulls(merge(defaults, over))
        errors = [
            f"{e.json_path}: {e.message[:100]}" for e in validator.iter_errors(merged)
        ]
        if errors:
            rejected += 1
            print(f"REJECT {name}/{os.path.basename(ci)}: {len(errors)} error(s)")
            for e in errors[:4]:
                print(f"    {e}")

print(f"\n{total} ci values files checked, {rejected} rejected", file=sys.stderr)
