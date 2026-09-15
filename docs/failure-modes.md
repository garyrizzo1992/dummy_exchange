# Failure-mode exercises

| Scenario | Expected behaviour | Exercise |
|---|---|---|
| Worker restart | Unprocessed outbox rows remain durable; the next worker resumes. | Stop and restart `exchange-worker` while orders are open. |
| Duplicate delivery | Fill unique constraint makes duplicate execution a no-op. | Replay an outbox event twice. |
| Database unavailable | Readiness returns 503; writes fail without partial commit. | Stop PostgreSQL and call `/readyz`. |
| Redis unavailable | Current vertical slice does not depend on Redis for correctness. Future stream/rate-limit features degrade independently. | Point `REDIS_URL` at an unavailable endpoint. |
| Partial order processing | Transaction rolls back reservation, order, and outbox together. | Terminate API during an order request and inspect tables. |
