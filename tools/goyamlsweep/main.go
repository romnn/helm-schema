// Command goyamlsweep observes Helm's pinned YAML reader and text stages.
package main

import (
	"fmt"
	"io"
	"os"
	"runtime/debug"
)

const (
	exitHarness      = 1
	exitRefused      = 2
	yamlSourceDigest = "8896c202ce52cfd954fc8913e1cdd4a1c9685b6ccf9aad25272572d4cd6c0e8e"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	pins, err := pinnedVersions()
	if err != nil {
		fmt.Fprintln(stderr, err)
		return exitRefused
	}
	if len(args) == 1 && args[0] == "version" {
		for _, pin := range pins {
			fmt.Fprintln(stdout, pin)
		}
		return 0
	}
	if len(args) == 1 && args[0] == "yaml" {
		args = nil
	}
	return cmdYAML(args, stdin, stdout, stderr)
}

// pinnedVersions rejects module drift before the oracle accepts any request.
func pinnedVersions() ([]string, error) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return nil, fmt.Errorf("goyamlsweep has no build information")
	}
	if info.GoVersion != "go1.26.5" {
		return nil, fmt.Errorf("goyamlsweep needs go1.26.5, linked %s", info.GoVersion)
	}
	expected := []struct{ path, version, replacement string }{
		{"go.yaml.in/yaml/v2", "v2.4.3", "./third_party/yaml-v2.4.3"},
		{"sigs.k8s.io/yaml", "v1.6.0", ""},
		{"github.com/Masterminds/sprig/v3", "v3.3.0", ""},
		{"helm.sh/helm/v4", "v4.2.3", ""},
	}
	lines := []string{"go " + info.GoVersion}
	for _, want := range expected {
		found := false
		for _, actual := range info.Deps {
			if actual.Path != want.path {
				continue
			}
			if actual.Version != want.version {
				return nil, fmt.Errorf("%s linked %s, want %s", want.path, actual.Version, want.version)
			}
			line := actual.Path + " " + actual.Version
			if want.replacement == "" {
				if actual.Replace != nil {
					return nil, fmt.Errorf("%s has an unexpected replacement", want.path)
				}
			} else {
				if actual.Replace == nil || actual.Replace.Path != want.replacement || actual.Replace.Version != "(devel)" {
					return nil, fmt.Errorf("%s is not the observed pinned source copy", want.path)
				}
				line += " => " + actual.Replace.Path
			}
			lines = append(lines, line)
			found = true
			break
		}
		if !found {
			return nil, fmt.Errorf("%s is absent from linked modules", want.path)
		}
	}
	return append(lines, "yaml-source "+yamlSourceDigest), nil
}
