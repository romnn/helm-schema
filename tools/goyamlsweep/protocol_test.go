package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode"
)

func TestUnicodeProtocolUsesPinnedOperations(t *testing.T) {
	result, err := yamlRespond(yamlRequest{Op: "unicode", From: 0xd7ff, To: 0xe001})
	if err != nil {
		t.Fatal(err)
	}
	points, ok := result.([]unicodeResult)
	if !ok || len(points) != 2 {
		t.Fatalf("unexpected scalar range: %#v", result)
	}
	for _, point := range points {
		ch := rune(point.Codepoint) //nolint:gosec // G115: The requested fixture range is bounded above by 0xe001.
		if point.Quote != yamlQuote(string(ch)) || point.Space != unicode.IsSpace(ch) {
			t.Fatalf("changed Unicode operation: %+v", point)
		}
	}
}

func TestRenderDefaultsDoesNotReadCLIConfiguration(t *testing.T) {
	chart := t.TempDir()
	if err := os.Mkdir(filepath.Join(chart, "templates"), 0o755); err != nil {
		t.Fatal(err)
	}
	for name, body := range map[string]string{
		"Chart.yaml":              "apiVersion: v2\nname: oracle-fixture\nversion: 0.1.0\n",
		"templates/manifest.yaml": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {{ .Release.Name }}\n  namespace: {{ .Release.Namespace }}\n",
	} {
		if err := os.WriteFile(filepath.Join(chart, name), []byte(body), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("HELM_NAMESPACE", "must-not-use")
	t.Setenv("KUBECONFIG", filepath.Join(chart, "missing-kubeconfig"))
	result := renderDefaults(chart, "1.29.0")
	if result.Error != nil {
		t.Fatal(*result.Error)
	}
	if !strings.Contains(result.Stdout, "name: t\n") || !strings.Contains(result.Stdout, "namespace: default\n") {
		t.Fatalf("rendered non-default configuration: %s", result.Stdout)
	}
}
