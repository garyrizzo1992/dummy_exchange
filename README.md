# Dummy Exchange

Dummy Exchange is a Rust project for practising trading. Registered accounts start
with $100,000 in pretend dollars, which you can use to place orders for BTC,
ETH and SOL. Prices follow public Coinbase market data; orders, balances and
settlement remain simulated. No real money changes hands.

## Development URLs

| Service | URL | Access |
|---|---|---|
| Exchange frontend | [api.garyrizzo.dev/v1/ui/](https://api.garyrizzo.dev/v1/ui/) | Public market view; create an account or sign in to trade. |
| Exchange REST API | [api.garyrizzo.dev/v1](https://api.garyrizzo.dev/v1) | Public market endpoints; account and order endpoints require a Bearer JWT from `/v1/auth/register` or `/v1/auth/login`. |
| Grafana | [grafana.garyrizzo.dev](https://grafana.garyrizzo.dev) | Grafana login; dashboards, metrics, pod logs and traces. |
| Argo CD | [argocd.garyrizzo.dev](https://argocd.garyrizzo.dev) | Argo CD login; deployment and sync status. |
| Prometheus | [prometheus.garyrizzo.dev](https://prometheus.garyrizzo.dev) | Cloudflare Access email-code login for allowed operators. |

These hostnames use Cloudflare Tunnel to reach private Kubernetes services.
PostgreSQL, Redis, the Kubernetes API and application health/metrics endpoints
remain private. See [development access](provisioning/terraform/docs/dev-access.md)
for authentication and private forwarding instructions.

## Deployment

The development environment follows `main`. The deployment setup covers OCI
infrastructure, a kubeadm Kubernetes cluster and application releases through
GitHub Actions and Argo CD.

### The environment

Terraform creates two paid x86_64 VMs for the Kubernetes control plane and worker.
A third VM runs trusted GitHub Actions jobs. All three have private IPs and no
public IPs. Operators connect through OCI Bastion; the CI runner reaches the
nodes directly over private SSH.

Ansible prepares the nodes and installs Kubernetes with kubeadm. It also sets up
Calico networking, local-path storage, External Secrets and Argo CD. Terraform
runs the Ansible playbook when the nodes or tracked setup files change.

Five Helm charts separate the API, matching worker, exchange market generator,
PostgreSQL and monitoring. One Argo CD application coordinates the three business
charts; PostgreSQL and monitoring have independent applications. Optional legacy
Redis stays with database infrastructure. See the [chart layout](deploy/helm/README.md).

### From a code change to a running release

1. A pull request runs Rust formatting, Clippy, tests and a release build. It
   also checks the Helm chart and builds all three application images without
   publishing them.
2. When an application change reaches `main`, GitHub Actions publishes the API,
   worker and simulator images to Docker Hub. Each image has a commit-based tag,
   build information and a software dependency list.
3. The promotion job commits the exact image digests to each business chart's
   `values-dev.yaml` in one commit.
   Digests pin the release to specific images, even if their tags change later.
4. Argo CD picks up that commit and syncs the three business charts together.
   Once database infrastructure is healthy, it waits for the API secret and runs
   the shared database migration before updating all three business processes.

Argo CD also corrects manual changes to managed Kubernetes resources and prunes
business resources deleted from Git. Database and monitoring cleanup is explicit.
To roll back an application release, revert its image
digests in Git. Database migrations are not automatically reversed.

A successful application release run means the images were published and recorded in Git.
Check Argo CD for `Synced` and `Healthy` to confirm the rollout has finished.

### Infrastructure changes and drift

| Workflow | What it does |
|---|---|
| [application.yml](.github/workflows/application.yml) | Tests application changes, publishes images and records releases in Git. |
| [infrastructure.yml](.github/workflows/infrastructure.yml) | Validates pull requests. On main, saves a Terraform plan and applies that exact plan. |
| [drift.yml](.github/workflows/drift.yml) | Checks on weekdays whether OCI matches Terraform. Reports changes or errors without applying anything. |

Infrastructure applies and drift checks use the private OCI runner and share a
concurrency group so they cannot overlap. Terraform state lives in a versioned
OCI Object Storage bucket, with locking handled by the native OCI backend.

Pull requests validate infrastructure on GitHub-hosted runners without OCI
credentials or access to cloud state. Only trusted main jobs run on the private
runner. Once the initial setup is complete, qualifying infrastructure changes
on main apply automatically, with no approval step between plan and apply.

Application builds run for changes to Cargo files, `crates/`, `migrations/`,
the Helm chart or the application workflow. A commit that only updates
`values-dev.yaml` does not trigger another build. Helm-only changes do rebuild
the images. New application runs cancel older runs for the same branch.

Infrastructure checks run for changes to `provisioning/`, the development
Argo CD Application or the infrastructure workflow. They use Terraform 1.16.4
and apply with `-parallelism=1`. Drift checks run at 04:17 UTC on weekdays:
plan exit code `0` passes, `1` means an error, and `2` means changes were found.
They never apply changes; notifications follow your GitHub settings.

### Secrets and access

The CI runner authenticates to OCI through its instance identity, so GitHub does
not need an OCI API private key. Ansible's SSH key comes from the GitHub `dev`
environment. A final cleanup step removes it and the saved plan, even after a
job failure. That cleanup cannot run if the runner goes offline.

Application passwords and the JWT signing secret live in OCI Vault. External
Secrets uses the worker VM's identity to read them and create the Kubernetes
Secret. Git and Terraform contain identifiers, not application secret values.
That identity belongs to the VM, so workloads on the worker must be trusted.
Protect Kubernetes Secrets with appropriate RBAC and storage security too.

### First-time setup

1. In WSL or Linux, from `provisioning/terraform`, check remote state access with `terraform init`
   and `terraform state list`. State is stored at `dev/terraform.tfstate` in the
   versioned `terraform-state` bucket.
2. Use administrator credentials to apply the runner, networking, IAM and Vault
   resources locally. CI cannot change tenancy-level policies or dynamic groups.
3. Register the private `oci-dev` runner with a short-lived GitHub registration
   token passed to `provisioning/ansible/register-runner.sh`. Register it again
   if the VM is replaced. Never run untrusted pull-request code on this runner.
4. Create the PostgreSQL password, JWT secret and Grafana password in OCI Vault
   outside Terraform. Enable External Secrets and set the Vault OCID and secret
   names in the API, PostgreSQL and monitoring charts' `values-dev.yaml` files.
   Keep existing database
   credentials when moving an existing cluster. If the Vault endpoint does not
   resolve yet, wait for DNS; do not bypass TLS or put passwords in Terraform.
5. Configure GitHub using the settings below, then run the infrastructure
   workflow on `main`. Check that nodes and ExternalSecret are `Ready`, and
   that Argo CD is `Synced` and `Healthy`.

Create a GitHub environment called `dev` and restrict it to `main`.

| GitHub setting | Where | Purpose |
|---|---|---|
| `CI_SSH_PUBLIC_KEY` | `dev` secret | SSH public key installed on the nodes. |
| `CI_SSH_PRIVATE_KEY` | `dev` secret | Matching private key for Ansible. |
| `OCI_IMAGE_OCID` | `dev` variable | x86_64 image used by all three VMs. |
| `OCI_TENANCY_OCID` | `dev` variable | OCI tenancy identifier. |
| `BASTION_CLIENT_CIDRS` | `dev` variable | Restricted operator addresses as a JSON array, such as `["203.0.113.10/32"]`. |
| `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` | Repository secrets | Credentials for publishing images. |
| `CLOUDFLARE_API_TOKEN` | `dev` secret | Free Cloudflare Tunnel, DNS and HTTPS setting management. |

Public Terraform settings live in `provisioning/terraform/envs/dev.tfvars`.
The promotion job needs permission to push image digests to `main`. If branch
protection blocks it, allow the Actions bot or use a narrowly scoped GitHub App
token. Keep main protected against force pushes and deletion.

### Running and troubleshooting workflows

In GitHub's Actions tab, choose a workflow and select Run workflow on `main`.
Manual runs do not need a matching file change. On other branches, application
runs only test, infrastructure runs only validate, and drift checks are skipped.

Read the failed step's output first. If an infrastructure job is waiting, check
that `oci-dev` is online and another infrastructure or drift job is not running.
Do not run a local apply at the same time. Only clear a Terraform state lock
after confirming that its owning operation has stopped. State versioning still
needs a tested recovery procedure.

### Current deployment limits

This is a two-node development cluster, with no high availability. The paid VMs
and CI runner consume trial credit; this is not an Always Free setup. Services
stay private, and development volumes use the worker's local disk. Replacing
that VM loses the local data. Important data needs durable storage and backups.

See the [deployment guide](deploy/README.md) for the OCI and local Minikube
setups, or the [monitoring guide](monitoring/README.md) for dashboards and alerts.

## How it works

The API handles login, orders and account balances. A separate worker matches
buy and sell orders, while the simulator changes prices and adds orders for
users to trade against.

The services share a PostgreSQL database for balances, orders and completed
trades. Docker Compose also starts Redis, though the Rust code does not use it yet.

Orders match at the best available price, with older orders going first when
prices are equal. Placing an order reserves the money needed to fill it.
That money is used when the order fills or released if you cancel it.

### Database and matching decisions

PostgreSQL stores the exchange data. Accepted orders and market updates also
write outbox events in their transactions, avoiding the need for Kafka in this
project. The current worker polls open orders directly; it does not consume or
mark outbox events as processed.

Workers use a PostgreSQL advisory transaction lock for each instrument, so only
one worker matches that instrument at a time. Other workers can handle other
instruments or take over after a restart. A unique constraint on fills prevents
duplicate inserts from updating balances again.

### Hosting the services

All services need the same PostgreSQL database through `DATABASE_URL`. The API
also needs `JWT_SECRET`, shared by its replicas. Use a secret manager for these
values. Keep database traffic private and use TLS across untrusted networks.
Use a least-privilege database role for the apps and a separate migration role.

| Service | Configuration and access | Scaling |
|---|---|---|
| API | `API_BIND` defaults to `127.0.0.1:3000`; use `0.0.0.0:3000` in containers. Terminate public HTTPS at an ingress or proxy if exposing it. | Multiple replicas can share the JWT secret without sticky sessions. |
| Worker | `WORKER_ID` defaults to a generated UUID. `WORKER_METRICS_BIND` defaults to `0.0.0.0:3001` for private `/healthz` and `/metrics`. | Multiple replicas are supported; matching stays serial per instrument. |
| Simulator | `SIMULATION_SEED` defaults to `42`. `SIMULATOR_METRICS_BIND` defaults to `0.0.0.0:3002` for private `/healthz` and `/metrics`. | Start with one replica to avoid competing price updates. |

The services do not need their own persistent volumes. PostgreSQL does: use
durable storage and test backup restoration before storing important data.
Keep worker and simulator ports out of public ingress. Their health endpoints
check the HTTP listener, not database connectivity or progress in the background loop.

Run migrations once per release, before updating the apps. Schema changes must
work with the previous app version during a rolling update. Allow time for
shutdown and use non-root containers with read-only filesystems where practical.
Load test before choosing CPU and memory limits, and account for every replica's
database connections. JSON logs go to stdout; the API supports `x-request-id`.
Redis is optional and unused by the current Rust services. JWT key rotation
would need support for validating tokens signed with the previous key.

## Start with Docker

With Docker and Compose installed, run these commands from the project folder:

```powershell
# Start the database and wait for it to be ready.
docker compose up -d --wait postgres

# Build the API and create the database tables.
docker compose run --build --rm api migrate

# Start the rest of the project.
docker compose up --build
```

The database migrations create BTC, ETH and SOL markets with starting prices.

The API runs at http://localhost:3000. You can view metrics in Prometheus at
http://localhost:9090 or open Grafana at http://localhost:3001.
Log in to the local Grafana instance with `admin` / `admin`.

To stop the services without deleting the database:

```powershell
docker compose down
```

The passwords in Compose are only for local testing.

## Run the Rust code directly

Install Rust and PostgreSQL 16 or newer, then create a database.

Copy the settings file:

```powershell
Copy-Item .env.example .env
```

Set `DATABASE_URL` in `.env` to your PostgreSQL connection string.
Set `JWT_SECRET` to your own signing secret. Keep `.env` out of Git.

Create the tables:

```powershell
cargo run -p exchange-api -- migrate
```

Start each service in its own terminal:

```powershell
cargo run -p exchange-api
cargo run -p exchange-worker
cargo run -p exchange-simulator
```

Use the project folder as your working directory so the services can load `.env`.

## Try it

In PowerShell, create a demo account and use the returned login token to check
its balance. Change the email if you have already registered this account.

```powershell
$account = @{
    email = "demo@example.test"
    password = "local-demo-only"
} | ConvertTo-Json

$login = Invoke-RestMethod -Method Post -Uri "http://localhost:3000/v1/auth/register" -ContentType "application/json" -Body $account
$headers = @{ Authorization = "Bearer $($login.access_token)" }

Invoke-RestMethod -Uri "http://localhost:3000/v1/accounts/balances" -Headers $headers
Invoke-RestMethod -Uri "http://localhost:3000/v1/instruments"
```

The [API guide](crates/api/README.md) lists the routes for placing orders,
cancelling orders and reading trades.

## Project folders

- `crates/domain`: order types, validation and matching rules. Start here.
- `crates/api`: HTTP routes, authentication and database calls.
- `crates/worker`: matching orders and updating balances.
- `crates/simulator`: generating prices and market orders.
- `migrations`: database tables and changes.
- `provisioning`: Terraform and Ansible for the OCI Kubernetes cluster.
- `deploy`: the Helm chart and Argo CD configuration.
- `monitoring`: Prometheus, Grafana and their settings.
- `.github/workflows`: build, deployment and infrastructure checks.

## Check your changes

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The tests cover order validation and matching. Tests against a running database
are still missing.

Use `/healthz` to check whether the API is running and `/readyz` to check its
database connection. Prometheus reads metrics from `/metrics`.

### Failure checks

These are manual exercises, not automated test coverage:

- Restart a worker while orders are open. Uncommitted changes should roll back,
  and matching should resume from the orders stored in PostgreSQL.
- Run two workers and check that the instrument lock prevents concurrent matching
  of the same market, with no duplicate balance updates.
- Stop PostgreSQL after the services start. API readiness should return `503`,
  and the worker should log errors and retry. Check recovery after restarting it.
- Stop Redis. Current trading should be unaffected because the Rust code does
  not use it.
- Interrupt an order request before its transaction commits. Check that the
  reservation, order and outbox event roll back together. A lost HTTP response
  alone does not prove that the transaction failed.

## Current limitations

The project is for learning and is not ready for real-money trading. It currently
hashes passwords with SHA-256, which is unsuitable for storing real user passwords.

Do not reuse the demo credentials, and keep private keys, tokens, passwords and
Terraform state out of Git. The development cluster stores data on local disks.
It needs a storage and backup plan before you put important data on it.
