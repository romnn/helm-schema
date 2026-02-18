{{- define "full-fixture.name" -}}
full-fixture
{{- end -}}

{{- define "full-fixture.fullname" -}}
{{- printf "%s" (include "full-fixture.name" .) -}}
{{- end -}}
