{{- define "a3" -}}{{ if .Values.a }}{{ .Values.t }}{{ else if .Values.b }}{{ b64dec .Values.t }}{{ else }}{{ .Values.t }}{{ end }}{{- end -}}
