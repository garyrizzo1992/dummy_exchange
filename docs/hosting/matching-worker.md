# Matching worker hosting contract

Binary: `exchange-worker` (`cargo run -p exchange-worker`). This is a background execution process; it has no public HTTP listener and should not be exposed through ingress.

## Process and scaling model

The worker polls authoritative orders and uses a PostgreSQL advisory transaction lock keyed by instrument. Many replicas can run safely: only one holds a given instrument’s matching lease at a time, while other workers quickly yield and retry. This gives active/active deployment and restart recovery without a leader-election service.

| Item | Requirement |
|---|---|
| Network | Outbound PostgreSQL connectivity only for the current implementation. No inbound port. |
| Identity | Set a unique, stable-per-process `WORKER_ID` for log/event attribution. |
| Filesystem | No persistent volume or local state. A non-root, read-only filesystem is appropriate. |
| Shutdown | Send `SIGTERM` and allow time for the current transaction to finish or roll back. PostgreSQL releases advisory transaction locks automatically on rollback/disconnect. |
| Scaling | Add replicas externally. Throughput is serialized per instrument by design; adding instruments allows parallel matching. |

## Required configuration

| Variable | Required | Meaning |
|---|---:|---|
| `DATABASE_URL` | Yes | PostgreSQL connection string to the same database used by the API. |
| `WORKER_ID` | Recommended | Distinct process identifier, e.g. a deployment-generated instance ID. |
| `RUST_LOG` | No | JSON logging filter. |

Do not run database migrations from every worker. Apply them once before workers receive traffic.

## Operational signals

The worker emits JSON logs, including matching failures and successful fill batches. It has no HTTP health endpoint in the current vertical slice; use process supervision plus database connectivity/log signals. A future production increment should add a small private health server or heartbeat record before relying on platform readiness probes for the worker.

## Resource starting point

Begin at 0.25–0.5 vCPU and 128–256 MiB memory. Matching is CPU-light in the current single-instrument implementation; benchmark with realistic book depth and order flow before setting limits. Ensure PostgreSQL has capacity for every worker’s connections.

## Failure behaviour

If PostgreSQL is unavailable, the worker records the error and retries on its next polling cycle; it cannot execute a partial transaction. On restart, open orders remain durable and another worker can acquire the instrument lock. Duplicate fill insertion is guarded by the database unique constraint.
