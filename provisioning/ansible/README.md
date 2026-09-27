# Ansible

This directory configures the OCI Kubernetes nodes after Terraform creates them.

Run commands from this directory in WSL:

```bash
ansible-galaxy collection install -r collections/requirements.yml -p collections
ansible-inventory --graph
ansible-playbook playbooks/cluster.yml
```

The OCI dynamic inventory reads controller credentials from `~/.oci/config`.
Do not commit API keys, SSH private keys, or vault-password files.

The inventory selects nodes by their `Application`, `Environment`, and
`Component` freeform tags. Terraform must label the two nodes respectively as
`control-plane` and `worker` before the cluster playbook can target both groups.
