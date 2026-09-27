package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

// serveAll feeds lines to one server and returns its answers by id (null id
// under -1), requiring one answer per request, in request order.
func serveAll(t *testing.T, lines []string) map[int64]serveResponse {
	var stdout, stderr bytes.Buffer
	if rc := cmdServe(nil, strings.NewReader(strings.Join(lines, "\n")+"\n"), &stdout, &stderr); rc != 0 {
		t.Errorf("serve exited %d: %s", rc, stderr.String())
	}
	answers := map[int64]serveResponse{}
	out := strings.Split(strings.TrimSuffix(stdout.String(), "\n"), "\n")
	if len(out) != len(lines) {
		t.Errorf("%d answers for %d requests:\n%s", len(out), len(lines), stdout.String())
		return answers
	}
	for i, line := range out {
		var response serveResponse
		if err := json.Unmarshal([]byte(line), &response); err != nil {
			t.Errorf("answer %q: %v", line, err)
			continue
		}
		id := int64(-1)
		if response.ID != nil {
			id = int64(*response.ID)
		}
		var request struct{ ID *int64 }
		if json.Unmarshal([]byte(lines[i]), &request) == nil && request.ID != nil && *request.ID != id {
			t.Errorf("answer %d has id %d, request %d", i, id, *request.ID)
		}
		answers[id] = response
	}
	return answers
}

type probe struct {
	chart, values string
}

func requestLine(t *testing.T, dir string, id int, p probe) string {
	data, err := json.Marshal(map[string]any{
		"id": id, "op": "template", "release": "adjudication", "chart": filepath.Join(dir, p.chart),
		"kube_version": "1.29.0", "values": []string{filepath.Join(dir, p.values)}, "skip_schema_validation": true,
		"stdout_path": filepath.Join(dir, fmt.Sprintf("%d.stdout", id)), "stderr_path": filepath.Join(dir, fmt.Sprintf("%d.stderr", id)),
	})
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

// serveFixture writes the sweep fixture chart, a deprecated copy (every
// render logs a record) and a set of values files.
func serveFixture(t *testing.T) (string, []probe) {
	dir := t.TempDir()
	writeFiles(t, filepath.Join(dir, "demo"), fixtureChart)
	deprecated := map[string]string{}
	for name, data := range fixtureChart {
		deprecated[name] = data
	}
	deprecated["Chart.yaml"] += "deprecated: true\n"
	writeFiles(t, filepath.Join(dir, "old"), deprecated)
	var probes []probe
	for id, row := range fixtureRows {
		name := fmt.Sprintf("v%d.json", id)
		writeFiles(t, dir, map[string]string{name: row.values})
		probes = append(probes, probe{"demo", name}, probe{"old", name})
	}
	probes = append(probes, probe{"demo", "absent.json"})
	return dir, probes
}

// Each request is answered, in order, with exactly what one Helm CLI call
// prints: the same stdout, stderr (log records included) and exit code as a
// lone run, also with several servers rendering at once. Run under -race.
func TestServeAnswersEachRequestAsOneCLICall(t *testing.T) {
	isolateEnv(t)
	dir, probes := serveFixture(t)
	sink := installLogSink(&bytes.Buffer{})
	type want struct {
		rc             int
		stdout, stderr []byte
	}
	wants := make([]want, len(probes))
	for i, p := range probes {
		var stdout []byte
		rc, stderr := sink.exclusive(func() (int, []byte) {
			rc, out, errText := templateRun("adjudication", filepath.Join(dir, p.chart), "1.29.0", []string{filepath.Join(dir, p.values)}, true)
			stdout = out
			return rc, errText
		})
		wants[i] = want{rc, stdout, stderr}
	}
	// Four servers side by side, as the battery runs them, each serving every probe twice.
	var wg sync.WaitGroup
	for server := range 4 {
		wg.Go(func() {
			base := server * 2 * len(probes)
			var lines []string
			for i := range 2 * len(probes) {
				lines = append(lines, requestLine(t, dir, base+i, probes[i%len(probes)]))
			}
			answers := serveAll(t, lines)
			for i := range lines {
				id := base + i
				w := wants[i%len(probes)]
				a := answers[int64(id)]
				stdout, errOut := os.ReadFile(filepath.Join(dir, fmt.Sprintf("%d.stdout", id)))
				stderr, errErr := os.ReadFile(filepath.Join(dir, fmt.Sprintf("%d.stderr", id)))
				if a.Error != "" || a.ExitCode == nil || *a.ExitCode != w.rc || a.MaxRSSBytes <= 0 || a.PeakBytes <= 0 || a.HeldBytes <= 0 || a.HeldBytes > trimAboveBytes || errors.Join(errOut, errErr) != nil ||
					!bytes.Equal(stdout, w.stdout) || !bytes.Equal(stderr, w.stderr) {
					t.Errorf("request %d (%v): answer %+v\nstdout %q\nstderr %q\nwant rc %d stdout %q stderr %q",
						id, probes[i%len(probes)], a, stdout, stderr, w.rc, w.stdout, w.stderr)
				}
			}
		})
	}
	wg.Wait()

	// What the lone runs are: a manifest, a template abort, a deprecation record, a values-file error.
	if !strings.HasPrefix(string(wants[0].stdout), "---\n# Source: demo/templates/cm.yaml\n") || wants[0].rc != 0 || len(wants[0].stderr) != 0 {
		t.Errorf("defaults render: %d %q %q", wants[0].rc, wants[0].stdout, wants[0].stderr)
	}
	if wants[4].rc != 1 || !strings.HasPrefix(string(wants[4].stderr), "Error: execution error at (demo/templates/cm.yaml") ||
		!strings.HasSuffix(string(wants[4].stderr), "boom requested\n\nUse --debug flag to render out invalid YAML\n") {
		t.Errorf("abort render: %d %q", wants[4].rc, wants[4].stderr)
	}
	if string(wants[1].stderr) != "level=WARN msg=\"this chart is deprecated\"\n" || wants[1].rc != 0 {
		t.Errorf("deprecated render: %d %q", wants[1].rc, wants[1].stderr)
	}
	if last := wants[len(wants)-1]; last.rc != 1 || !strings.HasPrefix(string(last.stderr), "Error: open ") {
		t.Errorf("missing values file: %d %q", last.rc, last.stderr)
	}
}

// Malformed and unsupported requests and unwritable outputs are errors (harness
// failures), never an exit code.
func TestServeRefusesWhatIsNoCLICall(t *testing.T) {
	isolateEnv(t)
	dir, _ := serveFixture(t)
	unwritable := strings.Replace(requestLine(t, dir, 3, probe{"demo", "v0.json"}), filepath.Join(dir, "3.stdout"), filepath.Join(dir, "missing", "3.stdout"), 1)
	answers := serveAll(t, []string{
		`{"id": 1, "op": "lint"}`,
		`not json`,
		`{"id": 4, "op": "template"}`,
		strings.Replace(requestLine(t, dir, 2, probe{"demo", "v0.json"}), `"release":"adjudication"`, `"release":""`, 1),
		unwritable,
		strings.Replace(requestLine(t, dir, 5, probe{"demo", "v0.json"}), filepath.Join(dir, "v0.json"), "-", 1),
		strings.Replace(requestLine(t, dir, 6, probe{"demo", "v0.json"}), filepath.Join(dir, "v0.json"), "https://example.invalid/v.yaml", 1),
		strings.Replace(requestLine(t, dir, 7, probe{"demo", "v0.json"}), `"kube_version":"1.29.0"`, `"kube_version":""`, 1),
	})
	for _, id := range []int64{1, -1, 4, 2, 3, 5, 6, 7} {
		if answers[id].Error == "" || answers[id].ExitCode != nil {
			t.Errorf("request %d: %+v", id, answers[id])
		}
	}
}

// A render that ends abnormally (the CLI would exit 2) is an error, and its
// outputs are not written.
func TestServeReportsAnAbnormalEndAsAnError(t *testing.T) {
	dir := t.TempDir()
	sink := installLogSink(&bytes.Buffer{})
	req := serveRequest{ID: new(uint64), Op: "template", Release: "adjudication", Chart: dir, KubeVersion: "1.29.0",
		Values: []string{filepath.Join(dir, "values.json")}, StdoutPath: filepath.Join(dir, "out"), StderrPath: filepath.Join(dir, "err")}
	panics := func(string, string, string, []string, bool) (int, []byte, []byte) { panic("boom") }
	answer := serve(sink, req, panics)
	if answer.ExitCode != nil || !strings.HasPrefix(answer.Error, "abnormal: helm template would exit 2: panic: boom") {
		t.Fatalf("answer %+v", answer)
	}
	if _, err := os.Stat(req.StdoutPath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("outputs written for an abnormal end (%v)", err)
	}
}
