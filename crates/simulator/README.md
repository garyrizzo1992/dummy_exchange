# Market simulator

`exchange-simulator` updates the reference prices for BTC/USD, ETH/USD and
SOL/USD on each cycle. It replaces the system's buy and sell orders around those
prices, leaving user orders alone.

## Run

```powershell
cargo run -p exchange-simulator
```

## Hosting requirements

- The simulator needs outbound PostgreSQL access. Keep it out of public ingress; its HTTP listener is for private metrics and health checks.
- Use the same PostgreSQL database as the API and matching worker.
- Prices and market orders are stored in PostgreSQL. No local state or volume is needed.
- Start with one simulator. Multiple instances can use the database safely, but each changes the prices independently. Assign different markets to different simulators before adding replicas.
- Allow a short shutdown grace period for database transactions.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | Not set | PostgreSQL connection string. |
| `SIMULATION_SEED` | No | `42` | Integer seed that repeats price movements when starting from a clean state. |
| `SIMULATOR_METRICS_BIND` | No | `0.0.0.0:3002` | Private HTTP listener for `/metrics` and `/healthz`. |
| `RUST_LOG` | No | Rust default | JSON log filter. |

For local development, the simulator loads `.env` from the project root.

## Operational notes

The simulator creates a system market-maker account with simulated inventory.
It updates `market_state` and system orders. The matching worker handles trades
and account balance updates.

### Kubernetes database credentials

The chart sets `PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`, `PGPASSWORD`, and
`PGSSLMODE`. The shared `exchange-config` crate passes credentials directly to
SQLx, including passwords containing URL punctuation. `DATABASE_URL` remains
supported and takes precedence for existing Compose and local environments.
