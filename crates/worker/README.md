# Matching Worker

`exchange-worker` is the internal execution service. It reads open orders, applies price-time matching, writes fills, updates order state, and settles simulated user balances in PostgreSQL.

## Run

```powershell
cargo run -p exchange-worker
```

## Hosting requirements

- **Network:** outbound PostgreSQL access only. It has no HTTP port and must not be exposed through ingress.
- **Database:** PostgreSQL is required and must be the same database used by the API and simulator.
- **Persistence:** no local disk or volume is required.
- **Scaling:** safe to scale horizontally. Workers obtain a PostgreSQL advisory lock per instrument, so exactly one worker mutates an instrument’s book at a time. Extra workers can process other instruments or take over after a failure.
- **Shutdown:** allow the current database transaction to complete or roll back. Transaction locks are released automatically on disconnect.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string. |
| `WORKER_ID` | Recommended | Generated UUID | Process identifier included in logs. |
| `WORKER_METRICS_BIND` | No | `0.0.0.0:3001` | Private listener exposing `/metrics` and `/healthz`. |
| `RUST_LOG` | No | Rust default | JSON log filter. |

The binary loads root `.env` for local development.

## Failure behaviour

If PostgreSQL is unavailable, the worker logs the failure and retries. Orders and fills are transactional, and duplicate fill insertion is guarded by a database unique constraint. A worker restart cannot lose committed trades; another worker can acquire the released instrument lease.

`/metrics` exposes worker ticks/errors, total fills, user buy/sell fills, and execution-notional summaries. Trade metrics are labelled by instrument only, keeping Prometheus cardinality bounded when the worker fleet scales.
