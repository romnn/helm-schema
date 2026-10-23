{{- define "rd" -}}k: {{ ternary "a" "b" .v | quote }}{{- end -}}
