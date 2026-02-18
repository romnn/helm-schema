package main

import (
	"bytes"
	"fmt"
	"os"
	"text/template"

	sprig "github.com/Masterminds/sprig/v3"
)

func t1() {
	const tpl = `
{{- define "labels.helper" -}}
extra: true
{{- end -}}
metadata:
  labels: {{ include "labels.helper" . | nindent 4 }}
	# static siblings must still parse
	app.kubernetes.io/name: app
`

	// Build a template with sprig funcs + our helm-like include.
	tmpl := template.New("demo")

	// Close over tmpl so include can look up named templates after Parse.
	funcs := sprig.TxtFuncMap()
	funcs["include"] = func(name string, data any) (string, error) {
		var b bytes.Buffer
		t := tmpl.Lookup(name)
		if t == nil {
			return "", fmt.Errorf("include: template %q not found", name)
		}
		if err := t.Execute(&b, data); err != nil {
			return "", fmt.Errorf("include %q: %w", name, err)
		}
		return b.String(), nil
	}

	// Important: Funcs must be added before Parse.
	tmpl = tmpl.Funcs(funcs)

	// Parse and render.
	tmpl = template.Must(tmpl.Parse(tpl))
	if err := tmpl.Execute(os.Stdout, map[string]any{}); err != nil {
		panic(err)
	}

	data := map[string]any{}
	t := template.Must(template.New("demo").Parse(tpl))
	_ = t.Execute(os.Stdout, data)
}

func t2() {
	const tpl = `{{- range $k, $v := .Env }}
.dot.key = {{ .key }}
$k       = {{ $k }}
$v.value = {{ $v.value }}
-- 
{{- end }}`

	data := map[string]map[string]string{
		"FOO": {"key": "FOO", "value": "bar"},
		"BUZ": {"key": "BUZ", "value": "baz"},
	}

	t := template.Must(template.New("demo").Parse(tpl))
	_ = t.Execute(os.Stdout, map[string]any{"Env": data})
}

func main() {
	t1()
}
