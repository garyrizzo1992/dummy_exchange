#!/usr/bin/env bash
# Forward a private dev Service to localhost through OCI Bastion and kubectl.
set -euo pipefail

usage() {
  echo 'Usage: bash provisioning/ansible/dev-forward.sh <service> [local-port]'
  echo 'Services: api, grafana, argocd, prometheus, postgres, redis, worker, simulator, postgres-exporter, redis-exporter'
}
if [[ $# == 0 || ${1:-} == --help ]]; then usage; exit 0; fi
if [[ $# -gt 2 ]]; then usage >&2; exit 1; fi

namespace=dummy-exchange
service="dummy-exchange-dev-dummy-exchange-$1"
case "$1" in
  api|grafana) service_port=3000; default_port=3000 ;;
  argocd) namespace=argocd; service=argocd-server; service_port=443; default_port=8443 ;;
  prometheus) service_port=9090; default_port=9090 ;;
  postgres) service_port=5432; default_port=5432 ;;
  redis) service_port=6379; default_port=6379 ;;
  worker) service_port=3001; default_port=3001 ;;
  simulator) service_port=3002; default_port=3002 ;;
  postgres-exporter) service_port=9187; default_port=9187 ;;
  redis-exporter) service_port=9121; default_port=9121 ;;
  *) usage >&2; exit 1 ;;
esac
local_port="${2:-$default_port}"
[[ "$local_port" =~ ^[0-9]{1,5}$ ]] && (( 10#$local_port > 0 && 10#$local_port < 65536 )) || {
  echo 'Local port must be an integer from 1 to 65535.' >&2; exit 1;
}

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/managed-ssh.sh"
mapfile -t connection < <(managed_ssh_connection dummy-exchange-dev-control-plane control-plane-ssh)
[[ ${#connection[@]} == 3 ]] || { echo 'Control-plane SSH session setup failed.' >&2; exit 1; }
proxy="${connection[0]//<privateKey>/$SSH_KEY}"
# Use a distinct high controller port per Service. A conflict fails without killing other forwards.
remote_port=$((20000 + service_port))
[[ "$1" != grafana ]] || remote_port=23003
printf -v remote_command '%q ' sudo kubectl --kubeconfig /etc/kubernetes/admin.conf \
  -n "$namespace" port-forward --address=127.0.0.1 "service/$service" "$remote_port:$service_port"
echo "Forwarding $namespace/$service to 127.0.0.1:$local_port. Keep this terminal open; Ctrl-C stops it."
exec ssh -t -i "$SSH_KEY" -o "ProxyCommand=$proxy" \
  -o StrictHostKeyChecking=accept-new -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=30 -L "127.0.0.1:$local_port:127.0.0.1:$remote_port" \
  -p "${connection[1]}" "${connection[2]}" "$remote_command"
