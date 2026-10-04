#!/usr/bin/env bash
# Run kubectl on the dev controller through an OCI Bastion session.
set -euo pipefail

if [[ $# == 0 || ${1:-} == --help ]]; then
  echo "Usage: bash provisioning/ansible/dev-kubectl.sh <kubectl arguments>"
  exit 0
fi

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/managed-ssh.sh"
mapfile -t connection < <(managed_ssh_connection dummy-exchange-dev-control-plane control-plane-ssh)
[[ ${#connection[@]} == 3 ]] || { echo 'Control-plane SSH session setup failed.' >&2; exit 1; }
proxy="${connection[0]//<privateKey>/$SSH_KEY}"
printf -v remote_command '%q ' sudo kubectl --kubeconfig /etc/kubernetes/admin.conf "$@"
exec ssh -i "$SSH_KEY" -o "ProxyCommand=$proxy" \
  -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 \
  -p "${connection[1]}" "${connection[2]}" "$remote_command"
