package main

import (
	"bytes"
	"fmt"
	"path/filepath"
	"testing"
)

// One fixture per class of landing.py classify_helm, with the Python text
// quirks the port reproduces.
func TestClassifyMatchesTheGate(t *testing.T) {
	const (
		linted = "==> Linting /w/c/cand\n"
		failed = "\nError: 1 chart(s) linted, 1 chart(s) failed\n"
	)
	cases := []struct {
		name, mode string
		rc         int
		log        string
		want       string
	}{
		{"lint pass", "lint", 0, linted + "\n1 chart(s) linted, 0 chart(s) failed\n", "pass"},
		{"lint pass with warnings", "lint", 0, "level=INFO msg=x\n" + linted + "[INFO] Chart.yaml: icon is recommended\n\n1 chart(s) linted, 0 chart(s) failed\n", "pass"},
		{"lint values reject", "lint", 1, linted + "[ERROR] values.yaml: - at '': false schema\n" + failed, "reject"},
		{"lint template reject", "lint", 1, linted + "[ERROR] templates/: values don't meet the specifications of the schema(s)\n" + failed, "reject"},
		{"lint other error", "lint", 1, linted + "[ERROR] Chart.yaml: version is required\n" + failed, "unresolved:unknown-lint-failure"},
		{"lint error without [ERROR]", "lint", 1, linted + failed, "unresolved:unknown-lint-failure"},
		{"lint no summary", "lint", 0, linted, "unresolved:no-lint-summary"},
		{"lint U+2028 starts a line", "lint", 1, linted + "[ERROR] values.yaml: x [ERROR] Chart.yaml: y\n" + failed, "unresolved:unknown-lint-failure"},
		{"lint CR is a line break", "lint", 0, linted + "\r\n1 chart(s) linted, 0 chart(s) failed\r\n", "pass"},
		{"abnormal exit", "lint", 2, "panic: boom\n", "unresolved:exit-2"},
		{"abnormal template exit", "template", 137, "", "unresolved:exit-137"},
		{"missing dependency", "template", 1, "Error: an error occurred while checking for chart dependencies. You may need to run 'helm dependency build' to fetch missing dependencies: found in Chart.yaml, but missing in charts/ directory: x\n", "unresolved:missing-dependency"},
		{"loader", "lint", 1, "==> Linting /c\nError unable to check Chart.yaml file in chart: stat /c/Chart.yaml: no such file or directory\n" + failed, "unresolved:loader"},
		{"loader in a log record", "lint", 0, "level=INFO msg=\"funcMap fail\" message=\"unable to load chart\"\n" + linted + "\n1 chart(s) linted, 0 chart(s) failed\n", "unresolved:loader"},
		{"invalid kube version", "lint", 1, "Error: invalid kube version '1.x': bad\n", "unresolved:invalid-kube-version"},
		{"kube version incompatible", "template", 1, "Error: chart requires kubeVersion: >=1.30 which is incompatible with Kubernetes v1.29.0\n", "unresolved:kube-version-incompatible"},
		{"values file", "lint", 1, "Error: open /p/ov/1.json: no such file or directory\n", "unresolved:values-file"},
		{"values file parse", "template", 1, "Error: failed to parse /p/ov/1.json: error converting YAML to JSON\n", "unresolved:values-file"},
		{"usage", "template", 1, "Error: unknown flag: --nope\n", "unresolved:usage"},
		{"template pass", "template", 0, "level=WARN msg=x\n", "pass"},
		{"template execution error", "template", 1, "Error: execution error at (c/templates/a.yaml:2:4): no\n\nUse --debug flag to render out invalid YAML\n", "reject"},
		{"template parse error", "template", 1, "Error: parse error at (c/templates/a.yaml:3): unexpected EOF\n", "reject"},
		{"template yaml error", "template", 1, "Error: YAML parse error on c/templates/a.yaml: error converting YAML to JSON\n", "reject"},
		{"template executing", "template", 1, "Error: template: c/templates/a.yaml:3:5: executing \"x\" at <.Values.a.b>: nil pointer\n", "reject"},
		{"template multi-line executing", "template", 1, "Error: c/templates/a.yaml:3:5\n  executing \"x\" at <y>: z\n", "reject"},
		{"template multi-line executing, Unicode space", "template", 1, "Error: c/templates/a.yaml:3:5\n executing \"x\" at <y>: z\n", "reject"},
		{"template schema in control", "template", 1, "Error: values don't meet the specifications of the schema(s) in the following chart(s):\n", "unresolved:schema-in-control"},
		{"template unknown", "template", 1, "Error: something else\n", "unresolved:unknown-template-failure"},
		{"invalid UTF-8", "template", 1, "Error: execution error at (\xff\xfe", "reject"},
	}
	for _, c := range cases {
		if got := classify(c.mode, c.rc, []byte(c.log)); got != c.want {
			t.Errorf("%s: classify = %q, want %q", c.name, got, c.want)
		}
	}
}

// Every record that could change a class bumps the sink's counter.
func TestSensitiveRecords(t *testing.T) {
	for record, want := range map[string]bool{
		"level=INFO msg=\"funcMap fail\" message=\"Image tags must be strings.\"\n": false,
		"level=INFO msg=\"warning: skipped value for x: Not a table.\"\n":           false,
		"level=INFO msg=\"funcMap fail\" message=\"unable to load chart\"\n":        true,
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
	})
	values := filepath.Join(root, "values.json")
	cases := []struct {
		mode, chart, kube, values, want string
	}{
		{"lint", "ok", "1.29.0", "missing.json", "1 unresolved:values-file"},
		{"template", "ok", "1.29.0", "missing.json", "1 unresolved:values-file"},
		{"lint", "nochart", "1.29.0", "", "1 unresolved:loader"},
		{"template", "nochart", "1.29.0", "", "1 unresolved:unknown-template-failure"},
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
}

func TestClassifyCommandReportsAMissingLog(t *testing.T) {
	var out, errs bytes.Buffer
	if rc := run([]string{"classify", "lint", "0", filepath.Join(t.TempDir(), "absent.log")}, &out, &errs); rc != 0 || out.String() != "unresolved:no-log\n" {
		t.Fatalf("rc=%d out=%q err=%q", rc, out.String(), errs.String())
	}
}
