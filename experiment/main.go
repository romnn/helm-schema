package main

import (
	"os"
	"text/template"
)

func main() {
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
