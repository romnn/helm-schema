{{- define "sel" -}}{{ if .Values.flag }}{{ .Values.t }}{{ else }}{{ b64dec .Values.t }}{{ end }}{{- end -}}
