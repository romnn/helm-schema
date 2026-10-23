{{ define "h" }}{{ if .Values.choose }}{{ dict }}{{ end }}
{{ end }}{{ define "outer" }}{{ include "h" . }}{{ end }}