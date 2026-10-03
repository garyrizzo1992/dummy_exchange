Terraform creates the OCI development environment. Kubernetes runs on two paid
x86_64 VMs, set up with kubeadm.

Run Terraform from WSL or Linux. Install Terraform, the Linux OCI CLI, and
Ansible in that environment. Terraform finds the adjacent Ansible directory
relative to this module; no host-specific path or operating-system input is needed.
Use Linux paths for credentials in your ignored `terraform.tfvars` and configure
the OCI CLI through `~/.oci/config` or `OCI_CLI_CONFIG_FILE`.

Before the first bootstrap, make the Ansible wrapper executable:

```bash
chmod +x provisioning/ansible/ansible-playbook-wrapper.sh
bash scripts/terraform-cloudflare.sh init
```

Run these commands from the repository root. The Cloudflare wrapper exports the
token only in its own process and the Terraform process it starts.

A third VM runs trusted GitHub Actions jobs on the private network. It
authenticates through OCI instance identity. External Secrets reads application
secrets from OCI Vault, so you do not pass them to Terraform.

See [deployment](../../README.md#deployment) for authentication, initial setup
and recovery instructions.

Public API routing uses a [free Cloudflare Tunnel with Terraform-managed DNS](docs/cloudflare.md)
at `api.garyrizzo.dev`. The existing OCI nodes and application Services stay private; no OCI
load balancer is created. The ignored `provisioning/.cloudflare` API token must
allow tunnel and DNS management. Configure `CLOUDFLARE_API_TOKEN` in the GitHub
`dev` environment for CI and drift checks.

## Directory conventions

This directory is one Terraform root module. Files group declarations by purpose;
they do not define execution order. Terraform resolves resource dependencies.

| File | Responsibility |
| --- | --- |
| `versions.tf` | Terraform version and provider requirements |
| `providers.tf` | Provider authentication and configuration |
| `backend.tf` | Remote state backend |
| `variables.tf` | All input declarations, grouped by purpose |
| `locals.tf` | Shared derived values and tags |
| `outputs.tf` | All exported values |
| `main.tf` | Project compartment and availability-domain lookup |
| `network.tf` | VCN, subnets, gateways, and routes |
| `security.tf` | Network security groups and traffic rules |
| `compute.tf` | Kubernetes nodes and optional CI runner |
| `bastion.tf` | Operator access and caller-IP lookup |
| `iam.tf` | Dynamic groups and instance-principal policies |
| `vault.tf` | Application vault and encryption key |
| `cloudflare.tf` | Free Cloudflare Tunnel, DNS, and HTTPS redirect |
| `bootstrap.tf` | Ansible action and configuration change triggers |
| `migrations.tf` | Historical state address migrations |

Add inputs to `variables.tf` and outputs to `outputs.tf`, rather than alongside
resources. Keep environment values in `envs/*.tfvars`; keep local credentials in
the ignored `terraform.tfvars` or environment variables. The Cloudflare token is
supplied through `CLOUDFLARE_API_TOKEN`. Provider credentials and secret values
must not be added to committed environment files.

`examples/always-free.tfvars.example` is an optional compute override.
Cloud-init templates live in `templates/`,
optional input examples in `examples/`, and supplementary guides in `docs/`.
Resources reference helper files through `path.module`. Keep existing resource
addresses and migration blocks when reorganizing files so existing deployments
retain their state bindings.

Run the module checks before submitting changes:

```bash
terraform fmt -check -recursive
terraform validate
terraform test
```

The Terraform tests use mocked providers and do not create cloud resources.

## State

Terraform keeps its state at `dev/terraform.tfstate` in the versioned
`terraform-state` Object Storage bucket. The native OCI backend handles locking
automatically.

```bash
terraform init
terraform state list
```

## Optional A1 compute sizing

`examples/always-free.tfvars.example` sizes a **new** ARM64 two-node cluster at 4 OCPUs
and 24 GB total, with the dedicated CI VM disabled. Supply a matching ARM64
Oracle Linux image. The application workflow builds amd64 and arm64 images;
Ansible selects the corresponding Helm binary. Wait for new multi-platform
image digests before deploying on ARM64 and check dependency image support.

Existing E5 defaults remain intact. Do not apply the A1 example to an existing
cluster without a migration and backup plan: replacing nodes loses local-path
data. Disabling the CI runner disables the current private-runner workflow
unless you supply another trusted runner. The example budgets compute only;
check OCI home-region eligibility, capacity, and all other services separately.
