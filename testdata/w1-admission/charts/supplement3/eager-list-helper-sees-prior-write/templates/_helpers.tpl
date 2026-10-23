{{- define "check" -}}{{ if kindIs "string" .flag }}{{ fail "stale" }}{{ end }}{{- end -}}
