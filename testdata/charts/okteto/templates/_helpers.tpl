{{/* vim: set filetype=mustache: */}}
{{/*
Expand the name of the chart.
*/}}
{{- define "okteto.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "okteto.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "okteto.defaultBackendName" -}}
{{- if .Values.defaultBackend.nameOverride -}}
{{- print .Values.defaultBackend.nameOverride -}}
{{- else -}}
{{- printf "%s-ingress-nginx-defaultbackend" .Release.Name -}}
{{- end -}}
{{- end -}}

{{- define "okteto.fullNameCronJob" -}}
  {{- printf "%s-%s" (include "okteto.fullname" .context) .suffix | trunc 52 | trimSuffix "-" -}}
{{- end -}}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "okteto.chart" -}}
{{- printf "%s-%s" .Chart.Name (include "okteto.version" .) | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Return the chart versions as a yaml file.
NOTE: The fromYaml | toYaml pipeline is used so comments are omitted from the
final yaml and not rendered in the configmap template
*/}}
{{- define "okteto.allVersion" -}}
{{ $.Files.Get "versions.yaml" | fromYaml | toYaml }}
{{- end -}}

{{- define "okteto.version" -}}
{{- $file := (include  "okteto.allVersion" . | fromYaml) -}}
{{- print (get $file "OKTETO_VERSION") -}}
{{- end -}}

{{- define "okteto.cliVersion" -}}
{{- $file := (include  "okteto.allVersion" . | fromYaml) -}}
{{- print (get $file "OKTETO_CLI_VERSION") -}}
{{- end -}}

{{/*
The default public URL of the web app. Can be overriden with .Values.publicOverride. It defaults to okteto.SUBDOMAIN
*/}}
{{- define "okteto.public" -}}
{{- $subdomain := required "A valid .Values.subdomain is required" .Values.subdomain }}
{{- $name := printf "%s.%s" "okteto" $subdomain }}
{{- default $name .Values.publicOverride -}}
{{- end -}}

{{/*
Returns the private IP of the controller exposing the Okteto control plane.
When Gateway API is enabled, reads from endpoints.okteto.gatewayAPI.controller.ip.
Otherwise (ingress mode), reads endpoints.okteto.ingress.controller.ip first, then falls back to ingress.ip (legacy).
When ingress-nginx is enabled and no static IP is configured, resolves the service host at runtime.
When ingress-nginx is disabled, a static IP must be provided via one of the controller.ip values.

Note on the `required` calls below: Helm's `required` returns the value when it is non-empty
and only fails (with the provided message) when it is empty. The two `required` branches
therefore both validate that the IP is set AND return it for downstream use — they are not
pure error branches.
*/}}
{{- define "okteto.controllerprivateip" -}}
{{- $resolvedIP := include "okteto.controlplane.ip" . -}}
{{- if (index .Values "ingress-nginx" "enabled") -}}
{{- if $resolvedIP -}}
{{- $resolvedIP -}}
{{- else -}}
{{- printf "$(%s_SERVICE_HOST)" (regexReplaceAll "-" (include "ingress-nginx.controller.fullname" (index .Subcharts "ingress-nginx")) "_") | upper -}}
{{- end -}}
{{- else if .Values.endpoints.okteto.gatewayAPI.enabled -}}
{{/* Returns $resolvedIP when set; fails with the message below when empty. */}}
{{- required "When 'endpoints.okteto.gatewayAPI.enabled' is true you must set '.Values.endpoints.okteto.gatewayAPI.controller.ip'" $resolvedIP -}}
{{- else if .Values.endpoints.okteto.ingress.enabled -}}
{{/* Returns $resolvedIP when set; fails with the message below when empty. */}}
{{- required "When 'endpoints.okteto.ingress.enabled' is true and 'ingress-nginx.enabled' is false you must set '.Values.endpoints.okteto.ingress.controller.ip' (or the legacy '.Values.ingress.ip')" $resolvedIP -}}
{{- end -}}
{{- end -}}

{{- define "okteto.buildkitname" -}}
{{- $name :=  include "okteto.fullname" . }}
{{- printf "%s-buildkit-%s" $name (substr 0 10 ((printf "%s%s%t%s" .Values.buildkit.persistence.size .Values.buildkit.persistence.class .Values.buildkit.rootless.enabled .Values.unsupported.buildkitCacheVersion) | sha256sum )) -}}
{{- end -}}

{{/*
Returns the private IP of the service exposing buildkit

*/}}
{{- define "okteto.buildkitprivateip" -}}
{{- $name :=  include "okteto.fullname" . }}
{{- printf "$(%s_BUILDKIT_SERVICE_HOST)" (regexReplaceAll "-" $name "_") | upper -}}
{{- end -}}

{{/*
Returns the private IP of the service exposing ssh-agent
*/}}
{{- define "okteto.sshagentprivateip" -}}
{{- $name :=  include "okteto.sshAgent" . }}
{{- printf "$(%s_SERVICE_HOST)" (regexReplaceAll "-" $name "_") | upper -}}
{{- end -}}

{{/*
Returns the name of the ssh-agent deployment name
*/}}
{{- define "okteto.sshAgent" -}}
{{- $name := include "okteto.fullname" . }}
{{- printf "%s-ssh-agent" $name -}}
{{- end -}}

{{/*
Returns the wildcard domain form
*/}}
{{- define "okteto.wildcard" -}}
{{- printf "*.%s" .Values.subdomain -}}
{{- end -}}

{{/*
Returns the name of the mutation webhook
*/}}
{{- define "okteto.webhook" -}}
{{- $name := include "okteto.fullname" . }}
{{- printf "%s-mutation-webhook" $name -}}
{{- end -}}

{{/*
Returns the name of the cluster role to be bind to every namespace member service account
*/}}
{{- define "okteto.namespacesclusterroles" -}}
{{- if .Values.clusterRole -}}
{{ .Values.clusterRole }}
{{- else if .Values.serviceAccounts.roleBindings.namespaces -}}
{{ .Values.serviceAccounts.roleBindings.namespaces }}
{{- else -}}
{{ fail "'.Values.serviceAccounts.roleBindings.namespaces' cannot be empty. You must specify a cluster role to bind to user's service account within each namespace they have access to"}}
{{- end -}}
{{- end -}}

{{/*
Returns the name of the cluster role to be bind to every user's service account within global preview environments
*/}}
{{- define "okteto.previewsclusterroles" -}}
{{- if .Values.serviceAccounts.roleBindings.previews -}}
{{ .Values.serviceAccounts.roleBindings.previews }}
{{- else -}}
{{ fail "'.Values.serviceAccounts.roleBindings.previews' cannot be empty. You must specify a cluster role to bind to user's service account within each global preview environment"}}
{{- end -}}
{{- end -}}

{{/*
Returns the name of the cluster role to be bind to every Okteto user via cluster role binding
*/}}
{{- define "okteto.clusterrolebinding" -}}
{{- if .Values.globalClusterRole -}}
{{ .Values.globalClusterRole }}
{{- else -}}
{{ .Values.serviceAccounts.clusterRoleBinding }}
{{- end -}}
{{- end -}}

{{/*
Returns the list of annotations to be applied for every user's service account
*/}}
{{- define "okteto.serviceaccountsannotations" -}}
{{- /*
Needed to avoid a nil pointer when user key was removed from helm chart values
*/}}
{{- $user := .Values.user | default dict }}
{{- $serviceaccounts := $user.serviceAccount | default dict }}
{{- if $serviceaccounts.annotations -}}
{{ .Values.user.serviceAccount.annotations | toJson }}
{{- else -}}
{{ .Values.serviceAccounts.annotations | toJson }}
{{- end -}}
{{- end -}}

{{/*
Returns the list of labels to be applied for every user's service account
*/}}
{{- define "okteto.serviceaccountslabels" -}}
{{- /*
Needed to avoid a nil pointer when user key was removed from helm chart values
*/}}
{{- $user := .Values.user | default dict }}
{{- $serviceaccounts := $user.serviceAccount | default dict }}
{{- if $serviceaccounts.labels -}}
{{ .Values.user.serviceAccount.labels | toJson }}
{{- else -}}
{{ .Values.serviceAccounts.labels | toJson }}
{{- end -}}
{{- end -}}

{{/*
Returns the map of role bindings to apply per namespace for the users within those namespaces
*/}}
{{- define "okteto.serviceaccountsextrarolebindings" -}}
{{- /*
Needed to avoid a nil pointer when user key was removed from helm chart values
*/}}
{{- $user := .Values.user | default dict }}
{{- $extrarolebindings := $user.extraRoleBindings | default dict }}
{{- if $extrarolebindings.roleBindings -}}
{{ .Values.user.extraRoleBindings.roleBindings | toJson }}
{{- else -}}
{{ .Values.serviceAccounts.extraRoleBindings | toJson }}
{{- end -}}
{{- end -}}

{{- define "okteto.joinListWithComma" -}}
{{- $local := dict "first" true -}}
{{- range $k, $v := . -}}{{- if not $local.first -}},{{- end -}}{{- $v -}}{{- $_ := set $local "first" false -}}{{- end -}}
{{- end -}}

{{- define "okteto.dockerconfigRoot" -}}
{{- printf "{ \"auths\": { " -}}
{{- $local := dict "first" true -}}
{{- range $name, $cred := . -}}
{{- if not $local.first -}}
{{- printf ", " -}}
{{- end -}}
{{- if $cred.token -}}
{{- printf "\"%s\": { \"auth\": \"%s\" }" $cred.url $cred.token -}}
{{- else -}}
{{- printf "\"%s\": { \"auth\": \"%s\" }" $cred.url (printf "%s:%s" $cred.user $cred.password | b64enc) -}}
{{- end -}}
{{- $_ := set $local "first" false -}}
{{- end -}}
{{- printf " } }" -}}
{{- end -}}

{{- define "okteto.hasEcrRegistry" -}}
{{- $local := dict "ecr" "false" -}}
{{- range $name, $cred := . -}}
{{- if hasSuffix ".amazonaws.com" $cred.url -}}
{{- $_ := set $local "ecr" "true" -}}
{{- end -}}
{{- end -}}
{{- printf "%s" $local.ecr -}}
{{- end -}}

{{- define "okteto.ecrURL" -}}
{{- range $name, $cred := . -}}
{{- if hasSuffix ".amazonaws.com" $cred.url -}}
{{- printf "%s" $cred.url -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "okteto.ecrRegion" -}}
{{- range $name, $cred := . -}}
{{- if hasSuffix ".amazonaws.com" $cred.url -}}
{{- $parts := split "." $cred.url -}}
{{- printf "%s" $parts._3 -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "okteto.ecrAccessKey" -}}
{{- range $name, $cred := . -}}
{{- if hasSuffix ".amazonaws.com" $cred.url -}}
{{- printf "%s" $cred.user -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "okteto.ecrSecretKey" -}}
{{- range $name, $cred := . -}}
{{- if hasSuffix ".amazonaws.com" $cred.url -}}
{{- printf "%s" $cred.password -}}
{{- end -}}
{{- end -}}
{{- end -}}



{{/*
Return the appropriate apiVersion for CronJob.
*/}}
{{- define "okteto.cronJob.apiVersion" -}}
{{- if semverCompare "<1.21-0" .Capabilities.KubeVersion.GitVersion -}}
{{- print "batch/v1beta1" -}}
{{- else -}}
{{- print "batch/v1" -}}
{{- end -}}
{{- end -}}

{{/*
The default public URL of buildkit
*/}}
{{- define "okteto.buildkit" -}}
{{- if and .Values.buildkit.ingress.enabled .Values.ingress.tlsSecret -}}
{{- printf "%s.%s" "okteto" .Values.subdomain }}
{{- else -}}
{{- printf "%s.%s" "buildkit" .Values.subdomain }}
{{- end -}}
{{- end -}}

{{- define "okteto.registry" -}}
{{- printf "%s.%s" "registry" .Values.subdomain }}
{{- end -}}

{{- define "okteto.registry.serviceAccountName" -}}
{{- default (printf "%s-registry" (include "okteto.fullname" .)) .Values.registry.serviceAccountName }}
{{- end -}}

{{/* Returns "true" if the Okteto control plane should be exposed via Ingress */}}
{{- define "okteto.controlplane.ingress.enabled" -}}
{{- print .Values.endpoints.okteto.ingress.enabled -}}
{{- end -}}

{{/* Returns "true" if the Okteto control plane should be exposed via Gateway API */}}
{{- define "okteto.controlplane.gateway.enabled" -}}
{{- print .Values.endpoints.okteto.gatewayAPI.enabled -}}
{{- end -}}

{{/* Resolves the IngressClass for the Okteto control plane ingresses.
     Priority:
       1. endpoints.okteto.ingress.class (new)
       2. ingress.oktetoIngressClass (legacy, if not "nginx")
       3. ingress.annotations["kubernetes.io/ingress.class"] (deprecated annotation fallback)
       4. "nginx" (default) */}}
{{- define "okteto.controlplane.ingressClass" -}}
{{- if .Values.endpoints.okteto.ingress.class -}}
{{- .Values.endpoints.okteto.ingress.class -}}
{{- else if not (eq .Values.ingress.oktetoIngressClass "nginx") -}}
{{- .Values.ingress.oktetoIngressClass -}}
{{- else if index .Values.ingress.annotations "kubernetes.io/ingress.class" -}}
{{- index .Values.ingress.annotations "kubernetes.io/ingress.class" -}}
{{- else -}}
{{- "nginx" -}}
{{- end -}}
{{- end -}}

{{/* Resolves the controller IP for the Okteto control plane (used for internal DNS resolution).
     When Gateway API mode is enabled, returns endpoints.okteto.gatewayAPI.controller.ip.
     Otherwise (ingress mode), returns endpoints.okteto.ingress.controller.ip, falling back to ingress.ip (legacy). */}}
{{- define "okteto.controlplane.ip" -}}
{{- if .Values.endpoints.okteto.gatewayAPI.enabled -}}
{{- .Values.endpoints.okteto.gatewayAPI.controller.ip -}}
{{- else -}}
{{- .Values.endpoints.okteto.ingress.controller.ip | default .Values.ingress.ip -}}
{{- end -}}
{{- end -}}

{{/* Merges ingress annotations from endpoints.okteto.ingress.annotations (takes precedence)
     and ingress.annotations (legacy fallback). Returns the effective annotation map as JSON. */}}
{{- define "okteto.controlplane.ingress.annotations" -}}
{{- merge .Values.endpoints.okteto.ingress.annotations .Values.ingress.annotations | toJson -}}
{{- end -}}

{{/* Returns the custom labels for Okteto control plane Ingress resources as JSON. */}}
{{- define "okteto.controlplane.ingress.labels" -}}
{{- .Values.endpoints.okteto.ingress.labels | toJson -}}
{{- end -}}

{{/* Returns the parentRefs list for Okteto control plane HTTPRoute resources as JSON. */}}
{{- define "okteto.controlplane.gateway.parentRefs" -}}
{{- $ref := dict -}}
{{- $_ := set $ref "name" .Values.endpoints.okteto.gatewayAPI.gateway.name -}}
{{- $_ := set $ref "namespace" .Values.endpoints.okteto.gatewayAPI.gateway.namespace -}}
{{- if .Values.endpoints.okteto.gatewayAPI.gateway.section -}}
{{- $_ := set $ref "sectionName" .Values.endpoints.okteto.gatewayAPI.gateway.section -}}
{{- end -}}
{{- list $ref | toJson -}}
{{- end -}}

{{/* Returns "true" if the kube-apiserver proxy should be rendered. */}}
{{- define "okteto.kubeAPIServerProxy.enabled" -}}
{{- $gatewayAPIEnabled := eq "true" (include "okteto.controlplane.gateway.enabled" .) -}}
{{- $clusterEndpoint := .Values.cluster.endpoint -}}
{{- $clusterEndpointEmpty := empty $clusterEndpoint -}}
{{- $enabled := and $gatewayAPIEnabled $clusterEndpointEmpty -}}
{{- print $enabled -}}
{{- end -}}

{{/* Hostname used for the Kubernetes API endpoint. */}}
{{- define "okteto.kubeAPIServerProxy.hostname" -}}
{{- printf "kubernetes.%s" .Values.subdomain -}}
{{- end -}}

{{/* TLS secret mounted by the kube-apiserver proxy for Gateway-to-proxy backend TLS. */}}
{{- define "okteto.kubeAPIServerProxy.tlsSecret" -}}
{{- .Values.kubeAPIServerProxy.tls.secretName | default (include "okteto.ingress.tlsSecret" .) -}}
{{- end -}}

{{- define "okteto.privateEndpoints" -}}
{{- printf "%s.%s" "private-endpoints" .Values.subdomain }}
{{- end -}}

{{/* Returns the Kubernetes Service name that handles private-endpoints auth requests.
     When the dedicated private-endpoints component is enabled it has its own Service;
     otherwise the API Service acts as the handler. */}}
{{- define "okteto.privateEndpointsServiceName" -}}
{{- if eq (include "okteto.privateEndpoints.enabled" .) "true" -}}
{{- printf "%s-private-endpoints" (include "okteto.fullname" .) -}}
{{- else -}}
{{- printf "%s-api" (include "okteto.fullname" .) -}}
{{- end -}}
{{- end -}}

{{- define "okteto.internalCertificate" -}}
{{ printf "%s-internal-tls-%s" (include "okteto.fullname" .) .Values.internalCertificate.version }}
{{- end -}}

{{- define "okteto.installDaemonset" -}}
{{- if not .Values.daemonset.enabled }}
{{- print false }}
{{- else if .Values.overrideRegistryResolution.enabled }}
{{- print true }}
{{- else if .Values.overrideFileWatchers.enabled }}
{{- print true }}
{{- else if .Values.privateRegistry }}
{{- print true }}
{{- else }}
{{- print false }}
{{- end -}}
{{- end -}}

{{- define "okteto.daemonset.privateRegistryDestinationPath" -}}
  {{- if .Values.daemonset.configurePrivateRegistriesInNodes.enabled }}
    {{- print "/var/lib/kubelet/config.json" }}
  {{- else }}
    {{- print "" }}
  {{- end -}}
{{- end -}}

{{/*
  Returns whether the daemonset should run the private CA task.

  It is an AND gate on top of "okteto.privateca.enabled": '.Values.daemonset.injectCA.enabled'
  can only switch the task off, never on. Note this cannot be written as
  'and (...) (include "okteto.privateca.enabled" .)' because include returns a string, and
  the string "false" is truthy.
*/}}
{{- define "okteto.daemonset.privateca.enabled" -}}
{{- if not .Values.daemonset.injectCA.enabled -}}
{{- print false -}}
{{- else -}}
{{- include "okteto.privateca.enabled" . -}}
{{- end -}}
{{- end -}}

{{- define "okteto.tlsSecret" -}}
{{- if .Values.wildcardCertificate.create }}
{{- printf "%s-selfsigned" .Values.wildcardCertificate.name }}
{{- else }}
{{- .Values.wildcardCertificate.name }}
{{- end -}}
{{- end -}}

{{- define "okteto.ingress.tlsSecret" -}}
{{- if .Values.ingress.tlsSecret }}
{{- print .Values.ingress.tlsSecret }}
{{- else }}
{{- print (include "okteto.tlsSecret" .) }}
{{- end -}}
{{- end -}}

{{- define "okteto.registry.tlsSecret" -}}
{{- if .Values.registry.ingress.tlsSecret }}
{{- print .Values.registry.ingress.tlsSecret }}
{{- else }}
{{- print (include "okteto.tlsSecret" .) }}
{{- end -}}
{{- end -}}

{{- define "okteto.buildkit.tlsSecret" -}}
{{- if .Values.buildkit.ingress.tlsSecret }}
{{- print .Values.buildkit.ingress.tlsSecret }}
{{- else }}
{{- print (include "okteto.tlsSecret" .) }}
{{- end -}}
{{- end -}}

{{/*
  Returns the name of the auth provider
*/}}
{{- define "okteto.authProvider" -}}
{{- if .Values.auth.google.enabled }}
{{- print "google" }}
{{- else if .Values.auth.github.enabled }}
{{- print "github" }}
{{- else if .Values.auth.bitbucket.enabled }}
{{- print "bitbucket" }}
{{- else if .Values.auth.openid.enabled }}
{{- print "openid" }}
{{- else if .Values.auth.token.enabled }}
{{- print "token" }}
{{- else }}
{{- print "n/a" }}
{{- end -}}
{{- end -}}

{{- define "okteto.tlsSecretsToReload" -}}
{{- if .Values.wildcardCertificate.privateCA.enabled }}
{{- printf "%s,%s" .Values.wildcardCertificate.name .Values.wildcardCertificate.privateCA.secret.name }}
{{- else }}
{{- print (include "okteto.tlsSecret" .) }}
{{- end -}}
{{- end -}}

{{- define "okteto.oktetoAffinity" -}}
{{- if .Values.affinity.nodeAffinity }}
{{- toYaml .Values.affinity.nodeAffinity }}
{{- else }}
{{- toYaml .Values.affinity.oktetoPool }}
{{- end -}}
{{- end -}}

{{- define "okteto.privateca.enabled" -}}
{{- if .Values.wildcardCertificate.privateCA.enabled }}
{{- print true }}
{{- else if .Values.wildcardCertificate.create }}
{{- print true }}
{{- else -}}
{{- print false }}
{{- end -}}
{{- end -}}

{{- define "okteto.privateca.secretname" -}}
{{- if .Values.wildcardCertificate.privateCA.enabled -}}
{{- .Values.wildcardCertificate.privateCA.secret.name -}}
{{- else if .Values.wildcardCertificate.create -}}
{{- include "okteto.tlsSecret" . -}}
{{- else -}}
{{- fail "either .Values.wildcardCertificate.privateCA.enabled or .Values.wildcardCertificate.create must be true" -}}
{{- end -}}
{{- end -}}

{{- define "okteto.privateca.secretkey" -}}
{{- if .Values.wildcardCertificate.privateCA.enabled -}}
{{- .Values.wildcardCertificate.privateCA.secret.key -}}
{{- else if .Values.wildcardCertificate.create -}}
{{- print "ca.crt" -}}
{{- else -}}
{{- fail "either .Values.wildcardCertificate.privateCA.enabled or .Values.wildcardCertificate.create must be true" -}}
{{- end -}}
{{- end -}}

{{- define "okteto.privateEndpoints.enabled" -}}
{{- if .Values.unsupported.privateEndpointsOverride -}}
{{- fail "'.Values.unsupported.privateEndpointsOverride' is deprecated, use instead '.Values.privateEndpoints'" -}}
{{- end -}}
{{- if .Values.publicOverride }}
{{- print true }}
{{- else }}
{{- print false }}
{{- end -}}
{{- end -}}

{{- define "okteto.authToken.enabled" -}}
{{- if eq (include "okteto.authProvider" . ) "token" }}
{{- print true }}
{{- else }}
{{- print false }}
{{- end -}}
{{- end -}}

{{- define "okteto.authToken" -}}
  {{- if eq (include "okteto.authToken.enabled" .) "false" -}}
    {{- fail "Cannot generate auth token. Token auth is disable. Set '.Values.auth.token.enabled=true' and try again" -}}
  {{- end -}}
  {{- if .Values.auth.token.adminToken -}}
    {{- /* TODO @jpf-okteto: Remove this condition when helm schema is included */ -}}
    {{- /* https://okteto.atlassian.net/browse/PROD-258 */ -}}
    {{- if not (kindIs "string" .Values.auth.token.adminToken) -}}
      {{- fail (printf "'.Values.auth.token.adminToken' must be a string, currently it is %s" (kindOf .Values.auth.token.adminToken)) -}}
    {{- end -}}
    {{- if not (or (eq (len .Values.auth.token.adminToken) 40) (eq (len .Values.auth.token.adminToken) 8)) -}}
      {{- fail "'.Values.auth.token.adminToken' must be an alphanumeric of 8 or 40 characters string" -}}
    {{- end -}}
    {{- print .Values.auth.token.adminToken -}}
  {{- else -}}
    {{- $emptySA := dict "metadata" (dict "labels" dict) -}}
    {{- $adminSA := lookup "v1" "ServiceAccount" .Release.Namespace "okteto-admin" -}}
    {{- $currentToken := index ($adminSA | default $emptySA).metadata.labels "dev.okteto.com/token" -}}
    {{- if $currentToken -}}
      {{- print $currentToken -}}
    {{- else -}}
      {{- /*
      We use default namespace as release namespace might not exist when the template is rendered if the flag
      --create-namespace is specified when installing Okteto
      */}}
      {{- $defaultNs := lookup "v1" "Namespace" "" "default" -}}
      {{- if $defaultNs -}}
        {{- $token := substr 0 8 (index ($defaultNs).metadata.uid) -}}
        {{- print $token -}}
      {{- else -}}
        {{- /*
        Derive a pseudo random password out of the release data. This should
        rarely happen. It would probably only get hit using a --dry-run install
        */}}
        {{- $name := default .Release.Namespace "default" -}}
        {{- $site := default .Chart.Name "default" -}}
        {{- $secret := .Release.Name -}}
        {{- $pass := derivePassword 1 "long" $secret $name $site -}}
        {{- print (substr 0 8 ($pass | b64enc | lower)) -}}
      {{- end -}}
    {{- end -}}
  {{- end -}}
{{- end -}}

{{- define "okteto.webhook.userIngress" -}}
{{- if (get .Values "okteto-nginx").enabled -}}
{{- print "okteto-nginx" -}}
{{- else -}}
{{- print "ingress-nginx" -}}
{{- end -}}
{{- end -}}

{{/* Resolves the IngressClass for dev namespace ingresses.
     Reads endpoints.dev.ingress.class first, then falls back to ingress.class (legacy). */}}
{{- define "okteto.dev.ingress.class" -}}
{{- if .Values.endpoints.dev.ingress.class -}}
{{- .Values.endpoints.dev.ingress.class -}}
{{- else if .Values.ingress.class -}}
{{- .Values.ingress.class -}}
{{- end -}}
{{- end -}}

{{/* Resolves the forceClass flag for dev namespace ingresses.
     True if either endpoints.dev.ingress.forceClass or ingress.forceIngressClass (legacy) is true. */}}
{{- define "okteto.dev.ingress.forceClass" -}}
{{- or .Values.endpoints.dev.ingress.forceClass .Values.ingress.forceIngressClass -}}
{{- end -}}

{{/* Resolves the forceSubdomain flag for dev namespace ingresses.
     True if either endpoints.dev.ingress.forceSubdomain or ingress.forceIngressSubdomain (legacy) is true. */}}
{{- define "okteto.dev.ingress.forceSubdomain" -}}
{{- or .Values.endpoints.dev.ingress.forceSubdomain .Values.ingress.forceIngressSubdomain -}}
{{- end -}}

{{/* Merges ingress annotations from endpoints.dev.ingress.annotations (takes precedence)
     and namespace.ingress.annotations (legacy fallback). Returns the effective annotation map as JSON. */}}
{{- define "okteto.dev.ingress.annotations" -}}
{{- merge .Values.endpoints.dev.ingress.annotations .Values.namespace.ingress.annotations | toJson -}}
{{- end -}}

{{/* Merges ingress labels from endpoints.dev.ingress.labels (takes precedence)
     and namespace.ingress.labels (legacy fallback). Returns the effective label map as JSON. */}}
{{- define "okteto.dev.ingress.labels" -}}
{{- merge .Values.endpoints.dev.ingress.labels .Values.namespace.ingress.labels | toJson -}}
{{- end -}}

{{/* Resolves the subdomain for dev ingress host generation.
     Reads endpoints.dev.ingress.subdomain first, then falls back to the top-level subdomain. */}}
{{- define "okteto.dev.ingress.subdomain" -}}
{{- if .Values.endpoints.dev.ingress.subdomain -}}
{{- .Values.endpoints.dev.ingress.subdomain -}}
{{- else -}}
{{- .Values.subdomain -}}
{{- end -}}
{{- end -}}

{{- define "okteto.ingress.class" -}}
{{- $class := include "okteto.dev.ingress.class" . -}}
{{- if (get .Values "okteto-nginx").enabled -}}
{{- print (default "okteto-nginx" $class) -}}
{{- else -}}
{{- print (default "okteto-controlplane-nginx" $class) -}}
{{- end -}}
{{- end -}}

{{- define "okteto.ingress.mode" -}}
  {{- if and (get .Values "okteto-nginx").enabled (get .Values "ingress-nginx").enabled -}}
    {{- print "dual" }}
  {{- else if and (not (get .Values "okteto-nginx").enabled) (get .Values "ingress-nginx").enabled -}}
    {{- print "single" }}
  {{- else -}}
    {{- print "disabled" }}
  {{- end -}}
{{- end -}}

{{- define "okteto.externalResources.enabled" -}}
  {{- if .Capabilities.APIVersions.Has "dev.okteto.com/v1/External" }}
    {{- print true }}
  {{- else -}}
    {{- print false }}
  {{- end -}}
{{- end -}}

{{- define "okteto.tolerations" -}}
{{- if or .globalsTolerations .poolTolerations }}
tolerations:
{{ with .globalsTolerations }}
{{- toYaml . }}
{{- end}}
{{- if .poolTolerations }}
- key: "okteto-node-pool"
  operator: "Equal"
  value: "{{ .poolTolerations }}"
  effect: "NoSchedule"
{{- end -}}
{{- else }}
tolerations: []
{{- end -}}
{{- end -}}

{{- define "okteto.nodeSelectors" -}}
{{- if or .poolTolerations .nodeSelectors }}
nodeSelector:
{{- if .poolTolerations }}
  okteto-node-pool: "{{ .poolTolerations }}"
{{- end -}}
  {{- range $key, $val := .nodeSelectors }}
  {{ $key }}: {{ $val | quote }}
  {{- end }}
{{- else }}
nodeSelector: {}
{{- end -}}
{{- end -}}

{{- define "okteto.defaultBackend.enabled" -}}
  {{- if and .Values.defaultBackend.enabled (or (get .Values "ingress-nginx").defaultBackend.enabled (get .Values "okteto-nginx").defaultBackend.enabled) }}
  {{ fail "'.Values.defaultBackend.enabled' is incompatible with '.Values.ingress-nginx.defaultBackend.enabled' or '.Values.okteto-nginx.defaultBackend.enabled'."}}
  {{- end }}
  {{- print .Values.defaultBackend.enabled }}
{{- end -}}

{{- define "okteto.telemetryJobSpecTemplate" -}}
metadata:
  labels:
    app.kubernetes.io/component: "telemetry"
    app.kubernetes.io/part-of: "okteto"
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- if .Values.podLabels }}
{{ toYaml .Values.podLabels | indent 4 }}
{{- end }}
{{- if .Values.telemetry.labels }}
{{ toYaml .Values.telemetry.labels | indent 4 }}
{{- end }}
{{- if or .Values.telemetry.annotations .Values.podAnnotations }}
  annotations:
  {{- if .Values.podAnnotations }}
{{ toYaml .Values.podAnnotations | indent 4 }}
  {{- end }}
  {{- if .Values.telemetry.annotations }}
{{ toYaml .Values.telemetry.annotations | indent 4 }}
  {{- end }}
{{- end }}
spec:
  affinity:
    nodeAffinity:
{{ include "okteto.oktetoAffinity" . | indent 6 }}
  serviceAccountName: {{ include "okteto.fullname" . }}
  priorityClassName: {{ .Values.telemetry.priorityClassName | default .Values.globals.priorityClassName }}
  {{- include "okteto.tolerations" (dict "globalsTolerations" .Values.globals.tolerations.okteto "poolTolerations" .Values.tolerations.oktetoPool ) | nindent 2 }}
  {{- include "okteto.nodeSelectors" (dict "poolTolerations" .Values.tolerations.oktetoPool "nodeSelectors" .Values.globals.nodeSelectors.okteto ) | nindent 2 }}
  restartPolicy: Never
  containers:
    - name: telemetry
      image: {{ include "okteto.image.backend" . }}
      imagePullPolicy: {{ .Values.pullPolicy }}
      args: [ "telemetry" ]
{{- if .Values.telemetry.extraEnv }}
      env:
{{ toYaml .Values.telemetry.extraEnv | indent 8 }}
{{- end }}
      envFrom:
        - configMapRef:
            name: "{{ include "okteto.fullname" . }}"
        - secretRef:
            name: {{ include "okteto.fullname" . }}
        - secretRef:
            name: "{{ .Values.cloud.secret.name }}"
            optional: true
      resources:
{{ toYaml .Values.telemetry.resources | indent 8 }}
{{- end -}}

{{- define "okteto.defaultClusterEndpoint" -}}
  {{- $subdomain := required "A valid .Values.subdomain is required" .Values.subdomain -}}
  {{- printf "https://kubernetes.%s" $subdomain -}}
{{- end -}}

{{- define "okteto.clusterEndpoint" -}}
  {{- $defaultEndpoint := include "okteto.defaultClusterEndpoint" . -}}
  {{- $endpoint := .Values.cluster.endpoint -}}
  {{- print (default $defaultEndpoint $endpoint) -}}
{{- end -}}

{{- define "okteto.skipCAInCredentialsQuery" -}}
  {{- $defaultEndpoint := include "okteto.defaultClusterEndpoint" . -}}
  {{- $endpoint := include "okteto.clusterEndpoint" . -}}
  {{- if eq $endpoint $defaultEndpoint  }}
  {{- print true }}
  {{- else }}
  {{- print false }}
  {{- end -}}
{{- end -}}

{{- define "okteto.botUserToken" -}}
  {{- $emptySA := dict "metadata" (dict "labels" dict) -}}
  {{- $adminSA := lookup "v1" "ServiceAccount" .Release.Namespace .Values.oktetoBotUser -}}
  {{- $currentToken := index ($adminSA | default $emptySA).metadata.labels "dev.okteto.com/token" -}}
  {{- if $currentToken -}}
    {{- print $currentToken -}}
  {{- else -}}
    {{- /*
    We use default namespace as release namespace might not exist when the template is rendered if the flag
    --create-namespace is specified when installing Okteto
    */}}
    {{- $defaultNs := lookup "v1" "Namespace" "" "default" -}}
    {{- if $defaultNs -}}
      {{- $token := substr 28 36 (index ($defaultNs).metadata.uid) -}}
      {{- print $token -}}
    {{- else -}}
      {{- /*
      Derive a pseudo random password out of the release data. This should
      rarely happen. It would probably only get hit using a --dry-run install
      */}}
      {{- $name := default .Release.Namespace "default" -}}
      {{- $site := default .Chart.Name "default" -}}
      {{- $secret := .Release.Name -}}
      {{- $pass := derivePassword 1 "maximum" $secret $name $site -}}
      {{- print (substr 0 8 ($pass | b64enc | lower)) -}}
    {{- end -}}
  {{- end -}}
{{- end -}}

{{/* Validates that the Resource Manager recommendations weight is between 0 and 1 */}}
{{- define "okteto.resourceManager.recommendations.weight" -}}
{{- if .Values.resourceManager.enabled -}}
  {{- if hasKey .Values.resourceManager "recommendations" -}}
    {{- if .Values.resourceManager.recommendations -}}
      {{- $weight := .Values.resourceManager.recommendations.weight -}}
      {{- if not $weight -}}
        {{- fail "resourceManager.recommendations.weight must be set when resourceManager is enabled and recommendations are configured" -}}
      {{- end -}}
      {{- $floatWeight := float64 $weight -}}
      {{- if or (lt $floatWeight 0.0) (gt $floatWeight 1.0) -}}
        {{- fail "resourceManager.recommendations.weight must be between 0 and 1" -}}
      {{- end -}}
      {{- $floatWeight -}}
    {{- else -}}
      {{- fail "resourceManager.recommendations must not be empty when resourceManager is enabled" -}}
    {{- end -}}
  {{- else -}}
    {{- fail "resourceManager.recommendations must be set when resourceManager is enabled" -}}
  {{- end -}}
{{- else -}}
  {{- if and (hasKey .Values.resourceManager "recommendations") (.Values.resourceManager.recommendations) (hasKey .Values.resourceManager.recommendations "weight") -}}
    {{- .Values.resourceManager.recommendations.weight -}}
  {{- end -}}
{{- end -}}
{{- end -}}

{{/* Returns the Resource Manager recommendations minimum CPU */}}
{{- define "okteto.resourceManager.recommendations.min.cpu" -}}
{{- $defaultCPU := "10m" -}}
{{- if .Values.resourceManager.enabled -}}
  {{- if and (hasKey .Values.resourceManager "recommendations") (.Values.resourceManager.recommendations) -}}
    {{- .Values.resourceManager.recommendations.min.cpu | default $defaultCPU -}}
  {{- else -}}
    {{- $defaultCPU -}}
  {{- end -}}
{{- end -}}
{{- end -}}

{{/* Returns the Resource Manager recommendations minimum Memory */}}
{{- define "okteto.resourceManager.recommendations.min.memory" -}}
{{- $defaultMemory := "10Mi" -}}
{{- if .Values.resourceManager.enabled -}}
  {{- if and (hasKey .Values.resourceManager "recommendations") (.Values.resourceManager.recommendations) -}}
    {{- .Values.resourceManager.recommendations.min.memory | default $defaultMemory -}}
  {{- else -}}
    {{- $defaultMemory -}}
  {{- end -}}
{{- end -}}
{{- end -}}

{{/* Validates the Resource Manager recommendations correction */}}
{{- define "okteto.resourceManager.recommendations.correction" -}}
{{- if .Values.resourceManager.enabled -}}
  {{- $correction := .Values.resourceManager.recommendations.correction | default 1.0 -}}
  {{- $floatCorrection := float64 $correction -}}
  {{- if le $floatCorrection 0.0 -}}
    {{- fail "resourceManager.recommendations.correction must be greater than 0" -}}
  {{- end -}}
  {{- $floatCorrection -}}
{{- end -}}
{{- end -}}

{{/* Validates the Resource Manager deletePeriodDays */}}
{{- define "okteto.resourceManager.deletePeriodDays" -}}
{{- if .Values.resourceManager.enabled -}}
  {{- $deletePeriodDays := .Values.resourceManager.deletePeriodDays -}}
  {{- $intDeletePeriodDays := int $deletePeriodDays -}}
  {{- if lt $intDeletePeriodDays 1 -}}
    {{- fail "resourceManager.deletePeriodDays must be an integer greater than zero" -}}
  {{- end -}}
  {{- $intDeletePeriodDays -}}
{{- end -}}
{{- end -}}

{{- define "okteto.KubeVersionGreaterThan1.29" -}}
{{- if semverCompare "<1.30-0" .Capabilities.KubeVersion.Version -}}
{{- print false -}}
{{- else -}}
{{- print true -}}
{{- end -}}
{{- end -}}

{{/*
  okteto.quantityToBytes returns the number of bytes as an int from a kubernetes quantity.
  More info here: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/quantity
*/}}
{{- define "okteto.quantityToBytes" -}}
{{- $size := . -}}
{{- $regex := `(\d+(?:\.\d+)?)|([a-zA-Z]+)` -}}
{{- $matches := regexFindAll $regex $size -1 -}}
{{- if eq (len $matches) 2 -}}
  {{- $numericPart := index $matches 0 -}}
  {{- $suffix := index $matches 1 -}}
  {{- $numericValue := float64 $numericPart -}}
  {{- $factor := 1 -}}
  {{- if eq $suffix "Ki" -}}
    {{- $factor = 1024 -}}
  {{- else if eq $suffix "k" -}}
    {{- $factor = mulf 1000 -}}
  {{- else if eq $suffix "Mi" -}}
    {{- $factor = mulf 1024 1024 -}}
  {{- else if eq $suffix "m" -}}
    {{- $factor = mulf 1000 1000 -}}
  {{- else if eq $suffix "Gi" -}}
    {{- $factor = mulf 1024 1024 1024 -}}
  {{- else if eq $suffix "g" -}}
    {{- $factor = mulf 1000 1000 1000 -}}
  {{- else if eq $suffix "Ti" -}}
    {{- $factor = mulf 1024 1024 1024 1024 -}}
  {{- else if eq $suffix "t" -}}
    {{- $factor = mulf 1000 1000 1000 1000 -}}
  {{- else if eq $suffix "Pi" -}}
    {{- $factor = mulf 1024 1024 1024 1024 1024 -}}
  {{- else if eq $suffix "p" -}}
    {{- $factor = mulf 1000 1000 1000 1000 1000 -}}
  {{- else if eq $suffix "Ei" -}}
    {{- $factor = mulf 1024 1024 1024 1024 1024 1024 -}}
  {{- else if eq $suffix "e" -}}
    {{- $factor = mulf 1000 1000 1000 1000 1000 1000 -}}
  {{- end -}}
  {{- $bytes := mulf $numericValue $factor -}}
  {{- $bytes -}}
{{- else -}}
  {{- fail (printf "Invalid storage size format for: %s" $size) -}}
{{- end -}}
{{- end -}}


{{/*
  okteto.formatImage formats an image image using repositry, tag and an optional registry.
  It takes an image object as an argument with the following shape: { registry,  repository, tag }
*/}}
{{- define "okteto.formatRegistryAndRepo" -}}
  {{- $fullname := .repository -}}
  {{- $reg := trimSuffix "/" (default "" .registry) -}}
  {{- if ne $reg "" -}}
    {{- $fullname = printf "%s/%s" $reg $fullname  -}}
  {{- end -}}
  {{- $fullname -}}
{{- end -}}
