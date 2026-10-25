#!/usr/bin/env bash
# Probe charts are isolated beneath the designer's clone.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HELM="${HELM:-/Users/roman/.local/share/mise/installs/helm/4.2.3/darwin-arm64/helm}"
exec python3 - "$ROOT" "$HELM" <<'PY'
import hashlib
import json
import pathlib
import shlex
import subprocess
import sys
import tempfile

root = pathlib.Path(sys.argv[1])
helm = pathlib.Path(sys.argv[2])
(root / "target/probes").mkdir(parents=True, exist_ok=True)
scratch = pathlib.Path(tempfile.mkdtemp(prefix="r3b-sem-", dir=root / "target/probes"))
version = subprocess.check_output([str(helm), "version"], text=True).strip()
if 'Version:"v4.2.3"' not in version:
    raise SystemExit(f"wrong Helm: {version}")
print(version)
print("helm_sha256:", hashlib.sha256(helm.read_bytes()).hexdigest())
print("script_sha256:", hashlib.sha256((root / "plan/value-algebra-r3b-probes/semantics.sh").read_bytes()).hexdigest())
print("command: bash plan/value-algebra-r3b-probes/semantics.sh")
print("scratch: $SCRATCH (unique directory under target/probes)\n")
failures = []
count = 0

def probe(name, body, want, rc=0, values="{}", files=None, mode="template", flags=(), alternatives=None, outcomes=None):
    global count
    count += 1
    chart = scratch / name
    (chart / "templates").mkdir(parents=True)
    (chart / "Chart.yaml").write_text(f"apiVersion: v2\nname: {name}\nversion: 0.1.0\n")
    (chart / "values.yaml").write_text(values + "\n")
    template = "v: " + body + "\n"
    if mode == "lint":
        template = 'apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: probe\ndata:\n  result: "' + body + '"\n'
    (chart / "templates/cm.yaml").write_text(template)
    for path, text in (files or {}).items():
        target = chart / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    command = [str(helm), mode]
    if mode == "template":
        command += ["p", str(chart), "--show-only", "templates/cm.yaml"]
    else:
        command += [str(chart)]
    command += list(flags)
    run = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15)
    output = run.stdout.replace(str(scratch), "$SCRATCH")
    passed = run.returncode == rc and all(part in output for part in want)
    if outcomes is not None:
        passed = any(run.returncode == status and marker in output for status, marker in outcomes)
        print("allowed outcomes:", outcomes, "observed exit:", run.returncode)
    observed = None
    if alternatives is not None:
        lines = [line[3:] for line in output.splitlines() if line.startswith("v: ")]
        observed = json.loads(lines[0]) if len(lines) == 1 else []
        passed = passed and len(observed) == 128 and set(observed) <= set(alternatives)
        print("allowed alternatives:", alternatives, "observed:", sorted(set(observed)))
    print(f"### {name}\nvalues.yaml: {values!r}\ntemplate: {template.rstrip()}")
    for path, text in sorted((files or {}).items()):
        print(f"file {path}: {text.rstrip()}")
    print("command:", shlex.join(command).replace(str(scratch), "$SCRATCH"))
    expectation = "one of " + repr(outcomes) if outcomes is not None else f"exit {rc}; contains {want!r}"
    print(f"exit: {run.returncode}\nexpected: {expectation}\ncheck: {'PASS' if passed else 'FAIL'}\noutput:")
    print("\n".join("  " + line if line else "" for line in output.splitlines()))
    print()
    if not passed:
        failures.append(name)

for name, expression in (
    ("ternary", 'ternary (index $s 0) "unused" true'),
    ("default", 'default "unused" (index $s 0)'),
    ("coalesce", 'coalesce (index $s 0)'),
    ("required", 'required "r" (index $s 0)'),
    ("first", 'first $s'), ("last", 'last $s'),
):
    probe("sem-call-copy-" + name, '{{ $s := splitList "," "b,a" }}{{ $x := ' + expression + ' }}{{ $_ := sortAlpha $s }}{{ $x }}/{{ index $s 0 }}', ["v: a/a" if name == "last" else "v: b/a"])
probe("sem-call-copy-stores", '{{ $s := splitList "," "b,a" }}{{ $d := dict "x" (index $s 0) }}{{ $l := list (index $s 0) }}{{ $_ := set $d "y" (index $s 0) }}{{ $_ := sortAlpha $s }}{{ $d.x }}/{{ index $l 0 }}/{{ $d.y }}', ["v: b/b/b"])
probe("sem-call-copy-shallow-map", '{{ $m := dict "x" "old" }}{{ $d := dict "m" $m }}{{ $l := list $m }}{{ $r := default (dict) $m }}{{ $_ := set $m "x" "new" }}{{ $d.m.x }}/{{ (index $l 0).x }}/{{ $r.x }}', ["v: new/new/new"])
for name, expression in (("index", "index $s 0"), ("and", "and true (index $s 0)"), ("or", 'or "" (index $s 0)')):
    probe("sem-live-slot-" + name, '{{ $s := splitList "," "b,a" }}{{ $x := ' + expression + ' }}{{ $text := printf "%s" $x }}{{ $_ := sortAlpha $s }}{{ $x }}/{{ $text }}', ["v: a/b"])
probe("sem-live-slot-consumption", '{{ $s := splitList "," "b,a" }}{{ $x := ternary (index $s 0) "unused" (eq (join "," (sortAlpha $s)) "a,b") }}{{ $x }}', ["v: a"])
for name, expression in (("set", 'set .Values "x" "new"'), ("unset", 'unset .Values "x"'), ("merge", 'merge .Values (dict "y" "new")'), ("default", 'default (dict) .Values')):
    probe("sem-map-view-" + name, '{{ $r := ' + expression + ' }}{{ $_ := set $r "alias" "yes" }}{{ typeOf $r }}/{{ .Values.alias }}', ["v: " + ("common.Values" if name == "default" else "map[string]interface {}") + "/yes"], values="x: old")
for name, expression in (("empty", "slice (list) -1 99"), ("nil", 'slice (fromJsonArray "null") -1 99'), ("empty-string-slice", 'slice (slice (splitList "," "a,b") 0 0) 0 2')):
    probe("sem-slice-" + name, '{{ $v := ' + expression + ' }}{{ typeOf $v }}/{{ eq $v nil }}', ["v: <nil>/true"])
probe("sem-slice-nil-interface", '{{ slice .Values.absent 0 }}', ["slice"], 1)
probe("sem-slice-extra-cap", '{{ $s := splitList "," "c,b,a" }}{{ $v := slice $s 0 1 1 }}{{ join "," (slice $v 0 3) }}', ["v: c,b,a"])
probe("sem-slice-extra-type-ignored", '{{ join "," (slice (splitList "," "a,b") "0" "1" (dict)) }}', ["v: a"])
probe("sem-slice-extra-side-effect", '{{ $d := dict }}{{ $_ := slice (list) -1 99 (set $d "x" "yes") }}{{ $d.x }}', ["v: yes"])
probe("sem-slice-extra-abort", '{{ slice (list) 0 0 (fail "extra evaluated") }}', ["extra evaluated"], 1)
probe("sem-slice-nonempty-invalid", '{{ slice (list 1) -1 99 }}', ["slice"], 1)
probe("sem-slice-empty-skips-coercion", '{{ typeOf (slice (list) (dict) (dict)) }}', ["v: <nil>"])
for op in ("merge", "mustMerge", "mergeOverwrite", "mustMergeOverwrite"):
    for case, args, expected in (("none", "", "true/true/0"), ("nil", ' (fromJson "null")', "true/true/0"), ("empty", " (dict)", "true/false/0"), ("populated", ' (dict "x" 1)', "true/false/1")):
        probe("sem-merge-nil-" + op.lower() + "-" + case, '{{ $d := fromJson "null" }}{{ $r := ' + op + ' $d' + args + ' }}{{ eq $d nil }}/{{ eq $r nil }}/{{ len $r }}', ["v: " + expected])
for op in ("merge", "mergeOverwrite", "mustMerge", "mustMergeOverwrite"):
    probe("sem-merge-order-" + op.lower(), '[{{ range $i := until 128 }}{{ if $i }},{{ end }}{{ $m := dict "x" 0 }}{{ $d := dict "a" $m "b" $m }}{{ $_ := ' + op + ' $d (dict "a" (dict "x" 1) "b" (dict "x" 2)) }}{{ $m.x }}{{ end }}]', [], alternatives=[1, 2])
probe("sem-merge-order-overlap", '[{{ range $i := until 128 }}{{ if $i }},{{ end }}{{ $m := dict "x" 0 }}{{ $d := dict "a" $m "b" $m }}{{ $src := dict "a" (dict "x" 1) "b" $m }}{{ $_ := mergeOverwrite $d $src }}{{ $m.x }}{{ end }}]', [], alternatives=[1])
for name, definition, expected in (("whitespace", '  ', "old"), ("comment", '{{/* empty */}}', "old"), ("runtime-empty", '{{ print "" }}', "")):
    probe("sem-namespace-tpl-" + name, '{{ define "h" }}old{{ end }}{{ tpl ' + json.dumps('{{ define "h" }}' + definition + '{{ end }}{{ include "h" . }}') + ' . | quote }}', ['v: ' + json.dumps(expected)])
    probe("sem-namespace-file-" + name, '{{ include "h" . | quote }}', ['v: ' + json.dumps(expected)], files={"templates/deep/_h.tpl": '{{ define "h" }}old{{ end }}', "templates/_h.tpl": '{{ define "h" }}' + definition + '{{ end }}'})
probe("sem-namespace-duplicate-nonempty", '{{ define "h" }}a{{ end }}{{ define "h" }}b{{ end }}ok', ['multiple definition of template "h"'], 1)
inner = '{{define "h"}}inner{{end}}{{include "h" .}}'
outer = '{{define "h"}}outer{{end}}{{tpl ' + json.dumps(inner) + ' .}}/{{include "h" .}}'
probe("sem-namespace-nested-clone", '{{ define "h" }}old{{ end }}{{ tpl ' + json.dumps(outer) + ' . }}/{{ include "h" . }}', ["v: inner/outer/old"])
lib = {"charts/lib/Chart.yaml": "apiVersion: v2\nname: lib\nversion: 0.1.0\ntype: library\n"}
probe("sem-library-ordinary-parse-excluded", 'ok', ["v: ok"], files={**lib, "charts/lib/templates/bad.yaml": '{{ definitelyNotAFunction }}'})
probe("sem-library-partial-parse-error", 'ok', ['function "definitelyNotAFunction" not defined'], 1, files={**lib, "charts/lib/templates/_bad.tpl": '{{ definitelyNotAFunction }}'})
probe("sem-library-ordinary-definition-excluded", '{{ include "ordinary" . }}', ['no template "ordinary"'], 1, files={**lib, "charts/lib/templates/ordinary.yaml": '{{ define "ordinary" }}bad{{ end }}'})
probe("sem-file-boundary-fresh-frame", '{{ $.Values.x }}/{{ .Values.x }}/{{ .Release.Name }}', ["v: original/original/shared"], values="x: original", files={"templates/z.yaml": '{{ $_ := set .Release "Name" "shared" }}{{ $ = dict "Values" (dict "x" "rebound") }}'})
probe("sem-file-boundary-captured-chart-root", '{{ $old := .Subcharts.child.Values }}{{ $_ := set .Values "child" (dict "x" "new") }}{{ $old.x }}/{{ .Subcharts.child.Values.x }}/{{ .Values.child.x }}', ["v: old/old/new"], files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "x: old\n"})
for n in (-1, 0, 3):
    for form in ("", "$i := "):
        probe("sem-range-integer-" + ("negative" if n < 0 else str(n)) + ("-one" if form else "-zero"), '{{ range ' + form + str(n) + ' }}x{{ else }}e{{ end }}', ["v: " + ("xxx" if n == 3 else "e")])
probe("sem-range-integer-two-variables", '{{ range $i,$v := 0 }}x{{ end }}', ["can't use 0 to iterate over more than one variable"], 1)
for expr, name in (('get (fromJson `{"n":3}`) "n"', "float"), ('int64 3', "int64")):
    probe("sem-range-integer-" + name, '{{ range ' + expr + ' }}x{{ end }}', ["v: xxx"] if name == "int64" else ["range can't iterate over 3"], 0 if name == "int64" else 1)
for n in (31, 32, 33):
    probe("sem-loop-bounds-" + str(n), '{{ $d := dict "x" "old" }}{{ range $i := until ' + str(n) + ' }}{{ if eq $i ' + str(n - 1) + ' }}{{ $_ := set $d "x" "new" }}{{ end }}{{ end }}{{ if ne $d.x "new" }}{{ required "suffix write missing" $.Values.need }}{{ end }}{{ $d.x }}', ["v: new"])
for n in (0, 1, 3):
    for variant in ("normal", "break", "continue", "overwrite"):
        control = {"normal": "", "break": '{{ break }}', "continue": '{{ continue }}', "overwrite": ""}[variant]
        tail = '{{ $_ := set $d "x" 9 }}' if variant == "overwrite" else ""
        expected = 9 if variant == "overwrite" else (0 if variant == "break" and n else n - 1)
        probe("sem-summary-loop-" + str(n) + "-" + variant, '{{ define "write" }}{{ $_ := set .d "x" .v }}{{ end }}{{ $d := dict "x" -1 }}{{ range $i := .Values.items }}{{ $_ := include "write" (dict "d" $d "v" $i) }}' + control + '{{ end }}' + tail + '{{ $d.x }}', ["v: " + str(expected)], values="items: " + json.dumps(list(range(n))))
probe("sem-output-boundary-within-file", '<no {{ print "value>" }}ok', ["v: ok"])
probe("sem-output-boundary-files", 'value>ok', ["v: value>ok"], files={"templates/z.yaml": "v: <no \n"})
probe("sem-output-boundary-discarded-include", '{{ define "h" }}{{ $_ := set .Values "x" "yes" }}TEXT{{ end }}{{ $_ := include "h" . }}{{ .Values.x }}', ["v: yes"])
probe("sem-output-boundary-notes-effects", 'ok', ["v: yes"], files={"templates/NOTES.txt": '{{ $_ := set .Values "x" "yes" }}NOTES-TEXT', "templates/00.yaml": 'v: {{ .Values.x }}'}, flags=("--show-only", "templates/00.yaml"))
for method in ("AsConfig", "AsSecrets"):
    expr = 'fromYaml ((.Files.Glob "files/*/same").' + method + ')'
    access = '(index $m "same")' if method == "AsConfig" else '(index $m "same" | b64dec)'
    probe("sem-cache-host-collision-" + method.lower(), '[{{ range until 128 }}{{ end }}{{ $root := . }}{{ range $i := until 128 }}{{ if $i }},{{ end }}{{ with $root }}{{ $m := ' + expr + ' }}{{ ' + access + ' | trim | quote }}{{ end }}{{ end }}]', [], files={"files/a/same": "one", "files/b/same": "two"}, alternatives=["one", "two"])
for name, expr in (("ternary", 'ternary (index $s 0) "unused" true'), ("default", 'default "unused" (index $s 0)'), ("coalesce", 'coalesce (index $s 0)'), ("required", 'required "r" (index $s 0)'), ("first", 'first $s')):
    probe("sem-call-copy-guard-" + name, '{{ $s := splitList "," "b,a" }}{{ $x := ' + expr + ' }}{{ $_ := sortAlpha $s }}{{ if ne $x "b" }}{{ required "need" .Values.need }}{{ end }}ok', ["v: ok"])
probe("sem-slice-empty-guard", '{{ $v := slice (list) -1 99 }}{{ if ne $v nil }}{{ required "need" .Values.need }}{{ end }}ok', ["v: ok"])
for flag in (False, True):
    probe("sem-cache-opposite-path-" + str(flag).lower(), '{{ define "h" }}{{ if .Values.flag }}{{ required "need" .Values.need }}{{ end }}ok{{ end }}{{ if .Values.flag }}{{ include "h" . }}{{ else }}{{ include "h" . }}{{ end }}', ["need"] if flag else ["v: ok"], 1 if flag else 0, values=json.dumps({"flag": flag}))
for n in (0, 1, 3):
    probe("sem-summary-loop-final-reassign-" + str(n), '{{ $x := "before" }}{{ range $v := .Values.items }}{{ $x = $v }}{{ end }}{{ $x }}', ["v: " + ("before" if n == 0 else str(n - 1))], values=json.dumps({"items": list(range(n))}))
for flag in (False, True):
    probe("sem-source-fail-short-circuit-" + str(flag).lower(), '{{ or .Values.flag (fail "reached") }}', ["v: true"] if flag else ["reached"], 0 if flag else 1, values=json.dumps({"flag": flag}))
probe("sem-source-selector-after-write", '{{ $_ := set .Values "kind" "ConfigMap" }}{{ if eq .Values.kind "ConfigMap" }}ok{{ else }}{{ required "unreachable" .Values.need }}{{ end }}', ["v: ok"], values="kind: Secret")
for run in range(16):
    probe("sem-merge-order-guard-" + str(run), '{{ $m := dict "x" 0 }}{{ $dst := dict "a" $m "b" $m }}{{ $_ := mergeOverwrite $dst (dict "a" (dict "x" 1) "b" (dict "x" 2)) }}{{ if ne $m.x 1 }}{{ required "alias order needs value" .Values.need }}{{ end }}ok', [], outcomes=[(0, "v: ok"), (1, "alias order needs value")])
print(f"TOTAL: {count}; passed: {count-len(failures)}; failed: {len(failures)}")
if failures:
    print("FAILED:", ", ".join(failures))
    raise SystemExit(1)
PY
