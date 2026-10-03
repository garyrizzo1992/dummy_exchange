{{- define "exchange-monitoring.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "exchange-monitoring.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name (include "exchange-monitoring.name" .) | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}

{{- define "exchange-monitoring.labels" -}}
app.kubernetes.io/name: {{ include "exchange-monitoring.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end }}

{{- define "exchange-monitoring.selectorLabels" -}}
app.kubernetes.io/name: {{ include "exchange-monitoring.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "exchange-monitoring.databaseHost" -}}
{{- default (printf "%s-postgres" (include "exchange-monitoring.fullname" .)) .Values.database.host -}}
{{- end -}}

{{- define "exchange-monitoring.databaseEnv" -}}
- name: PGHOST
  value: {{ include "exchange-monitoring.databaseHost" . | quote }}
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
{{- end -}}
