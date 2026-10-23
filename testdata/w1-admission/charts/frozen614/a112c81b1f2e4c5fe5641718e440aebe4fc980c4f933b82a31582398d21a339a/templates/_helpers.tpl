{{- define "w" -}}{{ ternary "a" "b" (and (set .Values "flag" false) .Values.flag) }}{{- end -}}
