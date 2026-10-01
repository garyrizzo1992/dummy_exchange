This Terraform project creates the OCI development environment and hosts
Kubernetes natively with kubeadm on two paid x86_64 VMs.

## State

State is stored at `dev/terraform.tfstate` in the versioned `terraform-state`
Object Storage bucket. Terraform's native OCI backend locks it automatically.

```powershell
terraform init
terraform state list
```
