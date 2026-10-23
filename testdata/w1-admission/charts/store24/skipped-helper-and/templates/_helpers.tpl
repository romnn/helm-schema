{{ define "h" }}{{ $m := dict "enabled" .Values.initial }}{{ $_ := and false (set $m "enabled" (not .Values.initial)) }}{{ if $m.enabled }}{{ fail "body" }}{{ end }}{{ end }}
