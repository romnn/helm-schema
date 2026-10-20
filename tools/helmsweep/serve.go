package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime/debug"
	"runtime/metrics"
	"time"

	chartutil "helm.sh/helm/v4/pkg/chart/common/util"
	"helm.sh/helm/v4/pkg/cli"
)

// serveRequest is one local Helm lint or template command.
// The server writes each CLI output stream to its named file before answering.
type serveRequest struct {
	ID                   *uint64  `json:"id"`
	Op                   string   `json:"op"`
	Release              string   `json:"release"`
	Chart                string   `json:"chart"`
	KubeVersion          string   `json:"kube_version"`
	Values               []string `json:"values"`
	SkipSchemaValidation bool     `json:"skip_schema_validation"`
	StdoutPath           string   `json:"stdout_path"`
	StderrPath           string   `json:"stderr_path"`
}

// serveResponse carries the CLI's exit code (0 or 1), or an error that is a
// harness failure: a malformed request, an abnormal end (the CLI would exit
// with another status) or unwritable outputs. Never a Helm verdict.
type serveResponse struct {
	ID       *uint64 `json:"id"`
	ExitCode *int    `json:"exit_code,omitempty"`
	// PeakBytes is the most memory the process held from the OS while this
	// command ran (sampled every millisecond): the command's own measurement.
	PeakBytes int64 `json:"peak_bytes"`
	// HeldBytes is what the idle server holds after the command, having
	// returned memory to the OS when it held more than trimAboveBytes.
	HeldBytes int64 `json:"held_bytes"`
	// MaxRSSBytes is the server's peak resident set size over its lifetime:
	// telemetry of the process, not of any one command.
	MaxRSSBytes int64  `json:"max_rss_bytes"`
	Error       string `json:"error,omitempty"`
	// These process totals distinguish reuse from a server restart.
	SchemaCompiles int64 `json:"schema_compiles"`
	SchemaMemoHits int64 `json:"schema_memo_hits"`
}

// trimAboveBytes: after a command leaving more held than this, the server
// returns its free memory to the OS before it idles.
const trimAboveBytes = 64 << 20

// cmdServe answers one JSON request per stdin line with one JSON response
// per stdout line, one request at a time, until EOF.
// Serial service attributes every log record to its command, on stderr as
// the CLI prints it. The caller runs one server per concurrent command and
// sets the environment a CLI child would get; the server does not change it.
func cmdServe(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) != 0 {
		fmt.Fprintln(stderr, "usage: helmsweep serve")
		return exitRefused
	}
	if err := unsupportedConfiguration(cli.New()); err != nil {
		fmt.Fprintln(stderr, "helmsweep serve: refused:", err)
		return exitRefused
	}
	chartutil.SetCompiledSchemaMemoLimit(2)
	defer chartutil.SetCompiledSchemaMemoLimit(0)
	sink := installLogSink(stderr)
	encoder := json.NewEncoder(stdout)
	encoder.SetEscapeHTML(false)
	reader := bufio.NewReaderSize(stdin, 1<<20)
	for {
		line, err := reader.ReadBytes('\n')
		if len(line) > 0 {
			var req serveRequest
			response := serveResponse{}
			if decodeErr := json.Unmarshal(line, &req); decodeErr != nil || req.ID == nil {
				response = serveResponse{ID: req.ID, Error: fmt.Sprintf("protocol: malformed request: %v", decodeErr)}
			} else {
				response = serve(sink, req, templateRun)
			}
			if encodeErr := encoder.Encode(response); encodeErr != nil {
				fmt.Fprintln(stderr, "helmsweep serve:", encodeErr)
				return exitHarness
			}
		}
		if err == io.EOF {
			return 0
		}
		if err != nil {
			fmt.Fprintln(stderr, "helmsweep serve:", err)
			return exitHarness
		}
	}
}

// renderFunc is templateRun's signature.
type renderFunc func(release, chartPath, kubeVersion string, valuesFiles []string, skipSchema bool) (int, []byte, []byte)

// serve dispatches one command and captures its log records into stderr.
func serve(sink *logSink, req serveRequest, render renderFunc) serveResponse {
	reply := serveResponse{ID: req.ID}
	if err := supportedRequest(req); err != nil {
		reply.Error = "protocol: " + err.Error()
		return reply
	}
	var stdout, stderrText []byte
	var rc int
	reply.PeakBytes = sampledPeak(func() {
		rc, stderrText = sink.exclusive(func() (int, []byte) {
			chartutil.ResetUnresolvedURNWarnings()
			rc, out, errText := guarded(req, render)
			stdout = out
			return rc, errText
		})
	})
	if memoryHeld() > trimAboveBytes {
		debug.FreeOSMemory()
	}
	reply.HeldBytes = memoryHeld()
	reply.SchemaCompiles, reply.SchemaMemoHits = chartutil.CompiledSchemaMemoStats()
	if rc != 0 && rc != 1 {
		reply.Error = fmt.Sprintf("abnormal: helm %s would exit %d: %s", req.Op, rc, stderrText)
		return reply
	}
	if err := os.WriteFile(req.StdoutPath, stdout, 0o644); err != nil {
		reply.Error = err.Error()
		return reply
	}
	if err := os.WriteFile(req.StderrPath, stderrText, 0o644); err != nil {
		reply.Error = err.Error()
		return reply
	}
	reply.ExitCode = &rc
	reply.MaxRSSBytes = peakRSS()
	return reply
}

// memoryHeld is the memory this process holds from the OS: everything the
// Go runtime mapped minus what it released.
func memoryHeld() int64 {
	samples := []metrics.Sample{{Name: "/memory/classes/total:bytes"}, {Name: "/memory/classes/heap/released:bytes"}}
	metrics.Read(samples)
	return int64(samples[0].Value.Uint64() - samples[1].Value.Uint64())
}

// sampledPeak runs run and returns the most memoryHeld while it ran.
func sampledPeak(run func()) int64 {
	peak := memoryHeld()
	stop, sampled := make(chan struct{}), make(chan int64)
	go func() {
		highest := peak
		ticker := time.NewTicker(time.Millisecond)
		defer ticker.Stop()
		for {
			select {
			case <-stop:
				sampled <- highest
				return
			case <-ticker.C:
				highest = max(highest, memoryHeld())
			}
		}
	}()
	run()
	close(stop)
	return max(<-sampled, memoryHeld())
}

// supportedRequest accepts only the local lint and template flags reproduced here.
// Stdin values would consume the protocol; remote paths would fetch.
func supportedRequest(req serveRequest) error {
	if req.KubeVersion == "" || len(req.Values) == 0 {
		return fmt.Errorf("unsupported request %q", req.Op)
	}
	switch req.Op {
	case "template":
		if req.Release == "" {
			return fmt.Errorf("template requires a release")
		}
	case "lint":
		if req.Release != "" || req.SkipSchemaValidation {
			return fmt.Errorf("lint does not accept template flags")
		}
	default:
		return fmt.Errorf("unsupported request %q", req.Op)
	}
	for _, path := range append([]string{req.Chart, req.StdoutPath, req.StderrPath}, req.Values...) {
		if !filepath.IsAbs(path) {
			return fmt.Errorf("%q is not an absolute local path", path)
		}
	}
	return nil
}

// guarded dispatches req; a panic ends it as it ends the CLI: exit 2.
func guarded(req serveRequest, render renderFunc) (rc int, stdout, stderr []byte) {
	defer func() {
		if r := recover(); r != nil {
			rc, stdout, stderr = 2, nil, fmt.Appendf(nil, "panic: %v\n\n%s", r, debug.Stack())
		}
	}()
	if req.Op == "lint" {
		return lintRun(req.Chart, req.KubeVersion, req.Values)
	}
	return render(req.Release, req.Chart, req.KubeVersion, req.Values, req.SkipSchemaValidation)
}
