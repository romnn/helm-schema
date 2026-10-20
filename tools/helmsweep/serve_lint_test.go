package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func lintRequestLine(t *testing.T, dir string, id int, values string) string {
	t.Helper()
	data, err := json.Marshal(map[string]any{
		"id": id, "op": "lint", "chart": filepath.Join(dir, "demo"),
		"kube_version": "1.29.0", "values": []string{filepath.Join(dir, values)},
		"stdout_path": filepath.Join(dir, fmt.Sprintf("%d.stdout", id)),
		"stderr_path": filepath.Join(dir, fmt.Sprintf("%d.stderr", id)),
	})
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

// A lint rejection keeps command output on stdout and cobra's final error on stderr.
// Subsequent requests reread values and schema bytes at the same paths.
func TestServeLintKeepsStreamsAndReadsFreshInputs(t *testing.T) {
	isolateEnv(t)
	dir, _ := serveFixture(t)
	for id, cell := range []struct {
		schema, values string
		code           int
	}{
		{fixtureCand, `{"replicas":"bad"}`, 1},
		{fixtureCand, `{"replicas":2}`, 0},
		{`{"properties":{"replicas":{"type":"string"}}}`, `{"replicas":2}`, 1},
	} {
		writeFiles(t, filepath.Join(dir, "demo"), map[string]string{schemaFile: cell.schema})
		writeFiles(t, dir, map[string]string{"changing.json": cell.values})
		answers := serveAll(t, []string{lintRequestLine(t, dir, id, "changing.json")})
		answer := answers[int64(id)]
		stdout, err := os.ReadFile(filepath.Join(dir, fmt.Sprintf("%d.stdout", id)))
		if err != nil {
			t.Fatal(err)
		}
		stderr, err := os.ReadFile(filepath.Join(dir, fmt.Sprintf("%d.stderr", id)))
		if err != nil {
			t.Fatal(err)
		}
		if answer.Error != "" || answer.ExitCode == nil || *answer.ExitCode != cell.code {
			t.Fatalf("cell %d: %+v", id, answer)
		}
		if !strings.HasPrefix(string(stdout), "==> Linting ") || strings.Contains(string(stdout), "Error: 1 chart(s)") {
			t.Errorf("cell %d stdout %q", id, stdout)
		}
		wantError := ""
		if cell.code == 1 {
			wantError = "Error: 1 chart(s) linted, 1 chart(s) failed\n"
		}
		if string(stderr) != wantError {
			t.Errorf("cell %d stderr %q, want %q", id, stderr, wantError)
		}
	}
}

// Each command gets the warning a fresh CLI emits, even after the same URN was used.
func TestServeLintResetsUnresolvedURNWarnings(t *testing.T) {
	isolateEnv(t)
	dir, _ := serveFixture(t)
	writeFiles(t, filepath.Join(dir, "demo"), map[string]string{
		schemaFile: `{"$ref":"urn:resident-prototype:unresolved"}`,
	})
	answers := serveAll(t, []string{
		lintRequestLine(t, dir, 0, "v0.json"),
		lintRequestLine(t, dir, 1, "v0.json"),
	})
	var first string
	for id := range 2 {
		answer := answers[int64(id)]
		if answer.Error != "" || answer.ExitCode == nil || *answer.ExitCode != 0 {
			t.Fatalf("cell %d: %+v", id, answer)
		}
		stderr, err := os.ReadFile(filepath.Join(dir, fmt.Sprintf("%d.stderr", id)))
		if err != nil {
			t.Fatal(err)
		}
		if strings.Count(string(stderr), "unresolved URN reference ignored") != 1 {
			t.Fatalf("cell %d stderr %q", id, stderr)
		}
		if id == 0 {
			first = string(stderr)
		} else if string(stderr) != first {
			t.Errorf("second warning %q differs from first %q", stderr, first)
		}
	}
}
