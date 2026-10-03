# Ansible

Ansible sets up Kubernetes on the OCI nodes created by Terraform.

Run commands from this directory in WSL:

```bash
bash ansible-playbook-wrapper.sh -i inventories/dev/terraform.ini playbooks/cluster.yml
```

The wrapper finds the nodes through OCI and connects through Bastion when you
run it locally. On the private CI runner, `SSH_CONNECTION_MODE=private` uses
private connections and instance identity. Terraform runs this wrapper
automatically.

Bootstrap applies PostgreSQL and monitoring as independent Argo applications,
waits for the database application to be `Synced` and `Healthy`, then applies
the business application containing API, worker and exchange Helm sources.
Existing legacy applications must follow the [ownership transfer](../../deploy/helm/README.md#transfer-an-existing-release)
before reprovisioning; the old deployment layout selector has been removed.

The optional OCI dynamic inventory reads credentials from `~/.oci/config`.
Do not commit API keys, SSH private keys, or vault-password files.

The inventory finds nodes using their `Application`, `Environment`, and
`Component` freeform tags. Terraform must tag one node as `control-plane` and the
other as `worker` so the cluster playbook can find both groups.

## Interactive SSH from WSL

Each script creates or reuses an OCI Bastion managed-SSH session, then opens a
shell on the chosen node. Run them from WSL:

```bash
bash connect-control-plane.sh
bash connect-worker.sh
```

The scripts use the Linux OCI CLI and `~/.oci/config`, with the SSH key at
`~/.ssh/id_ed25519`. Set `OCI_CLI_CONFIG_FILE` or `SSH_KEY_PATH` to override these
locations. With instance-principal authentication, set `OCI_TENANCY_OCID` so
the scripts do not need a local OCI configuration file.
