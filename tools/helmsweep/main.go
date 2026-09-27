// Command helmsweep reproduces the verdicts of `helm lint` and
// `helm template --skip-schema-validation` in-process with Helm v4.2.3's own
// code, for helm-schema's landing sweep gate. The only deviations from the
// Helm CLI are performance: a memoized schema compile (the one patch in
// PATCH.md), a bounded worker pool and a fail-closed on-disk
// verdict cache. See README.md for the command contract.
package main

import (
	"crypto/sha256"
	_ "embed"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"runtime/debug"
	"strconv"

	"helm.sh/helm/v4/pkg/chart/common"
	"helm.sh/helm/v4/pkg/cli"
)

//go:embed helm-memo.patch
var helmPatch []byte

const (
	helmModule       = "helm.sh/helm/v4"
	helmVersion      = "v4.2.3"
	helmReplacement  = "./third_party/helm-v4.2.3"
	jsonschemaModule = "github.com/santhosh-tekuri/jsonschema/v6"
	jsonschemaVer    = "v6.0.2"
	// helmReleaseBuild is `helm version` of the pinned v4.2.3 release binary
	// (version, Git commit, tree state, Go), linked in by build:helmsweep.
	helmReleaseBuild = "v4.2.3 43e8b7feece8beb0fcba47059ec9b522fd929a64 clean go1.26.5"
)

// Exit codes besides a cell's own rc.
const (
	exitHarness = 1 // I/O or other harness failure
	exitRefused = 2 // usage, or a roster, plan or copy that fails verification
)

func main() {
	if code, done := reexecClean(os.Args[1:], os.Stderr); done {
		os.Exit(code)
	}
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, "usage: helmsweep version | lint | template | classify | sweep | serve ...")
		return exitRefused
	}
	switch args[0] {
	case "version":
		return cmdVersion(stdout, stderr)
	case "lint", "template":
		return cmdCell(args[0], args[1:], stdout, stderr)
	case "classify":
		return cmdClassify(args[1:], stdout, stderr)
	case "sweep":
		return cmdSweep(args[1:], stderr)
	case "serve":
		return cmdServe(args[1:], os.Stdin, stdout, stderr)
	}
	fmt.Fprintf(stderr, "helmsweep: unknown command %q\n", args[0])
	return exitRefused
}

// identity is what the verdict cache and the version check pin: this
// executable's bytes and the module versions it was built from.
type identity struct {
	Executable string
	Go         string
	Helm       string
	// HelmBuild is the build information templates read as
	// .Capabilities.HelmVersion.
	HelmBuild  string
	JSONSchema string
	Patch      string
}

func buildIdentity() (identity, error) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return identity{}, errors.New("no build information in this binary")
	}
	helm := common.DefaultCapabilities.HelmVersion
	id := identity{
		Go:        info.GoVersion,
		HelmBuild: fmt.Sprintf("%s %s %s %s", helm.Version, helm.GitCommit, helm.GitTreeState, helm.GoVersion),
		Patch:     fmt.Sprintf("%x", sha256.Sum256(helmPatch)),
	}
	for _, dep := range info.Deps {
		switch dep.Path {
		case helmModule:
			if dep.Version == helmVersion && dep.Replace != nil && dep.Replace.Path == helmReplacement {
				id.Helm = dep.Version + " => " + dep.Replace.Path
			}
		case jsonschemaModule:
			if dep.Version == jsonschemaVer && dep.Replace == nil {
				id.JSONSchema = dep.Version
			}
		}
	}
	if id.Helm == "" || id.JSONSchema == "" {
		return identity{}, fmt.Errorf("binary is not built from %s %s => %s and %s %s", helmModule, helmVersion, helmReplacement, jsonschemaModule, jsonschemaVer)
	}
	exe, err := os.Executable()
	if err != nil {
		return identity{}, err
	}
	sum, err := fileSHA256(exe)
	if err != nil {
		return identity{}, err
	}
	id.Executable = sum
	return id, nil
}

func cmdVersion(stdout, stderr io.Writer) int {
	id, err := buildIdentity()
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep:", err)
		return exitHarness
	}
	fmt.Fprintf(stdout, "build %s\ngo %s\n%s %s\nhelm-build %s\n%s %s\npatch %s\n",
		id.Executable, id.Go, helmModule, id.Helm, id.HelmBuild, jsonschemaModule, id.JSONSchema, id.Patch)
	return 0
}

// cmdCell runs one lint or template cell, exactly as one Helm CLI call would:
// the log (the CLI's combined output for lint, its stderr for template) goes
// to stdout and the exit code is the CLI's.
func cmdCell(mode string, args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet(mode, flag.ContinueOnError)
	flags.SetOutput(stderr)
	kubeVersion := flags.String("kube-version", "", "Kubernetes version")
	valuesFile := flags.String("values", "", "values file (-f)")
	clearEnv := flags.Bool("helm-env-clear", false, "clear HELM_*, KUBECONFIG, proxies and HOME/XDG config first")
	if err := flags.Parse(args); err != nil || flags.NArg() != 1 || *valuesFile == "" || *kubeVersion == "" {
		fmt.Fprintf(stderr, "usage: helmsweep %s [--helm-env-clear] --kube-version K --values FILE CHART\n", mode)
		return exitRefused
	}
	if *clearEnv {
		if err := requireCleanEnv(); err != nil {
			fmt.Fprintln(stderr, "helmsweep: refused:", err)
			return exitRefused
		}
	}
	if err := unsupportedConfiguration(cli.New()); err != nil {
		fmt.Fprintln(stderr, "helmsweep: refused:", err)
		return exitRefused
	}
	sink := installLogSink(stderr)
	rc, log := sink.exclusive(func() (int, []byte) {
		return runCell(mode, flags.Arg(0), *kubeVersion, *valuesFile)
	})
	if _, err := stdout.Write(log); err != nil {
		return exitHarness
	}
	return rc
}

// cmdClassify prints the gate's class of one run: `classify lint|template RC LOG`.
func cmdClassify(args []string, stdout, stderr io.Writer) int {
	if len(args) != 3 || (args[0] != "lint" && args[0] != "template") {
		fmt.Fprintln(stderr, "usage: helmsweep classify lint|template RC LOG")
		return exitRefused
	}
	rc, err := strconv.Atoi(args[1])
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep: rc is not an integer")
		return exitRefused
	}
	log, err := os.ReadFile(args[2])
	if errors.Is(err, os.ErrNotExist) {
		fmt.Fprintln(stdout, "unresolved:no-log")
		return 0
	}
	if err != nil {
		fmt.Fprintln(stderr, "helmsweep:", err)
		return exitHarness
	}
	fmt.Fprintln(stdout, classify(args[0], rc, log))
	return 0
}
