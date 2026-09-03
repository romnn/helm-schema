{{- define "okteto.internalCertificate.data" -}}
{{- if .Values.internalCertificate.duration -}}
{{- fail "'.Values.internalCertificate.duration' is deprecated, use instead '.Values.internalCertificate.durationDays'" -}}
{{- end -}}
{{- $secretName := include "okteto.internalCertificate" . -}}
{{- $tlsSecret := lookup "v1" "Secret" .Release.Namespace $secretName -}}
{{- if $tlsSecret -}}
ca.crt: |- {{ index $tlsSecret.data "ca.crt" | b64dec | nindent 2 }}
tls.crt: |- {{ index $tlsSecret.data "tls.crt" | b64dec | nindent 2 }}
tls.key: |- {{ index $tlsSecret.data "tls.key" | b64dec | nindent 2 }}
{{- else -}}
{{- $oktetoInternalCA := genCA "okteto-internal-ca" (int .Values.internalCertificate.durationDays) -}}
{{- $webhookName := include "okteto.webhook" . -}}
{{- $regcredsName := printf "%s-regcreds" (include "okteto.fullname" .) -}}
{{- $buildkitAdapterName := printf "%s-buildkit-adapter" (include "okteto.fullname" .) -}}
{{- $altNames := list $webhookName (printf "%s.%s" $webhookName .Release.Namespace) (printf "%s.%s.svc" $webhookName .Release.Namespace) $regcredsName (printf "%s.%s" $regcredsName .Release.Namespace) (printf "%s.%s.svc" $regcredsName .Release.Namespace) $buildkitAdapterName (printf "%s.%s" $buildkitAdapterName .Release.Namespace) (printf "%s.%s.svc" $buildkitAdapterName .Release.Namespace) -}}
{{- $cert := genSignedCert $webhookName nil $altNames (int .Values.internalCertificate.durationDays) $oktetoInternalCA -}}
ca.crt: |- {{ $oktetoInternalCA.Cert | nindent 2 }}
tls.crt: |- {{ $cert.Cert | nindent 2 }}
tls.key: |- {{ $cert.Key | nindent 2 }}
{{- end -}}
{{- end -}}

{{- define "okteto.wildcardCertificate.data" -}}
{{- $tlsSecret := lookup "v1" "Secret" .Release.Namespace (include "okteto.tlsSecret" .) -}}
{{- $emptySecret := dict "metadata" (dict "annotations" dict) -}}
{{- if and $tlsSecret (eq (index ($tlsSecret | default $emptySecret).metadata.annotations "dev.okteto.com/fqdns") (include "okteto.fqdns" .)) -}}
ca.crt: |- {{ index $tlsSecret.data "ca.crt" | b64dec | nindent 2 }}
tls.crt: |- {{ index $tlsSecret.data "tls.crt" | b64dec | nindent 2 }}
tls.key: |- {{ index $tlsSecret.data "tls.key" | b64dec | nindent 2 }}
{{- else -}}
{{- $oktetoWildcardCA := genCA "okteto-wildcard-ca" 3650 -}}
{{- $wildcardAltNames := compact (list (include "okteto.wildcard" .) .Values.publicOverride) -}}
{{- $wildcardCert := genSignedCert (include "okteto.wildcard" .) nil $wildcardAltNames 3650 $oktetoWildcardCA -}}
ca.crt: |- {{ $oktetoWildcardCA.Cert | nindent 2 }}
tls.crt: |- {{ $wildcardCert.Cert | nindent 2 }}
tls.key: |- {{ $wildcardCert.Key | nindent 2 }}
{{- end -}}
{{- end -}}


{{- define "okteto.fqdns" -}}
{{ compact (list .Values.subdomain .Values.publicOverride) | toJson }}
{{- end -}}
