{{- define "dict-config.pdb" -}}
{{- if .config.enabled }}
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: dict-config
spec:
  minAvailable: {{ .config.minAvailable }}
  {{- with .config.maxUnavailable }}
  maxUnavailable: {{ . }}
  {{- end }}
{{- end }}
{{- end }}

{{- define "dict-config.ingress" -}}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: dict-config
spec:
  {{- with .config.className }}
  ingressClassName: {{ . }}
  {{- end }}
{{- end }}
