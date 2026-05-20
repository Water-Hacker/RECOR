{{/* Helper templates for the RÉCOR generic service chart. */}}

{{- define "recor-service.fullname" -}}
{{- if .Values.fullName -}}
{{- .Values.fullName | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "recor-%s" .Values.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "recor-service.labels" -}}
app.kubernetes.io/name: {{ include "recor-service.fullname" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: recor
app.kubernetes.io/component: {{ .Values.name }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
recor.cm/role: service
{{- end -}}

{{- define "recor-service.selectorLabels" -}}
app.kubernetes.io/name: {{ include "recor-service.fullname" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}
