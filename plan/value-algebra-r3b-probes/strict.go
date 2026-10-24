package main

import (
	"fmt"
	"os"
	"runtime"
	"runtime/debug"
	"strings"

	"helm.sh/helm/v4/pkg/chart/common"
	chart "helm.sh/helm/v4/pkg/chart/v2"
	"helm.sh/helm/v4/pkg/engine"
)

func main() {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		panic("missing build info")
	}
	version := ""
	for _, dep := range info.Deps {
		if dep.Path == "helm.sh/helm/v4" {
			version = dep.Version
			if dep.Replace != nil {
				panic("unexpected Helm replacement")
			}
		}
	}
	if version != "v4.2.3" || runtime.Version() != "go1.26.5" {
		panic("wrong Helm or Go version")
	}
	fmt.Printf("Helm module: %s\nGo: %s\nmode: engine.Engine{Strict:true}\n\n", version, runtime.Version())
	cases := []struct {
		name      string
		template  string
		want      string
		wantError bool
	}{
		{"s01-strict-field", `{{ .Values.absent }}`, `map has no entry for key "absent"`, true},
		{"s02-strict-index", `{{ index .Values "absent" | printf "%T" }}`, "<nil>", false},
		{"s03-strict-get", `{{ get .Values "absent" | printf "%q" }}`, `""`, false},
	}
	failures := 0
	for _, test := range cases {
		c := &chart.Chart{
			Metadata:  &chart.Metadata{Name: "probe", Version: "0.1.0", APIVersion: "v2"},
			Templates: []*common.File{{Name: "templates/cm.yaml", Data: []byte(test.template)}},
		}
		renderer := engine.Engine{Strict: true}
		out, err := renderer.Render(c, common.Values{"Values": common.Values{}})
		text := out["probe/templates/cm.yaml"]
		status := 0
		if err != nil {
			text = err.Error()
			status = 1
		}
		passed := (err != nil) == test.wantError && strings.Contains(text, test.want)
		check := "PASS"
		if !passed {
			check = "FAIL"
			failures++
		}
		fmt.Printf("### %s\nvalues: {}\ntemplate: %s\nengine status: %d\nexpected: error=%t; contains %q\ncheck: %s\noutput:\n  %s\n\n", test.name, test.template, status, test.wantError, test.want, check, text)
	}
	fmt.Printf("TOTAL: %d; passed: %d; failed: %d\n", len(cases), len(cases)-failures, failures)
	if failures > 0 {
		os.Exit(1)
	}
}
