{{- define "inner" -}}{{ .t }}{{- end -}}
{{- define "outer" -}}{{ include "inner" . }}{{- end -}}
