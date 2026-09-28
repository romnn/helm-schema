package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"testing"
)

// testdata/helm-classes.json is the diagnostic -> class table the runner's
// landing.py classify_helm is tested against too (a byte-identical copy in
// the runner's tests/): one fixture per class, with the Python text quirks
// the port reproduces.
func TestClassifyMatchesTheGate(t *testing.T) {
	data, err := os.ReadFile(filepath.Join("testdata", "helm-classes.json"))
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Name, Mode, Log, Class string
		RC                     int
	}
	if err := json.Unmarshal(data, &cases); err != nil {
		t.Fatal(err)
	}
	if len(cases) == 0 {
		t.Fatal("testdata/helm-classes.json holds no cases")
	}
	for _, c := range cases {
		if got := classify(c.Mode, c.RC, []byte(c.Log)); got != c.Class {
			t.Errorf("%s: classify = %q, want %q", c.Name, got, c.Class)
		}
	}
	// JSON cannot carry invalid UTF-8; Python reads it as U+FFFD.
	if got := classify("template", 1, []byte("Error: execution error at (\xff\xfe")); got != "reject" {
		t.Errorf("invalid UTF-8: classify = %q, want reject", got)
	}
}

// Every record that could change a class bumps the sink's counter.
func TestSensitiveRecords(t *testing.T) {
	for record, want := range map[string]bool{
		"level=INFO msg=\"funcMap fail\" message=\"Image tags must be strings.\"\n": false,
		"level=INFO msg=\"warning: skipped value for x: Not a table.\"\n":           false,
		"level=INFO msg=\"funcMap fail\" message=\"unable to detect chart\"\n":      true,
		"level=INFO msg=\"funcMap fail\" message=\"chart requires kubeVersion\"\n":  true,
		"level=INFO msg=\"x [ERROR] y\"\n":                                          true,
		"level=WARN msg=\"values don't meet the specifications of the schema\"\n":   true,
	} {
		if got := sensitiveRecord(record); got != want {
			t.Errorf("sensitiveRecord(%q) = %t, want %t", record, got, want)
		}
	}
}

// Helm's own failures, run in-process, reach the gate's unresolved classes
// (the CLI's (rc, class) for the same cells: evidence infra-cli.tsv).
func TestHelmCellsReachTheInfrastructureClasses(t *testing.T) {
	isolateEnv(t)
	root := t.TempDir()
	chart := "apiVersion: v2\nname: demo\nversion: 0.1.0\n"
	writeFiles(t, root, map[string]string{
		"values.json":            "{}",
		"ok/Chart.yaml":          chart,
		"ok/templates/cm.yaml":   "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: x\n",
		"nochart/values.yaml":    "a: 1\n",
		"dep/Chart.yaml":         chart + "dependencies:\n- name: sub\n  version: 1.0.0\n  repository: https://example.invalid\n",
		"dep/templates/cm.yaml":  "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: x\n",
		"kube/Chart.yaml":        chart + "kubeVersion: \">=1.40.0-0\"\n",
		"kube/templates/cm.yaml": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: x\n",
		"lib/Chart.yaml":         "apiVersion: v2\nname: lib\nversion: 0.1.0\ntype: library\n",
		"lib/templates/_h.tpl":   "{{- define \"lib.x\" -}}x{{- end -}}\n",
	})
	values := filepath.Join(root, "values.json")
	cases := []struct {
		mode, chart, kube, values, want string
	}{
		{"lint", "ok", "1.29.0", "missing.json", "1 unresolved:values-file"},
		{"template", "ok", "1.29.0", "missing.json", "1 unresolved:values-file"},
		{"lint", "nochart", "1.29.0", "", "1 unresolved:loader"},
		{"template", "nochart", "1.29.0", "", "1 unresolved:loader"},
		{"lint", "lib", "1.29.0", "", "0 pass"},
		{"template", "lib", "1.29.0", "", "1 unresolved:library-chart"},
		{"lint", "dep", "1.29.0", "", "0 unresolved:missing-dependency"},
		{"template", "dep", "1.29.0", "", "1 unresolved:missing-dependency"},
		{"lint", "kube", "1.29.0", "", "0 pass"},
		{"template", "kube", "1.29.0", "", "1 unresolved:kube-version-incompatible"},
		{"lint", "ok", "1.x", "", "1 unresolved:invalid-kube-version"},
		{"template", "ok", "1.x", "", "1 unresolved:invalid-kube-version"},
		{"lint", "ok", "1.29.0", "", "0 pass"},
		{"template", "ok", "1.29.0", "", "0 pass"},
	}
	for _, c := range cases {
		file := values
		if c.values != "" {
			file = filepath.Join(root, c.values)
		}
		rc, log := runCell(c.mode, filepath.Join(root, c.chart), c.kube, file)
		if got := fmt.Sprintf("%d %s", rc, classify(c.mode, rc, log)); got != c.want {
			t.Errorf("%s %s %s: %s, want %s; log:\n%s", c.mode, c.chart, c.kube, got, c.want, log)
		}
	}
	// An unreadable Chart.yaml is not the adjudicated missing-file loader case.
	// Only where the mode really denies reading (not on Windows or as root).
	t.Run("unreadable Chart.yaml", func(t *testing.T) {
		writeFiles(t, root, map[string]string{"unreadable/Chart.yaml": chart})
		path := filepath.Join(root, "unreadable/Chart.yaml")
		if err := os.Chmod(path, 0); err != nil {
			t.Fatal(err)
		}
		if _, err := os.ReadFile(path); !errors.Is(err, fs.ErrPermission) {
			t.Skipf("mode 000 does not deny reading here (%v)", err)
		}
		for mode, want := range map[string]string{"template": "1 unresolved:unknown-template-failure", "lint": "1 unresolved:unknown-lint-failure"} {
			rc, log := runCell(mode, filepath.Join(root, "unreadable"), "1.29.0", values)
			if got := fmt.Sprintf("%d %s", rc, classify(mode, rc, log)); got != want {
				t.Errorf("%s unreadable: %s, want %s; log:\n%s", mode, got, want, log)
			}
		}
	})
}

func TestClassifyCommandReportsAMissingLog(t *testing.T) {
	var out, errs bytes.Buffer
	if rc := run([]string{"classify", "lint", "0", filepath.Join(t.TempDir(), "absent.log")}, &out, &errs); rc != 0 || out.String() != "unresolved:no-log\n" {
		t.Fatalf("rc=%d out=%q err=%q", rc, out.String(), errs.String())
	}
}
