{{- define "v" -}}{{ .Values.t }}{{- end -}}
{{- define "w" -}}{{ include "v" . }}{{- end -}}
