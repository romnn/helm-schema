package main

import (
	"fmt"
	"regexp"
	"strings"
)

// The gate's classifier, /Volumes/T7/dev/round8/runner/landing.py
// classify_helm, over the text Python reads from a log: invalid UTF-8
// replaced, universal newlines, and Python's Unicode \s, \S, \d and
// str.splitlines line boundaries.
const (
	pySpace     = `\t\n\x0b\x0c\r\x1c-\x1f \x{85}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}`
	pyDigit     = `\p{Nd}`
	classPass   = "pass"
	classReject = "reject"

	schemaInControl = "values don't meet the specifications of the schema"
	missingFile     = `(no such file or directory|The system cannot find the file specified\.)`
)

type class struct {
	why string
	re  *regexp.Regexp
}

var (
	lintVerdictLine = regexp.MustCompile(`^\[ERROR\] (values\.yaml: |templates/)`)
	// Operational failures (the runner's own inputs or invocation),
	// recognized anywhere: never acceptable.
	operational = []class{
		{"invalid-kube-version", regexp.MustCompile(`invalid kube version`)},
		{"values-file", regexp.MustCompile(`(?m)^Error: (open |failed to parse )`)},
		{"usage", regexp.MustCompile(`(?m)^Error: (unknown (flag|command|shorthand)|accepts |requires )`)},
	}
	// The chart-property classes ACCEPTED_SWEEP_UNRESOLVED may excuse,
	// recognized only as Helm's outer diagnostic, never inside a payload: a
	// whole line of a lint that reported no [ERROR] message, and the whole
	// first "Error: " line of a template run. Helm wraps any Chart.yaml read
	// error; only a missing file is the adjudicated loader case.
	lintClasses = []class{
		{"loader", regexp.MustCompile(`^Error unable to check Chart\.yaml file in chart: stat .*Chart\.yaml: ` + missingFile + `$`)},
		{"missing-dependency", regexp.MustCompile(`^\[WARNING\] .*: chart directory is missing these dependencies: .*$`)},
	}
	templateClasses = []class{
		{"loader", regexp.MustCompile(`^Error: unable to detect chart at .*Chart\.yaml: open .*Chart\.yaml: ` + missingFile + `$`)},
		{"library-chart", regexp.MustCompile(`^Error: library charts are not installable$`)},
		{"missing-dependency", regexp.MustCompile(`^Error: an error occurred while checking for chart dependencies\. .*$`)},
		{"kube-version-incompatible", regexp.MustCompile(`^Error: chart requires kubeVersion: .*$`)},
	}
	templateVerdict = regexp.MustCompile(`(?m)^Error: (execution error at \(|template: |parse error at \(|YAML parse error on |[^` + pySpace + `]+:` + pyDigit + `+:` + pyDigit + `+\n[` + pySpace + `]+executing )`)
	lintPassSummary = regexp.MustCompile(`(?m)^1 chart\(s\) linted, 0 chart\(s\) failed$`)
	pyLineBreaks    = strings.NewReplacer("\x0b", "\n", "\x0c", "\n", "\x1c", "\n", "\x1d", "\n", "\x1e", "\n", "\u0085", "\n", " ", "\n", " ", "\n")
)

// pyText is the log as `pathlib.Path(log).read_text(errors="replace")` sees it.
func pyText(log []byte) string {
	text := strings.ToValidUTF8(string(log), "�")
	return strings.ReplaceAll(strings.ReplaceAll(text, "\r\n", "\n"), "\r", "\n")
}

// classify is classify_helm's class ('pass', 'reject' or 'unresolved:<why>')
// of one helm lint (mode "lint") or helm template run with exit code rc.
func classify(mode string, rc int, log []byte) string {
	text := pyText(log)
	if rc != 0 && rc != 1 {
		return fmt.Sprintf("unresolved:exit-%d", rc)
	}
	for _, p := range operational {
		if p.re.MatchString(text) {
			return "unresolved:" + p.why
		}
	}
	lines := strings.Split(pyLineBreaks.Replace(text), "\n")
	if mode == "lint" {
		lintErrors := false
		for _, line := range lines {
			lintErrors = lintErrors || strings.HasPrefix(line, "[ERROR]")
		}
		for _, p := range lintClasses {
			for _, line := range lines {
				if !lintErrors && p.re.MatchString(line) {
					return "unresolved:" + p.why
				}
			}
		}
		if rc == 0 {
			if lintPassSummary.MatchString(text) {
				return classPass
			}
			return "unresolved:no-lint-summary"
		}
		errorLines := 0
		for _, line := range lines {
			if strings.HasPrefix(line, "[ERROR]") {
				if !lintVerdictLine.MatchString(line) {
					return "unresolved:unknown-lint-failure"
				}
				errorLines++
			}
		}
		if strings.Contains(text, "==> Linting ") && strings.Contains(text, "Error: 1 chart(s) linted, 1 chart(s) failed") && errorLines > 0 {
			return classReject
		}
		return "unresolved:unknown-lint-failure"
	}
	outer := ""
	for _, line := range lines {
		if strings.HasPrefix(line, "Error: ") {
			outer = line
			break
		}
	}
	for _, p := range templateClasses {
		if p.re.MatchString(outer) {
			return "unresolved:" + p.why
		}
	}
	if rc == 0 {
		return classPass
	}
	if strings.Contains(text, schemaInControl) {
		return "unresolved:schema-in-control"
	}
	if templateVerdict.MatchString(text) {
		return classReject
	}
	return "unresolved:unknown-template-failure"
}

// identified reports whether class is a verdict the cache may keep.
func identified(class string) bool {
	return class == classPass || class == classReject
}
