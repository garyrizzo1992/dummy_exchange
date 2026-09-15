# Market simulator hosting contract

Binary: `exchange-simulator` (`cargo run -p exchange-simulator`). It produces independent deterministic price paths and replaces only system-owned liquidity for each simulated instrument. It has no inbound port.

| Variable | Required | Meaning |
|---|---:|---|
| `DATABASE_URL` | Yes | Authoritative PostgreSQL database shared with API and matching workers. |
| `SIMULATION_SEED` | No | Integer random seed; defaults to `42`, making a clean run reproducible. |
| `RUST_LOG` | No | JSON log filter. |

Run exactly one simulator replica initially. Multiple replicas preserve database integrity but generate competing price paths and liquidity churn. Partition simulator ownership by instrument before scaling this service. It needs no persistent filesystem or ingress—only PostgreSQL connectivity and a short shutdown grace period.
