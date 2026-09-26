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
)

var (
	lintVerdictLine = regexp.MustCompile(`^\[ERROR\] (values\.yaml: |templates/)`)
	infrastructure  = []struct {
		why string
		re  *regexp.Regexp
	}{
		{"missing-dependency", regexp.MustCompile(`missing these dependencies|but missing in charts/ directory|error occurred while checking for chart dependencies`)},
		{"loader", regexp.MustCompile(`unable to load chart|cannot load (Chart|requirements|values)\.yaml|Chart\.yaml file is missing|unable to check Chart\.yaml|non-absolute URLs|error unpacking`)},
		{"invalid-kube-version", regexp.MustCompile(`invalid kube version`)},
		{"kube-version-incompatible", regexp.MustCompile(`chart requires kubeVersion`)},
		{"values-file", regexp.MustCompile(`(?m)^Error: (open |failed to parse )`)},
		{"usage", regexp.MustCompile(`(?m)^Error: (unknown (flag|command|shorthand)|accepts |requires )`)},
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
	for _, p := range infrastructure {
		if p.re.MatchString(text) {
			return "unresolved:" + p.why
		}
	}
	if mode == "lint" {
		if rc == 0 {
			if lintPassSummary.MatchString(text) {
				return classPass
			}
			return "unresolved:no-lint-summary"
		}
		errorLines := 0
		for _, line := range strings.Split(pyLineBreaks.Replace(text), "\n") {
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
