# Continuous delivery

The `main` branch is the desired state of the development environment.

| Change | Automation | Result |
| --- | --- | --- |
| Rust, Dockerfile, Helm, or migration change | `application.yml` | Tests, publishes immutable images, and commits their digests to `values-dev.yaml`. |
| GitOps values change | Argo CD | Pulls the commit and reconciles the development cluster. |
| Terraform, Ansible, or Argo bootstrap change | `infrastructure.yml` | Pull-request validation; trusted main plan and apply on the private OCI runner. |
| Scheduled weekday check | `drift.yml` | Fails if Terraform detects infrastructure drift. |

Argo CD is the only component that deploys application manifests to Kubernetes.
Terraform creates OCI resources, while Ansible installs and bootstraps the
cluster, Calico, development storage, External Secrets, Argo CD, and the Argo CD Application.

Development PVCs use `local-path` storage on the worker VM. Replacing the worker
removes that local data; production needs durable CSI storage and backups.
The Argo CD migration job runs once per synchronization after PostgreSQL is
healthy and before application updates. Migrations must support the previous
application version during rolling updates.

## GitHub environment setup

Create a protected GitHub environment named `dev`. Add these environment
secrets:

- `CI_SSH_PUBLIC_KEY`, the SSH public key placed on the nodes
- `CI_SSH_PRIVATE_KEY`, the matching private key used by Ansible over private SSH

Keep `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` as repository secrets, since the
image build job does not use a GitHub environment. Set `OCI_IMAGE_OCID` as a `dev`
environment variable for the x86_64 image used by all three VMs. Also set
`OCI_TENANCY_OCID` and `BASTION_CLIENT_CIDRS` (a JSON array of restricted operator
CIDRs, such as `["203.0.113.10/32"]`). Public environment
settings live in the committed `provisioning/terraform/envs/dev.tfvars`.

The dedicated `oci-dev` runner is separate from Kubernetes and has no public IP.
Its instance principal provides renewable, short-lived OCI authentication; no
OCI API private key is stored in GitHub and GitHub OIDC federation is not needed.
Only trusted main jobs use it, and the `dev` environment accepts only `main`.
Pull requests deliberately validate without cloud credentials or state access.
Main jobs save a full plan and apply that exact plan, then remove the temporary
plan and SSH key. Never schedule untrusted PR code on this runner.

The runner can manage project resources and development state objects, but can
only read tenancy-level policies and dynamic groups. Bootstrap IAM changes must
be applied locally by an administrator. Main is protected against force pushes
and deletion; reviews are not required because the promotion bot pushes directly.

## One-time bootstrap

1. Confirm Terraform can read the native OCI backend with `terraform init` and
   `terraform state list`.
2. Locally apply the runner, its IAM/networking, Vault and worker secret-reader
   IAM with administrator credentials before running CI.
3. Register the runner using a short-lived GitHub registration token piped to
   `provisioning/ansible/register-runner.sh`. Re-register after VM replacement.
4. Create the three application secrets in OCI Vault outside Terraform. Enable
   External Secrets and set the Vault/secret OCIDs in `values-dev.yaml`. Preserve
   existing database credentials when migrating an existing cluster.
5. Configure GitHub as above, merge to main and run the infrastructure workflow.
   Verify nodes Ready, ExternalSecret Ready, and Argo CD Synced/Healthy.

External Secrets runs on the worker using its separate instance principal to
read project Vault secret bundles. It creates the namespaced Kubernetes Secret;
Git and Terraform contain only identifiers, never application secret values.
Instance-principal permissions belong to the VM, not exclusively to the ESO pod,
so all workloads on that worker must be trusted. Kubernetes Secrets still need
appropriate RBAC and storage protection.

If a new Vault is Active but its endpoint does not resolve, key/secret creation
must wait for OCI DNS. Do not bypass TLS or put secrets into Terraform as a workaround.

Infrastructure and drift jobs share a concurrency group. Do not run a local
apply concurrently. Force-unlock only after the owning operation is confirmed
stopped. State versioning does not replace tested recovery procedures.

This two-node cluster is not highly available. Paid x86 nodes and the additional
runner consume trial credit; this is not an Always Free configuration. Roll back
applications by reverting image digests in Git; migrations are not automatically
reversed.

The promotion job commits the image digests back to `main`. If branch protection
blocks `GITHUB_TOKEN` pushes, authorize the GitHub Actions bot to bypass that
rule or replace its token with a narrowly-scoped GitHub App token.

## References

- [OCI instance principals](https://docs.oracle.com/en-us/iaas/Content/Identity/Tasks/callingservicesfrominstances.htm)
- [External Secrets OCI Vault provider](https://external-secrets.io/latest/provider/oracle-vault/)
- [GitHub self-hosted runner security](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
- [Argo CD sync waves](https://argo-cd.readthedocs.io/en/stable/user-guide/sync-waves/)
