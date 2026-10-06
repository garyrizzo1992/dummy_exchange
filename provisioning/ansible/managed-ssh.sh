#!/usr/bin/env bash
set -euo pipefail

SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/id_ed25519}"
CONFIG="${OCI_CLI_CONFIG_FILE:-$HOME/.oci/config}"

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
  [[ -n "$node" && "$node" != "null" ]] || {
    echo "Running node $node_name was not found." >&2
    return 1
  }
  if [[ "${SSH_CONNECTION_MODE:-bastion}" == "private" ]]; then
    local private_ip
    private_ip="$(oci compute instance list-vnics --instance-id "$node" --all \
      --query 'data[?"is-primary"==`true`]|[0]."private-ip"' --raw-output)"
    [[ -n "$private_ip" && "$private_ip" != "null" ]] || return 1
    printf 'none\n22\nopc@%s\n' "$private_ip"
    return
  fi
  # RUNNING instances can precede the first Oracle Cloud Agent heartbeat.
  local deadline=$((SECONDS + 600))
  while true; do
    if plugin="$(oci instance-agent plugin list --compartment-id "$COMPARTMENT" \
      --instanceagent-id "$node" --name Bastion --query 'data[0].status' --raw-output 2>&1)"; then
      [[ "$plugin" == "RUNNING" ]] && break
    elif [[ "$plugin" != *"Plugin Bastion not present"* ]]; then
      echo "$plugin" >&2
      return 1
    fi
    if (( SECONDS >= deadline )); then
      echo "Timed out waiting for Bastion plugin on $node_name: $plugin" >&2
      return 1
    fi
    echo "Waiting for Bastion plugin on $node_name..." >&2
    sleep 10
  done

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
      --ssh-public-key-file "$SSH_KEY.pub" \
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
