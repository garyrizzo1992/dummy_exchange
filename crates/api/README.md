# Exchange API

`exchange-api` is the public backend for the simulated exchange. It authenticates users, creates simulated accounts, accepts/cancels orders, reserves funds, and exposes balances, fills, instruments, tickers, and order books.

## Run

From the workspace root:

```powershell
cargo run -p exchange-api
```

## Hosting requirements

- **Port:** `API_BIND`, default `127.0.0.1:3000`; set `0.0.0.0:3000` in a container.
- **Database:** PostgreSQL is required. The API and matching workers must use the same database.
- **Persistence:** no local disk or volume is required; database data is authoritative.
- **Ingress:** expose this service through HTTPS-capable ingress or a reverse proxy. The service itself serves HTTP.
- **Scaling:** stateless and safe to replicate. Ensure PostgreSQL connection capacity matches replica count.
- **Shutdown:** allow a short graceful shutdown period for in-flight HTTP requests.

## Container build

Build from the repository root so Cargo can see the full workspace:

```powershell
docker build -f crates/api/Dockerfile -t exchange-api .
```

The image listens on port `3000`; supply `DATABASE_URL` and `JWT_SECRET` at runtime. Do not bake `.env` files or secrets into the image.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string. |
| `JWT_SECRET` | Yes | Development fallback only | Shared high-entropy JWT signing secret; inject securely. |
| `API_BIND` | No | `127.0.0.1:3000` | Bind address and port. |
| `RUST_LOG` | No | Rust default | JSON log filtering, e.g. `info,exchange_api=debug`. |

The binary loads a root `.env` file for local development. Production should inject variables through its secret/configuration mechanism.

## Endpoints

| Endpoint | Purpose |
|---|---|
| `POST /v1/auth/register` | Register and receive a JWT. |
| `POST /v1/auth/login` | Authenticate and receive a JWT. |
| `POST /v1/orders` | Submit a market or limit order. |
| `POST /v1/orders/{id}/cancel` | Cancel an open order and release its reservation. |
| `GET /v1/orders`, `/v1/fills`, `/v1/accounts/balances` | Authenticated account data. |
| `GET /v1/instruments` | Tradable simulated markets. |
| `GET /v1/markets/{symbol}/ticker`, `/book` | Public market data. |
| `GET /healthz`, `/readyz`, `/metrics` | Liveness, database readiness, and metrics. |

## Operational notes

`/healthz` is a process check. `/readyz` returns 503 when PostgreSQL cannot be reached. Logs are JSON and support `x-request-id`; `/metrics` uses Prometheus exposition format. Run migrations once as a release operation rather than independently on every API replica.
