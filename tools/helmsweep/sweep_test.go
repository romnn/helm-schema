package main

import (
	"bytes"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A chart whose rows cover a schema rejection, a template abort, a template
// abort whose message names Helm's missing-Chart.yaml error (the lint cells'
// log records name it, so they must be attributed; the gate classifies the
// outer diagnostic, so every cell keeps its verdict), and a values file Helm
// cannot parse (every cell unresolved, so never cached).
var fixtureChart = map[string]string{
	"Chart.yaml":  "apiVersion: v2\nname: demo\nversion: 0.1.0\n",
	"values.yaml": "replicas: 1\nname: demo\nboom: false\nmessage: boom requested\n",
	"templates/cm.yaml": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {{ .Values.name }}\ndata:\n" +
		"  replicas: {{ .Values.replicas | quote }}\n{{- if .Values.boom }}{{ fail .Values.message }}{{ end }}\n",
}

const (
	fixtureBase = `{"$schema":"http://json-schema.org/draft-07/schema#","type":"object"}` + "\n"
	fixtureCand = `{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","properties":{"replicas":{"type":"integer"}}}` + "\n"
)

var fixtureRows = []struct{ name, values string }{
	{"defaults", "{}"},
	{"replicas", `{"replicas":"x"}`},
	{"boom", `{"boom":true}`},
	{"boom+loader", `{"boom":true,"message":"unable to detect chart at /x/Chart.yaml: open /x/Chart.yaml: no such file or directory"}`},
	{"unparsable", `{"replicas":`},
}

// The Helm v4.2.3 CLI's (rc, class) per row for base, cand and plain, as
// adjudicated in the evidence directory (fixture-cli.tsv).
var fixtureWant = [][3]string{
	{"0 pass", "0 pass", "0 pass"},
	{"0 pass", "1 reject", "0 pass"},
	{"0 pass", "0 pass", "1 reject"},
	{"0 pass", "0 pass", "1 reject"},
	{"1 unresolved:values-file", "1 unresolved:values-file", "1 unresolved:values-file"},
}

func sha(data string) string { return fmt.Sprintf("%x", sha256.Sum256([]byte(data))) }

func writeFiles(t *testing.T, root string, files map[string]string) {
	for name, data := range files {
		path := filepath.Join(root, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(data), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

// fixtureSweep lays out a frozen plan and prepared copies like the runner's
// and returns the roster path, its sha256 and the work directory.
func fixtureSweep(t *testing.T) (string, string, string) {
	dir := t.TempDir()
	work := filepath.Join(dir, "w")
	var roster strings.Builder
	roster.WriteString(rosterHeader + "\n")
	plan := map[string]string{"cand.schema.json": fixtureCand, "base.schema.json": fixtureBase}
	for id, row := range fixtureRows {
		plan[fmt.Sprintf("ov/%d.json", id)] = row.values
		plan[fmt.Sprintf("ov/%d.name", id)] = row.name
		fmt.Fprintf(&roster, "demo\t%d\t%s\t%s\tvalues-file\t1.29.0\tbaseline\t%s\t%s\n", id, row.name, sha(row.values), sha(fixtureCand), sha(fixtureBase))
	}
	writeFiles(t, filepath.Join(dir, "plan", "demo"), plan)
	for _, cell := range cells {
		writeFiles(t, filepath.Join(work, "demo", cell), fixtureChart)
	}
	writeFiles(t, filepath.Join(work, "demo", "base"), map[string]string{schemaFile: fixtureBase})
	writeFiles(t, filepath.Join(work, "demo", "cand"), map[string]string{schemaFile: fixtureCand})
	writeFiles(t, dir, map[string]string{"roster.tsv": roster.String()})
	return filepath.Join(dir, "roster.tsv"), sha(roster.String()), work
}

func newSweeper(t *testing.T, roster, work string) *sweeper {
	id, err := buildIdentity()
	if err != nil {
		t.Fatal(err)
	}
	return &sweeper{
		work: work, plan: filepath.Join(filepath.Dir(roster), "plan"), id: id, cache: verdictCache{filepath.Join(filepath.Dir(roster), "cache")},
		stderr: &bytes.Buffer{}, compile: make(chan struct{}, 1), warmed: map[[32]byte]bool{},
	}
}

// isolateEnv gives the test the environment cleanEnv gives a clean child,
// HOME, PATH and the caller's temporary-directory variables, restoring the
// original afterwards. The kept variables keep t.TempDir and Helm's own
// temporary files where the caller put them, not in the system temp.
func isolateEnv(t *testing.T) {
	kept := map[string]string{}
	for _, name := range keptTempVars {
		if value, ok := os.LookupEnv(name); ok {
			kept[name] = value
		}
	}
	for _, kv := range os.Environ() {
		name, _, _ := strings.Cut(kv, "=")
		t.Setenv(name, "")
		if err := os.Unsetenv(name); err != nil {
			t.Fatal(err)
		}
	}
	for name, value := range kept {
		t.Setenv(name, value)
	}
	t.Setenv("HOME", t.TempDir())
	t.Setenv("PATH", "/usr/bin:/bin")
}

func sweepOnce(t *testing.T, roster, rosterSHA, work string, jobs int) {
	s := newSweeper(t, roster, work)
	charts, err := s.prepare(roster, rosterSHA)
	if err != nil {
		t.Fatal(err)
	}
	s.sink = installLogSink(&bytes.Buffer{})
	s.run(charts, jobs)
	if len(s.failures) > 0 {
		t.Fatal(s.failures)
	}
}

// removeOutputs moves a chart's outputs to keep/ so the next sweep may run.
func removeOutputs(t *testing.T, work string) map[string][]byte {
	outputs := map[string][]byte{}
	entries, err := os.ReadDir(filepath.Join(work, "demo"))
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		path := filepath.Join(work, "demo", e.Name())
		if outputs[e.Name()], err = os.ReadFile(path); err != nil {
			t.Fatal(err)
		}
		if err := os.Remove(path); err != nil {
			t.Fatal(err)
		}
	}
	return outputs
}

// The sweep writes sweep-one.sh's rows and logs with the CLI's verdicts, on
// four workers under -race; a second sweep hits the cache for every cell and
// writes byte-identical outputs.
func TestSweepWritesTheGateFilesAndHitsOnRerun(t *testing.T) {
	roster, rosterSHA, work := fixtureSweep(t)
	isolateEnv(t)
	sweepOnce(t, roster, rosterSHA, work, 4)
	first := removeOutputs(t, work)

	var rows strings.Builder
	for id, row := range fixtureRows {
		fmt.Fprintf(&rows, "demo\t%d\t%s\t%s\tvalues-file\t1.29.0\tbaseline", id, row.name, sha(row.values))
		for cell, want := range fixtureWant[id] {
			rc, class, _ := strings.Cut(want, " ")
			fmt.Fprintf(&rows, "\t%s", rc)
			log := first[fmt.Sprintf("%s.%d.log", cells[cell], id)]
			mode := map[bool]string{true: "template", false: "lint"}[cell == 2]
			if got := classify(mode, int(rc[0]-'0'), log); got != class {
				t.Errorf("row %d %s: class %q, want %q; log:\n%s", id, cells[cell], got, class, log)
			}
		}
		rows.WriteString("\n")
	}
	if string(first["rows.tsv"]) != rows.String() {
		t.Fatalf("rows.tsv:\n%s\nwant:\n%s", first["rows.tsv"], rows.String())
	}

	sweepOnce(t, roster, rosterSHA, work, 4)
	second := removeOutputs(t, work)
	for name, data := range first {
		if name == "cells.tsv" {
			continue
		}
		if !bytes.Equal(second[name], data) {
			t.Errorf("%s differs on the cached rerun", name)
		}
	}
	// Every identified verdict hits; the unresolved row (4) misses again.
	if hits := strings.Count(string(second["cells.tsv"]), "\thit\t"); hits != 3*len(fixtureRows)-3 {
		t.Errorf("cached rerun hit %d cells, want %d:\n%s", hits, 3*len(fixtureRows)-3, second["cells.tsv"])
	}
	for _, cell := range cells {
		if !strings.Contains(string(second["cells.tsv"]), fmt.Sprintf("demo\t4\t%s\t1\tunresolved:values-file\tmiss\t", cell)) {
			t.Errorf("the unresolved %s cell of row 4 did not miss on the rerun:\n%s", cell, second["cells.tsv"])
		}
	}
}

// Every mismatch between the roster, the plan and the prepared copies refuses
// the sweep before any cell runs.
func TestSweepRefusesUnverifiedInputs(t *testing.T) {
	cases := map[string]func(t *testing.T, roster, work string) string{
		"roster hash": func(t *testing.T, roster, work string) string { return strings.Repeat("0", 64) },
		"roster header": func(t *testing.T, roster, work string) string {
			return rewriteRoster(t, roster, func(s string) string { return strings.Replace(s, "chart\t", "Chart\t", 1) })
		},
		"roster transport": func(t *testing.T, roster, work string) string {
			return rewriteRoster(t, roster, func(s string) string { return strings.Replace(s, "values-file", "set", 1) })
		},
		"roster kube version": func(t *testing.T, roster, work string) string {
			return rewriteRoster(t, roster, func(s string) string { return strings.Replace(s, "1.29.0", "1.29", 1) })
		},
		"override bytes": func(t *testing.T, roster, work string) string {
			writeFiles(t, filepath.Dir(roster), map[string]string{"plan/demo/ov/1.json": `{"replicas":"y"}`})
			return ""
		},
		"override name": func(t *testing.T, roster, work string) string {
			writeFiles(t, filepath.Dir(roster), map[string]string{"plan/demo/ov/1.name": "other"})
			return ""
		},
		"plan schema": func(t *testing.T, roster, work string) string {
			writeFiles(t, filepath.Dir(roster), map[string]string{"plan/demo/cand.schema.json": fixtureBase})
			return ""
		},
		"cand copy schema": func(t *testing.T, roster, work string) string {
			writeFiles(t, work, map[string]string{"demo/cand/values.schema.json": fixtureBase})
			return ""
		},
		"base copy extra file": func(t *testing.T, roster, work string) string {
			writeFiles(t, work, map[string]string{"demo/base/templates/extra.yaml": "x: 1\n"})
			return ""
		},
		"plain copy schema": func(t *testing.T, roster, work string) string {
			writeFiles(t, work, map[string]string{"demo/plain/values.schema.json": fixtureBase})
			return ""
		},
		"nested schema": func(t *testing.T, roster, work string) string {
			for _, cell := range cells {
				writeFiles(t, work, map[string]string{"demo/" + cell + "/charts/sub/values.schema.json": "{}"})
			}
			return ""
		},
		"symlink": func(t *testing.T, roster, work string) string {
			if err := os.Symlink("values.yaml", filepath.Join(work, "demo", "plain", "link.yaml")); err != nil {
				t.Fatal(err)
			}
			return ""
		},
		"existing output": func(t *testing.T, roster, work string) string {
			writeFiles(t, work, map[string]string{"demo/base.0.log": ""})
			return ""
		},
		"nonlocal ref": func(t *testing.T, roster, work string) string {
			return replaceSchema(t, roster, work, `{"$ref":"https://example.com/s.json"}`)
		},
		"unknown $schema": func(t *testing.T, roster, work string) string {
			return replaceSchema(t, roster, work, `{"$schema":"https://example.com/meta"}`)
		},
	}
	for name, damage := range cases {
		roster, rosterSHA, work := fixtureSweep(t)
		if got := damage(t, roster, work); got != "" {
			rosterSHA = got
		}
		if _, err := newSweeper(t, roster, work).prepare(roster, rosterSHA); err == nil {
			t.Errorf("%s: prepare accepted it", name)
		}
	}
}

// rewriteRoster edits the roster and returns its new hash.
func rewriteRoster(t *testing.T, roster string, edit func(string) string) string {
	data, err := os.ReadFile(roster)
	if err != nil {
		t.Fatal(err)
	}
	text := edit(string(data))
	writeFiles(t, filepath.Dir(roster), map[string]string{"roster.tsv": text})
	return sha(text)
}

// replaceSchema installs schema as the candidate everywhere, consistently.
func replaceSchema(t *testing.T, roster, work, schema string) string {
	writeFiles(t, filepath.Dir(roster), map[string]string{"plan/demo/cand.schema.json": schema})
	writeFiles(t, work, map[string]string{"demo/cand/values.schema.json": schema})
	return rewriteRoster(t, roster, func(s string) string { return strings.ReplaceAll(s, sha(fixtureCand), sha(schema)) })
}
