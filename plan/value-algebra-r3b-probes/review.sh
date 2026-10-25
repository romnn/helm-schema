#!/usr/bin/env bash
# Probe charts are isolated beneath the designer's clone.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HELM="${HELM:-/Users/roman/.local/share/mise/installs/helm/4.2.3/darwin-arm64/helm}"
exec python3 - "$ROOT" "$HELM" <<'PY'
import hashlib
import pathlib
import shlex
import subprocess
import sys
import tempfile

root = pathlib.Path(sys.argv[1])
helm = pathlib.Path(sys.argv[2])
(root / "target/probes").mkdir(parents=True, exist_ok=True)
scratch = pathlib.Path(tempfile.mkdtemp(prefix="r3b-", dir=root / "target/probes"))
version = subprocess.check_output([str(helm), "version"], text=True).strip()
if 'Version:"v4.2.3"' not in version:
    raise SystemExit(f"wrong Helm: {version}")
print(version)
print("helm_sha256:", hashlib.sha256(helm.read_bytes()).hexdigest())
print("command: bash plan/value-algebra-r3b-probes/review.sh")
print("scratch: $SCRATCH (unique directory under target/probes)\n")
failures = []
count = 0

def probe(name, body, want, rc=0, values="{}", files=None, mode="template", flags=()):
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
    print(f"### {name}\nvalues.yaml: {values!r}\ntemplate: {template.rstrip()}")
    for path, text in sorted((files or {}).items()):
        print(f"file {path}: {text.rstrip()}")
    print("command:", shlex.join(command).replace(str(scratch), "$SCRATCH"))
    print(f"exit: {run.returncode}\nexpected: exit {rc}; contains {want!r}\ncheck: {'PASS' if passed else 'FAIL'}\noutput:")
    print("\n".join("  " + line if line else "" for line in output.splitlines()))
    print()
    if not passed:
        failures.append(name)

probe("d01-arity-before-arguments", '{{ ternary (fail "later") }}', ["wrong number of args for ternary"], 1)
probe("d02-pipe-before-arity", '{{ fail "pipe-first" | ternary (fail "later") }}', ["pipe-first"], 1)
probe("d03-eager-left-to-right", '{{ define "mark" }}{{ $_ := set . "s" (print .s .next) }}{{ end }}{{ $d := dict "s" "" "next" "A" }}{{ $_ := ternary (include "mark" $d) (include "mark" (set $d "next" "B")) true }}{{ $d.s }}', ["v: AB"])
probe("d04-pipe-before-explicit", '{{ define "mark" }}{{ $_ := set . "s" (print .s .next) }}{{ end }}{{ $d := dict "s" "" "next" "P" }}{{ $_ := include "mark" $d | printf "%s%s" (include "mark" (set $d "next" "E")) }}{{ $d.s }}', ["v: PE"])
probe("d05-and-returns-operand", '{{ typeOf (and "yes" 7) }}/{{ typeOf (or "" 7) }}/{{ 7 | and false }}/{{ 7 | or "yes" }}', ["v: int/int/false/yes"])
probe("d06-coalesce-nil", '{{ $v := coalesce false 0 "" (dict) (list) }}{{ eq $v nil }}/{{ typeOf $v }}', ["v: true/<nil>"])
probe("d07-required-values", '{{ range $v := list false 0 (dict) (list) " " }}{{ $_ := required "bad" $v }}ok/{{ end }}', ["v: ok/ok/ok/ok/ok/"])
probe("d08-required-empty", '{{ required "empty" "" }}', ["empty"], 1)
probe("d09-required-nil", '{{ required "missing" (coalesce false) }}', ["missing"], 1)
probe("d10-required-empty-slice", '{{ $v := mustFromJson "[]" }}{{ $_ := required "bad" $v }}{{ printf "%s/%t" (typeOf $v) (eq $v nil) | quote }}', ['v: "[]interface {}/false"'])
probe("d11-map-index-scalar-snapshot", '{{ $d := dict "x" "old" }}{{ $v := index $d "x" }}{{ $_ := set $d "x" "new" }}{{ $v }}/{{ $d.x }}', ["v: old/new"])
probe("d12-map-range-snapshot-nested-live", '{{ $m := dict "x" "old" }}{{ $d := dict "a" 1 "b" $m }}{{ range $k,$v := $d }}{{ if eq $k "a" }}{{ $_ := set $d "b" (dict "x" "replacement") }}{{ $_ := set $m "x" "live" }}{{ else }}{{ $v.x }}{{ end }}{{ end }}', ["v: live"])
probe("d13-omit-pluck-alias", '{{ $m := dict "x" 0 }}{{ $d := dict "m" $m }}{{ $o := omit $d "none" }}{{ $p := pluck "m" $o }}{{ $_ := set (index $p 0) "x" 2 }}{{ $m.x }}', ["v: 2"])
probe("d14-append-prepend-sharing", '{{ $m := dict "x" 0 }}{{ $a := list $m }}{{ $b := prepend (append $a 9) 8 }}{{ $_ := set (index $b 1) "x" 3 }}{{ len $a }}/{{ $m.x }}', ["v: 1/3"])
probe("d15-merge-install-alias", '{{ $m := dict "x" 0 }}{{ $src := dict "m" $m }}{{ $dst := dict }}{{ $_ := merge $dst $src }}{{ $_ := set $dst.m "x" 4 }}{{ $src.m.x }}', ["v: 4"])
probe("d16-merge-existing-destination", '{{ $dst := dict "m" (dict "x" 1) }}{{ $src := dict "m" (dict "y" 2) }}{{ $_ := merge $dst $src }}{{ $_ := set $dst.m "y" 4 }}{{ $src.m.y }}/{{ $dst.m.x }}', ["v: 2/1"])
probe("d17-merge-overwrite", '{{ $dst := dict "x" 1 "m" (dict "z" 0) }}{{ $src := dict "x" 2 "m" (dict "z" 3) }}{{ $_ := mergeOverwrite $dst $src (dict "x" 4) }}{{ $dst.x }}/{{ $dst.m.z }}', ["v: 4/3"])
probe("d18-serialization-snapshot", '{{ $d := dict "x" 0 }}{{ $s := toJson $d }}{{ $_ := set $d "x" 1 }}{{ $s }}', ['v: {"x":0}'])
probe("d19-include-output-snapshot", '{{ define "emit" }}{{ .x }}{{ end }}{{ $d := dict "x" 0 }}{{ $s := include "emit" $d }}{{ $_ := set $d "x" 1 }}{{ $s }}/{{ include "emit" $d }}', ['v: 0/1'])
probe("d20-frame-shadow-assign", '{{ $x := "outer" }}{{ if true }}{{ $x := "inner" }}{{ $x = "changed" }}{{ $x }}/{{ end }}{{ $x }}{{ with dict }}{{ $x = "never" }}{{ else }}{{ $x = "else" }}{{ end }}/{{ $x }}', ["v: changed/outer/else"])
probe("d21-break-effects", '{{ $d := dict "x" 0 }}{{ range $i := until 3 }}{{ $_ := set $d "x" (add $d.x 1) }}{{ break }}{{ $_ := set $d "x" 99 }}{{ end }}{{ $d.x }}', ["v: 1"])
probe("d22-continue-effects", '{{ $d := dict "x" 0 }}{{ range $i := until 3 }}{{ $_ := set $d "x" (add $d.x 1) }}{{ continue }}{{ $_ := set $d "x" 99 }}{{ end }}{{ $d.x }}', ["v: 3"])
probe("d23-range-assignment-zero", '{{ $x := "outer" }}{{ range $x = list }}{{ end }}{{ printf "%s/%v" (typeOf $x) $x | quote }}', ['v: "[]interface {}/[]"'])
probe("d24-header-scope", '{{ $x := "outer" }}{{ if $x := "header" }}{{ $x }}{{ end }}/{{ $x }}', ["v: header/outer"])
probe("d25-live-any-operand", '{{ $s := splitList "," "b,a" }}{{ printf "%s/%s" (index $s 0) (join "," (sortAlpha $s)) }}', ["v: a/a,b"])
probe("d26-live-ternary-arm", '{{ $s := splitList "," "b,a" }}{{ ternary (index $s 0) (join "," (sortAlpha $s)) true }}', ["v: a"])
probe("d27-tostrings-alias", '{{ $s := splitList "," "b,a" }}{{ $v := toStrings $s }}{{ $_ := sortAlpha $v }}{{ index $s 0 }}', ["v: a"])
probe("d28-slice-capacity", '{{ $s := splitList "," "c,b,a" }}{{ $v := slice $s 0 1 }}{{ join "," (slice $v 0 3) }}', ["v: c,b,a"])
probe("d29-values-asmap-empty", '{{ $_ := set .Values.AsMap "x" 1 }}{{ hasKey .Values "x" }}', ["v: false"])
probe("d30-values-asmap-nonempty", '{{ $_ := set .Values.AsMap "x" 1 }}{{ .Values.x }}', ["v: 1"], values="seed: true")
probe("d31-values-table-alias", '{{ $_ := set (.Values.Table "child") "x" 1 }}{{ .Values.child.x }}', ["v: 1"], values="child: {seed: true}")
probe("d32-lint-required-fail", '{{ required "missing" .Values.missing }}{{ fail "ignored" }}ok', ["0 chart(s) failed"], mode="lint")
probe("d33-lint-message-type", '{{ required 1 "present" }}', ["expected string"], 1, mode="lint")
probe("d34-template-no-argument", '{{ define "h" }}{{ typeOf $ }}/{{ typeOf . }}{{ end }}{{ template "h" }}', ["v: <nil>/<nil>"])
probe("d35-helper-no-caller-local", '{{ $x := "caller" }}{{ define "h" }}{{ $x }}{{ end }}{{ include "h" . }}', ['undefined variable "$x"'], 1)
probe("d36-top-level-strip", 'left<no value>right', ["v: leftright"])
probe("d37-dict-coercion-odd-key", '{{ $d := dict 1 "one" "last" }}{{ get $d "1" }}/{{ printf "%q" (get $d "last") }}', ['v: one/""'])
probe("d38-fromjson-fresh", r'{{ $a := fromJson "{\"x\":0}" }}{{ $b := fromJson "{\"x\":0}" }}{{ $_ := set $a "x" 1 }}{{ $b.x }}', ["v: 0"])
probe("d39-required-live-subject", '{{ $s := splitList "," "b," }}{{ required (join "," (sortAlpha $s)) (index $s 0) }}', ["execution error"], 1)
probe("d40-global-sharing", '{{ $_ := set .Values.global "top" "parent" }}{{ $_ := set .Values.global.nest "x" "parent" }}{{ $_ := set .Values.global.nest.deep "x" "parent" }}{{ .Subcharts.child.Values.global.top }}/{{ .Subcharts.child.Values.global.nest.x }}/{{ .Subcharts.child.Values.global.nest.deep.x }}', ["v: old/old/parent"], values="global: {top: old, nest: {x: old, deep: {x: old}}}", files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "{}\n"})
probe("d41-release-shared-chart-separate", '{{ $_ := set .Subcharts.child.Release "Name" "shared" }}{{ $_ := set .Subcharts.child.Chart "Name" "childchanged" }}{{ .Release.Name }}/{{ .Chart.Name }}', ["v: shared/d41-release-shared-chart-separate"], files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "{}\n"})
probe("d42-render-order", '{{ .Values.order }}', ["v: DZA"], values='order: ""', files={"templates/deep/a.yaml": '{{ $_ := set .Values "order" (print .Values.order "D") }}\n', "templates/z.yaml": '{{ $_ := set .Values "order" (print .Values.order "Z") }}\n', "templates/y.yaml": '{{ $_ := set .Values "order" (print .Values.order "A") }}\n'})
probe("d43-parse-definition-precedence", '{{ include "shared" . }}', ["v: shallow"], files={"templates/deep/_h.tpl": '{{ define "shared" }}deep{{ end }}\n', "templates/_h.tpl": '{{ define "shared" }}shallow{{ end }}\n'})
probe("d44-template-replaced-per-file", '{{ .Template.Name }}/{{ .Values.saved }}', ["v: d44-template-replaced-per-file/templates/cm.yaml/d44-template-replaced-per-file/templates/z.yaml"], files={"templates/z.yaml": '{{ $_ := set .Values "saved" .Template.Name }}{{ $_ := set .Template "Name" "changed" }}\n'})
probe("d45-files-bytes-alias", '{{ $b := .Files.GetBytes "payload" }}{{ $v := slice $b 0 2 }}{{ printf "%T/%T" $b $v | quote }}', ['v: "[]uint8/[]uint8"'], files={"payload": "ba"})
probe("d46-map-range-rebind", '{{ $d := dict "a" "old-a" "b" "old-b" }}{{ range $k,$v := $d }}{{ $_ := set $d "b" "new-b" }}{{ $v }}/{{ end }}', ["v: old-a/old-b/"])
probe("d47-range-one-variable", '{{ range $v := list "a" "b" }}{{ $v }}{{ end }}', ["v: ab"])
probe("d48-nested-break", '{{ range $i := until 2 }}{{ range $j := until 2 }}{{ break }}{{ end }}{{ $i }}{{ end }}', ["v: 01"])
probe("d49-required-typed-nil-array", '{{ $v := fromJsonArray "null" }}{{ $_ := required "bad" $v }}{{ printf "%s/%t" (typeOf $v) (eq $v nil) | quote }}', ['v: "[]interface {}/true"'])
probe("d50-live-string-parameter", '{{ $s := splitList "," "b,a" }}{{ replace (index $s 0) (join "," (sortAlpha $s)) "b" }}', ["v: b"])
probe("d51-loop-fresh-objects", '{{ $a := list }}{{ range $i := until 2 }}{{ $d := dict "x" $i }}{{ $a = append $a $d }}{{ end }}{{ $_ := set (index $a 0) "x" 9 }}{{ (index $a 1).x }}', ["v: 1"])
probe("d52-helper-fresh-objects", '{{ define "new" }}{{ $d := dict "x" 0 }}{{ $_ := set . "m" $d }}{{ end }}{{ $a := dict }}{{ $b := dict }}{{ $_ := include "new" $a }}{{ $_ := include "new" $b }}{{ $_ := set $a.m "x" 9 }}{{ $b.m.x }}', ["v: 0"])
probe("d53-merge-nil-destination", '{{ $d := fromJson "null" }}{{ $r := merge $d (dict "x" 1) }}{{ eq $d nil }}/{{ $r.x }}', ["v: true/1"])
probe("d54-range-else-assignment", '{{ $x := "outer" }}{{ range $x = list }}{{ else }}{{ printf "%T" $x | quote }}{{ end }}', ['v: "[]interface {}"'])
probe("d55-exact-type-queries", '{{ typeIs "int64" (len "x") }}/{{ kindIs "int64" 1 }}/{{ kindIs "string" (len (list "a")) }}/{{ typeOf (len (list "a")) }}', ["v: false/false/false/int"])
probe("d56-append-string-slots-copy", '{{ $s := splitList "," "b,a" }}{{ $c := append $s "c" }}{{ $_ := sortAlpha $s }}{{ index $c 0 }}/{{ typeOf $c }}', ["v: b/[]interface {}"])
probe("d57-deepcopy-string-backing", '{{ $s := splitList "," "b,a" }}{{ $c := deepCopy $s }}{{ $_ := sortAlpha $c }}{{ index $s 0 }}/{{ index $c 0 }}', ["v: b/a"])
probe("d58-default-after-argument-write", '{{ $d := dict "x" 1 }}{{ default (unset $d "x") $d | len }}', ["v: 0"])
probe("d59-slot-versus-emitted-snapshot", '{{ $s := splitList "," "b,a" }}{{ $x := index $s 0 }}{{ $text := printf "%s" $x }}{{ $_ := sortAlpha $s }}{{ $x }}/{{ $text }}', ["v: a/b"])
probe("d60-strip-after-concatenation", '<no {{ print "value>" }}ok', ["v: ok"])
probe("d61-subchart-view-not-retargeted", '{{ $old := .Subcharts.child.Values }}{{ $_ := set .Values "child" (dict "x" "new") }}{{ $_ := set $old "x" "old" }}{{ .Values.child.x }}/{{ .Subcharts.child.Values.x }}', ["v: new/old"], files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "{}\n"})
probe("d62-files-backing-write", '{{ $b := .Files.GetBytes "payload" }}{{ $_ := (toDate "2006" "2026").AppendFormat (slice $b 0 0) "X" }}{{ .Files.Get "payload" }}', ["v: Xa"], files={"payload": "ba"})
probe("d63-global-parent-precedence", '{{ .Subcharts.child.Values.global.nest.x }}/{{ .Subcharts.child.Values.global.nest.onlychild }}', ["v: parent/child"], values="global: {nest: {x: parent}}", files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "global: {nest: {x: child, onlychild: child}}\n"})
probe("d64-child-render-shared-release", '{{ .Release.Name }}', ["v: child-write"], files={"charts/child/Chart.yaml": "apiVersion: v2\nname: child\nversion: 0.1.0\n", "charts/child/values.yaml": "{}\n", "charts/child/templates/write.yaml": '{{ $_ := set .Release "Name" "child-write" }}\n'})
probe("d65-lint-required-nil-becomes-string", '{{ if ne (typeOf (required "nil" .Values.missing)) "string" }}{{ required 1 "bad" }}{{ end }}ok', ["0 chart(s) failed"], mode="lint")
probe("d66-merge-empty-destination-field", '{{ $d := dict "x" 0 "s" "" }}{{ $_ := merge $d (dict "x" 2 "s" "filled") }}{{ $d.x }}/{{ $d.s }}', ["v: 2/filled"])
probe("d67-merge-overwrite-nested-alias", '{{ $nested := dict "x" 1 }}{{ $d := dict "m" $nested }}{{ $_ := mergeOverwrite $d (dict "m" (dict "x" 2)) }}{{ $nested.x }}', ["v: 2"])
probe("d68-slice-range-length-snapshot", '{{ $s := list "a" "b" }}{{ range $v := $s }}{{ $s = append $s "c" }}{{ $v }}{{ end }}/{{ len $s }}', ["v: ab/4"])
probe("d69-template-values-type", '{{ typeOf .Template }}', ["v: common.Values"])
probe("d70-template-object-snapshot", '{{ .Values.prior.Name }}/{{ .Template.Name }}', ["v: d70-template-object-snapshot/templates/z.yaml/d70-template-object-snapshot/templates/cm.yaml"], files={"templates/z.yaml": '{{ $_ := set .Values "prior" .Template }}\n'})
probe("d71-merge-populated-precedence", '{{ $d := dict "x" 1 }}{{ $_ := merge $d (dict "x" 2) (dict "x" 3) }}{{ $d.x }}', ["v: 1"])
probe("d72-mustmerge-success", '{{ $d := dict }}{{ $_ := mustMerge $d (dict "x" 2) }}{{ $_ := mustMergeOverwrite $d (dict "x" 3) }}{{ $d.x }}', ["v: 3"])
probe("d73-direct-complex-and-exponent", '{{ trunc 3+0i "abcdef" }}/{{ trunc 3e0 "abcdef" }}', ["v: abc/abc"])
probe("d74-grouped-complex", '{{ trunc (3+0i) "abcdef" }}', ["expected int; got complex128"], 1)
probe("d75-root-rebinding", '{{ $ = dict "x" "rebound" }}{{ .Values.x }}/{{ $.x }}', ["v: original/rebound"], values="x: original")
probe("d76-helper-break-boundary", '{{ define "h" }}{{ break }}{{ end }}{{ range until 1 }}{{ include "h" . }}{{ end }}', ['{{break}} outside {{range}}'], 1)
probe("d77-library-render-filter", '{{ include "library-helper" . }}', ["v: ok"], files={"charts/lib/Chart.yaml": "apiVersion: v2\nname: lib\nversion: 0.1.0\ntype: library\n", "charts/lib/templates/wrong.yaml": '{{ fail "library ordinary file ran" }}\n', "charts/lib/templates/_h.tpl": '{{ define "library-helper" }}ok{{ end }}\n'})
probe("d78-partial-not-rendered", '{{ include "partial-helper" . }}', ["v: ok"], files={"templates/_h.tpl": '{{ fail "partial body ran" }}{{ define "partial-helper" }}ok{{ end }}\n'})
print(f"TOTAL: {count}; passed: {count-len(failures)}; failed: {len(failures)}")
if failures:
    print("FAILED:", ", ".join(failures))
    raise SystemExit(1)
PY
