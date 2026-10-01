# Dummy Exchange

A practice trading exchange built in Rust. You can create an account, place
orders, and trade BTC, ETH and SOL with pretend dollars.

Every new account gets $100,000 in simulated money. There is no real trading,
no real money, and no connection to an external exchange.

## What runs

The API handles login, orders and account balances. The worker matches buy and
sell orders. The simulator changes prices and adds orders to the market.

All three use PostgreSQL. It keeps the account balances, orders and completed
trades. Redis is included in Docker Compose but is not used by the Rust code yet.

Orders with better prices are matched first. If two orders have the same price,
the older one goes first. Money is reserved when an order is placed, then used
when it fills or returned when it is cancelled.

## Start with Docker

Run these commands from the project folder. You need Docker with Compose.

```powershell
# Start the database and wait for it to be ready.
docker compose up -d --wait postgres

# Build the API and create the database tables.
docker compose run --build --rm api migrate

# Start the rest of the project.
docker compose up --build
```

The migrations also add the three markets and their starting prices.

Open the API at http://localhost:3000. Prometheus is at
http://localhost:9090 and Grafana is at http://localhost:3001.
The local Grafana login is `admin` / `admin`.

To stop the services without deleting the database:

```powershell
docker compose down
```

The passwords in Compose are only for local testing.

## Run the Rust code directly

You need Rust and PostgreSQL 16 or newer. Create a database first.

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

Then run these commands in three separate terminals:

```powershell
cargo run -p exchange-api
cargo run -p exchange-worker
cargo run -p exchange-simulator
```

Run them from the project folder so each service can load `.env`.

## Try it

This PowerShell example creates a demo account and uses its login token to
read the account balance. Use a different email if the account already exists.

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

## Find your way around

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

The tests cover order validation and matching rules. They do not yet test the
full application against a running database.

The API also provides `/healthz` to check that it is running, `/readyz` to
check its database connection, and `/metrics` for Prometheus.

## Deployment

GitHub Actions checks application changes and builds the images. On `main`,
it publishes them to Docker Hub and updates the development image versions in
Git. Argo CD reads those versions and updates Kubernetes.

Terraform creates the OCI infrastructure. Ansible sets up Kubernetes.
Application secrets are read from OCI Vault rather than stored in Git.

The setup details are in the [pipeline guide](docs/cicd.md) and
[deployment guide](deploy/README.md). Dashboards and alerts are covered in
the [monitoring guide](monitoring/README.md).

## Before using this for anything real

This is a learning project, not a real-money exchange. Passwords currently use
a basic SHA-256 hash, which is not suitable for real user passwords.

Do not reuse the demo credentials. Keep private keys, tokens, passwords and
Terraform state out of Git. The development cluster uses local-disk storage;
it needs a proper storage and backup plan before holding important data.
