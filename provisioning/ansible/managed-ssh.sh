#!/usr/bin/env bash
set -euo pipefail

if command -v oci.exe >/dev/null 2>&1; then
  OCI_COMMAND="oci.exe"
else
  OCI_COMMAND="oci"
fi

oci() { "$OCI_COMMAND" "$@" | tr -d '\r'; }
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/id_ed25519}"

if [[ -n "${OCI_CLI_CONFIG_FILE:-}" ]]; then
  CONFIG="$OCI_CLI_CONFIG_FILE"
elif [[ -f /mnt/c/Users/Gary/.oci/config ]]; then
  CONFIG="/mnt/c/Users/Gary/.oci/config"
else
  CONFIG="$HOME/.oci/config"
fi

TENANCY="${OCI_TENANCY_OCID:-$(awk -F= '/^tenancy[[:space:]]*=/ {print $2; exit}' "$CONFIG" | xargs)}"
COMPARTMENT="$(oci iam compartment list --compartment-id "$TENANCY" --compartment-id-in-subtree true --all \
  --query 'data[?name==`dummy-exchange` && "lifecycle-state"==`ACTIVE`]|[0].id' --raw-output)"
if [[ "${SSH_CONNECTION_MODE:-bastion}" == "bastion" ]]; then
BASTION="$(oci bastion bastion list --compartment-id "$COMPARTMENT" --all \
  --query 'data[?name==`dummy-exchange-dev-bastion` && "lifecycle-state"==`ACTIVE`]|[0].id' --raw-output)"
fi

session_id() {
  oci bastion session list --bastion-id "$BASTION" --all \
    --query "data[?\"display-name\"==\`$1\` && \"lifecycle-state\"==\`ACTIVE\`]|[0].id" \
    --raw-output
}

managed_ssh_connection() {
  local node_name="$1" session_name="$2"
  local node plugin session ttl command proxy port target

  node="$(oci compute instance list --compartment-id "$COMPARTMENT" --all \
    --query "data[?\"display-name\"==\`$node_name\` && \"lifecycle-state\"==\`RUNNING\`]|[0].id" \
    --raw-output)"
  plugin="$(oci instance-agent plugin list --compartment-id "$COMPARTMENT" \
    --instanceagent-id "$node" --name Bastion --query 'data[0].status' --raw-output)"

  [[ "$plugin" == "RUNNING" ]] || {
    echo "Bastion plugin on $node_name is $plugin." >&2
    return 1
  }

  session="$(session_id "$session_name")"
  if [[ -z "$session" || "$session" == "null" ]]; then
    ttl="$(oci bastion bastion get --bastion-id "$BASTION" \
      --query 'data."max-session-ttl-in-seconds"' --raw-output)"

    echo "Creating SSH session for $node_name..." >&2
    oci bastion session create-managed-ssh \
      --bastion-id "$BASTION" \
      --target-resource-id "$node" \
      --target-os-username opc \
      --target-port 22 \
      --ssh-public-key-file "$(if [[ "$OCI_COMMAND" == "oci.exe" ]]; then wslpath -w "$SSH_KEY.pub"; else printf '%s' "$SSH_KEY.pub"; fi)" \
      --session-ttl "$ttl" \
      --display-name "$session_name" \
      --wait-for-state SUCCEEDED >/dev/null

    session="$(session_id "$session_name")"
  fi

  [[ -n "$session" && "$session" != "null" ]] || {
    echo "SSH session for $node_name did not become active." >&2
    return 1
  }

  command="$(oci bastion session get --session-id "$session" \
    --query 'data."ssh-metadata".command' --raw-output)"
  proxy="${command#*ProxyCommand=\"}"
  proxy="${proxy%%\"*}"
  proxy="${proxy/ssh /ssh -o StrictHostKeyChecking=accept-new }"

  [[ "$command" =~ -p[[:space:]]+([0-9]+)[[:space:]]+([^[:space:]]+)$ ]] || return 1
  port="${BASH_REMATCH[1]}"
  target="${BASH_REMATCH[2]}"

  printf '%s\n%s\n%s\n' "$proxy" "$port" "$target"
}

connect_managed_ssh() {
  local node_name="$1" session_name="$2"
  local connection proxy port target

  mapfile -t connection < <(managed_ssh_connection "$node_name" "$session_name")
  proxy="${connection[0]}"
  port="${connection[1]}"
  target="${connection[2]}"

  echo "Opening SSH shell to $node_name..."
  exec ssh -i "$SSH_KEY" \
    -o "ProxyCommand=${proxy//<privateKey>/$SSH_KEY}" \
    -o StrictHostKeyChecking=accept-new \
    -o ServerAliveInterval=60 \
    -o ServerAliveCountMax=3 \
    -p "$port" \
    "$target"
}
