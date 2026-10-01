# Matching worker

`exchange-worker` reads open orders and matches them by price, then age. It
records completed trades and updates the orders and simulated account balances
in PostgreSQL.

## Run

```powershell
cargo run -p exchange-worker
```

## Hosting requirements

- The worker needs outbound PostgreSQL access. Keep it out of public ingress; its HTTP listener is for private metrics and health checks.
- Use the same PostgreSQL database as the API and simulator.
- No local disk or volume is needed.
- You can run multiple workers. A PostgreSQL advisory lock lets only one worker update a market's order book at a time. Other workers can process different markets or take over after a failure.
- Let the current database transaction finish or roll back during shutdown. PostgreSQL releases its locks when the connection closes.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | Not set | PostgreSQL connection string. |
| `WORKER_ID` | Recommended | Generated UUID | Identifies the worker in logs. |
| `WORKER_METRICS_BIND` | No | `0.0.0.0:3001` | Private HTTP listener for `/metrics` and `/healthz`. |
| `RUST_LOG` | No | Rust default | JSON log filter. |

For local development, the worker loads `.env` from the project root.

## Failure behaviour

If PostgreSQL becomes unavailable during matching, the worker logs the error and
retries. It saves order and trade changes in a transaction, and a database unique
constraint prevents duplicate trades. Committed trades survive a worker restart.
Another worker can take the market's lock when it is released.

`/metrics` reports matching cycles and errors, total trades, user buy and sell
trades, and summaries of trade value. Trade metrics use only instrument labels,
so adding workers does not create an unbounded number of label values.
