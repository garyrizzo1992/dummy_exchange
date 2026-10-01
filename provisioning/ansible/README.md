# Ansible

This directory configures the OCI Kubernetes nodes after Terraform creates them.

Run commands from this directory in WSL:

```bash
bash ansible-playbook-wrapper.sh -i inventories/dev/terraform.ini playbooks/cluster.yml
```

The wrapper discovers the nodes through OCI and creates Bastion SSH connections
for local runs. The private CI runner sets `SSH_CONNECTION_MODE=private` and uses
instance identity instead. Terraform invokes the same wrapper automatically.

The optional OCI dynamic inventory reads credentials from `~/.oci/config`.
Do not commit API keys, SSH private keys, or vault-password files.

The inventory selects nodes by their `Application`, `Environment`, and
`Component` freeform tags. Terraform must label the two nodes respectively as
`control-plane` and `worker` before the cluster playbook can target both groups.

## Interactive SSH from WSL

Run either script from WSL to create or reuse its OCI Bastion managed-SSH
session and open an interactive shell:

```bash
bash connect-control-plane.sh
bash connect-worker.sh
```

They use your existing Windows OCI CLI/configuration and the WSL key at
`~/.ssh/id_ed25519`. Set `SSH_KEY_PATH` if your key is elsewhere.
