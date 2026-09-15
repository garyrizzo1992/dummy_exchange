# Market Simulator

`exchange-simulator` creates the living crypto market. Every tick it evolves reference prices for BTC/USD, ETH/USD, and SOL/USD and refreshes system-owned bid/ask liquidity around each price. It never changes user-owned orders.

## Run

```powershell
cargo run -p exchange-simulator
```

## Hosting requirements

- **Network:** outbound PostgreSQL access only; no inbound port or ingress.
- **Database:** required, shared with the API and matching worker.
- **Persistence:** no volume/local state required. Reference prices and liquidity are persisted in PostgreSQL.
- **Scaling:** run one replica initially. Multiple simulators are database-safe but create competing price paths. Partition simulator ownership by instrument before scaling replicas.
- **Shutdown:** provide a short database transaction grace period.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string. |
| `SIMULATION_SEED` | No | `42` | Integer seed for reproducible price movement on a clean state. |
| `SIMULATOR_METRICS_BIND` | No | `0.0.0.0:3002` | Private listener exposing `/metrics` and `/healthz`. |
| `RUST_LOG` | No | Rust default | JSON log filter. |

The binary loads root `.env` for local development.

## Operational notes

The simulator bootstraps a system market-maker account with simulated inventory. It writes `market_state` and system orders only; matching and balance settlement remain the matching worker’s responsibility.
