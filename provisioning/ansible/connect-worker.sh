#!/usr/bin/env bash
set -euo pipefail

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/managed-ssh.sh"

connect_managed_ssh \
  "dummy-exchange-dev-worker" \
  "worker-ssh"
