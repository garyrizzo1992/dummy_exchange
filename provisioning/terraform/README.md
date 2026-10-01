This Terraform project creates the OCI development environment and hosts
Kubernetes natively with kubeadm on two paid x86_64 VMs.

A third private VM runs trusted GitHub Actions jobs using OCI instance identity.
Application secrets live in OCI Vault and are synchronized by External Secrets;
they are not Terraform inputs. See [continuous delivery](../../docs/cicd.md)
for authentication, bootstrap and recovery details.

## State

State is stored at `dev/terraform.tfstate` in the versioned `terraform-state`
Object Storage bucket. Terraform's native OCI backend locks it automatically.

```powershell
terraform init
terraform state list
```
