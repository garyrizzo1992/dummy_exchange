{{- define "dummy-exchange.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "dummy-exchange.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name (include "dummy-exchange.name" .) | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}

{{- define "dummy-exchange.labels" -}}
app.kubernetes.io/name: {{ include "dummy-exchange.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end }}

{{- define "dummy-exchange.selectorLabels" -}}
app.kubernetes.io/name: {{ include "dummy-exchange.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "dummy-exchange.applicationImage" -}}
{{- $image := .image -}}
{{- $repository := printf "%s/%s" .root.Values.imageRegistry $image.name -}}
{{- if $image.digest -}}
{{ $repository }}@{{ $image.digest }}
{{- else -}}
{{ $repository }}:{{ required "an image tag is required when digest is not set" $image.tag }}
{{- end -}}
{{- end }}
