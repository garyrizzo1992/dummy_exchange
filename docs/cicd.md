# Continuous delivery

The `main` branch is the desired state of the development environment.

| Change | Automation | Result |
| --- | --- | --- |
| Rust, Dockerfile, Helm, or migration change | `application.yml` | Tests, publishes immutable images, and commits their digests to `values-dev.yaml`. |
| GitOps values change | Argo CD | Pulls the commit and reconciles the development cluster. |
| Terraform, Ansible, or Argo bootstrap change | `infrastructure.yml` | Pull-request validation and plan; merge-to-main apply. |
| Scheduled weekday check | `drift.yml` | Fails if Terraform detects infrastructure drift. |

Argo CD is the only component that deploys application manifests to Kubernetes.
Terraform creates OCI resources, while Ansible installs and bootstraps the
cluster, Calico, Argo CD, and the Argo CD Application.

## GitHub environment setup

Create a protected GitHub environment named `dev`. Add these environment
secrets:

- `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`
- `OCI_TENANCY_OCID`, `OCI_USER_OCID`, `OCI_FINGERPRINT`, and `OCI_API_PRIVATE_KEY`
- `CI_SSH_PUBLIC_KEY`, an SSH public key placed on newly-created nodes

The current workflows use OCI API-key authentication because the tenancy trust
configuration cannot be created from this repository. Replace those long-lived
credentials with OCI workload identity federation for GitHub Actions once its
OCI IAM trust policy is configured.

## One-time bootstrap

1. Confirm Terraform can read the native OCI backend with `terraform init` and
   `terraform state list`.
2. Create the GitHub `dev` environment and its secrets.
3. Merge the CI/CD configuration to `main`.
4. Merge an infrastructure change or use the manual infrastructure workflow to
   run the initial cluster bootstrap.

The bootstrap generates a development-only Kubernetes secret outside Git so the
first Argo CD synchronization can succeed. For a shared or production
environment, replace it with OCI Vault and External Secrets before deployment.

The promotion job commits the image digests back to `main`. If branch protection
blocks `GITHUB_TOKEN` pushes, authorize the GitHub Actions bot to bypass that
rule or replace its token with a narrowly-scoped GitHub App token.
