{{- define "pt" -}}{{- if .Values.flag -}}{{ .Values.p }}{{- else -}}{{ tpl .Values.t $ }}{{- end -}}{{- end -}}
