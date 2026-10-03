{{- define "exchange-api.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "exchange-api.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name (include "exchange-api.name" .) | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}

{{- define "exchange-api.labels" -}}
app.kubernetes.io/name: {{ include "exchange-api.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end }}

{{- define "exchange-api.selectorLabels" -}}
app.kubernetes.io/name: {{ include "exchange-api.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "exchange-api.applicationImage" -}}
{{- $image := .image -}}
{{- $repository := printf "%s/%s" .root.Values.imageRegistry $image.name -}}
{{- if $image.digest -}}
{{ $repository }}@{{ $image.digest }}
{{- else -}}
{{ $repository }}:{{ required "an image tag is required when digest is not set" $image.tag }}
{{- end -}}
{{- end }}

{{- define "exchange-api.databaseHost" -}}
{{- default (printf "%s-postgres" (include "exchange-api.fullname" .)) .Values.database.host -}}
{{- end -}}

{{- define "exchange-api.databaseEnv" -}}
- name: PGHOST
  value: {{ include "exchange-api.databaseHost" . | quote }}
- name: PGPORT
  value: {{ .Values.database.port | quote }}
- name: PGDATABASE
  value: {{ .Values.database.name | quote }}
- name: PGUSER
  value: {{ .Values.database.user | quote }}
- name: PGSSLMODE
  value: {{ .Values.database.sslMode | quote }}
- name: PGPASSWORD
  valueFrom:
    secretKeyRef:
      name: {{ required "database.secret.name is required" .Values.database.secret.name }}
      key: {{ .Values.database.secret.key }}
{{- with .Values.extraEnv }}
{{ toYaml . }}
{{- end }}
{{- end -}}
