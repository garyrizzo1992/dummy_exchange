# Dummy Exchange

Dummy Exchange is a Rust project for practising trading. Each account starts
with $100,000 in pretend dollars, which you can use to place orders for BTC,
ETH and SOL. No real money changes hands, and it does not connect to an external
exchange.

## How it works

The API handles login, orders and account balances. A separate worker matches
buy and sell orders, while the simulator changes prices and adds orders for
users to trade against.

The services share a PostgreSQL database for balances, orders and completed
trades. Docker Compose also starts Redis, though the Rust code does not use it yet.

Orders match at the best available price, with older orders going first when
prices are equal. Placing an order reserves the money needed to fill it.
That money is used when the order fills or released if you cancel it.

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

## Deployment

GitHub Actions checks application changes and builds Docker images. Changes on
`main` publish those images to Docker Hub and update the development versions
in Git. Argo CD picks up the new versions and deploys them to Kubernetes.

Terraform creates the OCI infrastructure, and Ansible sets up Kubernetes.
The application reads its secrets from OCI Vault. The secret values stay out of Git.

See the [pipeline guide](docs/cicd.md) and
[deployment guide](deploy/README.md) for setup instructions, or the
[monitoring guide](monitoring/README.md) for dashboards and alerts.

## Current limitations

The project is for learning and is not ready for real-money trading. It currently
hashes passwords with SHA-256, which is unsuitable for storing real user passwords.

Do not reuse the demo credentials, and keep private keys, tokens, passwords and
Terraform state out of Git. The development cluster stores data on local disks.
It needs a storage and backup plan before you put important data on it.
