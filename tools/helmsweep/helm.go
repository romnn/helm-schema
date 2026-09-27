package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"log/slog"
	"os"
	"runtime/debug"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"helm.sh/helm/v4/pkg/action"
	"helm.sh/helm/v4/pkg/chart"
	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/chart/loader"
	"helm.sh/helm/v4/pkg/cli"
	"helm.sh/helm/v4/pkg/cli/values"
	"helm.sh/helm/v4/pkg/getter"
	"helm.sh/helm/v4/pkg/kube"
	releasev1 "helm.sh/helm/v4/pkg/release/v1"
)

// logSink is the writer behind the process-wide slog default, which Helm's
// library code and the std log package write to. It is installed once, with
// the handler the Helm CLI installs (pkg/cmd/root.go SetupLogging,
// internal/logging.NewLogger: text, no time attribute, debug off).
//
// A record cannot be attributed to a concurrently running cell, so outside an
// exclusive cell records go to the step writer, and every record that could
// change a cell's class (sensitiveRecord) bumps a counter. A cell during which
// the counter moved is re-run exclusively, when every record is its own and
// goes into its log exactly where the CLI would print it.
type logSink struct {
	gate      sync.RWMutex // held shared by concurrent cells, exclusively by an attributed run
	mu        sync.Mutex
	step      io.Writer
	capture   *bytes.Buffer
	sensitive atomic.Int64
}

var installSink = sync.OnceValue(func() *logSink {
	sink := &logSink{}
	handler := slog.NewTextHandler(sink, &slog.HandlerOptions{
		Level: slog.LevelDebug,
		ReplaceAttr: func(_ []string, a slog.Attr) slog.Attr {
			if a.Key == slog.TimeKey {
				return slog.Attr{}
			}
			return a
		},
	})
	slog.SetDefault(slog.New(debugOff{handler}))
	return sink
})

// installLogSink installs the sink once per process and points records
// outside an exclusive cell at step.
func installLogSink(step io.Writer) *logSink {
	sink := installSink()
	sink.mu.Lock()
	sink.step = step
	sink.mu.Unlock()
	return sink
}

func (s *logSink) Write(p []byte) (int, error) {
	if sensitiveRecord(string(p)) {
		s.sensitive.Add(1)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.capture != nil {
		return s.capture.Write(p)
	}
	if s.step != nil {
		return s.step.Write(p)
	}
	return len(p), nil
}

// shared runs a cell alongside others. attributable is false when a record
// that could change its class was logged while it ran.
func (s *logSink) shared(run func() (int, []byte)) (rc int, log []byte, attributable bool) {
	s.gate.RLock()
	defer s.gate.RUnlock()
	before := s.sensitive.Load()
	rc, log = run()
	return rc, log, s.sensitive.Load() == before
}

// exclusive runs a cell alone and prepends its records to its log, as the
// CLI prints them: every record is logged before the command's final output.
func (s *logSink) exclusive(run func() (int, []byte)) (int, []byte) {
	s.gate.Lock()
	defer s.gate.Unlock()
	var records bytes.Buffer
	s.mu.Lock()
	s.capture = &records
	s.mu.Unlock()
	rc, log := run()
	s.mu.Lock()
	s.capture = nil
	s.mu.Unlock()
	return rc, append(records.Bytes(), log...)
}

// debugOff is internal/logging.DebugCheckHandler with debug disabled: it
// drops exactly the Debug level and passes every other level.
type debugOff struct{ slog.Handler }

func (h debugOff) Enabled(_ context.Context, level slog.Level) bool {
	return level != slog.LevelDebug
}

func (h debugOff) WithAttrs(attrs []slog.Attr) slog.Handler {
	return debugOff{h.Handler.WithAttrs(attrs)}
}

func (h debugOff) WithGroup(name string) slog.Handler {
	return debugOff{h.Handler.WithGroup(name)}
}

// runCell runs one cell with fresh Helm settings, values, chart load and
// action. A panic ends the cell as it ends the CLI: exit 2 and the panic text.
func runCell(mode, chartPath, kubeVersion, valuesFile string) (rc int, out []byte) {
	defer func() {
		if r := recover(); r != nil {
			rc, out = 2, fmt.Appendf(nil, "panic: %v\n\n%s", r, debug.Stack())
		}
	}()
	if mode == "lint" {
		return lintCell(chartPath, kubeVersion, valuesFile)
	}
	return templateCell(chartPath, kubeVersion, valuesFile)
}

// cliError is how cobra ends a Helm command that returned err: "Error: " and
// the message on stderr, exit 1.
func cliError(out *bytes.Buffer, err error) (int, []byte) {
	fmt.Fprintln(out, "Error:", err.Error())
	return 1, out.Bytes()
}

// lintCell is `helm lint CHART --kube-version K -f VALUES` with every other
// flag at its default: pkg/cmd/lint.go newLintCmd's RunE for one path,
// writing stdout and stderr to one log as `> log 2>&1` does.
func lintCell(chartPath, kubeVersion, valuesFile string) (int, []byte) {
	var out bytes.Buffer
	settings := cli.New()
	if _, ok := initRootConfig(settings); !ok {
		return 1, out.Bytes()
	}
	client := action.NewLint()
	parsed, err := common.ParseKubeVersion(kubeVersion)
	if err != nil {
		return cliError(&out, fmt.Errorf("invalid kube version '%s': %s", kubeVersion, err))
	}
	client.KubeVersion = parsed
	client.Namespace = settings.Namespace()
	valueOpts := &values.Options{ValueFiles: []string{valuesFile}}
	vals, err := valueOpts.MergeValues(getter.All(settings))
	if err != nil {
		return cliError(&out, err)
	}

	result := client.Run([]string{chartPath}, vals)
	fmt.Fprintf(&out, "==> Linting %s\n", chartPath)
	if len(result.Messages) == 0 {
		for _, err := range result.Errors {
			fmt.Fprintf(&out, "Error %s\n", err)
		}
	}
	for _, msg := range result.Messages {
		fmt.Fprintf(&out, "%s\n", msg)
	}
	failed := 0
	if len(result.Errors) != 0 {
		failed++
	}
	fmt.Fprint(&out, "\n")

	summary := fmt.Sprintf("%d chart(s) linted, %d chart(s) failed", 1, failed)
	if failed > 0 {
		return cliError(&out, errors.New(summary))
	}
	fmt.Fprintln(&out, summary)
	return 0, out.Bytes()
}

// templateCell is `helm template t CHART --skip-schema-validation
// --kube-version K -f VALUES` with stdout discarded; the log is stderr.
func templateCell(chartPath, kubeVersion, valuesFile string) (int, []byte) {
	rc, _, stderr := templateRun("t", chartPath, kubeVersion, []string{valuesFile}, true)
	return rc, stderr
}

// templateRun is `helm template RELEASE CHART --kube-version K -f VALUES...`
// (plus --skip-schema-validation when skipSchema) with every other flag at
// its default: the configuration cobra.OnInitialize builds (pkg/cmd/root.go
// NewRootCmd), newTemplateCmd's RunE (pkg/cmd/template.go) with the install
// flags at their defaults (pkg/cmd/install.go addInstallFlags), then
// runInstall. It returns the CLI's exit code, stdout and stderr.
func templateRun(release, chartPath, kubeVersion string, valuesFiles []string, skipSchema bool) (int, []byte, []byte) {
	var stderr bytes.Buffer
	settings := cli.New()
	cfg, ok := initRootConfig(settings)
	if !ok {
		return 1, nil, stderr.Bytes()
	}

	client := action.NewInstall(cfg)
	client.Timeout = 300 * time.Second
	client.WaitStrategy = kube.HookOnlyStrategy
	client.SkipSchemaValidation = skipSchema
	parsed, err := common.ParseKubeVersion(kubeVersion)
	if err != nil {
		rc, text := cliError(&stderr, fmt.Errorf("invalid kube version '%s': %w", kubeVersion, err))
		return rc, nil, text
	}
	client.KubeVersion = parsed
	client.DryRunStrategy = action.DryRunClient
	client.ReleaseName = "release-name"
	client.Replace = true
	client.APIVersions = common.VersionSet([]string{})
	client.IncludeCRDs = false

	rel, err := runInstall([]string{release, chartPath}, client, &values.Options{ValueFiles: valuesFiles}, settings)
	if err != nil {
		if rel != nil {
			err = fmt.Errorf("%w\n\nUse --debug flag to render out invalid YAML", err)
		}
		rc, text := cliError(&stderr, err)
		return rc, nil, text
	}
	var manifests bytes.Buffer
	if rel != nil {
		fmt.Fprintln(&manifests, strings.TrimSpace(rel.Manifest))
		if !client.DisableHooks {
			for _, m := range rel.Hooks {
				fmt.Fprintf(&manifests, "---\n# Source: %s\n%s\n", m.Path, m.Manifest)
			}
		}
	}
	return 0, manifests.Bytes(), stderr.Bytes()
}

// initRootConfig is the cobra.OnInitialize of pkg/cmd/root.go NewRootCmd,
// which runs before every command; not ok where the CLI log.Fatal()s (the
// message goes through slog, exit 1).
func initRootConfig(settings *cli.EnvSettings) (*action.Configuration, bool) {
	cfg := action.NewConfiguration()
	if err := cfg.Init(settings.RESTClientGetter(), settings.Namespace(), os.Getenv("HELM_DRIVER")); err != nil {
		log.Print(err)
		return nil, false
	}
	return cfg, true
}

// runInstall is pkg/cmd/install.go runInstall for a local chart directory,
// without its signal handler.
func runInstall(args []string, client *action.Install, valueOpts *values.Options, settings *cli.EnvSettings) (*releasev1.Release, error) {
	name, chartRef, err := client.NameAndChart(args)
	if err != nil {
		return nil, err
	}
	client.ReleaseName = name

	cp, err := client.LocateChart(chartRef, settings)
	if err != nil {
		return nil, err
	}

	vals, err := valueOpts.MergeValues(getter.All(settings))
	if err != nil {
		return nil, err
	}

	chartRequested, err := loader.Load(cp)
	if err != nil {
		return nil, err
	}

	ac, err := chart.NewAccessor(chartRequested)
	if err != nil {
		return nil, err
	}
	if meta := ac.MetadataAsMap(); meta["Type"] != "" && meta["Type"] != "application" {
		return nil, fmt.Errorf("%s charts are not installable", meta["Type"])
	}
	if ac.Deprecated() {
		slog.Warn("this chart is deprecated")
	}
	if req := ac.MetaDependencies(); len(req) > 0 {
		if err := action.CheckDependencies(chartRequested, req); err != nil {
			return nil, fmt.Errorf("an error occurred while checking for chart dependencies. You may need to run 'helm dependency build' to fetch missing dependencies: %w", err)
		}
	}

	client.Namespace = settings.Namespace()
	ri, err := client.RunWithContext(context.Background(), chartRequested, vals)
	// pkg/cmd/root.go releaserToV1Release
	switch rel := ri.(type) {
	case releasev1.Release:
		return &rel, err
	case *releasev1.Release:
		return rel, err
	case nil:
		return nil, err
	default:
		return nil, fmt.Errorf("unsupported release type: %T", ri)
	}
}

// sensitiveRecord reports whether a log record could change a class if it
// were part of a cell's log: it matches one of the classifier's patterns or
// carries one of the strings the classifier looks for.
func sensitiveRecord(record string) bool {
	for _, p := range infrastructure {
		if p.re.MatchString(record) {
			return true
		}
	}
	if templateVerdict.MatchString(record) {
		return true
	}
	for _, marker := range []string{"[ERROR]", "==> Linting ", "chart(s) linted", schemaInControl} {
		if strings.Contains(record, marker) {
			return true
		}
	}
	return false
}
