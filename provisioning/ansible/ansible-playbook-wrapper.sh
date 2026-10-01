#!/usr/bin/env bash
set -euo pipefail

ansible_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
arguments=()
source "$ansible_dir/managed-ssh.sh"

ssh_config="$(mktemp)"
trap 'rm -f "$ssh_config"' EXIT
chmod 600 "$ssh_config"

add_host() {
  local name="$1" session="$2" connection proxy port target user host

  mapfile -t connection < <(managed_ssh_connection "$name" "$session")
  proxy="${connection[0]//<privateKey>/$SSH_KEY}"
  port="${connection[1]}"
  target="${connection[2]}"
  user="${target%@*}"
  host="${target#*@}"

  printf 'Host %s\n  HostName %s\n  User %s\n  Port %s\n  IdentityFile %s\n  StrictHostKeyChecking accept-new\n  ProxyCommand %s\n' \
    "${name#dummy-exchange-dev-}" "$host" "$user" "$port" "$SSH_KEY" "$proxy" >> "$ssh_config"
}

add_host "dummy-exchange-dev-control-plane" "control-plane-ssh"
add_host "dummy-exchange-dev-worker" "worker-ssh"

for argument in "$@"; do
  if [[ "$argument" =~ ^[A-Za-z]:[\\/] ]]; then
    arguments+=("$(wslpath -u "$argument")")
  else
    arguments+=("$argument")
  fi
done

export ANSIBLE_ROLES_PATH="$ansible_dir/roles"
export ANSIBLE_SSH_ARGS="-F $ssh_config"
cd "$ansible_dir"

if ansible-playbook "${arguments[@]}"; then
  exit 0
else
  exit $?
fi
