Terraform creates the OCI development environment. Kubernetes runs on two paid
x86_64 VMs, set up with kubeadm.

A third VM runs trusted GitHub Actions jobs on the private network. It
authenticates through OCI instance identity. External Secrets reads application
secrets from OCI Vault, so you do not pass them to Terraform.

See [continuous delivery](../../docs/cicd.md) for authentication, initial setup
and recovery instructions.

## State

Terraform keeps its state at `dev/terraform.tfstate` in the versioned
`terraform-state` Object Storage bucket. The native OCI backend handles locking
automatically.

```powershell
terraform init
terraform state list
```
