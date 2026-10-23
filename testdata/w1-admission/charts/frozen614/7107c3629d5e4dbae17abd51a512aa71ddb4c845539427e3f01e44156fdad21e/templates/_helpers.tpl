{{- define "rdw" -}}{{- $_ := set . "w" 1 -}}k: {{ ternary "a" "b" .v | quote }}{{- end -}}
