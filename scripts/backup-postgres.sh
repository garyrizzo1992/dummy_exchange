#!/usr/bin/env bash
# Run on the private OCI runner. Backups contain sensitive application data.
set -euo pipefail
umask 077
: "${BACKUP_NAMESPACE:?Set the OCI Object Storage namespace}"
: "${BACKUP_BUCKET:?Set the Terraform-managed backup bucket}"
task_backup_dir=$(mktemp -d)
trap 'rm -rf "$task_backup_dir"' EXIT
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/../provisioning/ansible/managed-ssh.sh"
mapfile -t connection < <(managed_ssh_connection dummy-exchange-dev-control-plane control-plane-ssh)
[[ ${#connection[@]} == 3 ]] || { echo 'Control-plane SSH setup failed.' >&2; exit 1; }
proxy="${connection[0]//<privateKey>/$SSH_KEY}"
backup_name="postgres/$(date -u +%Y/%m/%d)/$(date -u +%H%M%S)-${GITHUB_RUN_ID:-manual}.dump"
ssh -i "$SSH_KEY" -o "ProxyCommand=$proxy" -o StrictHostKeyChecking=accept-new \
  -p "${connection[1]}" "${connection[2]}" \
  'sudo kubectl --kubeconfig /etc/kubernetes/admin.conf exec -n dummy-exchange dummy-exchange-dev-dummy-exchange-postgres-0 -- pg_dump -U postgres -d dummy_exchange --format=custom --no-owner --no-acl' \
  > "$task_backup_dir/postgres.dump"
test -s "$task_backup_dir/postgres.dump"
sha256sum "$task_backup_dir/postgres.dump" | cut -d ' ' -f1 > "$task_backup_dir/postgres.sha256"
oci os object put --namespace-name "$BACKUP_NAMESPACE" --bucket-name "$BACKUP_BUCKET" \
  --name "$backup_name" --file "$task_backup_dir/postgres.dump" --no-overwrite >/dev/null
oci os object put --namespace-name "$BACKUP_NAMESPACE" --bucket-name "$BACKUP_BUCKET" \
  --name "$backup_name.sha256" --file "$task_backup_dir/postgres.sha256" --no-overwrite >/dev/null
printf 'Uploaded %s and checksum to private bucket %s\n' "$backup_name" "$BACKUP_BUCKET"
oci os object get --namespace-name "$BACKUP_NAMESPACE" --bucket-name "$BACKUP_BUCKET" \
  --name "$backup_name" --file "$task_backup_dir/downloaded.dump" >/dev/null
test "$(sha256sum "$task_backup_dir/downloaded.dump" | cut -d ' ' -f1)" = "$(cat "$task_backup_dir/postgres.sha256")"
# Restore into a newly named disposable database; never replace the live one.
restore_database="backup_verification_$(date -u +%s)_${RANDOM}"
remote_exec() {
  local command
  printf -v command '%q ' sudo kubectl --kubeconfig /etc/kubernetes/admin.conf exec \
    -i -n dummy-exchange dummy-exchange-dev-dummy-exchange-postgres-0 -- "$@"
  ssh -i "$SSH_KEY" -o "ProxyCommand=$proxy" -o StrictHostKeyChecking=accept-new \
    -p "${connection[1]}" "${connection[2]}" "$command"
}
cleanup_restore() {
  remote_exec dropdb -U postgres --if-exists "$restore_database" </dev/null || true
  rm -rf "$task_backup_dir"
}
remote_exec createdb -U postgres "$restore_database" </dev/null
trap cleanup_restore EXIT
restore_started=$SECONDS
remote_exec pg_restore -U postgres --dbname="$restore_database" --exit-on-error \
  --no-owner --no-acl < "$task_backup_dir/downloaded.dump"
restored_tables=$(remote_exec psql -U postgres -d "$restore_database" -Atc \
  "SELECT count(*) FROM pg_tables WHERE schemaname='public'" </dev/null)
test "$restored_tables" -ge 10
remote_exec psql -U postgres -d "$restore_database" -Atc \
  "SELECT count(*) AS migration_count FROM _sqlx_migrations WHERE success" </dev/null
printf 'Verified downloaded backup and disposable restore in %s seconds (%s tables).\n' \
  "$((SECONDS - restore_started))" "$restored_tables"
