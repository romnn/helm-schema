{{- define "t3" -}}{{ if eq .Values.f "a" }}{{ .Values.t }}{{ else if eq .Values.f "b" }}{{ trunc 1 .Values.t }}{{ else }}{{ .Values.t }}{{ end }}{{- end -}}
