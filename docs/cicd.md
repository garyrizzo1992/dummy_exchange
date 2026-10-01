# Continuous delivery

The development environment follows `main`. There are three GitHub Actions
pipelines: one for the apps, one for infrastructure, and one that checks for drift.

| Change | Automation | Result |
| --- | --- | --- |
| App or Helm change merged to main | `application.yml` | Tests, publishes images, and records their exact versions in `values-dev.yaml`. |
| GitOps values change | Argo CD | Pulls the commit and reconciles the development cluster. |
| Terraform, Ansible, or Argo setup change | `infrastructure.yml` | Checks pull requests. Plans and applies changes from main on the private OCI runner. |
| Weekday check | `drift.yml` | Reports changes that do not match Terraform. |

Argo CD is the only component that deploys application manifests to Kubernetes.
Terraform creates OCI resources, while Ansible installs and bootstraps the
cluster, Calico, development storage, External Secrets, Argo CD, and the Argo CD Application.

## App pipeline

File: [application.yml](../.github/workflows/application.yml).

It runs when a pull request targets main, or when a change is pushed to main,
and the change touches any of these files:

- `Cargo.toml` or `Cargo.lock`
- Anything in `crates/`, `migrations/` or `deploy/helm/`
- The app workflow itself

A push that only changes `values-dev.yaml` does not start this pipeline. This
keeps the deployment bot's commits from starting another build.

The jobs run in this order:

1. `test` checks Rust formatting, runs Clippy and tests, builds the release
   binaries, and checks that the development Helm chart renders correctly.
2. On a pull request, `verify-images` builds the API, worker and simulator images
   in parallel. It does not upload images or deploy anything.
3. On main, `publish-images` builds and uploads all three images to Docker Hub.
   Each gets a `sha-<commit>` tag, along with build information and a list of its
   software dependencies.
4. `promote-dev` records each image's digest in `values-dev.yaml`, then commits
   and pushes that file to main. A digest identifies the exact image, so a tag
   being changed later will not change what we deploy.

All these jobs use GitHub-hosted Ubuntu runners. Only `promote-dev` can write to
the repository; it uses the `dev` environment. The test job has a 20-minute
timeout, and the publishing job has a 30-minute timeout.

Argo CD picks up the new values from Git and deploys them. The GitHub pipeline
does not wait for the app to become healthy. A green main run means the images were
published and recorded in Git, not that the Kubernetes rollout has finished.
Check Argo CD for `Synced` and `Healthy` after a release.

Helm-only changes also rebuild the app images in the current pipeline. If a
new run starts for the same branch or pull request, GitHub cancels the older run.

## Infrastructure pipeline

File: [infrastructure.yml](../.github/workflows/infrastructure.yml).

It runs for pull requests to main and pushes to main when a change touches
`provisioning/`, the development Argo CD Application, or this workflow file.

There are two jobs:

1. `validate` checks Terraform formatting, downloads the providers without
   connecting to remote state, and validates the configuration. It runs on a
   GitHub-hosted Ubuntu runner. Pull requests stop here: they do not get OCI
   credentials, read cloud state, or change the environment.
2. `apply` runs only on main, after validation passes. It runs on our private
   `oci-dev` runner and uses the `dev` environment. It prepares the SSH key,
   connects to remote state, saves a Terraform plan, and applies that exact plan.
   Terraform runs Ansible when the nodes or tracked Ansible setup files change.
   Ansible connects to the nodes over private SSH, without Bastion sessions.

Both jobs use Terraform 1.16.4. Planning and applying use `-parallelism=1`, so
Terraform handles one resource operation at a time. There is no approval step
between plan and apply: a qualifying main run applies automatically.

The last step removes the temporary SSH key and plan, even if an earlier step
fails. This cleanup cannot run if the runner itself is offline.

Infrastructure runs and drift checks share the `terraform-dev` concurrency
group. Only one runs at a time, and a new run does not cancel the active one.
Do not start a local apply while either is running.

## Drift pipeline

File: [drift.yml](../.github/workflows/drift.yml).

It runs at 04:17 UTC, Monday to Friday, and can also be started manually on main.
It uses the private `oci-dev` runner, the `dev` environment, and Terraform 1.16.4.

It connects to remote state and runs a plan with `-detailed-exitcode`:

- `0`: no changes needed; the job passes.
- `1`: Terraform hit an error; the job fails.
- `2`: Terraform found changes; the job fails so we can review them.

This pipeline never applies changes. A failed run can mean drift or an error,
so read the plan output before deciding what to do. The workflow does not create
an issue or send a custom alert; notifications follow your GitHub settings.

## Running a pipeline yourself

Open the repository's **Actions** tab, choose a workflow, and select
**Run workflow**. Choose `main` for deployment or drift checks. Manual runs do
not need a matching file change.

The app pipeline on another branch runs tests but does not publish images.
The infrastructure pipeline on another branch only validates. The drift job
is skipped on another branch.

To check a run, open its jobs and read the failed step's output. If an
infrastructure run is waiting, check that `oci-dev` is online and that another
infrastructure or drift run is not already active. Do not clear a Terraform
state lock until you have confirmed the operation that owns it has stopped.

## Deployment notes

Development volumes use `local-path` storage on the worker VM. Replacing the worker
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
   External Secrets and set the Vault OCID and secret names in `values-dev.yaml`. Preserve
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
