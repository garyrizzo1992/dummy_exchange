#!/usr/bin/env bash
# gh api --method POST repos/garyrizzo1992/dummy_exchange/actions/runners/registration-token --jq .token | bash register-runner.sh
set -euo pipefail
IFS= read -r registration_token
registration_token="${registration_token//$'\r'/}"
[[ "$registration_token" =~ ^[A-Za-z0-9]+$ ]] || { echo 'A valid runner registration token is required on stdin.' >&2; exit 1; }
source "$(dirname -- "${BASH_SOURCE[0]}")/managed-ssh.sh"
mapfile -t connection < <(managed_ssh_connection dummy-exchange-dev-ci-runner runner-ssh)
[[ ${#connection[@]} == 3 ]] || exit 1

ssh -i "$SSH_KEY" -o StrictHostKeyChecking=accept-new \
  -o "ProxyCommand=${connection[0]//<privateKey>/$SSH_KEY}" \
  -p "${connection[1]}" "${connection[2]}" bash -s -- "$registration_token" <<'REMOTE'
set -euo pipefail
sudo cloud-init status --wait
sudo install -d -o ci-runner -g ci-runner /opt/github-runner
cd /opt/github-runner
if [[ ! -f config.sh ]]; then
  sudo -u ci-runner curl -fsSL --retry 3 -o runner.tar.gz https://github.com/actions/runner/releases/download/v2.337.0/actions-runner-linux-x64-2.337.0.tar.gz
  echo '70920811a4f8ad4328818682bca5c6469c1c942fab52448868071d0063816613  runner.tar.gz' | sha256sum -c -
  sudo -u ci-runner tar xzf runner.tar.gz
fi
if [[ ! -f .runner ]]; then
  sudo -u ci-runner ./config.sh --unattended --url https://github.com/garyrizzo1992/dummy_exchange \
    --token "$1" --name oci-dev --labels oci-dev --work _work
fi
if [[ ! -f .service ]]; then sudo ./svc.sh install ci-runner; fi
sudo ./svc.sh start
REMOTE
