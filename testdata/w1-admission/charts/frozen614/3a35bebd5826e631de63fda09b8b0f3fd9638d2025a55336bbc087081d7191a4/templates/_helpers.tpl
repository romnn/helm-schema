{{- define "sel" -}}{{ ternary .Values.t (b64dec .Values.t) .Values.flag }}{{- end -}}
