# Dependency hosting contract

## PostgreSQL (required)

PostgreSQL is the source of truth. It must be durable, backed up, monitored, and available to both API and worker processes. Required features are standard PostgreSQL plus the `uuid-ossp` extension used by the initial migration.

- Create a dedicated database and least-privilege runtime role.
- Apply `migrations/` once per release, before rolling out application versions that depend on them.
- Use durable storage and test backup restoration; balances, orders, fills, and audit history cannot be reconstructed from caches.
- Permit API and worker network access, preferably using encrypted connections and private routing.
- Size `max_connections` for all API/worker replicas plus administrative and migration connections.

## Redis (optional today)

Redis is not on the correctness path in the current vertical slice. It is reserved for future cache, rate limiting, websocket/SSE fan-out, and ephemeral coordination. A Redis outage must never invalidate an order, balance, fill, or audit record. If it is introduced, configure timeouts and explicit degradation behaviour instead of making the API unavailable by default.

## Secrets and configuration

Inject `DATABASE_URL` and `JWT_SECRET` using the hosting platform’s secret mechanism. Rotate secrets using a controlled rollout; JWT secret rotation will require support for a previous-key validation window when implemented. `.env.example` documents names only and is not a deployment mechanism.
