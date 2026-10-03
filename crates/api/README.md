# Exchange API

`exchange-api` handles the public HTTP requests. Users can log in, create
simulated accounts, place or cancel orders, and check their balances and trades.
The API reserves funds for orders and provides instruments, tickers and order books.

## Run

From the workspace root:

```powershell
cargo run -p exchange-api
```

## Hosting requirements

- Listen on `API_BIND`, which defaults to `127.0.0.1:3000`. Use `0.0.0.0:3000` in a container.
- The API needs PostgreSQL and must share the database with the matching workers.
- The database holds the data. The API does not need a local disk or volume.
- Put HTTPS ingress or a reverse proxy in front of the API. The API itself serves HTTP.
- You can run multiple API replicas because they are stateless. Make sure PostgreSQL can handle their connections.
- Allow a short shutdown grace period for requests already being processed.

## Container build

Build from the repository root so Cargo can see the full workspace:

```powershell
docker build -f crates/api/Dockerfile -t exchange-api .
```

The image listens on port `3000`. Supply `DATABASE_URL` and `JWT_SECRET` when
starting it. Do not include `.env` files or secrets in the image.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | Not set | PostgreSQL connection string. |
| `JWT_SECRET` | Yes | Development fallback only | Strong shared JWT signing secret; supply it securely. |
| `API_BIND` | No | `127.0.0.1:3000` | Bind address and port. |
| `RUST_LOG` | No | Rust default | Filter JSON logs, for example `info,exchange_api=debug`. |

For local development, the API loads `.env` from the project root. In
production, supply environment variables through your configuration and secret
management.

## Endpoints

| Endpoint | Purpose |
|---|---|
| `POST /v1/auth/register` | Create an account and receive a JWT. |
| `POST /v1/auth/login` | Log in and receive a JWT. |
| `POST /v1/orders` | Submit a market or limit order. |
| `POST /v1/orders/{id}/cancel` | Cancel an open order and release its reserved funds. |
| `GET /v1/orders`, `/v1/fills`, `/v1/accounts/balances` | Read account data after logging in. |
| `GET /v1/instruments` | List simulated markets available for trading. |
| `GET /v1/markets/{symbol}/ticker`, `/book` | Public market data. |
| `GET /healthz`, `/readyz`, `/metrics` | Check the process, database connection and metrics. |

## Operational notes

`/healthz` checks whether the API is running. `/readyz` returns 503 if it cannot
reach PostgreSQL. Logs use JSON and support `x-request-id`.

`/metrics` returns Prometheus-format metrics for HTTP requests, accepted and
cancelled orders, and the value of submitted orders. Order metrics use only
`instrument`, `side`, and `order_type` labels.

Run `exchange-api migrate` once before deploying API replicas for a release.

### Kubernetes database credentials

The chart sets `PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`, `PGPASSWORD`, and
`PGSSLMODE`. The shared `exchange-config` crate passes credentials directly to
SQLx, including passwords containing URL punctuation. `DATABASE_URL` remains
supported and takes precedence for existing Compose and local environments.
