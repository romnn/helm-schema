{{- define "hard-fixture.name" -}}
hard-fixture
{{- end -}}

{{- define "hard-fixture.fullname" -}}
{{ include "hard-fixture.name" . }}
{{- end -}}

{{- define "hard-fixture.renderLabels" -}}
{{- range $k, $v := .Values.extraLabels -}}
{{ $k }}: {{ $v | quote }}
{{- end -}}
{{- end -}}

{{- define "hard-fixture.merge" -}}
{{- $x := .values -}}
{{- range $i, $m := $x -}}
{{- range $k, $v := $m -}}
{{ $k }}: {{ $v | quote }}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "hard-fixture.render" -}}
{{- $value := index . "value" -}}
{{- $ctx := index . "context" -}}
{{- tpl $value $ctx -}}
{{- end -}}
