#!/usr/bin/env bash
# Run Terraform with the ignored local token when no token is already exported.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  token_file="$repo_root/provisioning/.cloudflare"
  [[ -f "$token_file" ]] || {
    echo 'Set CLOUDFLARE_API_TOKEN or provide provisioning/.cloudflare.' >&2
    exit 1
  }
  CLOUDFLARE_API_TOKEN="$(<"$token_file")"
  [[ -n "$CLOUDFLARE_API_TOKEN" && ! "$CLOUDFLARE_API_TOKEN" =~ [[:space:]] ]] || {
    echo 'provisioning/.cloudflare must contain one raw API token.' >&2
    exit 1
  }
  export CLOUDFLARE_API_TOKEN
fi

exec terraform "-chdir=$repo_root/provisioning/terraform" "$@"
