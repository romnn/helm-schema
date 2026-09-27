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

	"helm.sh/helm/v4/pkg/cli"
)

// serveRequest is one `helm template` the battery would otherwise spawn:
// `helm template RELEASE CHART --kube-version K [--skip-schema-validation]
// -f VALUES...`. The server writes the CLI's stdout and stderr to the named
// files before it answers.
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
	// render ran (sampled every millisecond): the render's own measurement.
	PeakBytes int64 `json:"peak_bytes"`
	// HeldBytes is what the idle server holds after the render, having
	// returned memory to the OS when it held more than trimAboveBytes.
	HeldBytes int64 `json:"held_bytes"`
	// MaxRSSBytes is the server's peak resident set size over its lifetime:
	// telemetry of the process, not of any one render.
	MaxRSSBytes int64  `json:"max_rss_bytes"`
	Error       string `json:"error,omitempty"`
}

// trimAboveBytes: after a render leaving more held than this, the server
// returns its free memory to the OS before it idles.
const trimAboveBytes = 64 << 20

// cmdServe answers one JSON request per stdin line with one JSON response
// per stdout line, one request at a time, until EOF. Serial service makes
// every log record written during a render that render's own, so its
// stderr is the CLI's exactly; the caller runs one server per concurrent
// render. The caller sets the environment a Helm CLI child would get (the
// server does not change it).
func cmdServe(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) != 0 {
		fmt.Fprintln(stderr, "usage: helmsweep serve")
		return exitRefused
	}
	if err := unsupportedConfiguration(cli.New()); err != nil {
		fmt.Fprintln(stderr, "helmsweep serve: refused:", err)
		return exitRefused
	}
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

// serve runs one request through render (templateRun), its log records
// captured into its stderr where the CLI prints them.
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
			rc, out, errText := guarded(req, render)
			stdout = out
			return rc, errText
		})
	})
	if memoryHeld() > trimAboveBytes {
		debug.FreeOSMemory()
	}
	reply.HeldBytes = memoryHeld()
	if rc != 0 && rc != 1 {
		reply.Error = fmt.Sprintf("abnormal: helm template would exit %d: %s", rc, stderrText)
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

// supportedRequest refuses what is not a local `helm template`: another
// op, an empty release or Kubernetes version (the CLI's default capabilities
// path is not reproduced), or a chart, values or output that is not an
// absolute local path — `-` would read the protocol's own stdin and a URL
// would fetch.
func supportedRequest(req serveRequest) error {
	if req.Op != "template" || req.Release == "" || req.KubeVersion == "" || len(req.Values) == 0 {
		return fmt.Errorf("unsupported request %q", req.Op)
	}
	for _, path := range append([]string{req.Chart, req.StdoutPath, req.StderrPath}, req.Values...) {
		if !filepath.IsAbs(path) {
			return fmt.Errorf("%q is not an absolute local path", path)
		}
	}
	return nil
}

// guarded runs render for req; a panic ends it as it ends the CLI: exit 2.
func guarded(req serveRequest, render renderFunc) (rc int, stdout, stderr []byte) {
	defer func() {
		if r := recover(); r != nil {
			rc, stdout, stderr = 2, nil, fmt.Appendf(nil, "panic: %v\n\n%s", r, debug.Stack())
		}
	}()
	return render(req.Release, req.Chart, req.KubeVersion, req.Values, req.SkipSchemaValidation)
}
