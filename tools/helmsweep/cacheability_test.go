package main

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"testing"
)

func tgz(t *testing.T, files map[string]string) string {
	var buffer bytes.Buffer
	zipped := gzip.NewWriter(&buffer)
	archive := tar.NewWriter(zipped)
	for name, data := range files {
		if err := archive.WriteHeader(&tar.Header{Name: name, Mode: 0o644, Size: int64(len(data)), Typeflag: tar.TypeReg}); err != nil {
			t.Fatal(err)
		}
		if _, err := archive.Write([]byte(data)); err != nil {
			t.Fatal(err)
		}
	}
	if err := archive.Close(); err != nil {
		t.Fatal(err)
	}
	if err := zipped.Close(); err != nil {
		t.Fatal(err)
	}
	return buffer.String()
}

// The sweep's cacheability parses every executed template with Helm's
// parser: a nondeterministic call anywhere (branches, defines, pipelines,
// packaged dependencies at any depth) or an unparsable template makes the
// chart uncacheable; lookup, non-template files and plain templates do not.
func TestChartCacheabilityFollowsTheBatteryPolicy(t *testing.T) {
	base := map[string]string{"Chart.yaml": "apiVersion: v2\nname: c\nversion: 1.0.0\n", "values.yaml": "a: '{{ now }}'\n"}
	nested := tgz(t, map[string]string{"sub/templates/a.yaml": `{{ define "x" }}{{ randAlphaNum 5 | quote }}{{ end }}`})
	cases := []struct {
		name  string
		files map[string]string
		want  bool
	}{
		{"plain", map[string]string{"templates/a.yaml": "a: {{ .Values.a | quote }}\n{{- include \"h\" . }}"}, true},
		{"lookup", map[string]string{"templates/a.yaml": `{{ $s := lookup "v1" "Secret" "ns" "s" }}{{ $s }}`}, true},
		{"helper outside templates", map[string]string{"files/a.yaml": "{{ now }}"}, true},
		{"now in a branch", map[string]string{"templates/a.yaml": "{{ if .Values.a }}{{ else }}{{ now }}{{ end }}"}, false},
		{"tpl", map[string]string{"templates/a.yaml": "{{ tpl .Values.a . }}"}, false},
		{"keys in a range", map[string]string{"templates/a.yaml": "{{ range keys .Values }}{{ . }}{{ end }}"}, false},
		{"genCA in a define", map[string]string{"templates/_h.tpl": `{{ define "h" }}{{ (genCA "x" 1).Cert }}{{ end }}`}, false},
		{"derivePassword in a with", map[string]string{"templates/a.yaml": `{{ with .Values }}{{ derivePassword 1 "l" "p" "u" "s" }}{{ end }}`}, false},
		{"packaged dependency", map[string]string{"charts/sub.tgz": tgz(t, map[string]string{"sub/charts/deep.tgz": nested})}, false},
		{"unparsable", map[string]string{"templates/a.yaml": "{{ if }}"}, false},
		{"not UTF-8", map[string]string{"templates/a.yaml": "\xff{{ .Values.a }}"}, false},
	}
	for _, c := range cases {
		dir := t.TempDir()
		writeFiles(t, dir, base)
		writeFiles(t, dir, c.files)
		got, reason, err := chartCacheability(dir)
		if err != nil || got != c.want || (!got && reason == "") {
			t.Errorf("%s: cacheable=%t reason=%q err=%v, want %t", c.name, got, reason, err, c.want)
		}
	}
}
