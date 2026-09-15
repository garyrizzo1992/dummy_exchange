# API service hosting contract

Binary: `exchange-api` (`cargo run -p exchange-api`). This is the public, stateless HTTP entrypoint. It may be replicated horizontally; no sticky sessions are required because authentication is JWT based.

## Process and network

| Item | Requirement |
|---|---|
| Listener | TCP address set by `API_BIND`; default `127.0.0.1:3000`. For a container, set `0.0.0.0:3000`. |
| Ingress | Route public HTTPS traffic to the configured port. TLS should terminate at the platform ingress/proxy. |
| Shutdown | Send `SIGTERM`; the process should be allowed a short grace period to finish active requests. |
| Filesystem | No persistent volume, writable filesystem, or local state required. Run as a non-root user with a read-only filesystem where practical. |
| Scaling | Scale on request rate/latency and database connection capacity. Each replica holds a SQL connection pool. |

## Required configuration

| Variable | Required | Meaning |
|---|---:|---|
| `DATABASE_URL` | Yes | PostgreSQL connection string. It must reach the authoritative exchange database. |
| `JWT_SECRET` | Yes | High-entropy signing secret shared by all API replicas. Supply through the platform secret mechanism, never an image or environment file committed to source control. |
| `API_BIND` | No | Bind address; use `0.0.0.0:3000` in a container. |
| `RUST_LOG` | No | Structured log filter, e.g. `info,exchange_api=debug`. |

The startup path runs SQL migrations. For a multi-replica production rollout, run migrations as a separate, single controlled release step instead of allowing every API replica to perform schema changes at startup.

## Health and observability

| Endpoint | Intended probe/use | Success |
|---|---|---|
| `GET /healthz` | Liveness | HTTP 200 while the process is responsive. |
| `GET /readyz` | Readiness | HTTP 200 only when PostgreSQL is reachable; HTTP 503 otherwise. |
| `GET /metrics` | Metrics scraper | Prometheus text exposition. |

Logs are JSON on stdout/stderr and HTTP request IDs are accepted/propagated with `x-request-id`. Capture stdout centrally. Trace/exporter configuration belongs to the deployment environment; the application must not assume a local collector.

## Dependency permissions

The API database role needs read/write access to `users`, `accounts`, `orders`, `fills`, `outbox_events`, and `audit_log`; schema migration authority should be a separate, more privileged release role. Database traffic must use TLS when it crosses a trusted network boundary.

## Resource starting point

Begin with 0.25–0.5 vCPU and 256–512 MiB memory per replica, then load test using `benchmarks/orders.js`. Database pool limits should be deliberately capped to prevent replica scaling from exhausting PostgreSQL connections.
