{{- define "s" -}}{{- if .Values.a -}}{{- $_ := set . "f" true -}}{{- end -}}{{- end -}}
